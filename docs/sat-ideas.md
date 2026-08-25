# SAT Solver Techniques for factoriosrc

> **Status**: This note is a collection of ideas and a preliminary analysis. The phase saving
> part of idea 3, idea 4 (lookahead, polarity selection only), the idea 1 foundation (recording
> the antecedents of deductions), the **full idea 1 (1-UIP conflict analysis and
> non-chronological backtracking)**, and **idea 2 up to its exact-position mode with
> propagation-level firing** have been implemented as opt-in experiments (see their sections
> below); the full idea 1 is correct (verified by a 16,200-configuration differential matrix)
> but **a loss on typical searches without idea 2** (the analysis re-treads closed branches
> without a persistent nogood database) — with the notable exception of very large searches
> like `B3/S23 64 64 1 -n a` where backjumping is a decisive win (see the idea 1 section).
> Idea 2 started as a guess-time check on an exact-position database and was then upgraded with
> counter-based unit propagation on the learned nogoods; the propagation-level firing is what
> actually recovers the re-treading loss (see the idea 2 section for the numbers): it turns
> some of the worst backjumping losses into wins and consistently shrinks enumeration work,
> though the plain chronological search still wins the typical medium instances.
> **Everything else in this note — the translated/cross-size mode of idea 2, the VSIDS-style
> activity part of idea 3, and the cell-selection variant of idea 4 — has not been
> implemented.** Every direction remains at the "worth trying" stage.

## Background and purpose

factoriosrc's core is a constraint satisfaction search over a three-dimensional (x, y, generation)
periodic cell grid. The README's todo list contains "Taking inspiration from SAT solvers (the CDCL
algorithm) and constraint programming". This note maps factoriosrc's current algorithm onto the
vocabulary of SAT solvers, points out the gaps, and lists some directions worth exploring. Each
direction only discusses the idea, the expected payoff, and the risks; it does not propose a
concrete implementation.

## Where we are: factoriosrc is already a DPLL solver

Overview of the search loop:

- `World::search` (`lib/src/search.rs:1285`) calls `step()` in a loop;
- `step()` (`lib/src/search.rs:841`) first calls `check_stack()` (propagation), then, if there is
  no conflict, `guess()` (branching);
- Propagation: `set_cell` (`lib/src/world.rs:942`) pushes the cell onto the `stack`; the part of
  the stack after `stack_index` forms a queue of cells to check (`lib/src/search.rs:526`).
  `check_affected` (`lib/src/search.rs:396`) checks the descriptors of the set cell itself, its
  neighbors, and its predecessor;
- Deduction: `RuleTable::implies` (`lib/src/rule.rs:677`) looks up a precomputed table by the
  descriptor and returns a `CheckResult` (`lib/src/rule.rs:376`), which may deduce the state of
  the successor, the current cell, all unknown neighbors (totalistic rules), or individual
  neighbors (non-totalistic rules, see the `forced` bits);
- Branching: `guess()` (`lib/src/search.rs:637`) walks the `next` chain (a fixed spatial order,
  built by `init_next`, `lib/src/world.rs:685`), picks the next unknown cell, and sets it to
  Alive, Dead, or a random state according to `Config::new_state`;
- Backtracking: `backtrack()` (`lib/src/search.rs:554`) pops the stack; when it reaches a
  `Reason::Guessed` cell, it flips the state (2-state rules) or tries the next state in the cycle
  (Generations rules, `Reason::TryAnother`);
- When a full assignment is found, `check_period` (`lib/src/search.rs:1242`) rejects patterns of a
  smaller period, then the search backtracks to enumerate the next solution.

Correspondence table:

| factoriosrc mechanism | SAT solver counterpart |
| --- | --- |
| `check_affected` + the `stack`/`stack_index` propagation queue | BCP (unit propagation); the incremental descriptor updates are similar to watchlists |
| The precomputed `RuleTable` | A precompiled failed-literal / unit-propagation closure |
| `guess()` walking the `next` chain | DPLL-style branching in a fixed order |
| `backtrack()` undoing the latest guess | Chronological backtracking |
| `Reason` (`lib/src/cell.rs:52`) | Records the decision level, but **not the antecedent** |

So the current algorithm is roughly DPLL with strong unit propagation. What is missing is exactly
the core of CDCL: conflict analysis, non-chronological backtracking (backjumping), clause
learning, restarts, and phase/activity heuristics.

## Structural differences that matter

Before going through the concrete directions, here are the differences between CA search and SAT
solving that decide which techniques transfer directly and which need adaptation:

- **The constraints are n-ary.** For each cell, the constraint is successor = f(current,
  neighborhood), where the neighborhood can have up to 24 cells. The implication graph is
  therefore a hypergraph: the antecedent of a deduction can be a set consisting of the cell
  itself, its successor, and several neighbors. A learned "nogood" is a combination of states of
  a set of cells that cannot all hold at once — an n-ary forbidden condition, not a binary
  clause.
- **Global constraints.** A non-empty front (`front_count == 0`), the population bound
  (`below_max == 0`), and the period check (`check_period`) are not local constraints. Conflict
  analysis must exclude them; they cannot be learned like ordinary conflicts.
- **Periodicity / translation invariance.** The world is periodic in x, y, and t (see
  `World::canonicalize_coord`). A learned nogood can be normalized to relative coordinates and
  reused at any position, any generation, and even across world sizes. This is a unique advantage
  of CA search over SAT, and it is the basis of idea 2 below.
- **Generations rules are multi-valued.** Branching cycles through the states
  (`Reason::TryAnother`), and the deductions of the underlying 2-state rule are asymmetric:
  neighbors can be deduced to be alive but not dead (see `check_generations_implied`,
  `lib/src/search.rs:332`). Learning and backjumping must preserve this asymmetry.
- **The goal is to enumerate all solutions.** factoriosrc does not stop at the first solution; it
  keeps enumerating (combined with `reduce_max_population` for optimization searches, and
  "search for the next solution" in the GUIs). This affects the applicability of restart-based
  techniques.
- **Unsafe hot paths.** Changes to `LifeCell`/`World` touch unsafe code (`lib/src/cell.rs`,
  `lib/src/world.rs`, `lib/src/search.rs`) and need Miri verification (see AGENTS.md).

## Idea 1: Record antecedents + conflict analysis + non-chronological backtracking

The core CDCL idea, and the foundation for most of the others.

### Status: implemented as an opt-in experiment

`Config::backjump` (CLI `--backjump`, plus toggles in the TUI and the egui UI; restricted to
2-state rules by `Config::check`) enables two things: the **recording** of antecedents and
decision levels (the foundation), and the **full 1-UIP conflict analysis** with
non-chronological backtracking on top of it.

**The recording layer**

- Each cell set by a rule-based deduction remembers its antecedent as the source cell
  (whose neighborhood descriptor produced the deduction), and each symmetry deduction
  remembers the mirrored cell (`Antecedent`, `lib/src/cell.rs`).
- The search stack records a parallel `TrailMeta` (`level`, `decision`, `antecedent`) in
  lockstep, plus a `current_level` counter. All pop sites (`backtrack`, the lookahead probe
  rollback) keep the metadata in sync, so the implication graph of the current partial
  assignment is always reconstructible.
- The exact antecedent of a deduction is *not* stored as a set of cells: it is recovered
  from the source cell by walking the known cells of its current neighborhood descriptor,
  excluding the deduced cell itself, and — importantly — filtering to the cells whose stack
  positions precede the deduction. The position filter is soundness-critical: a cell set
  after the deduction may not be part of the deduction's reason (it could even have changed
  the deduction; if it had, the search would have conflicted when it was set).
- The recording itself costs about 2.5-3% when enabled (measured on
  `B3/S23 26 8 4 -y 1 -n a` and `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2-`), and nothing when
  disabled (the code path is unchanged; the side structures are all empty). The analysis
  costs much more — see the performance paragraph below.

**The analysis** — a standard MiniSat-style 1-UIP conflict analysis on top of the recording.
It was implemented twice:

- **First attempt (reverted).** The analysis looked correct on the test suite, but a
  differential enumeration test against the default search (`B3/S23 3x3x1` with diagonal
  order and the `R1` transformation) showed it was **incomplete**: it enumerated a strict
  subset of the solutions (e.g. 2 of 3). It found the first two issues below and was
  reverted rather than shipped broken.
- **Second attempt (the current code).** The investigation narrowed the causes to structures
  that SAT solvers do not have, and the second attempt fixed all of them:

  1. **Decision-carrier levels.** When the search backtracks chronologically, the popped
     guess is re-set to its opposite state as a deduction with no antecedent
     (`Reason::Deduced`, `Antecedent::None`). These "flip" entries are conceptually re-tries
     of earlier decisions, so they are now recorded as **decision carriers**: each carrier
     occupies a whole level, `current_level` counts carriers (not guesses), and every level
     above the root has exactly one reasonless decision. This restores the standard
     invariant of one decision per level that the 1-UIP analysis relies on.
  2. **Stop at the decision carrier.** The resolution never replaces a reasonless literal
     by an empty reason: as soon as the walk reaches the current level's decision carrier,
     the analysis stops and uses it as the 1-UIP. (Resolving it "with an empty reason" was
     the resolution-rule mistake of the first attempt, which derived invalid nogoods.)
  3. **The chain position of the resumption point.** The search chain (the `next` links of
     the search order) is in a fixed spatial order that differs from the trail order, and
     the backjump truncation pops trail entries whose chain positions lie *before* the
     chain resumption point. The search then resumes behind those cells without ever
     visiting them again, which previously let the search report `Solved` with unknown
     cells (spotted by the same differential test on a `D2H`/`S0` configuration). The fix:
     each cell in the chain gets a rank (computed in chain order), the truncation resumes at
     the chain-earliest popped cell, and it additionally re-checks the trail entries whose
     descriptors were changed by the pops (a targeted re-queue instead of re-checking the
     whole trail).
  4. **The learned-clause reason as (cell, position) pairs.** The reason of a backjump flip
     (a clause) is only valid while its cells are set to the recorded values; the cells can
     be re-set by later backjumps, so the analysis must verify (by recorded stack positions)
     that the clause is still current, and fall back to chronological backtracking when it is
     not.

**Result (2026-08)**: **correctness is achieved** — the differential enumeration matrix
(8 rules x 2-3 world sizes x periods x 3 search orders x 6 symmetries x 6 transformations x
3 new-state strategies, 16,200 configurations) matches the default search exactly, including
the two corner cases that caught the first attempt. On the usual benchmarks it is **not a
performance win**: on `B3/S23 26 8 4 -y 1 -n a` it is ~30x slower than the default (38 s vs
1.25 s; with a depth gate even worse), and the default factorio-rule benchmark does not
finish in 120 s. The cause is re-treading: without the persistent nogood database (idea 2),
every learned clause is discarded as soon as its cells are popped, so the analysis regularly
re-explores already closed branches (on a tiny configuration the same solution was re-found
16 times). However, on very large searches the analysis wins decisively, matching the old
rlifesrc observation — see
*[Where backjumping shines](#where-backjumping-shines-b3s23-64-64-1--n-a)* below.

**What remains**: the analysis is kept behind the `Config::backjump` flag for
experimentation, and the recording layer (the foundation of idea 2) is verified sound. The
natural next step is idea 2 — a persistent nogood database — which is the only way to
recover the closed-branch knowledge that the re-treading loses; for typical (small and
medium) searches idea 1 is only useful as this foundation, while its greatest visible win
so far is the large-search regime (see below).

### Where backjumping shines: `B3/S23 64 64 1 -n a`

The exception that confirms the rlifesrc 2021 observation: searches over a **large world**
with **shallow local cascades**, where the analysis's jump distance is huge compared to the
re-propagated work. The case is a 64x64 period-1 search (a 64x64 still-life-like pattern in
Conway's Life), with the alive-first `new_state` strategy:

| Configuration | Time | Result |
| --- | --- | --- |
| (no flags) | > 90 s, no result | — |
| `--backjump` | **19 ms** | a 64x64 still-life-like pattern |
| `--lookahead` | 44 ms | **a different** 64x64 pattern |
| `--phase-saving` | 1.9 s | a slightly different 64x64 pattern |

Notes:

- This is the same qualitative picture as the old rlifesrc result ("backjumping is only
  useful for large (e.g., 64x64) still lifes"): on this case the conflicts are deep and the
  search space is huge, so the non-chronological jumps dominate the re-treading that makes
  backjumping a loss on small and medium cases.
- All three heuristics help here, but they find (slightly) different solutions: backjump and
  lookahead land on different patterns within milliseconds, and phase saving needs almost 2
  seconds for another one. The lookahead one is the most different. This matters when the
  tool is used to *search* for examples, not to enumerate: the found pattern depends on the
  heuristic.
- The case is a convenient sanity check for the idea-1 machinery: a few-millisecond run
  that produces a valid (period-1) pattern, compared to a search that does not finish within
  any reasonable time in the default configuration.

### What the antecedent is

When `check_descriptor_implied` (`lib/src/search.rs:74`) deduces the state of a successor,
neighbor, or current cell, the deduction is based on the descriptor of some cell. The antecedent
can therefore be defined as the known part of that descriptor: the state of the checked cell, the
state of its successor, and the states of its known neighbors. Currently `Reason::Deduced` only
records that a cell was deduced, not from what, so the implication graph is not recoverable.

### How it would work

Attach an antecedent set (a collection of (cell, state) pairs) to each deduced cell. When a
conflict occurs, resolve backwards through the antecedents from the conflicting cell, find the
1-UIP (first unique implication point), jump directly there, and flip it (2-state rules) or try
the next state (Generations rules), skipping unrelated stack frames. This is standard CDCL
conflict analysis plus backjumping, except that the graph also contains n-ary constraint nodes.

### Expected payoff

A large reduction in wasted backtracking. The benefit should be most visible in searches with
lots of local deduction (high periods, large neighborhoods, large worlds), where conflicts are
often decided far above the current guess.

### Risks and work involved

- Requires changing `LifeCell`/`Reason`, which sit in the unsafe hot path;
- Resolving through n-ary antecedents is more complex than binary clause resolution;
- Conflicts from global constraints (front, population) and from symmetry must be marked as
  non-learnable and excluded from the analysis;
- Generations asymmetry (see above).
- The flip-level issue described above: the backtracking thread of the search rewrites
  trail entries (a popped guess becomes a reasonless "deduction"), and the level metadata
  must make this sound for the analysis. This was the main risk, it caused the first 1-UIP
  attempt to be reverted, and it is now addressed by the decision-carrier levels and the
  stop-at-the-carrier rule (see the status section).

## Idea 2: Forbidden pattern memory (a nogood database)

The CA version of CDCL clause learning. A learned nogood is a set of relative coordinates plus
states that cannot be extended to a solution; it is essentially a forbidden local pattern.

### Status: the exact-position mode is implemented as an opt-in experiment

`Config::nogood` (CLI `--nogood`, plus toggles in the TUI and the egui UI) enables the nogood
database of `lib/src/nogood.rs`; it implicitly enables `Config::backjump` in `Config::check`,
and is restricted to 2-state rules for the same reason as backjumping.

**What is implemented.** Every successful conflict analysis records its result — the 1-UIP cell
with its rejected state, plus the literals of the learned clause with their states (at most
`MAX_NOGOOD_LITERALS` literals; larger patterns are not worth their index entries). The
database (`NogoodDb`) stores nogoods by *absolute* cell indices and indexes them by every
(cell, state) literal pair, so that a candidate guess finds the nogoods it would complete
without scanning the database. The database is bounded (the oldest half is evicted when full,
like clause-database reduction), and queries examine at most `MAX_QUERY_CANDIDATES` candidates
per index bucket. Before guessing a state, `guess()` consults the database: if exactly one
state completes some stored pattern, the other state is set instead, as a **decision carrier**
(its own decision level, no antecedent); if both states are blocked, the search backtracks at
once.

Two design points came out of the experiments:

- **Forced assignments must be decision carriers, not deductions.** The first version recorded
  a forced assignment as an ordinary deduction justified by an `Antecedent::Clause` built from
  the blocking nogood. As soon as one of those cells was popped, the clause went stale, and
  every analysis walking through the cell fell back to chronological backtracking — the
  spaceship benchmark went from ~38 s to unbounded. Treating forced assignments as decisions
  (which are conservative-sound stop points for the resolution) removed the degradation
  entirely.
- **Query cost needs bounds.** A popular anchor cell shares its index bucket with many nogoods;
  without the candidate cap (and without building the "other literals" vector only after a
  candidate fully matches), query time dominated the search.

**Iteration 1 — guess-time checks only (superseded).** The database was first consulted only
in `guess()` before choosing a state, and at the chronological backtrack flip. Correctness
verified; performance: it recovered roughly half of what backjumping loses on small
enumeration workloads (`B3/S23 4x4x2`: ~888 search calls vs ~2352 for backjumping alone), cost
nothing on large searches, but did not rescue any of the backjumping losses (the spaceship
case stayed beyond 130 s). An instrumented run showed *why*: every conflict analysis succeeded
and jumped (zero stale-reason fallbacks), yet enabling the database nearly doubled the number
of conflicts per unit of progress. Re-treading means replaying *deduction cascades* into
already-closed branches, and a guess-time check only fires when the last literal of a forbidden
pattern happens to be the cell being guessed — patterns completed by deduction are never
intercepted.

**Iteration 2 — propagation-level firing (the current code).** Each entry maintains a counter
of how many of its literals currently hold, updated incrementally through the existing
`(cell, state)` index in `set_cell`/`unset_cell`. When an entry reaches "one literal short",
its remaining unknown cell is forced away from the recorded state during propagation — unit
propagation on the learned nogoods — justified by an `Antecedent::Clause` built from the other
literals at fire time. When *all* literals hold (reachable when a wrong-state cell is unset and
later re-set to the recorded state, skipping the one-short window), a pending flag queues a
[`Confl::Nogood`] conflict; the flag is **re-validated when consumed**, because the queue can
empty right after the match and the step can end with a direct backtrack that pops matched
cells before the next check sees the flag. Lookahead probes are fully excluded from the
counters — including their rollback, which must run while `in_probe` is still set so that the
skipped updates stay symmetric. The guess-time check of iteration 1 became redundant and was
removed; the flip-time check stays (it never fires on current benchmarks).

Two earlier design findings remain relevant:

- **Forced assignments must not carry reasons that go stale silently.** The first version of
  iteration 1 recorded forced guesses as ordinary deductions justified by the blocking nogood;
  stale clauses made every analysis walking through them fall back to chronological
  backtracking, and the spaceship benchmark went from ~38 s to unbounded. Guess-time forcing
  was therefore recorded as decision carriers, and the propagation firings keep clause
  antecedents (whose staleness falls back safely).
- **Query cost needs bounds**: popular anchor cells share their index bucket with many nogoods;
  candidates are capped per query, and the "other literals" vector is built only after a
  candidate fully matches.

**Result (2026-08)**: correctness verified by differential enumeration tests (solution-set
equality across 2-state, non-totalistic, B0, B0S8, symmetry, transformation, max-population,
reduce-max-population, and feature combinations) plus Miri. Performance:

| Case | plain | `--backjump` | iter. 1 (guess-time) | iter. 2 (firing) |
| --- | --- | --- | --- | --- |
| `B3/S23 64 64 1 -n a` | > 90 s | 26 ms | 36 ms | **9 ms** |
| `B3/S23 26 8 4 -y 1 -n a` | 1.19 s | ~38 s | > 130 s (DNF) | **11.6 s** |
| enumerate all `B3/S23 4x4x2` | ~389 calls | ~2352 | ~888 | **~563** |
| enumerate all `B3/S23 5x5x2` | ~4649 calls | ~306,064 | — | **~12,246** |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2-` | 26.9 s | > 120 s (DNF) | > 120 s (DNF) | > 120 s (DNF) |

The propagation-level firing is what actually recovers the re-treading loss: it converts the
spaceship case from a DNF into a solve three times faster than backjumping alone, makes the
large-search case faster than backjumping alone, and shrinks enumeration work by one to two
orders of magnitude on the deeper instance. The plain chronological search still wins the
typical medium instances, and the factorio-rule profile of idea 1 (a DNF under the CDCL
machinery where the default search finishes) is unchanged — that regime seems limited by the
analysis overhead per conflict rather than by missing memory.

**What remains**: the translated mode below (relative coordinates + purity filters + transfer
through `increase_world_size`) is now the most promising direction, since the exact-position
memory has proven its worth. The static-pattern shortcut was skipped: dynamic learning
subsumes it.

### Normalization

A nogood is stored in relative coordinates, so it can be translated to any position and any
generation. Canonicalizing nogoods with `Config::symmetry` (picking a minimal representative of
each equivalence class) can significantly improve the hit rate.

### Reuse across world sizes

`increase_world_size` (`lib/src/world.rs:1361`) rebuilds the `World` from scratch, throwing away
all search experience. But a nogood learned in a smaller world remains valid in a larger world as
long as it does not rely on the "outside the world is dead" boundary assumption. This fits
factoriosrc's typical workflow of gradually enlarging the world while searching, and may be the
single biggest practical win among the ideas here.

### Mind the boundary conditions

Cells at the boundary of a small world are forced to the background state (`init_known`; for a
rule without `B0` the background is dead, for a `B0` rule it is the periodic background). A
nogood that relies on this is not valid in a larger world. Three distinct mechanisms make a
nogood boundary-dependent, and the first two do not show up in its literals at all:

1. **Known-cell literals.** A nogood whose literal cell has `Reason::Known` (a padding cell, a
   cell whose translated predecessor left the world, a user known cell, or a diagonal-width
   cell) relies on the forcing. Filter these out.
2. **Baked descriptor bits.** The descriptors of padding-frame cells have their missing
   neighbors baked in as the background state (`set_outside_neighbor`, null successors), and
   the conflict seed collects only the *known real cells* of the descriptor. A conflict found
   on such a cell can therefore depend on the background without any Known literal appearing
   in the nogood. Every `Antecedent::Descriptor` source on the resolution chain must be an
   *interior* cell — inside `[0,w) × [0,h)` with non-null predecessor and successor, which
   implies a complete neighborhood, since the padding exactly covers the radius.
3. **Excluded level-0 literals.** The analysis drops level-0 literals from the learned clause
   because they always hold; but for a persistent nogood, dropping a conjunct makes the
   forbidden pattern *stronger*. If any level-0 fact was relied upon during the resolution
   (trackable by watching for skipped level-0 literals), the nogood must be tagged as
   config-local.

Additionally, nogoods derived from `Confl::Symmetry` are not translatable: the mirror mapping
depends on the absolute world size (e.g. `D2H` uses `y ↦ h−1−y`). They are valid only within
one world size. The exact-position mode needs none of these filters (everything it relies on
holds throughout one world); all of them become mandatory for the translated mode below.

### Database management

Analogous to the clause databases of modern SAT solvers: evict by activity/LBD to bound memory
use. As a start, one could store only minimal conflict sets and evict the oldest entries when
full.

### An easier start

Precompute "rule-specific static forbidden patterns" that do not depend on the instance, e.g.
"this 2x2 patch can never be all-alive under this rule". This is a small static nogood library
that can be used to evaluate the hit rate and query cost before building the full machinery.

## Idea 3: Phase saving and decision heuristics

This idea has two parts. **Only the first part (phase saving) is implemented; the second part
(VSIDS-style activity) is not.**

### Phase saving

**Status: implemented as an experiment.** `Config::phase_saving` (CLI `--phase-saving`, plus
toggles in the TUI and the egui UI) remembers the last state of each cell and guesses it first.
It is off by default, so existing behavior is unchanged. Phase saving is stored per cell in
`LifeCell::phase` (`lib/src/cell.rs`), written in `World::set_cell` when enabled, and consulted
in `World::guess` (`lib/src/search.rs`). On save/load the phases of the cells on the stack are
rebuilt by replaying `set_cell`; the phases of cells that were unset are lost, which only affects
the heuristic.

### Experiment results (2026-08, release build, hyperfine)

| Case | Without phase saving | With phase saving | Ratio |
| --- | --- | --- | --- |
| `B3/S23 26 8 4 -y 1 -n a` (justfile bench 1, spaceship) | 1.10 s | 3.41 s | 0.32x (3.1x slower) |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` (justfile bench 2, Generations) | 2.00 s | 2.99 s | 0.67x (1.5x slower) |
| `B3/S23 26 8 4 -y 1` (default new state) | 35.5 s | 37.5 s | 0.94x (slightly slower) |
| `B3/S23 26 8 4 -y 1 -n r --seed 42` (random) | 2.56 s | 2.42 s | 1.06x (slightly faster) |
| `3457/357/5 20 16 7 -x 3 -s D2-` (default new state) | 2.46 s | 2.93 s | 0.84x (slightly slower) |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2-` (default rule, solves) | 26.7 s | 13.4 s | **2.0x faster** |
| `R3,C2,S2,B3,N+ 50 12 3 -x 1 -s D2-` (default new state) | 139.0 s / 139.6 s | 131.8 s / 132.2 s | **1.05x faster** |
| `R3,C2,S2,B3,N+ 50 12 3 -x 1 -s D2- -n r --seed 42` (random) | 34.1 s | 16.5 s | **2.1x faster** |

Notes:

- The effect depends strongly on the fixed `new_state` strategy. When the fixed strategy is
  already well matched to the task (guessing alive for spaceship searches, or dead in general),
  phase saving interferes and is slower. With random guessing it is mildly faster.
- On the default factorio rule `R3,C2,S2,B3,N+` — the rule this project is built around — phase
  saving helps: about 2x on the solving searches tested (26.7 s vs 13.4 s for
  `50 10 4 -x 2 -s D2-`, and 34.1 s vs 16.5 s for `50 12 3 -x 1 -s D2-` with the random
  strategy), but only about 5% on `50 12 3 -x 1 -s D2-` with the default dead strategy (139 s vs
  132 s, finding the same solution with population 124). The two runs of `50 10 4` find different
  solutions (population 72 vs 82), both valid.
- The user-proposed large case was originally mis-typed as `50 12 3 -x 2 -s D2-`, which turns
  out to be instant (~15 ms, NoSolution) in the current code, both before and after this change,
  so it cannot serve as a slow benchmark. The correct `50 12 3 -x 1 -s D2-` (~139 s, solves) is
  a realistic large case. R3 searches show a sharp difficulty cliff: some D2- period-3 cases are
  instantaneous (a structural no-solution argument), while period-4 or C1 cases easily exceed
  15 minutes.

Verdict: mixed, but the factorio-rule results make it worth keeping as an opt-in, especially
with the random `new_state` strategy. A follow-up could try restricting phase saving to certain
rules or new-state strategies, or combining it with idea 4 (probing).

### Phase saving (the MiniSat idea)

Each cell remembers the last state tried or deduced for it, and prefers that state when it is
guessed again. This replaces the global `Config::new_state` policy of Alive/Dead/Random. The
change is concentrated in `guess()`, barely touches data structures, and is equally valid when
enumerating multiple solutions (it is only a heuristic, not a correctness issue).

### VSIDS-style activity

**Status: not implemented.** This is still only an idea; it was deliberately left out of the
phase saving experiment so that the two heuristics could be evaluated separately (see the
"suggested order of exploration" section below).

Give cells that participate in conflicts a score bump, and branch on the most active cell. The
risk: the fixed spatial order of the `next` chain is an important synergy with the front
optimization and local propagation (the front argument in `docs/front.md` does not depend on the
guess order, but the order affects propagation efficiency). A safe approach is to use activity
only within a local window of the current order, or to make it an experimental switch to compare
against the fixed order.

## Idea 4: Probing before branching (lookahead)

**Status: implemented as an experiment** (polarity selection only; the cell-selection variant is
not implemented). `Config::lookahead` (CLI `--lookahead`, plus toggles in the TUI and the egui
UI) probes both states of the next unknown cell before guessing it, and guesses the better state
(or the only non-conflicting one). It is off by default. It only applies to 2-state rules and is
rejected for Generations rules by `Config::check` (the probe has no analogue for the dying
states; the old "Generations rules are skipped" behavior — verified to be unaffected by a
benchmark — was replaced by the explicit check). Probing is implemented
in `World::probe` (`lib/src/search.rs`): it temporarily sets the cell, propagates with a cap of
256 deductions (`MAX_PROBE_DEDUCTIONS`), scores the probe, and rolls it back. The rollback reuses
the existing set/unset + stack machinery; `World::in_probe` prevents probes from corrupting phase
saving when both are enabled.

The CA version of SatZ-style lookahead / DLIS.

- The current lookup tables already embed one level of failed literals:
  `Implication::NeighborhoodAlive/Dead` (`lib/src/rule.rs:356-365`) are exactly "setting an
  unknown neighbor to some state leads to a conflict", precomputed; the `forced` bits of
  non-totalistic rules are the same idea for individual neighbors.
- **One level deeper**: before branching, probe candidate cells — set, propagate for k steps,
  count deductions/check for conflicts, unset. Use the score to rank candidate cells. A conflict
  found while probing is a free failed-literal prune.
- The world is small, `check_affected` is cheap, and everything is local, which fits factoriosrc
  well.
- This needs a snapshot/rollback mechanism: the existing set/unset + stack machinery can be
  reused, or probing can run on a separate small stack.

### Experiment results (2026-08, release build, hyperfine)

| Case | Without lookahead | With lookahead | Ratio |
| --- | --- | --- | --- |
| `B3/S23 26 8 4 -y 1` (default new state) | 39.0 s | 5.66 s | **6.9x faster** |
| `B3/S23 26 8 4 -y 1 -n a` (spaceship, alive) | 1.20 s | 5.67 s | 0.21x (4.7x slower) |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` (Generations control) | 2.12 s | 2.14 s | 1.01x (no effect) |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2-` (default rule) | 27.8 s | 32.3 s | 0.86x (1.16x slower) |
| `R3,C2,S2,B3,N+ 50 12 3 -x 1 -s D2-` (default rule) | 141.7 s / 143.6 s | 230.8 s / 231.1 s | 0.62x (1.6x slower) |
| `R3,C2,S2,B3,N+ 50 12 3 -x 1 -s D2- -n r --seed 42` (random) | 35.7 s | 232.3 s | 0.15x (6.5x slower) |

Notes:

- Lookahead gives a dramatic win on the `B3/S23` default-strategy search (6.9x faster): with
  dead-first guessing, probes immediately discover failed literals and pick alive first, avoiding
  long dead branches.
- It is a clear loss on the default factorio rule searches (1.6x to 6.5x slower). Lowering the
  probe cap from 256 to 32 deductions did not change this (238 s vs 231 s on the 50x12 case), so
  the overhead is the fixed per-guess probe cost, not the propagation depth.
- The Generations control ran before the `Config::check` change: lookahead is now rejected
  for Generations rules, so this control cannot be reproduced with the current code (the
  old behavior — the probe was skipped — was the "no effect" that the row shows).
- The two runs of `50 12 3 -x 1 -s D2-` find the same solution (population 124) with and without
  lookahead; the probe overhead is pure loss on that search.

Verdict: mixed, and on the whole a net loss for the searches this project cares about most (the
default factorio rule). It is kept as an opt-in because the `B3/S23` default-strategy case shows
a large potential win. A follow-up could probe less often (e.g. only every k-th guess, or only
when the preferred state would be the default guess), probe a single state instead of both, or
only probe small neighborhoods where the check is cheap.

## Idea 5: Consistency across overlapping neighborhoods (an arc-consistency analogue)

Propagation currently only reasons inside a single cell's descriptor. The descriptors of two
adjacent cells share variables (shared neighbors + the successor chain), and together they can
deduce things that neither table can alone. This is the analogue of 2-consistency in CP, or local
resolution in SAT preprocessing.

- Fully precomputing this is infeasible (two neighborhoods give about 2^(2n) entries). A
  practical approach is to check "descriptor pairs" encountered during propagation on the fly,
  with a small cache (transposition-table style).
- A generative variant: analogous to BVE (bounded variable elimination), fold the deterministic
  dying chains of Generations rules (they are already deterministic propagation, so they can be
  eliminated like pseudo-variables).

## Idea 6: Multi-valued encoding for Generations rules

- `Reason::TryAnother` branches in a cycle over the states, while CDCL techniques live in the
  Boolean domain.
- Treat the dying chain as deterministic propagation (it already is), and restrict learning and
  backjumping to the dead/alive base layer; or encode Generations rules in Boolean variables (one
  variable per cell per state, with exactly-one constraints), so that the 2-state CDCL machinery
  applies unchanged.
- Either way, preserve the asymmetry of deduction (only alive can be deduced).

## Idea 7: Restarts (Luby sequence) — not recommended for now

- Restarts help find the first solution, but factoriosrc's typical use case is enumerating all
  solutions, where restarts only repeat work.
- They only make sense together with idea 2 (nogood memory) and phase saving, and they interact
  badly with save/load and incremental world growth.
- Reconsider only after the CDCL-style techniques above are mature.

## Other loose ideas

- **Component caching** (SAT's component caching): split the unknown cells into independent
  components by constraint dependency and solve each separately. The constraint graph is dense in
  periodic searches, so components are rare; a large world with many known cells might be an
  exception.
- **Cube-and-conquer**: split the search space into cubes to solve in parallel, for future
  parallel search (the egui frontend already runs the search on a background thread).
- **Encode as CNF and compare against an off-the-shelf SAT solver**: periodic pattern search can
  be encoded directly (one variable per cell per generation plus transition constraints). This
  could serve as a baseline for the ceiling of what a SAT-style approach can achieve.
- Borrowing from row-by-row searchers (e.g. qfind) is a separate direction (already in the
  README) and out of scope here.

## Suggested order of exploration

1. Idea 3, phase saving part: **done** — implemented as an opt-in `Config::phase_saving` and
   benchmarked (see the results above). Kept as opt-in because the payoff is rule-dependent. The
   VSIDS-style activity part of idea 3 is **not** done; it is an open follow-up, possibly as an
   experimental switch compared against the fixed order;
2. Idea 4, polarity-selection part: **done** — implemented as an opt-in `Config::lookahead` and
   benchmarked (see the results above). Kept as opt-in; it is a large win on the `B3/S23`
   default-strategy search but a loss on the default factorio rule. The cell-selection variant of
   idea 4 (which needs a small search-order refactor to stay sound) is **not** done;
3. The foundation of idea 1: attach antecedents to `Reason::Deduced` (record only, do not change
   the algorithm yet) — **done** — implemented as an opt-in `Config::backjump` (the flag records
   antecedents and decision levels, and also enables the full analysis; see the next item). The
   recording costs about 2.5-3% when enabled and nothing when disabled. This is the prerequisite
   for all later CDCL-style techniques;
4. The full idea 1 (1-UIP backjumping): **done, correctness verified** — implemented and made
   sound (three fixes: decision-carrier levels, stop-at-reasonless, chain-rank resumption +
   targeted re-check; see the idea 1 section). The differential matrix (16,200 configurations)
   matches the default search. Performance: a **loss on small and medium searches** (the analysis
   re-treads closed branches without a persistent nogood database, ~30x slower on the spaceship
   benchmark), but a **decisive win on very large searches** (e.g. `B3/S23 64 64 1 -n a` finds a
   pattern in 19 ms where the default search does not finish — the same regime rlifesrc
   observed). The flag stays opt-in for experimentation.
5. Idea 2, exact-position mode with propagation-level firing: **done, correctness verified** —
   implemented as an opt-in `Config::nogood` (implicitly enables backjumping; see the idea 2
   section). Iteration 1 (guess-time checks only) recovered part of the re-treading loss;
   iteration 2 (counter-based unit propagation on the learned nogoods) is the mechanism that
   actually pays off. Next: the translated/cross-size mode;
6. Ideas 5 and 6 as needed; idea 7 last.

## Things to re-check before implementing

- Conflicts from global constraints (front_count, below_max, check_period) are not learnable and
  must be marked specially in conflict analysis. See `docs/front.md` for the reasoning framework.
- Any unsafe change in `lib/src/world.rs`, `lib/src/search.rs`, or `lib/src/cell.rs` requires
  `cargo +nightly miri test test_miri`.
- New fields or states that need persistence must be synced with `WorldSerde`
  (`lib/src/world.rs:1384`) and the TUI/egui save formats (which are not interchangeable, see
  AGENTS.md).
- `Config::check()` (`lib/src/config.rs`) is the single source of truth for validation; if a new
  search strategy affects the supported rules, change `Config` first, then the UIs and docs.
- Each idea must be validated separately for the combination of "enumerate all solutions +
  reduce_max_population + incrementally larger worlds": is the learned information still valid
  after backtracking and after rebuilding the world?
- For the idea-1 machinery in particular, validate by comparing the *sets* of enumerated
  solutions against the default search (the raw solution counts differ even without backjumping,
  since the search may legitimately re-find a solution, e.g. a pattern and its generation
  rotation). This differential test caught the incompleteness bug in the first 1-UIP attempt.
