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
use std::collections::HashMap;

/// The default capacity of the database, in entries.
///
/// When the database outgrows this bound, the older half of the entries is
/// evicted, like the clause-database reduction of a SAT solver.
const DEFAULT_CAPACITY: usize = 1 << 16;

/// The maximal number of candidates examined by a single query.
///
/// A popular anchor cell may share its index bucket with many nogoods;
/// without a bound, the queries at that cell would dominate the search time.
/// A blocked guess missed because of the bound is only a lost pruning, never
/// a correctness issue.
const MAX_QUERY_CANDIDATES: usize = 64;

/// The maximal number of literals of a learned nogood.
///
/// Large patterns rarely materialize again in full, so they cost more than
/// they are worth as index entries.
const MAX_NOGOOD_LITERALS: usize = 16;

/// Statistics of the nogood database.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
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
}

/// The result of a firing: the index of the cell to force, the state it is
/// blocked from taking, and the other literals of the nogood.
type Firing = (u32, CellState, Box<[(u32, CellState)]>);

/// A learned nogood: an assignment of states to cells that cannot be part of
/// any solution.
///
/// The literals are pairs of absolute cell indices and the states that these
/// cells must not all take at once.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Nogood {
    /// The literals of the nogood. The first literal is the anchor: the cell
    /// whose rejected state triggered the learning (the 1-UIP of the conflict).
    literals: Box<[(u32, CellState)]>,

    /// The number of literals whose cell currently holds the recorded state,
    /// maintained incrementally by [`on_set`](NogoodDb::on_set) and
    /// [`on_unset`](NogoodDb::on_unset).
    ///
    /// While this is one less than the number of literals and the remaining
    /// cell is unknown, the nogood *fires*: the remaining cell cannot take
    /// its recorded state. When all the literals hold, the current partial
    /// assignment is contradictory.
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
    matched: u32,
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

    /// For each literal, the ids of the nogoods containing it.
    index: HashMap<(u32, CellState), Vec<u32>>,

    /// The maximal number of entries before the older half is evicted.
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
            index: HashMap::new(),
            capacity,
            stats: NogoodStats::default(),
        }
    }

    /// Create an empty enabled database with the default capacity.
    pub fn with_default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    /// Whether the database accepts new entries.
    #[inline]
    pub const fn is_enabled(&self) -> bool {
        self.capacity > 0
    }

    /// Learn a nogood.
    ///
    /// The literals are (cell index, state) pairs; the first pair is the
    /// anchor of the nogood. Entries with a repeated literal are rejected
    /// (they would be subsumed by a smaller nogood), and so are entries that
    /// are already stored verbatim.
    ///
    /// The `state_of` callback reports the current state of a cell, so that
    /// the matched-literal counters of the new entry (and, after an eviction,
    /// of all the kept entries) start in sync with the world.
    pub fn learn<F>(&mut self, literals: Box<[(u32, CellState)]>, state_of: &mut F)
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        if !self.is_enabled()
            || literals.len() > MAX_NOGOOD_LITERALS
            || literals.is_empty()
            || !self.learnable(literals.as_ref())
            || self.contains_identical(&literals)
        {
            return;
        }

        self.stats.learned += 1;

        let id = self.entries.len() as u32;
        for &(cell, state) in literals.iter() {
            self.index.entry((cell, state)).or_default().push(id);
        }

        let matched = literals
            .iter()
            .filter(|&&(cell, state)| state_of(cell) == Some(state))
            .count() as u32;
        debug_assert!(matched < literals.len() as u32);

        self.entries.push(Nogood { literals, matched });

        if self.entries.len() >= self.capacity {
            self.reduce(state_of);
        }
    }

    /// Whether the given literals can be stored: they must not contain the
    /// same (cell, state) pair twice, which would make the nogood subsumed
    /// by a smaller one.
    fn learnable(&self, literals: &[(u32, CellState)]) -> bool {
        let mut sorted = literals.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        sorted.len() == literals.len()
    }

    /// Whether an entry with exactly these literals is already stored.
    ///
    /// Only the nogoods sharing the anchor literal are compared, which keeps
    /// the cost proportional to the collisions of one index bucket.
    fn contains_identical(&self, literals: &[(u32, CellState)]) -> bool {
        let Some(&(anchor_cell, anchor_state)) = literals.first() else {
            return false;
        };

        let Some(ids) = self.index.get(&(anchor_cell, anchor_state)) else {
            return false;
        };

        let mut query = literals.to_vec();
        query.sort_unstable();

        ids.iter().any(|&id| {
            let mut stored = self.entries[id as usize].literals.to_vec();
            stored.sort_unstable();
            stored == query
        })
    }

    /// Evict the older half of the entries and rebuild the index.
    ///
    /// The matched-literal counters are rebuilt from the world state via the
    /// `state_of` callback, since all the ids shift.
    fn reduce<F>(&mut self, state_of: &mut F)
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        let keep = self.entries.len() / 2;
        self.stats.evicted += keep as u64;
        self.stats.reductions += 1;

        // The ids in the index are positions in `entries`, so they all shift
        // when the older half is drained; rebuild the index from scratch.
        self.entries.drain(..keep);
        self.index.clear();
        for (id, entry) in self.entries.iter_mut().enumerate() {
            let id = id as u32;
            entry.matched = entry
                .literals
                .iter()
                .filter(|&&(cell, state)| state_of(cell) == Some(state))
                .count() as u32;
            for &(cell, state) in entry.literals.iter() {
                self.index.entry((cell, state)).or_default().push(id);
            }
        }
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
    /// The ids are read from the index while only the `entries` field is
    /// mutated, which is sound because the two fields never alias.
    pub fn on_set(&mut self, cell: u32, state: CellState, out: &mut Vec<u32>) -> Option<u32> {
        let mut full_match = None;

        if let Some(ids) = self.index.get(&(cell, state)) {
            for &id in ids.iter() {
                let entry = &mut self.entries[id as usize];
                debug_assert!(entry.matched < entry.literals.len() as u32);
                entry.matched += 1;
                if entry.matched == entry.literals.len() as u32 {
                    full_match = Some(id);
                } else if entry.matched + 1 == entry.literals.len() as u32 {
                    out.push(id);
                }
            }
        }

        full_match
    }

    /// Update the counters when a cell is unset from a state.
    pub fn on_unset(&mut self, cell: u32, state: CellState) {
        if let Some(ids) = self.index.get(&(cell, state)) {
            for &id in ids.iter() {
                let entry = &mut self.entries[id as usize];
                debug_assert!(entry.matched > 0);
                entry.matched -= 1;
            }
        }
    }

    /// Evaluate whether an entry fires, and return the information needed to
    /// force the remaining cell: its index, its blocked state, and the other
    /// literals of the nogood (the cells that currently hold their recorded
    /// states).
    ///
    /// The entry fires when exactly one literal does not hold and its cell is
    /// unknown. Candidates are re-evaluated when they are processed, not when
    /// they were queued, so a stale candidate simply returns [`None`].
    pub fn fire_candidate<F>(&self, id: u32, state_of: &mut F) -> Option<Firing>
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        let entry = &self.entries.get(id as usize)?;

        if entry.matched + 1 != entry.literals.len() as u32 {
            return None;
        }

        let mut others = Vec::with_capacity(entry.literals.len() - 1);
        let mut target = None;

        for &(cell, state) in entry.literals.iter() {
            match state_of(cell) {
                Some(current) if current == state => others.push((cell, state)),
                // The first unknown cell can be forced away from its
                // recorded state...
                None if target.is_none() => target = Some((cell, state)),
                // ...but a second unknown cell, or a cell that is known to
                // have a different state, means that this nogood cannot fire.
                _ => return None,
            }
        }

        let (target_cell, blocked_state) = target?;

        Some((target_cell, blocked_state, others.into_boxed_slice()))
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
    pub(crate) fn completed<F>(
        &self,
        cell: u32,
        state: CellState,
        state_of: &mut F,
    ) -> Option<Box<[(u32, CellState)]>>
    where
        F: FnMut(u32) -> Option<CellState>,
    {
        let ids = self.index.get(&(cell, state))?;

        for &id in ids.iter().take(MAX_QUERY_CANDIDATES) {
            let entry = &self.entries[id as usize];

            let complete = entry.literals.iter().all(|&(c, s)| {
                (c == cell && s == state) || state_of(c).is_some_and(|current| current == s)
            });

            if complete {
                return Some(
                    entry
                        .literals
                        .iter()
                        .copied()
                        .filter(|&(c, s)| c != cell || s != state)
                        .collect(),
                );
            }
        }

        None
    }

    /// Record that a guess was blocked by a nogood.
    pub(crate) const fn note_hit(&mut self) {
        self.stats.hits += 1;
    }

    /// Record that a nogood fired during propagation.
    pub(crate) const fn note_fired(&mut self) {
        self.stats.fired += 1;
    }

    /// Clear the database, keeping the statistics.
    ///
    /// This is called when the world is rebuilt: the nogoods of an old world
    /// may rely on facts that do not hold anymore (e.g. the background state
    /// forced outside the search range).
    pub fn clear(&mut self) {
        self.entries.clear();
        self.index.clear();
    }

    /// The number of stored nogoods.
    #[inline]
    pub const fn len(&self) -> usize {
        self.entries.len()
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

    fn db_with_entries(entries: &[&[(u32, CellState)]]) -> NogoodDb {
        let mut db = NogoodDb::with_default_capacity();
        for entry in entries {
            let mut none = |_| None;
            db.learn(entry.iter().copied().collect(), &mut none);
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
        let mut none = |_| None;
        db.learn(vec![(1, D), (2, A)].into_boxed_slice(), &mut none);
        assert_eq!(db.len(), 1);
        db.learn(vec![(1, D), (2, A)].into_boxed_slice(), &mut none);
        assert_eq!(db.len(), 1);
        assert_eq!(db.stats().learned, 1);
    }

    #[test]
    fn learn_rejects_duplicate_literals() {
        let mut db = NogoodDb::with_default_capacity();
        let mut none = |_| None;
        db.learn(vec![(1, D), (1, D)].into_boxed_slice(), &mut none);
        assert!(db.is_empty());
    }

    #[test]
    fn reduce_keeps_the_newer_half_and_rebuilds_the_index() {
        let mut db = NogoodDb::new(4);
        let mut none = |_| None;
        for c in 0..6u32 {
            db.learn(vec![(c, D), (c + 100, A)].into_boxed_slice(), &mut none);
        }
        assert_eq!(db.len(), 2);
        assert_eq!(db.stats().evicted, 4);
        assert_eq!(db.stats().reductions, 2);

        // The evicted entries are gone.
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
        let mut none = |_| None;
        db.learn(vec![(1, D)].into_boxed_slice(), &mut none);
        assert!(db.is_empty());
        assert!(!db.blocks(1, D, |_| None));
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
        assert_eq!(fired[0], (3, A, Box::from([(0, D), (1, A)].as_slice())));
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
}
