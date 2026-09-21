//! The nogood database: a persistent memory of forbidden local patterns.
//!
//! This is the CA version of the learned-clause database of a CDCL SAT
//! solver. When the conflict analysis of [`Config::backjump`](crate::Config::backjump)
//! derives a clause, the corresponding nogood — a set of cells with states
//! that cannot all hold together in any solution — is recorded here, so that
//! the knowledge survives the backtracking that discards the trail.
//!
//! In this first, exact-position mode, a nogood is stored by the *absolute*
//! indices and states of its cells. It is valid only within the world where
//! it was learned ([`World`](crate::World)), because the derivation may rely
//! on facts specific to this configuration, e.g. the background state forced
//! on the cells outside the search range. The database is therefore cleared
//! whenever the world is rebuilt.

use crate::rule::CellState;
use rustc_hash::{FxHashMap, FxHashSet, FxHasher};
use std::hash::Hasher;

/// The literal-to-ids index of the database.
///
/// The index is queried once per cell assignment, so the default SipHash of
/// [`HashMap`](std::collections::HashMap) is a measurable part of the search
/// time; the Fx hash of `rustc-hash` is the fast non-cryptographic hasher
/// used by `rustc` itself. A collision only costs a little probing: the key
/// is mapped to its own bucket, and no other property of the hash is relied
/// on.
type LiteralMap = FxHashMap<u64, Vec<u32>>;

/// The set of the hashes of the stored nogoods.
type LiteralSet = FxHashSet<u64>;

/// The key of a literal in the index: the cell index and the state number,
/// packed into one integer.
#[inline]
fn literal_key(cell: u32, state: CellState) -> u64 {
    debug_assert!(matches!(state, CellState::Dead | CellState::Alive));
    (u64::from(cell) << 1) | u64::from(state.number() & 1)
}

/// A hash of the sorted literals of a nogood, used to reject duplicates.
///
/// A collision only rejects a duplicate-looking entry and loses pruning; it
/// never affects correctness.
fn literals_hash(literals: &[(u32, CellState)]) -> u64 {
    let mut hasher = FxHasher::default();
    for &(cell, state) in literals {
        hasher.write_u32(cell);
        hasher.write_u8(state.number());
    }
    hasher.finish()
}

/// The minimal capacity of the database, in entries.
///
/// When the database outgrows its capacity, half of the entries are evicted,
/// worst first by the recency ranking of [`NogoodDb::reduce`], like the
/// clause-database reduction of a SAT solver.
///
/// The per-set maintenance walks the index bucket of the set literal, whose
/// size grows with the number of stored entries and with their average
/// length, so a smaller database is proportionally cheaper to maintain. The
/// minimal capacity keeps small worlds cheap; larger worlds scale the
/// capacity with the number of cells (see [`NogoodDb::with_world_size`]),
/// because a database that only remembers the last few thousand steps cannot
/// reuse anything on a long search. A fixed capacity of this size was the old
/// default and is still the floor for the automatic choice.
const DEFAULT_CAPACITY: usize = 1 << 11;

/// The factor applied to the number of cells for the automatic capacity.
///
/// The live database then holds between `FACTOR * cells / 2` and
/// `FACTOR * cells` entries, while the average index bucket stays roughly
/// constant as the world grows, because the number of distinct literals also
/// grows with the number of cells.
const ADAPTIVE_CAPACITY_FACTOR: usize = 4;

/// The maximal number of candidates examined by a single query.
///
/// A popular anchor cell may share its index bucket with many nogoods;
/// without a bound, the queries at that cell would dominate the search time.
/// A blocked guess missed because of the bound is only a lost pruning, never
/// a correctness issue.
const MAX_QUERY_CANDIDATES: usize = 64;

/// The maximal number of literals of a learned nogood.
///
/// The learned clauses of the conflict analysis are often much longer than
/// the descriptor that seeded them, because resolving the current-level
/// literals can accumulate many lower-level literals. A sweep on the
/// benchmark workloads (see `docs/sat-ideas.md`) found that rejecting the
/// long clauses discards most of the pruning power: allowing them shortens
/// the deep first-result benchmark by a factor of two. The bound is only a
/// guard against pathological growth; it is well above the observed clause
/// lengths.
const MAX_NOGOOD_LITERALS: usize = 96;

/// The number of evicted nogoods kept for the all-time usage report.
///
/// The main database evicts half of its entries when it reaches capacity, so
/// the most-used entries of a long search are usually gone from it. This small
/// secondary list remembers the most-used evicted entries (with their final
/// use counts) so that [`NogoodDb::top_entries`] can report the all-time
/// most-used patterns. It is diagnostic state only: it is never queried for
/// propagation and never affects the search.
const HALL_OF_FAME: usize = 64;

/// The number of bins of the reuse-distance histogram.
///
/// A distance is measured in database reductions between learning an entry
/// and its first use, and is binned by powers of two: `0`, `1`, `2`, `3`,
/// `4-7`, `8-15`, `16-31`, `32-63`, and `64+`.
const REUSE_DISTANCE_BINS: usize = 9;

/// The bin index of a first-use distance, in reductions.
#[inline]
const fn reuse_bin(distance: u32) -> usize {
    match distance {
        0 => 0,
        1 => 1,
        2 => 2,
        3 => 3,
        4..=7 => 4,
        8..=15 => 5,
        16..=31 => 6,
        32..=63 => 7,
        _ => 8,
    }
}

/// The bound of the recently-evicted hash memory, as a multiple of the
/// database capacity.
///
/// The memory is diagnostic: it counts entries that are learned again after
/// having been evicted. At the observed learning rate, roughly `capacity / 2`
/// entries are evicted per reduction, so twice the capacity remembers about
/// four reductions of history.
const EVICTED_MEMORY_FACTOR: usize = 2;

/// The absolute bound of the recently-evicted hash memory, in hashes.
///
/// On very large worlds the capacity can be tens of thousands of entries;
/// this keeps the diagnostic overhead bounded. Entries evicted before the
/// most recent flush are not remembered, so
/// [`NogoodStats::relearned_evicted`] is a lower bound.
const EVICTED_MEMORY_LIMIT: usize = 1 << 20;

/// Statistics of the nogood database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NogoodStats {
    /// The number of nogoods that have been stored.
    pub learned: u64,

    /// The number of times a guess or a backtrack flip was blocked by a
    /// nogood. Most blocked states are caught earlier by propagation-level
    /// firing instead; see [`fired`](NogoodStats::fired).
    pub hits: u64,

    /// The number of times a nogood fired during propagation: a cell was
    /// forced away from the recorded state because all the other literals of
    /// the nogood held.
    pub fired: u64,

    /// The number of entries evicted when the database was reduced.
    pub evicted: u64,

    /// The number of times the database has been reduced.
    pub reductions: u64,

    /// The number of completion queries that found a non-empty index bucket.
    ///
    /// A query looks up the nogoods sharing a `(cell, state)` literal and
    /// checks whether one of them would be completed by that assignment.
    /// Empty buckets are not counted: they cost one hash lookup and nothing
    /// else.
    pub queries: u64,

    /// The number of completion queries whose index bucket was larger than
    /// [`MAX_QUERY_CANDIDATES`](self::MAX_QUERY_CANDIDATES).
    ///
    /// A capped query examines only a prefix of its bucket, so it may miss a
    /// matching entry; that is a lost pruning, never a correctness issue.
    /// A high ratio means that the cap is binding and the bucket structure
    /// needs attention.
    pub capped_queries: u64,

    /// The total number of literals of the stored nogoods, counted at learn
    /// time.
    ///
    /// Together with [`learned`](NogoodStats::learned) this gives the average
    /// length of the entries ever stored; the current entries may be shorter
    /// after evictions.
    pub literals_total: u64,

    /// The number of learned clauses rejected because they had more than
    /// [`MAX_NOGOOD_LITERALS`](self::MAX_NOGOOD_LITERALS) literals or
    /// contained a repeated cell.
    ///
    /// These clauses are never stored, so their pruning power is lost
    /// entirely. With the current bound the count is small on the benchmark
    /// workloads: the long clauses of the conflict analysis are useful, and
    /// rejecting them was a major source of lost pruning.
    pub rejected_long: u64,

    /// The number of distinct learned nogoods that have been used at least
    /// once, either by firing during propagation or by blocking a guess or a
    /// chronological flip.
    ///
    /// Unlike [`fired`](NogoodStats::fired) and [`hits`](NogoodStats::hits),
    /// which count events, this counts entries. It is cumulative: an entry is
    /// counted once, when it is first used, and is not forgotten when the
    /// database later evicts it.
    pub used_learned: u64,

    /// The number of times a learned nogood was fully matched by the current
    /// assignment and queued as a conflict.
    ///
    /// This is a conflict detection rather than a unit propagation: see
    /// [`fired`](NogoodStats::fired).
    pub full_matches: u64,

    /// A histogram of the lengths of the learned nogoods, in literals.
    ///
    /// `length_histogram[n]` is the number of stored nogoods that were
    /// learned with exactly `n` literals. Index `0` is unused, and clauses
    /// longer than [`MAX_NOGOOD_LITERALS`](self::MAX_NOGOOD_LITERALS) are
    /// counted in [`rejected_long`](NogoodStats::rejected_long) instead. The
    /// histogram is cumulative over the whole search, like
    /// [`learned`](NogoodStats::learned).
    pub length_histogram: [u64; MAX_NOGOOD_LITERALS + 1],

    /// A histogram of the LBD (literal block distance) of the learned
    /// nogoods at learn time.
    ///
    /// The LBD is the number of distinct decision levels of the literals of
    /// the learned clause, plus one for the 1-UIP cell, the closest analogue
    /// of the glue measure of a SAT solver. It is indexed like
    /// [`length_histogram`](NogoodStats::length_histogram), since the LBD
    /// never exceeds the number of literals. The database does not use the
    /// LBD for eviction; it is recorded so that a quality-based policy can be
    /// evaluated with data instead of assumptions.
    pub lbd_histogram: [u64; MAX_NOGOOD_LITERALS + 1],

    /// A histogram of the lengths of the learned nogoods at their *first*
    /// use, in literals.
    ///
    /// Comparing it with [`length_histogram`](NogoodStats::length_histogram)
    /// shows which lengths are overrepresented among the entries that
    /// actually fire or block a guess. Short entries enjoy a large
    /// event-rate advantage, because they are one literal short of a match
    /// far more often, so the raw use counts alone do not separate "useful"
    /// from "frequently triggerable".
    pub used_length_histogram: [u64; MAX_NOGOOD_LITERALS + 1],

    /// A histogram of the LBD of the learned nogoods at their first use.
    ///
    /// See [`lbd_histogram`](NogoodStats::lbd_histogram); this is the
    /// subset that was used at least once.
    pub used_lbd_histogram: [u64; MAX_NOGOOD_LITERALS + 1],

    /// A histogram of the first-use delay of the learned nogoods, in
    /// database reductions.
    ///
    /// `reuse_distance_histogram[b]` counts the entries whose first use
    /// happened in bin `b` after they were learned; the bins are powers of
    /// two, see [`REUSE_DISTANCE_BINS`]. It estimates how long learned
    /// entries must survive eviction to be reused at all. Entries evicted
    /// before their first use are counted in
    /// [`evicted_unused`](NogoodStats::evicted_unused) instead, so the two
    /// together describe the retention need.
    pub reuse_distance_histogram: [u64; REUSE_DISTANCE_BINS],

    /// The number of stored entries that were evicted without ever having
    /// been used.
    ///
    /// These entries never pruned anything while they were live: evicting
    /// them was free, and a high ratio means that the database mostly churns
    /// speculative entries.
    pub evicted_unused: u64,

    /// The sum of the use counts of the evicted entries at the time of their
    /// eviction.
    ///
    /// This is the amount of proven propagation work that the reduction
    /// threw away; together with [`evicted_unused`](NogoodStats::evicted_unused)
    /// it separates "lost knowledge" from "reclaimed dead weight".
    pub evicted_uses_total: u64,

    /// The number of learned nogoods whose exact literal set had been
    /// evicted from the database before and was learned again.
    ///
    /// This is a direct lower bound on the churn caused by eviction: it
    /// counts the cases in which the search re-derived a pattern that the
    /// database used to know. The evicted patterns are remembered in a
    /// bounded set, and a hash collision can only hide a relearning, so the
    /// counter never overstates the churn.
    pub relearned_evicted: u64,

    /// The number of times the recently-evicted pattern memory was dropped
    /// because it reached its bound.
    ///
    /// Each flush forgets older evictions, so
    /// [`relearned_evicted`](NogoodStats::relearned_evicted) is a lower
    /// bound whose tightness decreases with this counter.
    pub evicted_memory_flushes: u64,

    /// The number of candidate entries examined by
    /// [`fire_candidate`](NogoodDb::fire_candidate).
    ///
    /// Together with [`fired`](NogoodStats::fired) this gives the hit rate
    /// of propagation-level firing; a candidate that turned out stale or
    /// blocked counts as an attempt but not as a firing.
    pub fire_attempts: u64,

    /// The number of stored entries that were already fully matched by the
    /// current partial assignment when they were learned.
    ///
    /// This is the chronological-fallback case of the conflict analysis: the
    /// entry cannot fire usefully right away, but the backtracking that
    /// follows resynchronizes its counter.
    pub learned_ready: u64,
}

// `Default` is implemented by hand because `[u64; N]` only implements
// `Default` for small `N`, and the histogram has one bin per literal count.
impl Default for NogoodStats {
    fn default() -> Self {
        Self {
            learned: 0,
            hits: 0,
            fired: 0,
            evicted: 0,
            reductions: 0,
            queries: 0,
            capped_queries: 0,
            literals_total: 0,
            rejected_long: 0,
            used_learned: 0,
            full_matches: 0,
            length_histogram: [0; MAX_NOGOOD_LITERALS + 1],
            lbd_histogram: [0; MAX_NOGOOD_LITERALS + 1],
            used_length_histogram: [0; MAX_NOGOOD_LITERALS + 1],
            used_lbd_histogram: [0; MAX_NOGOOD_LITERALS + 1],
            reuse_distance_histogram: [0; REUSE_DISTANCE_BINS],
            evicted_unused: 0,
            evicted_uses_total: 0,
            relearned_evicted: 0,
            evicted_memory_flushes: 0,
            fire_attempts: 0,
            learned_ready: 0,
        }
    }
}

/// A learned nogood: an assignment of states to cells that cannot be part of
/// any solution.
///
/// The literals are pairs of absolute cell indices and the states that these
/// cells must not all take at once. They are kept sorted by `(cell, state)`,
/// so that duplicate entries are recognized by a hash.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Nogood {
    /// The LBD of the learned clause: the number of distinct decision levels
    /// of its literals plus one for the 1-UIP cell.
    ///
    /// This is diagnostic state. It is never used by the search yet, and the
    /// database does not rank or evict by it; it is stored so that the
    /// usefulness of a glue-based policy can be measured.
    lbd: u8,

    /// The literals of the nogood, sorted.
    literals: Box<[(u32, CellState)]>,
}

/// A database of learned nogoods.
///
/// The nogoods are indexed by their literals: for every (cell index, state)
/// pair, the index lists the nogoods containing it, so that a candidate guess
/// can find the nogoods that it would complete without scanning the whole
/// database.
#[derive(Debug)]
pub struct NogoodDb {
    /// The stored nogoods, in insertion order. Ids in the index are positions
    /// in this vector.
    entries: Vec<Nogood>,

    /// The number of literals of each entry that do not currently hold,
    /// indexed by entry id and maintained incrementally by
    /// [`on_set`](NogoodDb::on_set) and [`on_unset`](NogoodDb::on_unset).
    ///
    /// While this is one and the remaining cell is unknown, the nogood
    /// *fires*: the remaining cell cannot take its recorded state. When all
    /// the literals hold, the current partial assignment is contradictory.
    ///
    /// The counters live in a dense array next to the entries, not inside
    /// them, so that the hot bucket walk of `on_set` and `on_unset` touches
    /// only compact memory.
    ///
    /// This relies on the following invariants:
    ///
    /// - every set and unset of a cell outside a lookahead probe updates the
    ///   counters through the `(cell, state)` index;
    /// - the database starts empty in every world (fresh worlds, save/load,
    ///   and world growth), so the counters are built up from a clean state;
    /// - a firing prevents the last unknown cell from completing the nogood,
    ///   and a full match that arises through a re-set cell is caught by the
    ///   full-match check of [`on_set`](NogoodDb::on_set).
    remaining: Vec<u32>,

    /// How many times each entry has been used, indexed by entry id.
    ///
    /// An entry is used when it fires during propagation or when it blocks a
    /// guess or a chronological flip. The counts are parallel to
    /// [`entries`](NogoodDb::entries) and are drained together with them on a
    /// reduction; the cumulative [`used_learned`](NogoodStats::used_learned)
    /// counter in the statistics is not.
    uses: Vec<u32>,

    /// The reduction epoch in which each entry was learned, indexed by entry
    /// id.
    ///
    /// Together with the current [`NogoodStats::reductions`] this gives the
    /// first-use delay recorded in
    /// [`reuse_distance_histogram`](NogoodStats::reuse_distance_histogram).
    learned_epoch: Vec<u32>,

    /// The reduction epoch in which each entry was last used, indexed by
    /// entry id.
    ///
    /// An entry that has not been used yet keeps the epoch in which it was
    /// learned. This is diagnostic for now; a recency-based eviction policy
    /// would rank entries by it instead of by insertion order.
    last_used_epoch: Vec<u32>,

    /// The hashes of recently evicted entries, used to count relearnings
    /// after eviction.
    ///
    /// The set is diagnostic: it is never part of propagation. It is bounded
    /// by [`EVICTED_MEMORY_FACTOR`] times the capacity, further capped at
    /// [`EVICTED_MEMORY_LIMIT`], and dropped when it grows past the bound.
    recently_evicted: FxHashSet<u64>,

    /// The most-used entries that have been evicted, in descending use order.
    ///
    /// This is a bounded diagnostic record, not part of the live database: it
    /// is never indexed and never queried during propagation. It exists so
    /// that [`top_entries`](NogoodDb::top_entries) can report the all-time
    /// most-used patterns even after a reduction. See [`HALL_OF_FAME`].
    hall_of_fame: Vec<(u32, Nogood)>,

    /// For each literal, the ids of the nogoods containing it.
    index: LiteralMap,

    /// The hashes of the sorted literals of the stored entries, used to
    /// reject verbatim duplicates without comparing entries.
    hashes: LiteralSet,

    /// The maximal number of entries before half of the database is evicted.
    ///
    /// Zero disables the database entirely: nothing is learned or queried.
    capacity: usize,

    /// The statistics of the database.
    stats: NogoodStats,
}

impl Default for NogoodDb {
    fn default() -> Self {
        Self::with_default_capacity()
    }
}

impl NogoodDb {
    /// Create an empty database with the given capacity.
    ///
    /// A capacity of zero creates a disabled database that never learns or
    /// blocks anything; this is used when the nogood feature is off.
    pub fn new(capacity: usize) -> Self {
        Self {
            entries: Vec::new(),
            remaining: Vec::new(),
            uses: Vec::new(),
            learned_epoch: Vec::new(),
            last_used_epoch: Vec::new(),
            recently_evicted: FxHashSet::default(),
            hall_of_fame: Vec::new(),
            index: LiteralMap::default(),
            hashes: LiteralSet::default(),
            capacity,
            stats: NogoodStats::default(),
        }
    }

    /// Create an empty enabled database with the default capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    /// Create an empty enabled database with a capacity adapted to the
    /// number of cells in the search world.
    ///
    /// The capacity is `max(2048, 4 * world_size)`. On a large world, a fixed
    /// small database only remembers the recent history of a long search, and
    /// the same patterns are learned and evicted over and over without being
    /// reused; scaling the capacity with the world keeps the average index
    /// bucket roughly constant while letting learned patterns live long
    /// enough to be useful.
    pub fn with_world_size(world_size: usize) -> Self {
        Self::new(DEFAULT_CAPACITY.max(ADAPTIVE_CAPACITY_FACTOR.saturating_mul(world_size)))
    }

    /// Whether the database accepts new entries.
    #[inline]
    pub const fn is_enabled(&self) -> bool {
        self.capacity > 0
    }

    /// Learn a nogood.
    ///
    /// The literals are (cell index, state) pairs. The literals are stored
    /// sorted. Entries with a repeated cell are rejected (they could never
    /// hold together, and would be subsumed by a smaller nogood), and so are
    /// entries that are already stored verbatim.
    ///
    /// The `lbd` argument is the LBD analogue of the learned clause: the
    /// number of distinct decision levels of its literals plus one for the
    /// 1-UIP cell. It is diagnostic state for now; the database records it
    /// and never uses it for propagation or eviction.
    ///
    /// The `state_of` callback reports the current state of a cell, so that
    /// the unmatched-literal counters of the new entry (and, after an
    /// eviction, of all the kept entries) start in sync with the world.
    pub fn learn<F>(&mut self, mut literals: Box<[(u32, CellState)]>, lbd: u8, state_of: &mut F)
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        if !self.is_enabled() {
            return;
        }

        if literals.len() > MAX_NOGOOD_LITERALS {
            self.stats.rejected_long += 1;
            return;
        }

        if literals.is_empty() {
            return;
        }

        // The LBD can never exceed the number of literals, because the
        // distinct decision levels are a subset of them.
        debug_assert!(lbd as usize <= literals.len());

        // Canonicalize the literals, so that a duplicate stored entry has the
        // same hash. A cell appearing twice means that the nogood can never
        // hold (two states of a cell cannot both hold at once), so it is
        // useless.
        literals.sort_unstable();
        if literals.windows(2).any(|window| window[0].0 == window[1].0) {
            self.stats.rejected_long += 1;
            return;
        }

        let hash = literals_hash(&literals);
        if !self.hashes.insert(hash) {
            return;
        }

        // The pattern was learned before and its entry has since been
        // evicted: the search had to rediscover knowledge that the database
        // used to hold.
        if self.recently_evicted.remove(&hash) {
            self.stats.relearned_evicted += 1;
        }

        self.stats.learned += 1;
        self.stats.literals_total += literals.len() as u64;
        self.stats.length_histogram[literals.len()] += 1;
        self.stats.lbd_histogram[lbd as usize] += 1;

        let id = self.entries.len() as u32;
        for &(cell, state) in literals.iter() {
            self.index
                .entry(literal_key(cell, state))
                .or_default()
                .push(id);
        }

        let matched = literals
            .iter()
            .filter(|&&(cell, state)| state_of(cell) == Some(state))
            .count() as u32;
        // Usually an entry is learned while its anchor cell is unset or
        // rejected, so it is at most one literal short of a full match.
        // An entry learned by the chronological fallback of the conflict
        // analysis may start fully matched: the analysis learned it while
        // the 1-UIP cell still held its rejected state, and the
        // backtracking that follows unsets that cell immediately, bringing
        // the counter back in sync. In the meantime the current partial
        // assignment really is contradictory, which is what a full match
        // means.
        debug_assert!(matched <= literals.len() as u32);
        if matched as usize == literals.len() {
            self.stats.learned_ready += 1;
        }

        self.remaining.push(literals.len() as u32 - matched);
        self.uses.push(0);
        self.learned_epoch.push(self.stats.reductions as u32);
        self.last_used_epoch.push(self.stats.reductions as u32);
        self.entries.push(Nogood { lbd, literals });

        if self.entries.len() >= self.capacity {
            self.reduce(state_of);
        }
    }

    /// Evict entries to make room and rebuild the index.
    ///
    /// The entries are ranked from least to most useful and the worst
    /// `entries.len() / 2` are evicted:
    ///
    /// 1. the least recently used entry is evicted first, where "recently
    ///    used" is measured in database reductions and a fresh entry counts
    ///    as used at its learning epoch;
    /// 2. ties are broken by fewer uses first; and
    /// 3. the remaining ties evict the older entry first.
    ///
    /// This is the Kissat `used` semantics adapted to the CA database: an
    /// entry that fired during the current reduction interval survives, an
    /// entry that has not fired since is evicted, and a fresh entry gets one
    /// full interval to prove itself, the same grace period the oldest-half
    /// policy gave to the newest half. The difference is that the space is
    /// spent on the entries that actually propagate instead of on insertion
    /// order.
    ///
    /// The unmatched-literal counters are rebuilt from the world state via
    /// the `state_of` callback, since all the ids shift.
    fn reduce<F>(&mut self, state_of: &mut F)
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        let count = self.entries.len();
        let evict_count = count / 2;
        self.stats.evicted += evict_count as u64;
        self.stats.reductions += 1;

        // Select the worst entries. The key is a total order because the id
        // is unique, so the selected set is deterministic.
        let mut order = (0..count as u32).collect::<Vec<_>>();
        order.select_nth_unstable_by_key(evict_count, |&id| {
            let id = id as usize;
            (self.last_used_epoch[id], self.uses[id], id as u32)
        });
        let mut evict = vec![false; count];
        for &id in &order[..evict_count] {
            let id = id as usize;
            evict[id] = true;
            if self.uses[id] == 0 {
                self.stats.evicted_unused += 1;
            }
            self.stats.evicted_uses_total += u64::from(self.uses[id]);
        }

        // Move the kept entries into fresh parallel arrays, preserving their
        // insertion order, and rebuild the index from scratch: the ids in the
        // index are positions in `entries`, so all of them shift when entries
        // are evicted.
        let entries = std::mem::take(&mut self.entries);
        let uses = std::mem::take(&mut self.uses);
        let learned_epoch = std::mem::take(&mut self.learned_epoch);
        let last_used_epoch = std::mem::take(&mut self.last_used_epoch);
        let keep = count - evict_count;
        let mut new_entries = Vec::with_capacity(keep);
        let mut new_remaining = Vec::with_capacity(keep);
        let mut new_uses = Vec::with_capacity(keep);
        let mut new_learned_epoch = Vec::with_capacity(keep);
        let mut new_last_used_epoch = Vec::with_capacity(keep);
        self.index.clear();
        self.hashes.clear();

        for (id, entry) in entries.into_iter().enumerate() {
            if evict[id] {
                self.recently_evicted.insert(literals_hash(&entry.literals));
                self.remember_hall(uses[id], entry);
                continue;
            }

            let new_id = new_entries.len() as u32;
            self.hashes.insert(literals_hash(&entry.literals));
            let matched = entry
                .literals
                .iter()
                .filter(|&&(cell, state)| state_of(cell) == Some(state))
                .count() as u32;
            new_remaining.push(entry.literals.len() as u32 - matched);
            new_uses.push(uses[id]);
            new_learned_epoch.push(learned_epoch[id]);
            new_last_used_epoch.push(last_used_epoch[id]);
            for &(cell, state) in entry.literals.iter() {
                self.index
                    .entry(literal_key(cell, state))
                    .or_default()
                    .push(new_id);
            }
            new_entries.push(entry);
        }

        // Remember the evicted patterns for the relearning counter. This is
        // diagnostic; drop the memory when it reaches its bound, which makes
        // the relearning counter a lower bound.
        let memory_bound = EVICTED_MEMORY_FACTOR
            .saturating_mul(self.capacity)
            .min(EVICTED_MEMORY_LIMIT);
        if self.recently_evicted.len() > memory_bound {
            self.recently_evicted.clear();
            self.stats.evicted_memory_flushes += 1;
        }

        self.entries = new_entries;
        self.remaining = new_remaining;
        self.uses = new_uses;
        self.learned_epoch = new_learned_epoch;
        self.last_used_epoch = new_last_used_epoch;
    }

    /// Update the counters when a cell is set to a state.
    ///
    /// The ids of the entries that just reached "one literal short of a full
    /// match" are pushed to `candidates` for the caller to evaluate with
    /// [`fire_candidate`](NogoodDb::fire_candidate). If *all* the literals of
    /// an entry hold after this update, the current partial assignment
    /// contains a forbidden pattern; the id of one such entry is returned,
    /// and the caller must treat it as a conflict. This can happen even
    /// though every transition was checked: a cell set to the wrong state can
    /// be unset later and re-set to the recorded state, skipping the
    /// one-literal-short window.
    ///
    /// The ids are read from the index while only the `remaining` field is
    /// mutated, which is sound because the two fields never alias.
    pub fn on_set(&mut self, cell: u32, state: CellState, out: &mut Vec<u32>) -> Option<u32> {
        let mut full_match = None;

        if let Some(ids) = self.index.get(&literal_key(cell, state)) {
            for &id in ids.iter() {
                let remaining = &mut self.remaining[id as usize];
                debug_assert!(*remaining > 0);
                *remaining -= 1;
                let remaining = *remaining;
                if remaining == 0 {
                    full_match = Some(id);
                    self.stats.full_matches += 1;
                } else if remaining == 1 {
                    out.push(id);
                }
            }
        }

        // The borrow of the index ends with the loop above, so the matched
        // entry can be noted as used.
        if let Some(id) = full_match {
            self.note_entry_used(id);
        }

        full_match
    }

    /// Update the counters when a cell is unset from a state.
    pub fn on_unset(&mut self, cell: u32, state: CellState) {
        if let Some(ids) = self.index.get(&literal_key(cell, state)) {
            for &id in ids.iter() {
                let remaining = &mut self.remaining[id as usize];
                debug_assert!(*remaining < self.entries[id as usize].literals.len() as u32);
                *remaining += 1;
            }
        }
    }

    /// Evaluate whether an entry fires, and return the cell to force and the
    /// state it is blocked from taking.
    ///
    /// The entry fires when exactly one literal does not hold and its cell is
    /// unknown. Candidates are re-evaluated when they are processed, not when
    /// they were queued, so a stale candidate simply returns [`None`]. Every
    /// call is counted in
    /// [`fire_attempts`](NogoodStats::fire_attempts), whether or not it fires.
    pub fn fire_candidate<F>(&mut self, id: u32, state_of: &mut F) -> Option<(u32, CellState)>
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        self.stats.fire_attempts += 1;

        if *self.remaining.get(id as usize)? != 1 {
            return None;
        }

        let entry = self.entries.get(id as usize)?;

        let mut target = None;

        for &(cell, state) in entry.literals.iter() {
            match state_of(cell) {
                Some(current) if current == state => {}
                // The first unknown cell can be forced away from its
                // recorded state...
                None if target.is_none() => target = Some((cell, state)),
                // ...but a second unknown cell, or a cell that is known to
                // have a different state, means that this nogood cannot fire.
                _ => return None,
            }
        }

        target
    }

    /// The literals of an entry, for seeding the conflict analysis of a
    /// full-match conflict.
    #[inline]
    pub(crate) fn entry_literals(&self, id: u32) -> &[(u32, CellState)] {
        &self.entries[id as usize].literals
    }

    /// Whether the entry is still fully matched by the current assignment.
    ///
    /// A queued full-match flag can go stale when the search unwinds before
    /// the flag is consumed (for example, when the queue empties right after
    /// the match and the step ends with a direct backtrack), so the flag must
    /// be re-validated before it is turned into a conflict.
    pub(crate) fn is_full_match<F>(&self, id: u32, state_of: &mut F) -> bool
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        self.entries.get(id as usize).is_some_and(|entry| {
            entry
                .literals
                .iter()
                .all(|&(cell, state)| state_of(cell) == Some(state))
        })
    }

    /// Check whether guessing `state` for the cell with the given index would
    /// complete a learned nogood.
    ///
    /// A nogood is completed when all of its literals hold: the queried cell
    /// takes the queried state, and every other cell currently has exactly
    /// the recorded state, as determined by `state_of`.
    ///
    /// Return `true` if such a nogood exists, meaning that the guess cannot
    /// lead to a solution and should be replaced or backtracked from.
    pub fn blocks<F>(&mut self, cell: u32, state: CellState, mut state_of: F) -> bool
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        if !self.is_enabled() {
            return false;
        }

        let hit = self.completed(cell, state, &mut state_of).is_some();

        if hit {
            self.stats.hits += 1;
        }

        hit
    }

    /// Find the first learned nogood that would be completed by assigning
    /// `state` to the cell with the given index, and return its remaining
    /// literals: the pairs of all the *other* cells and the states they must
    /// currently have.
    ///
    /// See [`blocks`](NogoodDb::blocks) for the meaning of completion. This
    /// is the read-only part of the query; it does not update the statistics.
    ///
    /// The candidates are checked without building anything; the literal
    /// vector is allocated only for the matching entry, since a popular
    /// anchor cell may share its index bucket with many nogoods.
    ///
    /// The query statistics are updated here: a query is counted when its
    /// index bucket is non-empty, and separately when the bucket is larger
    /// than the candidate cap.
    pub(crate) fn completed<F>(
        &mut self,
        cell: u32,
        state: CellState,
        state_of: &mut F,
    ) -> Option<Box<[(u32, CellState)]>>
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        let ids = self.index.get(&literal_key(cell, state))?;

        self.stats.queries += 1;
        if ids.len() > MAX_QUERY_CANDIDATES {
            self.stats.capped_queries += 1;
        }

        let mut found = None;
        for &id in ids.iter().take(MAX_QUERY_CANDIDATES) {
            let entry = &self.entries[id as usize];

            let complete = entry.literals.iter().all(|&(c, s)| {
                (c == cell && s == state) || state_of(c).is_some_and(|current| current == s)
            });

            if complete {
                let literals = entry
                    .literals
                    .iter()
                    .copied()
                    .filter(|&(c, s)| c != cell || s != state)
                    .collect();
                found = Some((id, literals));
                break;
            }
        }

        // The borrow of the index ends with the loop above, so the entry can
        // be noted as used.
        let (id, literals) = found?;
        self.note_entry_used(id);
        Some(literals)
    }

    /// Record that a guess was blocked by a nogood.
    pub(crate) const fn note_hit(&mut self) {
        self.stats.hits += 1;
    }

    /// Record that the nogood with the given id fired during propagation.
    pub(crate) fn note_fired(&mut self, id: u32) {
        self.stats.fired += 1;
        self.note_entry_used(id);
    }

    /// Record that the entry with the given id was used.
    ///
    /// An entry is used when it fires during propagation, blocks a guess or a
    /// chronological flip, or is fully matched by a cell assignment. This
    /// updates the cumulative use count, the first-use diagnostics, and the
    /// last-use epoch; it is the single place that maintains them.
    ///
    /// The caller must hold a valid id: a reduction never runs in the middle
    /// of a use, so the parallel arrays stay in sync.
    fn note_entry_used(&mut self, id: u32) {
        let id = id as usize;
        debug_assert!(id < self.entries.len());

        if self.uses[id] == 0 {
            let length = self.entries[id].literals.len();
            let lbd = self.entries[id].lbd as usize;
            let distance = (self.stats.reductions as u32).wrapping_sub(self.learned_epoch[id]);
            self.stats.used_learned += 1;
            self.stats.used_length_histogram[length] += 1;
            self.stats.used_lbd_histogram[lbd] += 1;
            self.stats.reuse_distance_histogram[reuse_bin(distance)] += 1;
        }

        self.uses[id] = self.uses[id].saturating_add(1);
        self.last_used_epoch[id] = self.stats.reductions as u32;
    }

    /// The number of currently stored entries that have been used at least
    /// once.
    #[inline]
    pub fn used_entries(&self) -> usize {
        self.uses.iter().filter(|&&uses| uses > 0).count()
    }

    /// Remember an evicted entry in the all-time usage record.
    ///
    /// The entry is inserted into [`hall_of_fame`](NogoodDb::hall_of_fame) in
    /// descending use order and the list is truncated to [`HALL_OF_FAME`].
    /// Unused entries are skipped, since they can never make the list.
    fn remember_hall(&mut self, uses: u32, entry: Nogood) {
        if uses == 0 || HALL_OF_FAME == 0 {
            return;
        }
        let position = self
            .hall_of_fame
            .iter()
            .position(|&(hall_uses, _)| hall_uses < uses)
            .unwrap_or(self.hall_of_fame.len());
        if position >= HALL_OF_FAME {
            return;
        }
        self.hall_of_fame.insert(position, (uses, entry));
        self.hall_of_fame.truncate(HALL_OF_FAME);
    }

    /// The `n` entries with the highest all-time use count, ordered by
    /// descending use count, together with their use counts.
    ///
    /// This merges the live entries with the evicted entries remembered in
    /// the all-time record (see [`HALL_OF_FAME`]), so it is not limited to
    /// the current database. Ties keep the live entries first, in insertion
    /// order. Entries that have never been used are not returned.
    pub fn top_entries(&self, n: usize) -> Vec<(u32, &[(u32, CellState)])> {
        let mut top = (0..self.entries.len() as u32)
            .filter(|&id| self.uses[id as usize] > 0)
            .map(|id| {
                (
                    self.uses[id as usize],
                    &self.entries[id as usize].literals[..],
                )
            })
            .collect::<Vec<_>>();
        top.extend(
            self.hall_of_fame
                .iter()
                .map(|(uses, entry)| (*uses, &entry.literals[..])),
        );
        top.sort_by_key(|&(uses, _)| std::cmp::Reverse(uses));
        top.truncate(n);
        top
    }

    /// Clear the database, keeping the statistics.
    ///
    /// This is called when the world is rebuilt: the nogoods of an old world
    /// may rely on facts that do not hold anymore (e.g. the background state
    /// forced outside the search range).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.remaining.clear();
        self.uses.clear();
        self.learned_epoch.clear();
        self.last_used_epoch.clear();
        self.recently_evicted.clear();
        self.hall_of_fame.clear();
        self.index.clear();
        self.hashes.clear();
    }

    /// The number of stored nogoods.
    #[inline]
    pub const fn len(&self) -> usize {
        self.entries.len()
    }

    /// The maximal number of entries before the database is reduced.
    #[inline]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Whether the database stores no nogoods.
    #[inline]
    pub const fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The statistics of the database.
    #[inline]
    pub const fn stats(&self) -> &NogoodStats {
        &self.stats
    }
}

#[cfg(test)]
mod test {
    use super::*;

    const D: CellState = CellState::Dead;
    const A: CellState = CellState::Alive;

    /// Learn an entry with an all-unknown world state and a length-based LBD.
    ///
    /// The tests that exercise the eviction and use statistics pass their own
    /// `state_of` through [`NogoodDb::learn`] when they need one.
    fn learn(db: &mut NogoodDb, literals: &[(u32, CellState)]) {
        let mut none = |_| None;
        db.learn(
            literals.to_vec().into_boxed_slice(),
            literals.len() as u8,
            &mut none,
        );
    }

    fn db_with_entries(entries: &[&[(u32, CellState)]]) -> NogoodDb {
        let mut db = NogoodDb::with_default_capacity();
        for entry in entries {
            learn(&mut db, entry);
        }
        db
    }

    #[test]
    fn blocks_when_all_literals_hold() {
        let mut db = db_with_entries(&[&[(10, D), (11, A), (12, D)]]);

        // Only the queried cell is missing.
        assert!(db.blocks(12, D, |c| match c {
            10 => Some(D),
            11 => Some(A),
            _ => None,
        }));

        // One other literal disagrees.
        assert!(!db.blocks(12, D, |c| match c {
            10 => Some(D),
            11 => Some(D),
            _ => None,
        }));
    }

    #[test]
    fn blocks_requires_exact_state() {
        let mut db = db_with_entries(&[&[(10, D), (11, A)]]);
        assert!(!db.blocks(11, D, |_| None));
        assert!(db.blocks(11, A, |c| (c == 10).then_some(D)));
    }

    #[test]
    fn learn_stores_and_dedupes_identical_entries() {
        let mut db = NogoodDb::with_default_capacity();
        learn(&mut db, &[(1, D), (2, A)]);
        assert_eq!(db.len(), 1);
        learn(&mut db, &[(1, D), (2, A)]);
        assert_eq!(db.len(), 1);
        assert_eq!(db.stats().learned, 1);
    }

    #[test]
    fn learn_rejects_a_repeated_cell() {
        let mut db = NogoodDb::with_default_capacity();
        // An exact duplicate and two different states of the same cell can
        // never hold together, so both entries are useless.
        learn(&mut db, &[(1, D), (1, D)]);
        learn(&mut db, &[(1, A), (1, D)]);
        assert!(db.is_empty());
        assert_eq!(db.stats().rejected_long, 2);
    }

    #[test]
    fn learn_stores_the_literals_sorted() {
        let mut db = NogoodDb::with_default_capacity();
        learn(&mut db, &[(9, A), (1, D), (5, A)]);
        assert_eq!(db.entry_literals(0), &[(1, D), (5, A), (9, A)]);
    }

    #[test]
    fn reduce_evicts_the_least_recently_used_and_rebuilds_the_index() {
        let mut db = NogoodDb::new(4);
        for c in 0..6u32 {
            learn(&mut db, &[(c, D), (c + 100, A)]);
        }
        assert_eq!(db.len(), 2);
        assert_eq!(db.stats().evicted, 4);
        assert_eq!(db.stats().reductions, 2);

        // No entry was ever used, so the oldest ids are evicted first and
        // the newest two entries are the survivors.
        assert!(!db.blocks(0, D, |_| Some(A)));
        assert!(!db.blocks(3, D, |_| Some(A)));

        // The kept entries still block when their other literal holds.
        assert!(db.blocks(4, D, |c| (c == 104).then_some(A)));
        assert!(!db.blocks(4, D, |_| None));
        assert!(db.blocks(5, D, |c| (c == 105).then_some(A)));
    }

    #[test]
    fn disabled_db_never_learns_or_blocks() {
        let mut db = NogoodDb::new(0);
        assert!(!db.is_enabled());
        learn(&mut db, &[(1, D)]);
        assert!(db.is_empty());
        assert!(!db.blocks(1, D, |_| None));
    }

    #[test]
    fn world_size_scales_the_capacity() {
        // Small worlds keep the minimal capacity.
        assert_eq!(NogoodDb::with_world_size(0).capacity(), DEFAULT_CAPACITY);
        assert_eq!(NogoodDb::with_world_size(512).capacity(), DEFAULT_CAPACITY);
        // Larger worlds scale it with the number of cells.
        assert_eq!(
            NogoodDb::with_world_size(1000).capacity(),
            ADAPTIVE_CAPACITY_FACTOR * 1000
        );
    }

    #[test]
    fn stats_track_lengths_and_uses() {
        let mut db = NogoodDb::with_default_capacity();
        learn(&mut db, &[(1, D), (2, A)]);
        learn(&mut db, &[(3, D), (4, A), (5, D)]);

        assert_eq!(db.stats().length_histogram[2], 1);
        assert_eq!(db.stats().length_histogram[3], 1);
        assert_eq!(db.stats().used_learned, 0);
        assert_eq!(db.used_entries(), 0);

        // A completion query uses the first entry.
        assert!(db.blocks(2, A, |c| (c == 1).then_some(D)));
        assert_eq!(db.stats().used_learned, 1);
        assert_eq!(db.used_entries(), 1);

        // Firing the second entry uses it too.
        let mut candidates = Vec::new();
        db.on_set(3, D, &mut candidates);
        db.on_set(5, D, &mut candidates);
        let mut state_of = |c: u32| match c {
            3 => Some(D),
            5 => Some(D),
            _ => None,
        };
        for id in candidates {
            if db.fire_candidate(id, &mut state_of).is_some() {
                db.note_fired(id);
            }
        }
        assert_eq!(db.stats().fired, 1);
        assert_eq!(db.stats().used_learned, 2);
        assert_eq!(db.used_entries(), 2);

        // Both entries were used once; ties keep insertion order.
        let top = db.top_entries(2);
        assert_eq!(top.len(), 2);
        assert_eq!(top[0].0, 1);
        assert_eq!(top[0].1, &[(1, D), (2, A)]);
        assert_eq!(top[1].0, 1);
        assert_eq!(top[1].1, &[(3, D), (4, A), (5, D)]);
    }

    #[test]
    fn top_entries_remembers_evicted_entries() {
        let mut db = NogoodDb::new(4);
        learn(&mut db, &[(0, D), (100, A)]);

        // Use the first entry five times.
        for _ in 0..5 {
            let mut candidates = Vec::new();
            db.on_set(100, A, &mut candidates);
            for id in candidates {
                let mut state_of = |c: u32| (c == 100).then_some(A);
                if db.fire_candidate(id, &mut state_of).is_some() {
                    db.note_fired(id);
                }
            }
            db.on_unset(100, A);
        }

        // Force reductions that evict the used entry.
        for c in 1..12u32 {
            learn(&mut db, &[(c, D), (c + 100, A)]);
        }
        assert!(db.stats().reductions > 0);

        // The evicted entry is still the all-time most used.
        let top = db.top_entries(1);
        assert_eq!(top.len(), 1);
        assert_eq!(top[0].0, 5);
        assert_eq!(top[0].1, &[(0, D), (100, A)]);
    }

    #[test]
    fn full_match_counts_as_a_use() {
        let mut db = db_with_entries(&[&[(0, D), (1, A), (2, D)]]);
        let mut candidates = Vec::new();
        db.on_set(0, D, &mut candidates);
        db.on_set(2, D, &mut candidates);
        assert_eq!(db.stats().full_matches, 0);

        assert_eq!(db.on_set(1, A, &mut candidates), Some(0));
        assert_eq!(db.stats().full_matches, 1);
        assert_eq!(db.stats().used_learned, 1);
    }

    #[test]
    fn clear_empties_but_keeps_stats() {
        let mut db = db_with_entries(&[&[(1, D), (2, A)]]);
        let learned = db.stats().learned;
        db.clear();
        assert!(db.is_empty());
        assert_eq!(db.stats().learned, learned);
    }

    /// A world state for the counter tests: cell `i` is known when bit `i`
    /// of `known` is set, and then it is dead or alive according to bit `i`
    /// of `alive`. Unknown cells have their bits clear in `known`.
    #[allow(clippy::needless_borrows_for_generic_args)]
    fn state_of_fn(alive: u64, known: u64) -> impl Fn(u32) -> Option<CellState> {
        move |cell: u32| {
            if cell < 64 && known & (1 << cell) != 0 {
                Some(if alive & (1 << cell) != 0 { A } else { D })
            } else {
                None
            }
        }
    }

    #[test]
    fn on_set_counts_matched_literals_and_reports_full_match() {
        let mut db = db_with_entries(&[&[(0, D), (1, A), (2, D)]]);
        let mut candidates = Vec::new();

        // Cell 1 is unknown; cells 0 and 2 hold their recorded states.
        // Setting cell 2 makes the entry one literal short of a match.
        assert_eq!(db.on_set(0, D, &mut candidates), None);
        assert!(candidates.is_empty());
        assert_eq!(db.on_set(2, D, &mut candidates), None);
        assert_eq!(candidates.len(), 1);

        // Completing the match is a full match, reported as a conflict.
        assert_eq!(
            db.on_set(1, A, &mut candidates),
            Some(0),
            "the id of the fully matched entry"
        );
    }

    #[test]
    fn on_set_reports_one_short_entries_as_candidates() {
        let mut db = db_with_entries(&[&[(0, D), (1, A), (2, D)]]);
        let mut candidates = Vec::new();

        // Cells 0 and 2 hold; the entry is one literal short.
        assert_eq!(db.on_set(0, D, &mut candidates), None);
        assert_eq!(db.on_set(2, D, &mut candidates), None);

        // Setting cell 1 to its recorded state would be a full match; this
        // transition is caught by the full-match check above. Instead, unset
        // cell 2 and re-set it to check that the counters track the state.
        db.on_unset(2, D);
        assert_eq!(db.on_set(2, D, &mut candidates), None);
    }

    #[test]
    fn fire_candidate_forces_the_unknown_cell() {
        // Cell 3 does not exist in the state bitmap, so it is unknown.
        let mut db = db_with_entries(&[&[(0, D), (1, A), (3, A)]]);

        let mut candidates = Vec::new();
        assert_eq!(db.on_set(0, D, &mut candidates), None);

        assert!(
            !candidates.iter().any(|&id| {
                db.fire_candidate(id, &mut state_of_fn(0b00, 0b01))
                    .is_some()
            }),
            "not yet one literal short"
        );

        db.on_set(1, A, &mut candidates);
        let fired: Vec<_> = candidates
            .iter()
            .filter_map(|&id| db.fire_candidate(id, &mut state_of_fn(0b10, 0b11)))
            .collect();
        assert_eq!(fired.len(), 1);
        assert_eq!(fired[0], (3, A));
    }

    #[test]
    fn fire_candidate_rejects_known_wrong_cells_and_stale_ids() {
        // Cell 2 is set to the wrong state (alive instead of dead).
        let mut db = db_with_entries(&[&[(0, D), (1, A), (2, D)]]);
        let mut candidates = Vec::new();

        db.on_set(0, D, &mut candidates);
        db.on_set(1, A, &mut candidates);

        for &id in candidates.iter() {
            assert!(
                db.fire_candidate(id, &mut state_of_fn(0b010, 0b111))
                    .is_none(),
                "a known wrong-state cell prevents firing"
            );
        }

        // An out-of-range id is stale and evaluates to nothing.
        let mut so = state_of_fn(0, 0);
        assert!(db.fire_candidate(9999, &mut so).is_none());
    }

    #[test]
    fn stats_track_lbd_and_first_use_histograms() {
        let mut db = NogoodDb::with_default_capacity();
        learn(&mut db, &[(1, D), (2, A)]);

        assert_eq!(db.stats().lbd_histogram[2], 1);
        assert_eq!(db.stats().used_lbd_histogram[2], 0);
        assert_eq!(db.stats().used_length_histogram[2], 0);

        assert!(db.blocks(2, A, |c| (c == 1).then_some(D)));
        assert_eq!(db.stats().used_lbd_histogram[2], 1);
        assert_eq!(db.stats().used_length_histogram[2], 1);

        // A second use does not count twice.
        assert!(db.blocks(2, A, |c| (c == 1).then_some(D)));
        assert_eq!(db.stats().used_lbd_histogram[2], 1);
    }

    #[test]
    fn reuse_distance_counts_reductions_until_first_use() {
        let mut db = NogoodDb::new(4);
        // Three entries, then the target, then a fourth to trigger a
        // reduction that keeps the two newest entries: the target survives
        // its learning epoch.
        learn(&mut db, &[(1, D), (101, A)]);
        learn(&mut db, &[(2, D), (102, A)]);
        learn(&mut db, &[(0, D), (100, A)]);
        learn(&mut db, &[(3, D), (103, A)]);
        assert_eq!(db.stats().reductions, 1);

        // The first use of the target happens one reduction after learning.
        assert!(db.blocks(100, A, |c| (c == 0).then_some(D)));
        assert_eq!(db.stats().reuse_distance_histogram[1], 1);
        assert_eq!(db.stats().reuse_distance_histogram[0], 0);
    }

    #[test]
    fn reduce_records_evicted_use_counts() {
        let mut db = NogoodDb::new(4);
        learn(&mut db, &[(0, D), (100, A)]);
        assert!(db.blocks(100, A, |c| (c == 0).then_some(D)));

        // The first reduction evicts the never-used entries ...
        for c in 1..4u32 {
            learn(&mut db, &[(c, D), (c + 100, A)]);
        }
        assert_eq!(db.stats().evicted_unused, 2);
        assert_eq!(db.stats().evicted_uses_total, 0);

        // ... and the second one evicts the used entry once it is the least
        // recently used, together with the remaining old unused entry.
        for c in 4..6u32 {
            learn(&mut db, &[(c, D), (c + 100, A)]);
        }
        assert_eq!(db.stats().reductions, 2);
        assert_eq!(db.stats().evicted_unused, 3);
        assert_eq!(db.stats().evicted_uses_total, 1);
    }

    #[test]
    fn reduce_evicts_a_stale_used_entry_before_fresh_entries() {
        let mut db = NogoodDb::new(4);
        learn(&mut db, &[(0, D), (100, A)]);
        assert!(db.blocks(100, A, |c| (c == 0).then_some(D)));
        for c in 1..4u32 {
            learn(&mut db, &[(c, D), (c + 100, A)]);
        }
        // The used entry survives the first reduction: within the same
        // epoch it outranks the never-used entries. That query was its only
        // use, so it is stale in the next interval.
        assert_eq!(db.used_entries(), 1);

        // In the next interval the fresh entries rank above the stale used
        // entry, which is evicted with the other old entry.
        for c in 4..6u32 {
            learn(&mut db, &[(c, D), (c + 100, A)]);
        }
        assert!(!db.blocks(100, A, |c| (c == 0).then_some(D)));
        assert!(db.blocks(104, A, |c| (c == 4).then_some(D)));
    }

    #[test]
    fn relearning_an_evicted_entry_is_counted() {
        let mut db = NogoodDb::new(4);
        learn(&mut db, &[(0, D), (100, A)]);
        for c in 1..4u32 {
            learn(&mut db, &[(c, D), (c + 100, A)]);
        }
        assert_eq!(db.stats().relearned_evicted, 0);

        // The first entry was evicted by the reduction; learning the same
        // literal set again is a relearning.
        learn(&mut db, &[(0, D), (100, A)]);
        assert_eq!(db.stats().relearned_evicted, 1);
        assert_eq!(db.stats().learned, 5);
    }

    #[test]
    fn learned_ready_counts_fully_matched_entries() {
        let mut db = NogoodDb::with_default_capacity();
        let mut state_of = |c: u32| Some(if c == 1 { D } else { A });
        db.learn(vec![(1, D), (2, A)].into_boxed_slice(), 2, &mut state_of);

        // Both literals already hold, so the entry starts fully matched.
        assert_eq!(db.stats().learned_ready, 1);
        assert_eq!(db.remaining[0], 0);
    }

    #[test]
    fn fire_attempts_counts_evaluations() {
        let mut db = db_with_entries(&[&[(0, D), (1, A), (3, A)]]);
        let mut candidates = Vec::new();
        db.on_set(0, D, &mut candidates);
        db.on_set(1, A, &mut candidates);

        let before = db.stats().fire_attempts;
        for &id in candidates.iter() {
            let mut state_of = state_of_fn(0b10, 0b11);
            db.fire_candidate(id, &mut state_of);
        }
        let attempted = db.stats().fire_attempts - before;
        assert_eq!(attempted, candidates.len() as u64);
        assert_eq!(db.stats().fired, 0, "firing is noted by the caller");
    }
}
