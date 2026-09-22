# SAT-Inspired Search in factoriosrc

> **Scope.** This is an implementation note, not a general survey of SAT
> solving. It describes the search code in this repository and separates
> implemented experiments from proposed work. The experimental options are
> disabled by default.

The search engine is not a CNF SAT solver, but several parts of its constraint
search have close SAT or constraint-programming analogues. This note records
which ideas have been tried, what the current implementation actually does,
and what remains worth exploring.

## Current Status

The public configuration is defined by `Config` in `lib/src/config.rs`. The
seven experimental switches are available from the CLI as
`--phase-saving`, `--lookahead`, `--backjump`, `--nogood`,
`--nogood-capacity`, `--nogood-guard`, and `--activity`; the TUI and egui
frontends expose the same options.

| Technique | Current status | Scope and important limits |
| --- | --- | --- |
| Local propagation and chronological search | Implemented by default | The baseline search uses the fixed `next` chain, incremental descriptors, and chronological backtracking. |
| Phase saving | Implemented, opt-in | Remembers the last real state of a cell and tries it first. Works with supported two-state and Generations rules. |
| Lookahead | Implemented, opt-in | Probes both states of the next cell and chooses a polarity. Two-state rules only; it does not choose a different cell. |
| Conflict analysis and backjumping | Implemented, opt-in | A 1-UIP-style analysis for local rule, symmetry, and learned-nogood conflicts. Two-state rules only. A protocol guard keeps enumeration free of repeated solutions. |
| Exact-position nogood database | Implemented, opt-in | Learns from successful local conflict analysis and propagates learned forbidden patterns. It uses absolute cell indices, a recency-ranked database whose capacity scales with the world size (`max(2048, 4 * cells)` by default, or an explicit `Config::nogood_capacity`), and is valid only in the current `World`. Two-state rules only. Clause minimization was investigated and reverted after a negative performance result; allowing long learned clauses was later found to be a large win. |
| Quality-based nogood eviction or dynamic capacity | Recency eviction implemented; quality tiers and dynamic capacity not implemented | `NogoodDb::reduce` evicts the worst half by last-use epoch, then by fewest uses, at the world-size capacity. A short or low-LBD protected tier and dynamic capacity remain unmeasured candidates. `NogoodStats` and `NogoodDb` record the first-use delay, the quality of learned and used entries, the evicted-use breakdown, and relearnings after eviction; the measurements are in [Measured retention snapshot](#measured-retention-snapshot-2026-09-20) and [Recency eviction A/B](#recency-eviction-ab-2026-09-20). See [Eviction policy and capacity selection](#eviction-policy-and-capacity-selection). |
| Machinery cost guard | Implemented, opt-in (`--nogood-guard`) | Bounds the work of conflict analysis and nogood maintenance per search step; suspends them when the work exceeds the budget and re-probes with backoff, so a regime where the machinery does not pay off falls back to the plain search. Enables `--nogood` (and therefore `--backjump`) implicitly. Deterministic and not serialized. Off by default during the experimental phase, so that it does not mask `--nogood` optimizations. See [Cost Guard for the Experimental Machinery](#cost-guard-for-the-experimental-machinery). |
| VSIDS-style activity | Implemented, opt-in | Bumps the cells of recent conflicts and guesses the most active cell among a small window of the search-order chain. Changes the branching order only, so it works with both two-state and Generations rules. Not serialized. |
| Translated or cross-size nogoods | Not implemented | The current database is not normalized to relative coordinates. |
| Dynamic cell selection for lookahead | Not implemented | Lookahead only changes the state tried for the next cell. |
| Restarts, component caching, and CNF encoding | Not implemented | These remain possible future experiments, not current search modes. |

`--nogood` enables `backjump` implicitly in `Config::check()`, and
`--nogood-guard` enables `nogood` (and therefore `backjump`) implicitly.
`lookahead`, `backjump`, and `nogood` are rejected for Generations rules
because their current reasoning is defined only for the two-state layer.
Phase saving and activity have no such restriction: activity only reorders
the branching cell, which is meaningful for every rule family.

The status terms in this document have a precise meaning:

- **Implemented** means that the code path exists behind a configuration
  switch; it does not imply that it is faster than the default.
- **Correctness-tested** means that repository tests compare outcomes or
  exercise the relevant invariants.
- **Measured** is reserved for results with a reproducible command and
  recorded environment. The benchmark section records the current one-run
  snapshot; older ad-hoc measurements are not treated as evidence.

## Baseline Search

`World` represents a finite searchable region of an `(x, y, t)` cell grid for
a pattern with a periodic, possibly translated or transformed, time cycle.
The cells outside the searchable region are fixed to the rule's background
state. For each cell, a descriptor summarizes the known part of its
neighborhood and its successor relationship. `RuleTable::implies()` uses
precomputed local information to report conflicts and deductions.

The main entry points are `World::search()` and `World::step()` in
`lib/src/search.rs`. One search step follows this pattern:

1. `World::set_cell()` updates the cell, the affected descriptors, the
   successor cache, population/front counters, and the search stack.
2. `check_stack()` consumes the part of the stack after `stack_index` as a
   propagation queue. `check_affected()` checks the pending nogood state,
   global constraints, symmetry deductions, and the descriptors of the cell,
   its predecessor, and its neighbors.
3. Local rule checks may deduce the successor, the current cell, all unknown
   neighbors for totalistic rules, or selected neighbors for non-totalistic
   rules. Generations rules use a separate check that preserves their
   deterministic dying-state transitions and asymmetric deductions.
4. If propagation reaches a fixed point, `guess()` follows the `next` chain
   from `start` and assigns the next unknown cell. Depending on the options,
   it may first run lookahead or consult phase saving; otherwise it follows
   `Config::new_state`.
5. A conflict is handled by local conflict analysis when `backjump` is
   enabled and the conflict is analyzable. Otherwise, `backtrack()` removes
   assignments until it can try another state.
6. When every cell is assigned, `World::search()` calls `check_period()`.
   A pattern with a smaller period is rejected by ordinary backtracking, and a
   valid solution is returned for enumeration.

The closest SAT terminology is:

| factoriosrc mechanism | SAT or CP analogue | Difference that matters |
| --- | --- | --- |
| Descriptor checks and the stack queue | Unit propagation / BCP | A deduction can depend on an n-ary neighborhood descriptor rather than a clause watched by two literals. |
| Precomputed `RuleTable` implications | Local propagation table | The table describes the cellular-automaton transition, not a CNF encoding. |
| `next`-chain branching | DPLL branching | The default variable order is spatial and is also used by the front optimization. |
| `backtrack()` | Chronological backtracking | Generations rules cycle through multiple states with `Reason::TryAnother`. |
| `Antecedent` and `TrailMeta` | An implication graph and decision levels | These metadata exist only when backjumping is enabled and are separate from `Reason`. |
| `NogoodDb` | A learned-clause database | Current entries are absolute, world-local forbidden assignments, not reusable clauses. |

## Constraints That Affect SAT Techniques

Several properties of the search make a direct CDCL translation unsound or
unprofitable.

### N-ary local constraints

A transition constraint relates a cell, its successor, and a neighborhood of
up to 24 cells. Its implication graph is therefore closer to a hypergraph
than to a Boolean clause graph. A learned nogood is a set of cell-state pairs
that cannot all occur together.

### Global constraints

`front_count` and `below_max` are incrementally maintained global constraints.
An empty front or an exceeded population bound is reported as
`Confl::Global`, which is handled by chronological backtracking even when
backjumping is enabled. The period check is performed after a complete
assignment and is also not a local learned conflict. These failures must not
be treated as ordinary descriptor clauses without a separate validity proof.

The front invariant is documented separately in [`docs/front.md`](front.md).
That document is the source for the translation/reflection assumptions behind
`init_front()` and `front_count`.

### Background and boundaries

For ordinary rules, the background outside the search range is dead. For a
`B0` rule, it is a uniform periodic background described by the rule. The
searched period must be a multiple of the background period. In particular,
"empty" means "equal to the background state", not necessarily dead; for a
rule with both `B0` and `S-max`, the background is permanently alive and the
population counts dead cells instead.

This boundary state is part of the semantics of a `World`. A pattern learned
near padding or known cells cannot automatically be assumed valid at another
position or in a larger world.

### Multi-valued Generations rules

Generations rules branch through dead, alive, and dying states. Dying states
advance deterministically, and the underlying two-state implication table has
an intentional asymmetry: for example, a neighbor may be forced alive without
being forceable dead. The current antecedent and polarity logic does not model
this as a Boolean clause system, so backjumping, nogood learning, and
lookahead are restricted to two-state rules.

### Enumeration rather than one satisfying assignment

`factoriosrc` normally continues after a solution in order to enumerate more
solutions, and it can lower the population bound after a solution when
`reduce_max_population` is enabled. A heuristic is therefore judged both by
whether it preserves the solution set and by how it changes traversal work.

A chronological flip (the retry of the other state of a guess) marks its
first branch as exhausted; undoing it would re-report the solutions found
there, so the
[enumeration protocol guard](#preserving-the-enumeration-protocol) prevents
this. For configurations that differ only in the experimental switches, the
number of reported solutions is therefore a valid differential oracle, not
just the set. Equivalent encodings, such as generation rotations, still
appear as separate solutions.

### Unsafe hot paths and persistence

`LifeCell` and the graph of raw pointers in `World` are performance-sensitive
unsafe code. Changes to `lib/src/cell.rs`, `lib/src/world.rs`, or
`lib/src/search.rs` require the Miri check described in `AGENTS.md`.

The serialized `World` stores the configuration, ordinary search stack, and
visible search state, but not `TrailMeta`, decision-level arrays, flip levels,
or the nogood database. Loading replays stack assignments without
reconstructing the original antecedent graph. Phase history for unset cells is
also not serialized. This affects performance and heuristic state, not the
intended semantics of a completed search.

## Conflict Analysis and Backjumping

This is the current implementation of the first CDCL-inspired experiment. It
is enabled by `Config::backjump` or implicitly by `Config::nogood` and is
restricted to two-state rules.

### Recorded reasons

`Reason` remains a small enum describing how a cell was set:
`Known`, `Deduced`, `Guessed`, or `TryAnother`. It does not contain a decision
level or an antecedent.

When backjumping is enabled, `World` records a parallel `TrailMeta` entry for
each stack entry. It contains:

- the decision level;
- whether the entry is a decision carrier;
- whether the carrier is a flip; and
- an optional `Antecedent`.

The current `Antecedent` variants are:

- `Descriptor(source)`: a rule-table deduction came from the source cell's
  descriptor;
- `Symmetry(source)`: a symmetry deduction copied the source cell; and
- `Clause(literals)`: a learned clause forced the cell.

For a descriptor antecedent, the exact literal set is recovered when the
analysis needs it. The source descriptor contributes its currently known
neighbors, source cell, and successor, excluding the target cell and filtering
to stack positions that preceded the deduction. The position filter is
essential: a cell assigned later may have changed the descriptor and cannot be
retroactively treated as a cause.

### Decision carriers

A normal guess starts a decision level. When chronological backtracking flips
a two-state guess, the opposite state is represented as a reasonless
`Deduced` entry with `decision = true`. This entry is a **decision carrier**:
it represents a retry of the same decision and ensures that every active level
has exactly one reasonless decision entry. A carrier of this kind is a
**flip** (`TrailMeta::flip`): the first branch of the decision is exhausted.

The 1-UIP walk stops at that carrier instead of resolving a reasonless literal
with an empty antecedent. This convention is specific to the mutable trail
used by factoriosrc and is part of the soundness invariant of the current
analysis.

### Analysis and resumption

For a local conflict, `World::analyze()` performs the following work:

1. Seed the conflict with descriptor literals for a rule conflict, the two
   cells for a symmetry conflict, or the stored literals for a nogood conflict.
2. Resolve current-level literals through their recorded antecedents until one
   current-level literal remains: the first unique implication point (1-UIP).
3. If a learned-clause antecedent no longer matches the recorded stack
   positions, abandon the analysis and fall back to chronological
   backtracking.
4. Pop the trail to the backjump target: the highest decision level of the
   remaining literals, never at or below the deepest flip carrier (see
   below).
5. Because trail order and the spatial `next` chain differ, resume at the
   chain-earliest popped cell and re-check descriptors affected by the pops.
6. Set the 1-UIP cell to the opposite state with a temporary learned-clause
   antecedent.

The temporary clause is valid only while its recorded cells remain at their
recorded stack positions. It is not a persistent database entry unless
`nogood` is also enabled.

`Confl::Rule`, `Confl::Symmetry`, and `Confl::Nogood` can enter this analysis.
`Confl::Global` cannot. Lookahead finding that both polarities conflict and a
failed `check_period()` also use ordinary backtracking.

### Preserving the enumeration protocol

Chronological backtracking enumerates without repetition because the trail
records which branches are exhausted: after a flip, the search never returns
to the first branch of that decision. Conflict analysis discards this
information. After a solution is reported, the continuation flips the deepest
carrier; if the flipped assignment conflicts, the 1-UIP is the flip itself,
and restoring it re-derives the just-reported solution. Popping the flip and
re-guessing the cell has the same effect one level down. Without a guard, an
exhausted `B3/S23 3 3 2` search — one canonical solution — reported it 16
times with backjumping and 5 times with nogood learning.

`World::analyze()` therefore tracks the flip levels (`World::flip_levels`) and
applies two rules:

- The backjump target is clamped to `max(clause_level, deepest_flip_level)`,
  so the analysis never pops a flip carrier: everything popped lies in
  branches that are still being explored.
- When the target would reach the deepest flip carrier — the 1-UIP is the
  flip itself or one of its deductions — the analysis skips the restoration
  and falls back to chronological backtracking.

Skipping a flip branch is sound because the learned clause proves it
contradictory: all its literals are still on the trail, and the 1-UIP's
rejected state was assigned inside the flip branch. The fallback may still
learn the nogood; such an entry can start fully matched, and the backtracking
that follows resyncs its counter. First-result searches are not measurably
affected; see the current benchmark snapshot below.

### Correctness status

The repository contains explicit solution-set and invariant tests for
backjumping, including:

- ordinary, non-totalistic, B0, symmetry, and transformation configurations;
- deeper searches and max-population searches;
- `reduce_max_population`;
- the backjump trail metadata invariant, including the flip-carrier lockstep;
- the enumeration protocol: solution counts must match the plain search
  (`B3/S23 3 3 2` reports one solution with backjumping or nogood);
- combinations with lookahead and the nogood database; and
- suspension and resumption of the whole machinery by the cost guard with a
  forced zero budget.

The comparison oracle is both the set and the number of serialized solutions.
These tests establish the behavior of the checked configurations; they are not
an exhaustive configuration matrix and they do not establish a performance
improvement. Backjumping remains opt-in because conflict analysis can cost
more than chronological search when learned information is not retained across
backtracking. The [cost guard](#cost-guard-for-the-experimental-machinery)
bounds that cost at run time.

## Exact-Position Nogood Learning

The nogood database is the current implementation of clause-learning memory.
It is enabled by `Config::nogood` and automatically enables backjumping.

### Representation and lifetime

A learned entry is a bounded list of `(absolute cell index, state)` literals:
the rejected state of the 1-UIP cell and the states of the learned clause.
The canonical form of an entry is the sorted literal list. The database
indexes every literal so that a matching entry can be found without scanning
all entries, and it keeps a hash of the sorted list so that verbatim
duplicates are rejected at learn time.

The current implementation deliberately uses absolute indices:

- it does not store relative coordinates;
- it does not canonicalize by translation, symmetry, or generation;
- it cannot transfer entries between positions or world sizes; and
- it is cleared when a world is loaded or rebuilt, including
  `increase_world_size()`.

The database is persistent only across backtracking inside one `World`. It
does not persist across save/load or world growth.

The current implementation limits are:

| Limit | Current value | Purpose |
| --- | ---: | --- |
| Database capacity | `max(2048, 4 * cells)` entries, or `Config::nogood_capacity` | When full, the worst half by the recency ranking is evicted and the index is rebuilt. |
| Literals per nogood | `96` | Guards against pathological clause growth; the observed clauses stay below it. |
| Candidates checked by one indexed query | `64` | Bounds work for a popular `(cell, state)` bucket. |

These are implementation limits, not correctness assumptions. Missing a
candidate because of the query cap loses pruning but must not change the
solution set.

The counter maintenance walks the index bucket of every real assignment, so a
database of a few thousand entries is cheap to maintain, and that is what an
early capacity sweep on the small benchmark workloads selected. The sweep did
not cover large worlds. On a large world the search can learn tens of
millions of clauses: with a fixed `1 << 11` entries the database only
remembers the last few thousand search steps, and the same local patterns are
learned, used once or twice, and evicted over and over. The learned
information then does not reduce the step count at all, while every eviction
and every bucket walk still costs time.

The default capacity therefore scales with the number of cells in the world,
including the padding ring: `NogoodDb::with_world_size` uses
`max(DEFAULT_CAPACITY, ADAPTIVE_CAPACITY_FACTOR * world_size)` with
`ADAPTIVE_CAPACITY_FACTOR = 4`. This keeps the average index bucket roughly
constant as the world grows, because the number of distinct literals grows
with the number of cells too, while letting learned patterns live long enough
to be reused. `Config::nogood_capacity` overrides the automatic choice.
Eviction uses the recency ranking described in
[Eviction policy and capacity selection](#eviction-policy-and-capacity-selection);
the older simple oldest-half policy and the earlier activity/LBD-based
experiments are recorded there. An activity/LBD-based eviction was slower on
the small workloads before the capacity became adaptive and was dropped, and a
Minisat-style antecedent-cone minimization was implemented, passed the
differential correctness tests, and was removed because its per-conflict walk
made both first-result and enumeration searches slower. The current
implementation does not minimize learned nogoods.

The capacity effect was measured on 2026-09-18 with a release build on the
same machine as the benchmark snapshot below, single runs, the default
`new_state`, and the command
`factoriosrc-tui new --no-tui --format json -r B3/S23 100 10 4 -x 1 --nogood
--nogood-capacity N`:

| Capacity | Steps | Wall time |
| ---: | ---: | ---: |
| 2,048 (old fixed default) | 233,591,288 | 2,018 s |
| 16,384 | 24,790,612 | 355 s |
| 19,584 (automatic default, padded cells) | 26,178,710 | 390 s |
| 32,768 | 16,045,721 | 315 s |
| 65,536 | 14,943,369 | 442 s |

The plain search finds the same first solution in 235,633,435 steps and 49 s,
so the old default made `--nogood` 41 times slower without reducing the step
count; the automatic default cuts the steps by 9.0 times and the wall time by
5.2 times, and an explicit larger capacity cuts the steps by 14.6 times. The
step count keeps falling with the capacity while the wall time has its
optimum near 32,768 entries, because a larger database makes every assignment
walk longer index buckets.

On the smaller benchmark worlds the scaled capacity is close to the old
default: `B3/S23 26 8 4 -y 1 -n a` has 1,120 cells and uses 4,480 entries
(509,110 steps / 2.73 s at 2,048; 476,438 steps / 3.06 s at 4,480), and
`B2n3/S23-q 30 9 4 -x 1 -n a` has 1,408 cells and uses 5,632 entries
(2,387,731 steps / 14.77 s at 2,048; 2,161,350 steps / 16.84 s at 5,632).
The scaled default trades a small wall-time cost on those cases for a large
improvement on large worlds.

The length bound and the hot paths were revised together after the benchmark
sweep:

- The bound was originally `16` literals, on the assumption that long patterns
  rarely materialize again in full. The benchmark workloads contradict the
  assumption: most learned clauses are longer than 16 literals, and rejecting
  them discarded most of the pruning power. Raising the bound to `96` roughly
  halves the deep first-result benchmark and cuts the enumeration benchmark by
  more than an order of magnitude.
- The counters are kept in the dense `NogoodDb::remaining` array instead of
  inside the entries, so the bucket walk only touches compact memory.
- The index keys are packed into one integer and hashed with the
  `rustc_hash::FxHashMap` and `FxHashSet` types of the `rustc-hash` crate
  instead of the default SipHash, and duplicate rejection uses a hash of the
  sorted literals instead of re-sorting stored entries at every query.

A two-watched-literal propagation layer, modeled on MiniSat and Glucose, was
implemented and dropped. Under this search's chronological backtracking the
watch lists thrash as cells are re-set and unset, so the per-event scans of
the clause literals cost more than the incremental counters, even though the
counters touch more entries per assignment.

### Eviction policy and capacity selection

This subsection records the research pass over the eviction policy and the
resulting implementation. The capacity is still the world-size budget
described below; `NogoodDb::reduce` now evicts the worst half by a recency
ranking instead of the older half by insertion order.

#### The implemented recency eviction

`NogoodDb::reduce` ranks every live entry by

1. the database reduction epoch of its last use, where a fresh entry counts as
   used at its learning epoch (`NogoodDb::last_used_epoch`),
2. then by fewer cumulative `uses` first, and
3. then by older id.

The worst `entries.len() / 2` entries are evicted with
`select_nth_unstable_by_key`; the id makes the sort key a total order, so the
selected set is deterministic and the selection is linear in the database size
before the index rebuild. An entry that fired during the current reduction
interval survives, an entry that has not fired since is evicted, and a fresh
entry gets one full interval to prove itself — the same grace period the
oldest-half policy gave to the newest half. This is the Kissat `used`
semantics adapted to the CA database. The capacity formula is unchanged, so
the comparison isolates the eviction policy.

The earlier design and its rationale are kept below as the research record.
`NogoodDb::reduce` used to evict the *older half* of `NogoodDb::entries`
unconditionally: it drained the first `entries.len() / 2` insertion-ordered
entries without looking at `uses`, `remaining`, or the literal count, so an
entry that fired many times could be discarded while a never-used newer entry
was kept.

Unlike a CDCL solver, the database does not need to protect *locked* entries.
A learned clause that is currently the reason of an assignment is stored as
`Antecedent::Clause(Box<[(*const LifeCell, u32)]>)`, i.e. as cell pointers and
the stack positions they held when the clause was recorded, not as a database
id. `World::reason_literals` revalidates those positions and falls back to
chronological backtracking when the reason is stale, so an entry can be evicted
at any time without invalidating an in-flight deduction. The eviction policy
therefore has more freedom than in a SAT solver.

#### Capacity is a maintenance budget

`NogoodDb::on_set` and `on_unset` walk the `(cell, state)` index bucket of
every real assignment and update the `remaining` counter of every entry in it.
For `entries` live entries of average length `avg_len` over a world of `size`
cells (including the padding ring), the average bucket length is

    avg_bucket ≈ entries * avg_len / (2 * size),

because there are about `2 * size` distinct `(cell, state)` literals. Keeping
the per-assignment work bounded therefore requires

    entries ≤ 2 * size * avg_bucket / avg_len.

Choosing `avg_bucket ≈ 2 * avg_len` gives `entries ≤ 4 * size`, which is
exactly `ADAPTIVE_CAPACITY_FACTOR = 4`. The factor is a *maintenance budget*,
not an estimate of how much knowledge is worth keeping. Any occurrence-list
design has the same scaling — the watch lists of the two-watched-literal layer
that was tried and dropped grow like `entries / size` too — so the
per-assignment cost is linear in the database size at fixed world size. That is
why the measured wall time has an optimum near 32,768 entries even while the
step count keeps falling.

The other half of the problem is the *retention need*: how long a learned
pattern must stay in the database to be reused. It is governed by the reuse
distance (the number of conflicts between two learnings of the same pattern),
which grows with the world and with the length of the search, not by the world
size alone. A more principled rule would be

    capacity ≈ min(maintenance budget, retention need),

with the retention need estimated online from reuse statistics. The current
formula implements only the first term, with `4 * size` standing in for the
second, and `DEFAULT_CAPACITY` as the floor for small worlds.

#### What the reference solvers do

The reference implementations consulted for this note (MiniSat, Glucose, and
Kissat) all rank entries by a quality measure instead of age:

| Solver | Reduce trigger | Eviction ranking | Protected entries |
| --- | --- | --- | --- |
| MiniSat (`Solver::reduceDB`, `reduceDB_lt`) | `learnts.size() - nAssigns() >= max_learnts`; `max_learnts` starts at `nClauses()/3` and grows geometrically (`learntsize_inc = 1.1`, `learntsize_adjust_inc = 1.5`) | clause activity, decayed by `clause_decay = 0.999` | binary clauses and locked clauses |
| Glucose (`Solver::reduceDB`, `reduceDB_lt`) | `firstReduceDB = 2000` conflicts, then every `nbclausesbeforereduce` conflicts, incremented by `incReduceDB`/`specialIncReduceDB` | LBD first, then activity; roughly half removed | binary clauses, `LBD <= 2`, and the best 10% by activity; the `chanseok` mode keeps all clauses with `LBD <= coLBDBound` |
| Kissat (`kissat_reduce`, `collect_reducibles`, `compute_tier_limits`) | conflict interval updated with `UPDATE_CONFLICT_LIMIT(reduce, ..., SQRT)` | smaller `size`, then smaller `glue` (LBD) | locked (`reason`) clauses, recently used `glue <= tier1`, very recently used `glue <= tier2` |

Two Kissat details are directly relevant. A clause's `used` field is set to
`MAX_USED` whenever the clause is used as a reason in analysis
(`mark_clause_as_used`) and is decremented by one at every reduce
(`collect_reducibles`), so it is a *recency* counter rather than a cumulative
use count. The `tier1`/`tier2` glue limits are recomputed from the observed
glue-usage distribution (`tier1relative = 50%`, `tier2relative = 90%`), so the
"good glue" threshold adapts to the instance, and the deleted fraction grows
with the number of reductions from about `reducelow = 50%` to
`reducehigh = 90%`.

`NogoodDb::uses` is cumulative and is never decayed, so it is not directly the
Kissat `used` counter. The database statistics show that the most-used learned
entries are short (3–18 literals, 220–386 uses, three of them 3-literal
clauses) while the length histogram is dominated by long clauses (mean 32.6 on
the deep benchmark), so a policy that protects short or recently-used entries
is the most direct candidate.

#### Candidate directions

This research list is kept as written. Only the recency policy (2) is
implemented; the rest are not. The directions are ordered by how much they
change the per-assignment hot path.

1. **Protected short-clause tier.** Keep the shortest entries (for example
   `literals.len() <= 4`, or a `uses`-ranked top fraction) in a reserved share
   of the capacity and evict only from the rest. The capacity is unchanged, so
   the bucket length and the per-assignment cost stay where the capacity sweep
   put them.
2. **Recency-based eviction.** Give each entry a decayed use counter in the
   style of the Kissat `used` field and evict the lowest counters instead of
   the oldest half. The extra sort happens at reduce time, which is dominated
   by the index rebuild. Implemented with the reduction epoch of the last use
   as the counter; see [Recency eviction A/B](#recency-eviction-ab-2026-09-20).
3. **Dynamic capacity.** Grow the capacity with reductions or search length,
   subject to the maintenance budget above, instead of fixing it to the world
   size. This changes the hot path and needs its own measurements.
4. **Glue/LBD analogue.** `World::analyze` knows the decision level of every
   literal of the learned clause, so the number of distinct levels is a natural
   analogue of LBD. Ranking by it, and keeping low-glue entries permanently as
   Glucose does, is the closest match to the reference solvers, but it needs a
   new per-entry field and a promotion rule.

#### Instrumentation for the retention question

None of the candidate policies can be chosen from the existing counters alone.
The database therefore records additional diagnostic state, all O(1) per
learned entry, per use, and per reduction:

- `NogoodStats::lbd_histogram` and `NogoodStats::used_lbd_histogram` are the
  distributions of the LBD analogue at learn time and at first use.
  `World::learn_analysis_nogood` computes the LBD from `World::cell_level` and
  passes it to `NogoodDb::learn`, which stores it in the private `Nogood`
  entry; it is diagnostic and never used by propagation or eviction.
- `NogoodStats::used_length_histogram` is the length distribution at first
  use, the right comparison for `length_histogram`: the raw use counts of the
  all-time record are biased toward short entries, which are one literal short
  of a match far more often, so only the learned-versus-used distributions
  separate "useful" from "frequently triggerable".
- `NogoodStats::reuse_distance_histogram` is the distribution of the number of
  database reductions between learning an entry and its first use
  (`NogoodDb::learned_epoch` against `NogoodStats::reductions`), binned by
  powers of two. Together with `evicted_unused` it estimates the retention
  need: how long an entry must survive to be reused at all.
- `NogoodStats::evicted_unused` and `NogoodStats::evicted_uses_total` separate
  the evicted entries that never pruned anything from the proven propagation
  work that a reduction throws away.
- `NogoodStats::relearned_evicted` counts the exact literal sets that were
  evicted and later learned again. `NogoodDb` remembers recently evicted
  hashes in a bounded set (`EVICTED_MEMORY_FACTOR` times the capacity, capped
  by `EVICTED_MEMORY_LIMIT`); `evicted_memory_flushes` counts how often the
  bound was hit, so the counter is a documented lower bound on the churn.
- `NogoodStats::fire_attempts` counts `NogoodDb::fire_candidate` evaluations,
  so `fired / fire_attempts` is the hit rate of propagation-level firing.
- `NogoodStats::learned_ready` counts entries that already started fully
  matched, the chronological-fallback case of the conflict analysis.

`NogoodDb` also keeps `last_used_epoch` per entry, updated in the single
`NogoodDb::note_entry_used` path that maintains `uses`, `used_learned`, the
first-use histograms, and the reuse distance. It is the primary ranking key of
the implemented recency eviction.

#### Measured retention snapshot (2026-09-20)

The counters above were measured on 2026-09-20 on the same development machine
as the benchmark snapshots below, with a release build, single runs per
command, and the first solution as the stopping condition (the instrumented
working tree is commit `31a6183` plus the uncommitted instrumentation diff).
The no-error runs use the default `new_state`; the `-n a` cases are marked.
The step counts are deterministic and reproduce the earlier snapshots exactly
(for example, the deep case is 26,178,710 steps at the automatic capacity
both before and after the new counters), which confirms that the
instrumentation does not change the search. Commands:

- `factoriosrc-tui new --no-tui --format json -r B3/S23 100 10 4 -x 1 --nogood`
  (automatic capacity, 19,584 entries) and the same with
  `--nogood-capacity 32768`;
- `factoriosrc-tui new --no-tui --format json -r B2n3/S23-q 30 9 4 -x 1 -n a --nogood`;
- `factoriosrc-tui new --no-tui --format json -r B3/S23 26 8 4 -y 1 -n a --nogood`.

| Case (capacity) | Steps | Wall | Learned | Used | Evicted unused | Relearned after eviction | First use in interval 0 / 1 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 100 10 4 -x 1` (19,584) | 26,178,710 | 401 s | 10,348,205 | 50.2% | 49.8% | 1.12% | 5,020,853 / 174,362 |
| `B3/S23 100 10 4 -x 1` (32,768) | 16,045,721 | 314 s | 6,376,268 | 50.0% | 49.9% | 1.04% | 3,090,421 / 95,474 |
| `B2n3/S23-q 30 9 4 -x 1 -n a` (5,632) | 2,161,350 | 17 s | 930,038 | 47.0% | 53.0% | 1.02% | 413,795 / 23,051 |
| `B3/S23 26 8 4 -y 1 -n a` (4,480) | 476,438 | 3 s | 201,275 | 49.4% | 50.6% | 1.24% | 93,967 / 5,455 |

*Used* and *Relearned after eviction* are shares of `learned`; *Evicted unused*
is the share of evicted entries that had never been used
(`evicted_unused / evicted`). *First use in interval 0 / 1* is the two
non-empty bins of `reuse_distance_histogram`; every other bin is zero.

The measurements answer the question the candidate list could not:

- The first use of an entry always happens in the reduction interval in which
  it was learned (about 97% of used entries) or in the immediately following
  one; no entry is first used later. The previous oldest-half policy already
  gave every entry enough life to be tried at least once, so an entry's
  *first* use did not need a new protection rule. The lifetime after the
  first use is what the policy controls.
- About half of the learned entries are never used before they are evicted:
  each reduction discards roughly half dead weight and half entries that had
  been doing work. There is real space to reclaim.
- `evicted_uses_total` is within a few percent of `fired + full_matches` on
  every case, so the database eventually evicts essentially all of the
  entries that were propagating, because `reduce` ignores `uses` and
  `last_used_epoch`. Exact-set relearning after eviction is about 1%, so the
  loss is not mainly re-derivation of the same absolute pattern; it is the
  replacement of the working set by fresh, half-unused entries.
- Quality predicts whether an entry is used at all, monotonically. On the
  deep automatic-capacity run, the share of learned entries that were used at
  least once falls from 95.0% at 1–8 literals to 17.5% at 65–96 literals, and
  from 73.4% at LBD 1–4 to 14.9% at LBD 25+. This is a first-use probability
  under a fixed, capacity-limited lifetime, not a measured step-count
  benefit; the length-bound sweep above remains the evidence that long
  clauses still carry pruning power.

#### Recency eviction A/B (2026-09-20)

The recency eviction replaced the oldest-half selection and was compared
against it on the same machine, release build, single runs, first solution
unless noted. The only difference between the two binaries is
`NogoodDb::reduce`. Commands are those of the retention snapshot above, plus
the exhaustive runs
`factoriosrc-tui new --no-tui --format json -r B3/S23 W W 2 -n a --no-stop --nogood`
for `W` in 5, 6, and 7.

| Case | Old steps | New steps | Change | Old wall | New wall |
| --- | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 100 10 4 -x 1` (19,584, first solution) | 26,178,710 | 20,286,918 | −22.5% | 401 s | 308 s |
| `B3/S23 100 10 4 -x 1` (32,768, first solution) | 16,045,721 | 16,773,101 | +4.5% | 314 s | 329 s |
| `B2n3/S23-q 30 9 4 -x 1 -n a` (5,632, first solution) | 2,161,350 | 2,061,555 | −4.6% | 17.3 s | 16.6 s |
| `B3/S23 26 8 4 -y 1 -n a` (4,480, first solution) | 476,438 | 442,437 | −7.1% | ~3 s | ~3 s |
| `B3/S23 64 64 1 -n a` (first solution) | 2,007 | 2,007 | 0 | 0.005 s | 0.007 s |
| `B3/S23 20 20 2 -n r --seed 1 --no-stop` (10th solution) | 16,612 | 20,097 | +21.0% | 0.100 s | 0.104 s |
| `B3/S23 6 6 2 -n a --no-stop` (exhaustion, 86 solutions) | 21,990 | 18,846 | −14.3% | — | — |
| `B3/S23 7 7 2 -n a --no-stop` (exhaustion, 9,538 solutions) | 823,888 | 653,103 | −20.7% | — | — |

The exhaustive rows are the strongest evidence: they count the work of
enumerating every solution, so they cannot be explained by a lucky traversal
path, and the solution counts are identical. The first-result deep case
improves at the automatic capacity while the explicit 32,768 override
regresses slightly; at that size the oldest-half policy's longer retention of
rare, cold entries is worth more than the extra lifetime of the hot ones, and
the automatic capacity is the default. At the default 19,584 entries the new
policy reaches the wall time of the old policy at 32,768 (308 s against 314 s)
with the smaller per-assignment cost. The `20 20 2 --seed 1` row regresses by
21% in steps, but it is one seed of a tiny absolute cost (0.1 s to the tenth
solution). The recency eviction therefore replaces the oldest-half policy as
the default. A short or low-LBD protected tier remains the follow-up
candidate, and dynamic capacity remains the least attractive direction
because it raises the per-assignment cost that the capacity formula
deliberately bounds.

An earlier activity/LBD-based eviction was tried before the capacity became
adaptive and was slower on the small workloads. Those workloads are not the
same regime: the recency policy above was measured after the capacity became
adaptive, it only changes which half is evicted, and it is a net win on the
exhaustive metrics. Any further change to the ranking (for example a low-LBD
tier) must keep the differential solution-set and solution-count tests and be
measured on both the small and the large benchmark worlds.

`NogoodStats` retains the useful instrumentation from these experiments:
`queries`, `capped_queries`, `literals_total`, `rejected_long`, `used_learned`,
`full_matches`, the histograms and eviction counters described above, in
addition to learning, firing, hit, and eviction counts. The database also
keeps a bounded all-time record of the most-used evicted entries, exposed with
their coordinates by `World::nogood_top()`. `World::nogood_stats()` and
`World::search_steps()` expose the counters in non-TUI JSON output, including
the new histograms. Non-TUI `--no-stop` also supports measuring the time to a
later solution.

### Propagation-level firing

Each database entry maintains a counter of literals whose cells currently hold
the recorded states, stored in the dense `NogoodDb::remaining` array. Real
`set_cell()` and `unset_cell()` operations update the counters through the
`(cell, state)` index.

- When exactly one literal is missing and its cell is unknown, the database
  forces that cell away from the recorded state. This is unit propagation on
  a learned nogood, and the forced assignment receives a `Clause` antecedent.
- When all literals match, a pending `Confl::Nogood` is queued. The match is
  revalidated when the pending conflict is consumed because the trail may
  have unwound in the meantime.
- Lookahead probes do not update the counters. Their rollback also runs in
  probe mode so that temporary assignments are excluded symmetrically.

The propagation path is important: a forbidden pattern can be completed by
deductions, not only by the final guess. A general scan before every guess is
not part of the current `guess()` path. `NogoodDb::completed()` is used when
chronological backtracking considers the opposite state of a two-state guess;
if that state would complete a stored nogood, the subtree is skipped.

If a forced assignment's clause antecedent becomes stale, conflict analysis
does not use it and falls back to chronological backtracking. This preserves
soundness when cells are later popped and assigned again.

### Usage instrumentation

The statistics separate the ways a learned entry can affect the search, so that
"how much of what was learned was useful" can be read off a run:

- `learned` counts the entries ever stored, `literals_total` and
  `length_histogram` describe their lengths in literals, and `rejected_long`
  counts the clauses refused by the length or repeated-cell guard.
- `used_learned` counts the distinct entries that fired or blocked at least
  once. It is cumulative and survives eviction, unlike `fired` and `hits`,
  which count events rather than entries.
- `fired` counts unit propagation on a learned nogood, `full_matches` counts a
  full match detected by `on_set()`, and `hits` counts a completed guess or
  chronological flip found by the `completed()` query.
- `evicted` and `reductions` show the churn of the bounded database. Because
  the live database is small, most learned entries are eventually evicted, so
  `NogoodDb::top_entries()` also remembers the most-used evicted entries in
  `HALL_OF_FAME`, and `World::nogood_top(n)` reports the all-time most-used
  patterns as `NogoodTop` values with `(x, y, t, state)` coordinates.
- `lbd_histogram`, `used_length_histogram`, `used_lbd_histogram`, and
  `reuse_distance_histogram` describe the quality of what is learned and what
  is actually used, and how long an entry must live to be used at all; see
  [Instrumentation for the retention question](#instrumentation-for-the-retention-question).
- `evicted_unused`, `evicted_uses_total`, `relearned_evicted`,
  `evicted_memory_flushes`, `fire_attempts`, `learned_ready`, and
  `bucket_updates` quantify the cost of the eviction policy, the maintenance
  work of the counters, and the effectiveness of propagation-level firing.

`World::search_stats()` reports the search work counters separately from the
database statistics: `steps`, `guesses`, `cell_sets`, `cell_unsets`,
`check_affected_calls`, `descriptor_checks`, `backtracks`, `analyses`,
`analysis_resolutions`, `analysis_literals`, `analysis_scanned`,
`queued_cells`, and the guard counters `guard_suspensions` and
`guard_resumes`. They are the basis of the
[cost guard](#cost-guard-for-the-experimental-machinery) and are included in
the non-TUI JSON output.

`World::search_steps()` is a configuration-independent work counter: the number
of internal search steps (a propagation round followed by a guess, a conflict,
or a backtrack) accumulated across `search()` calls. It is a better comparison
metric than `cells_checked()`, which is only the current stack depth.

The instrumentation is diagnostic and does not change the search: the same
`--nogood` runs produce the same solutions, the same `cells_checked()`, and the
same `search_steps()` with and without it. Its cost is O(1) per learned entry,
per firing, and per reduction. An interleaved A/B of the instrumented and
pre-instrumentation release builds on the development machine showed no
consistent difference (the 95% confidence interval of the paired wall-time
difference over 20 pairs included zero); the added work is estimated below 0.5%
of the runtime, while the wall-time noise of that machine is about an order of
magnitude larger. The instrumentation is therefore kept. That A/B predates the
retention counters; the LBD is computed once per learned clause from the
analysis data, the first-use histograms are updated once per entry, and the
recently-evicted hashes are inserted at a reduction that already hashes every
kept entry, so the additions stay on the same events rather than adding scans.

### Worked example: a trailing-boundary nogood

The all-time most-used entries of the deep `B3/S23 26 8 4 -y 1 -n a` run are
short clauses on the trailing boundary row `y = 0`. The second-ranked entry,

    (11, 0, 3) = Alive, (12, 0, 0) = Dead, (13, 0, 0) = Alive,

is an instance of a general two-step boundary lemma, and it can be derived by
hand without any SAT machinery. The derivation is a useful check on what the
database actually stores.

Setup. The configuration has `dx = 0`, `dy = 1`, so `World::canonicalize_coord()`
maps a generation-3 cell to generation 0 one row higher:

    successor(x, y, 3) = (x, y + 1, 0).

The cells with `y = -1` are padding and are fixed to the dead background. The
successor of the padding cell `(x, -1, 3)` is therefore the real cell
`(x, 0, 0)`.

Boundary lemma. For an interior `x`,

    (x+1, 0, 0) = Alive  iff  (x, 0, 3), (x+1, 0, 3), (x+2, 0, 3) are all Alive.

Proof. `(x+1, 0, 0)` is the successor of the dead padding cell `P =
(x+1, -1, 3)`. Of the eight neighbours of `P`, the `y = -2` cells are outside
the allocated world and the `y = -1` cells are padding; all are dead. The only
possibly-live neighbours are `(x, 0, 3)`, `(x+1, 0, 3)`, and `(x+2, 0, 3)`.
Because `P` is dead, `B3/S23` makes its successor alive exactly when `P` has
three live neighbours, i.e. when all three of those cells are alive. QED.

Derivation. Suppose all three literals of the learned entry hold:
`(11,0,3)=A`, `(13,0,0)=A`, `(12,0,0)=D`. Apply the lemma to the padding cell
`(13, -1, 3)`, whose successor is `(13,0,0)`: since that successor is alive, its
neighbours `(12,0,3)`, `(13,0,3)`, and `(14,0,3)` are all alive. Then the padding
cell `(12, -1, 3)`, whose successor is `(12,0,0)`, has the live neighbours
`(11,0,3)` (given), `(12,0,3)`, and `(13,0,3)`, so by the lemma its successor
`(12,0,0)` is alive — contradicting `(12,0,0)=D`. Hence the entry is a valid
nogood. In CDCL terms, the analysis resolved the two padding descriptors into
the implication `(11,0,3) ∧ (13,0,0) → (12,0,0)`, took `(12,0,0)` as the 1-UIP,
and stored its negation.

The lemma instantiated at `x = 10, 11, 12` appears at the top of the usage list
as separate absolute entries. The database does not normalize by translation, so
each instance is learned and stored independently.

The entry is not an artifact of the conflict analysis: the found 26×8 solution
satisfies the lemma. Evolving generation 0 gives
`gen3 row 0 = ooo.......oo............o.` and
`gen0 row 0 = .o........................`; for example `(1,0,0)` is alive because
`(0,0,3)`, `(1,0,3)`, and `(2,0,3)` are all alive. The forbidden pattern itself
does not occur in the solution (the solution has `(13,0,0) = Dead`), which is
why the entry prunes partial assignments rather than excluding the answer.

All five highest-use entries of this run touch row `y = 0` and the last
generation `t = 3` (the one that wraps around to generation 0), and none touches
row `y = 7`. This is not a coincidence:
the trailing padding row feeds row 0 across the period boundary, so the
strongest boundary lemmas are anchored there. The consequence for
relative-coordinate learning is discussed under
["Validity boundary for future translated nogoods"](#validity-boundary-for-future-translated-nogoods).

### Validity boundary for future translated nogoods

Relative-coordinate or cross-size learning is not implemented. Any future
version must prove that a learned entry does not depend on facts that change
under translation or resizing, including:

- padding cells, user-known cells, diagonal-width boundaries, and the
  background baked into incomplete descriptors;
- the B0 background phase and the meaning of an empty cell;
- absolute symmetry mappings whose coordinates depend on world dimensions;
  and
- assumptions that were treated as level-0 facts during conflict analysis.

It must also define how rule configuration, transformation, and pattern
symmetry participate in the identity of a reusable entry. The current exact
position database intentionally makes none of these claims.

The worked example above shows the boundary issue in a structured form. The
most-used entries of the deep benchmark are horizontal translations of one
lemma, anchored at the trailing row `y = 0`, where the padding row feeds
generation 0 across the period boundary. Such an entry is invariant under a
horizontal translation as long as the translation keeps the whole literal set
inside the interior, but it is not invariant under a vertical translation:
moving it away from `y = 0` replaces the padding neighbours of the anchor with
ordinary cells, and moving it toward the padding changes the neighbour set. A
future translated database therefore needs at least the distance (and, for
`dx != 0` or diagonal drift, the direction) to the nearest padding row or column
in the identity of an entry, or it must refuse to translate entries that touch a
boundary at all. The same reasoning applies to the `x` padding columns and to
the diagonal-width boundary.

The `t` coordinate needs a similar decision. The learned boundary entries live
at `t = 3` and `t = 0`, so translating them vertically by one row is equivalent
to rotating the generations by one step, which changes which generation is
anchored as generation 0 and therefore interacts with the front optimization.
Generation rotation is a re-encoding of the same pattern, not a free reuse, so
the current exact-position design correctly treats it as distinct.

### Correctness status

The repository tests exercise learning and propagation directly and compare
the solution sets of default and nogood-enabled searches. The covered cases
include ordinary, non-totalistic, B0, symmetry, transformation,
max-population, `reduce_max_population`, save/load, world growth, and feature
combinations. These tests establish the checked configurations, not the
performance of the database or the safety of a future translated mode.

## Cost Guard for the Experimental Machinery

Conflict analysis, the backjump re-check, and the nogood database are
experimental machinery whose cost is not bounded by the search itself. On
some rules the machinery costs far more than the pruning it provides, and
without a bound the feature can be orders of magnitude slower than the plain
search. This section records the measured cost profile and the guard that
bounds it.

### Measured cost profile

`SearchStats` counts the search work: the queued cells of each propagation
round (`queued_cells`), the descriptors checked (`descriptor_checks`), and the
work of conflict analysis (`analyses`, `analysis_resolutions`,
`analysis_scanned`). `NogoodStats::bucket_updates` counts the index bucket
entries visited by the incremental counter maintenance. Profiling with `perf`
on the factorio rule (`R3,C2,S2,B3,N+`, radius 3) showed:

- the plain search processes one queued cell per step;
- with `--backjump`, the re-check after a backjump restarts the propagation
  queue far below the top of the stack, and the search processes about 1,200
  to 1,700 queued cells per step (about 22,000 descriptor checks per step
  against 137 in the plain search);
- with `--nogood` and chronological backtracking, about 93% of the time is
  `NogoodDb::on_set`/`on_unset`, because the capacity formula makes the
  average index bucket grow with the average clause length, and the factorio
  rule learns long clauses;
- conflict analysis itself is below 2% of the work in both cases, so it is
  not the cost driver.

The machinery work measure (`queued_cells + analysis_scanned +
bucket_updates`) on the ungated `--nogood` is about 26,000 operations per
step over the first million steps of the factorio rule (1,664 queued cells,
184 scanned, 24,245 bucket updates) and grows to about 34,000 as the database
fills. The deep Life benchmark (`B3/S23 100 10 4 -x 1`) has the same shape:
about 473 queued cells, 73 scanned descriptor cells, and 16,500 bucket
updates per step, about 17,000 machinery operations in total, against 40
descriptor checks per step in the plain search. The 64x64 period-1 case is
the opposite regime: 137 machinery operations per step in total, and it
finishes in 2,007 steps.

### The guard

`World` keeps a `Guard` (in `lib/src/world.rs`) that bounds the machinery work
`queued_cells + analysis_scanned + bucket_updates` per search step. Every
`GUARD_INTERVAL` (1,024) steps it compares the work of the last interval with
`GUARD_BUDGET` (512) per step; when the work exceeds the budget, the guard
suspends the machinery:

- conflicts are handled by chronological backtracking instead of
  `World::analyze`, so no new clauses are learned;
- the nogood database is set inactive (`NogoodDb::set_active`), so it neither
  propagates nor maintains its counters, and a pending nogood conflict is
  dropped.

The suspension lasts `GUARD_SUSPENSION` (2^18) steps, doubled for each
consecutive suspension up to `GUARD_MAX_BACKOFF` (6) and decayed after a
clean interval; a suspended machinery is re-probed after the cooldown. On
resume, `NogoodDb::resync` rebuilds the unmatched-literal counters from the
world state, because they were not maintained while the database was
inactive.

The guard is deterministic (its decisions depend only on the work counters,
not on wall time) and it is heuristic state: it is not serialized, and a
loaded world starts with the machinery active. It is opt-in via
`Config::nogood_guard` (`--nogood-guard`), which enables the nogood database
and therefore backjumping; without the flag the machinery always stays active
and `--nogood` alone behaves exactly as it did before the guard was
introduced. Short searches that finish before the first evaluation keep the
full machinery, which preserves the 64x64 win. A budget of 512 separates the
measured regimes: the 64x64 period-1 search does about 137 machinery
operations per step and keeps the machinery, while the deep benchmark (about
17,000, dominated by bucket maintenance) and the factorio rule (about 26,000
to 34,000) suspend it.

`World::search_stats()` exposes the guard counters (`guard_suspensions`,
`guard_resumes`) together with the work counters, and the non-TUI JSON output
includes them.

#### Parameter selection

The constants are calibrated on the measured workloads, not derived from
first principles:

- `GUARD_BUDGET = 512` machinery operations per step is a policy threshold
  between the cheap regime (64x64: about 137 operations per step) and the
  expensive ones (deep: about 17,000; factorio: about 26,000 to 34,000). The
  plain search does 40 to 140 descriptor checks per step, so the budget lets
  the machinery run while its work stays of the same order as the baseline
  propagation. It is a coarse cost proxy, not a wall-time estimate.
- `GUARD_INTERVAL = 1024` steps is the evaluation granularity. It averages
  out single-step noise (one deep backjump can be arbitrarily expensive) and
  bounds a re-probe of suspended machinery to about a thousand expensive
  steps, so a probe cannot become the cost the guard is meant to bound. It
  was 4096 initially; the shorter interval reduced the probe overhead
  measurably on the small workloads.
- `GUARD_SUSPENSION = 2^18` steps is the base suspension. The first
  suspension is `2^18 << 1` steps, which is about 0.1 to 0.4 seconds of plain
  search at the measured rates, so probes are a small fraction of the time;
  the backoff doubles up to `2^18 << 6` steps so a persistently unprofitable
  regime is probed rarely.

The overspending test is absolute rather than relative to the plain search:
every `GUARD_INTERVAL` steps, the guard compares the machinery work added
since the last evaluation with `GUARD_BUDGET * interval`. A clean interval
advances the baseline and decays the backoff, so the machinery is re-probed
sooner once it has become cheap.

### Measured effect (2026-09-22)

Measured on the same development machine as the other snapshots, release
build, single runs, first solution unless noted. The *no guard* column is
`--nogood` alone with the recency eviction, i.e. the pre-guard behavior. The
large factorio case is a 60-second sample because the plain search takes
about 841 seconds and the point is the rate, not the solution.

| Case | Plain | `--nogood --nogood-guard` | `--nogood` (no guard) |
| --- | ---: | ---: | ---: |
| `B3/S23 64 64 1 -n a` (first) | >90 s | 2,007 steps / 0.009 s | 2,007 / 0.007 s |
| `B3/S23 100 10 4 -x 1` (first) | 235,633,435 / 50.2 s | 235,441,747 / 56.1 s | 20,286,918 / 308 s |
| `R3,C2,S2,B3,N+ 100 17 3 -x 2 -s D2- -n a` (60 s sample) | 80,000,000 steps | 80,000,000 steps | 1,000,000 steps |
| `R3,C2,S2,B3,N+ 20 8 4 -x 2 -s D2- -n a` (exhaustion) | 150,737 / 0.106 s | 150,121 / 0.122 s | 64,035 / 0.87 s |
| `B3/S23 26 8 4 -y 1 -n a` (first) | 6,627,619 / 1.28 s | 5,238,929 / 1.22 s | 442,437 / 3.0 s |
| `B2n3/S23-q 30 9 4 -x 1 -n a` (first) | 17,226,567 / 4.30 s | 17,184,405 / 4.80 s | 2,061,555 / 16.6 s |
| `B3/S23 7 7 2 -n a --no-stop` (exhaustion, 9,538 solutions) | 3,539,579 / 0.64 s | 3,530,540 / 0.72 s | 653,103 / ~4 s |

The guard removes the catastrophic regime: on the large factorio rule the
search runs at the plain rate (about 1.27M steps per second) instead of about
21k, and the deep benchmark finishes within 12% of the plain wall time
instead of 6 times slower. The step reductions that `--nogood` produced on
the deep, small, and INT benchmarks are mostly given up, because they were
wall-time losses; the small case is even slightly faster than plain because
the short active phase improves the traversal. The 64x64 win is preserved
because that search finishes in 2,007 steps, before the first budget
evaluation.

The guard is opt-in, so the `--nogood` column of the [benchmark
snapshot](#benchmark-snapshot) and the pre-guard numbers in the table above
still describe the default behavior of `--nogood` alone; the guard only
changes a run when `--nogood-guard` is passed.

### Correctness status

`test_guard_suspension_preserves_solutions` forces a zero budget so that the
guard suspends and re-probes repeatedly, and checks that the solution sets
match the search with the machinery always active; it enables the guard with
`Config::nogood_guard`. `test_guard_disabled_by_default` checks that a zero
budget does not suspend anything without the flag. The existing differential
tests cover the same configurations with the guard off.

## Branching Heuristics

### Phase saving

`Config::phase_saving` is an opt-in heuristic. When enabled, `LifeCell::phase`
records the last real state assigned to the cell, whether by a guess, a
deduction, or initial configuration. When the cell is guessed again, that
state is tried first; if no phase exists, `Config::new_state` is used.

Unsetting a cell does not clear its phase. Lookahead assignments are temporary
and do not update it. Consequently, with both options enabled, lookahead
chooses the polarity for the next cell before phase saving is consulted.

On save/load, phases of cells restored by replaying the stack are rebuilt, but
phases of cells that had already been unset are lost. This changes only the
heuristic. Repository tests cover finding solutions, solution-set equality,
and the save/load option behavior for phase saving.

### Activity-based cell selection

`Config::activity` is an opt-in branching heuristic inspired by the VSIDS
activity heuristic of SAT solvers. `World` keeps an activity value per cell. A
conflict bumps the cells that participated in it: an analyzed conflict bumps
the 1-UIP cell and the literals of the learned clause in `World::analyze()`,
while a conflict handled by chronological backtracking bumps its seed cells in
`World::bump_conflict()`. The bump amount grows geometrically after each
conflict and is rescaled when it becomes too large, so that recent conflicts
weigh more.

`World::guess()` normally follows the `next` chain. With activity enabled, it
scans a fixed window (`ACTIVITY_WINDOW`, currently 8) of the next unknown cells
of the chain and guesses the one with the highest activity; ties keep the chain
order, and a search where no cell has any activity yet behaves exactly like the
default. Keeping the window small is deliberate: the chain order is aligned
with the front optimization and with the locality of descriptor propagation,
and replacing it globally is not automatically a win. The front cells remain
near the window head, but a sufficiently active later cell can still be chosen
before them, so the interaction with `init_front()` is a measurement question.

Because the branching cell may be later in the chain than the earliest unknown
cell, the cursor cannot simply advance to `cell.next` after a guess. The search
keeps the cursor at the earliest unknown cell and, in
`World::backtrack()` and `World::analyze()`, moves it to the chain-earliest
cell that becomes unknown again, using `World::chain_pos` (which is now
allocated when either `backjump` or `activity` is enabled).

The activity is heuristic state, so it is not serialized: save/load and
`increase_world_size()` restart it from zero, like the phases of cells that have
already been unset. Repository tests cover finding solutions, solution-count
and solution-set equality for two-state and Generations rules, and combinations
with backjump, nogood, phase saving, and lookahead.

## Lookahead

`Config::lookahead` is an opt-in polarity-selection experiment for two-state
rules. `World::probe()` examines the next unknown cell from the current search
chain:

1. Temporarily assign `Dead`, propagate, and record whether a conflict occurs
   and how much work was produced.
2. Roll back the complete probe.
3. Repeat for `Alive`.
4. If one probe conflicts, choose the other state. If both conflict, report a
   conflict to ordinary backtracking. If neither conflicts, choose the state
   with more propagation; ties choose `Dead`.

Propagation is bounded by `MAX_PROBE_DEDUCTIONS` (`256`). The existing
set/unset and stack machinery performs the rollback. Probe assignments are
excluded from phase-saving history and nogood counters.

`Config::check()` rejects lookahead for Generations rules; it is not silently
skipped for them. The experiment does not select a better variable, and it
does not change the correctness of the search. Repository tests cover
solution-set equality, max-population behavior, save/load, and combinations
with the other implemented options.

The precomputed rule table already performs a form of failed-literal pruning
inside one descriptor. Lookahead adds one bounded level of runtime probing,
but its fixed cost per branch means that it should remain an opt-in heuristic
until reproducible benchmarks show a useful regime.

## Other Proposed Directions

These ideas are deliberately kept separate from the implemented code paths.

### Consistency across overlapping descriptors

Adjacent descriptors share cells and successor relationships. Checking pairs
of descriptors could derive facts that neither local table derives alone, an
analogue of stronger local consistency in constraint programming. Full
precomputation is too large; an on-demand check with a bounded cache is a
possible experiment. Generations' deterministic dying chains could also be
compressed as a preprocessing step, but the state asymmetry must be preserved.

The boundary lemma in the worked example is a concrete instance: it composes the
descriptors of two adjacent padding cells into a fact that neither descriptor
implies alone. The current search only discovers such facts through conflict
analysis; a bounded pair-check could derive them directly.

### Boolean or multi-valued learning for Generations

Possible approaches include learning only in the dead/alive base layer,
eliminating deterministic dying states, or using one-hot variables with
exactly-one constraints. None is implemented, and each must preserve the
current `Reason::TryAnother` enumeration semantics.

### Restarts

Restarts are primarily useful for finding one solution. For enumeration they
repeat work unless combined with a sound persistent memory such as translated
nogoods, and they complicate save/load and incremental world growth. They are
not a current priority.

### Other comparisons

Component caching, cube-and-conquer, and a direct CNF encoding remain useful
research directions. A CNF encoding could provide an external baseline, but it
would be a comparison tool rather than a drop-in replacement for the
descriptor-based search; see the LLS comparison below for a first measurement.
Row-by-row searchers such as `qfind` are a separate
algorithmic direction and are outside this note.

## Correctness and Maintenance

`Config::check()` and `Config::parse_rule()` in `lib/src/config.rs` are the
source of truth for supported rules and feature validation. In particular,
they determine the two-state restrictions and make `nogood` imply
`backjump`. Update this document after changing those checks, not before.

The implementation and tests relevant to this note are concentrated in:

- `lib/src/config.rs`: options, rule support, and validation;
- `lib/src/search.rs`: propagation, branching, probing, backtracking, and
  conflict analysis;
- `lib/src/world.rs`: cells, trail metadata, global counters, save/load, and
  integration tests;
- `lib/src/cell.rs`: `Reason` and `Antecedent`; and
- `lib/src/nogood.rs`: the exact-position database and its unit tests.

When checking a change:

- compare the **number and sets** of enumerated solutions with the default
  search;
- include B0/background behavior, symmetry and transformation, population
  bounds, `reduce_max_population`, and option combinations when the change
  affects learning or backtracking;
- remember that save/load and `increase_world_size()` intentionally discard
  learned nogoods, the original conflict-analysis metadata, and the activity
  values; and
- run Miri for unsafe search-internal changes as specified in `AGENTS.md`.

Use file paths and symbol names in this note instead of line numbers. When an
implementation detail changes, update the status table and the relevant
section together. Do not describe an experiment as a performance improvement
without a reproducible command, commit, build profile, environment, stopping
condition, and result.

## Benchmark Snapshot

This is the latest recorded snapshot, measured on 2026-09-15 with a release
build, single runs, and a 60-second per-cell timeout (120 seconds for the
listed combinations). The `--activity` column was measured on 2026-09-16 on
the same machine and with the same protocol. The `--nogood` column was
re-measured on 2026-09-18 on the same machine after the database capacity
became adaptive (`max(2048, 4 * cells)` entries, 96-literal bound); for
enumeration, the value is the time to the 10th solution with `--no-stop`. That
column predates the recency eviction and the cost guard; the re-measured rows
are in [Recency eviction A/B](#recency-eviction-ab-2026-09-20) and
[Measured effect](#measured-effect-2026-09-22), and the whole column needs a
rerun. Replace this table on a future rerun instead of appending another
historical table.

| Case | Plain | `--phase-saving` | `--lookahead` | `--backjump` | `--nogood` | `--activity` |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 26 8 4 -y 1 -n a` | 1.227 s | 3.752 s | 5.665 s | 36.538 s | 3.053 s | 1.520 s |
| `B3/S23 64 64 1 -n a` | >60 s | 1.854 s | 0.049 s | 0.026 s | 0.006 s | 0.026 s |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` | 2.269 s | 3.208 s | N/A | N/A | N/A | 2.523 s |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2- -n a` | >60 s | 13.622 s | 30.922 s | >60 s | >60 s | ~60 s |
| `B2n3/S23-q 30 9 4 -x 1 -n a` | 4.154 s | 3.498 s | N/A | >60 s | 16.404 s | 2.685 s |
| `B3/S23 20 20 2 -n r --seed 1 --no-stop` to 10th solution | 5.716 s | N/A | N/A | N/A | 0.090 s | 2.114 s |

On the deep `26 8 4` case the combined options `--nogood --phase-saving` and
`--nogood --lookahead` take 3.450 s and 9.240 s with the adaptive capacity
(9.189 s and 8.180 s with the old fixed 2,048-entry database), while
`--backjump --phase-saving` still exceeds 120 seconds. The factorio `--activity` run
finishes just under the timeout (about 59.7 s) and is recorded as `~60 s`. The
enumeration guard makes solution counts match the plain search; `B3/S23 5 5 2`
reports 26 solutions with backjumping and nogood.

### Search-step and nogood-usage snapshot

This second snapshot was measured on 2026-09-17 on the same machine, with the
usage instrumentation of `World::search_steps()`, `NogoodStats`, and
`World::nogood_top()` added to the working tree. The `--nogood` column and its
database statistics below were re-measured on 2026-09-18 after the adaptive
capacity became the default, and they also predate the recency eviction and
the cost guard; see [Recency eviction A/B](#recency-eviction-ab-2026-09-20)
and [Measured effect](#measured-effect-2026-09-22). The commands are
`factoriosrc-tui new --no-tui --format json [-–backjump|--nogood] ...` in a
release build; `--nogood` also prints the database statistics. These are single
runs, but the step counts of the fixed-`--new-state` searches are
deterministic, so only the wall times are noisy (they varied by up to a factor
of two between runs on the loaded machine, so they are not tabulated here).

| Case | Plain steps | `--backjump` steps | `--nogood` steps |
| --- | ---: | ---: | ---: |
| `B3/S23 26 8 4 -y 1 -n a` | 6,627,619 | 13,606,085 | 476,438 |
| `B3/S23 64 64 1 -n a` | >100 s | 2,912 | 2,007 |
| `B2n3/S23-q 30 9 4 -x 1 -n a` | 17,226,567 | >100 s | 2,161,350 |
| `B3/S23 16 6 3 -y 1 -n a` | 23,237 | 18,619 | 9,283 |
| `B3/S23 17 12 3 -y 1 -s D2\| -n a` | 321,038 | 346,485 | 57,550 |
| `B3/S23 6 6 2 -n a --no-stop` (exhaustion) | 60,501 | 53,128 | 21,990 |
| `B3/S23 5 5 2 -n a --no-stop` (exhaustion) | 4,649 | 4,506 | 2,722 |

The `--backjump` column confirms the known limitation: without the persistent
database, the analysis discards its clauses at every backtrack, and the deep
`26 8 4` case takes about twice as many steps as plain chronological search.
`--nogood` cuts the steps by an order of magnitude on the deep and INT cases
and by a smaller factor elsewhere. On `B2n3/S23-q 30 9 4` the smaller step
count still costs more wall time than plain because the database churns
heavily, which is the regime the status table warns about.

The database statistics of the same runs show that learning is useful but
mostly short-lived:

| Case | Learned | Used (distinct) | Fired | Full matches | Evicted | Reductions | Avg len |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 26 8 4 -y 1` | 201,275 | 99,468 (49%) | 526,966 | 5,369 | 197,120 | 88 | 32.6 |
| `B3/S23 64 64 1` | 101 | 36 (36%) | 77 | 0 | 0 | 0 | 11.9 |
| `B2n3/S23-q 30 9 4 -x 1` | 930,038 | 437,064 (47%) | 2,165,836 | 19,402 | 926,464 | 329 | 36.9 |
| `B3/S23 16 6 3 -y 1` | 4,161 | 1,438 (35%) | 3,543 | 35 | 3,072 | 3 | 21.1 |
| `B3/S23 17 12 3 -y 1 -s D2\|` | 23,996 | 11,517 (48%) | 51,077 | 322 | 22,344 | 14 | 36.8 |
| `B3/S23 6 6 2 --no-stop` | 8,643 | 3,614 (42%) | 19,301 | 224 | 7,168 | 7 | 17.5 |

The live database is bounded by the adaptive capacity, so on the deep cases
only a few thousand entries exist at once and roughly 98% of everything
learned is eventually evicted; `used_learned` and the all-time `HALL_OF_FAME`
record are what make the usage numbers meaningful after those reductions. The
guess-time `hits` and `queries` counters stay at zero on these first-result
runs: the pruning comes from propagation-level `fired` and from `full_matches`,
not from the `completed()` query. The length histogram of the deep case is
broad (mode 12 literals, mean 32.6, a tail to the 96-literal bound, and a
secondary bump around 64–67), while the all-time most-used entries are short:
the top entries have 3–18 literals and 220–386 uses each, and three of them
are 3-literal clauses. Short learned clauses are rare but fire far more often
than the long ones that dominate the histogram.

## Comparison with Logic Life Search (LLS)

[Logic Life Search](https://gitlab.com/OscarCunningham/logic-life-search) (LLS)
is the reference tool for the "encode the search as CNF and run an off-the-shelf
SAT solver" approach (the CNF-encoding idea listed above). To check whether the
experimental options here are chasing the right target, LLS was run on the
tutorial examples and on the two-state rows of the benchmark table, and
factoriosrc was run on the tutorial examples. LLS version: commit `ecf6c24`
(master), Python 3.14.7, kissat 4.0.4, default settings except
`--background vacuum` (all rules involved are non-`B0`, so the vacuum background
matches factoriosrc's dead-outside semantics). Machine, toolchain, single-run
policy, and the 60-second hard timeout are the same as in the protocol above.
Both the wall time of the whole process and LLS's own `Total solver time` (the
SAT solve only) are recorded.

### How LLS models the search differently

When searching for a period-`p` pattern with a per-period displacement, LLS
creates `p + 1` generations in a fixed width × height box and constrains
generation `p` to equal generation 0 translated by the displacement.
factoriosrc instead creates `p` generations and maps the successors of the last
generation's cells onto generation 0 at the translated position (via
`canonicalize_coord`). The two encodings describe the same periodic space-time
patterns, but the boundary handling differs:

- LLS forces the cells of generation 0 whose translated image leaves the box to
  the background, so the phase-0 pattern cannot touch the edge toward which it
  drifts. factoriosrc's wrap-around only forces the opposite edge of the last
  generation to die out; its 26×8 solution below indeed has live cells on the
  leading edge, which LLS's encoding forbids. The same nominal box is therefore
  *not* the same instance, and same-box numbers slightly favor factoriosrc.
  LLS was additionally given one extra row or column of margin in the drift
  direction, where noted, and still timed out.
- LLS has no analogue of factoriosrc's non-empty-front constraint. For the
  64×64 period-1 case this matters: LLS accepts the trivial all-dead pattern,
  while factoriosrc excludes it by construction.
- Notation otherwise matches: LLS's `p3 x0 y1` displacement convention
  corresponds to factoriosrc's `-y 1` up to mirroring, and LLS's `D2|` symmetry
  is spelled the same way on factoriosrc's CLI. The tutorial's 16×6 and 17×12
  boxes worked in both tools without size adjustments.

### Results

| Search problem | LLS (wall / solver) | factoriosrc | Notes |
| --- | ---: | ---: | --- |
| Tutorial 1: 25-cell c/3 ship, `B3/S23`, 16×6 box | 1.140 s / 0.799 s | 0.009 s | `lls -c -b 16 6 -s p3 x0 y1`; factoriosrc `B3/S23 16 6 3 -y 1 -n a`. Both found the tutorial's 25-cell ship. The tutorial reports 1.7 s with an older solver. |
| Tutorial 2: mirror-symmetric c/3 ship, `B3/S23`, 17×12, `D2\|` | 15.698 s / 15.313 s | 0.105 s | `lls -c -b 17 12 -s p3 x0 y1 -s "D2\|"`; factoriosrc `B3/S23 17 12 3 -y 1 -s D2\| -n a`. First solutions depend on the solver: 62 cells here (12.2 s on a second run), ~69 cells in the tutorial, 34 cells for factoriosrc. The tutorial reports 57.5 s. |
| `B3/S23 26 8 4 -y 1 -n a` (c/4 ship) | >60 s (timeout); a follow-up run without the timeout solved it in ~587 s wall / 586.6 s solver | 1.245 s (plain) | `lls -c -b 26 8 -s p4 x0 y1`; also >60 s with `-b 26 9`. LLS encoding: 1,015 variables, 116,442 clauses; its 53-cell ship differs from factoriosrc's edge-touching 57-cell one. |
| `B3/S23 64 64 1 -n a` (period 1) | 4.07 s / 0.092 s, but the solution is the all-dead pattern; with `-p ">=100"`: >60 s (a dry run did not even finish encoding within 180 s) | >60 s (plain); 0.014–1.854 s with experimental modes | LLS has no non-empty requirement; the ≥100-population attempt died in LLS's cardinality encoding, not in the solver. |
| `B2n3/S23-q 30 9 4 -x 1` (INT c/4 ship) | >60 s (timeout; also >60 s with `-b 31 9`) | 4.213 s (plain) | `lls -c -b 30 9 -s p4 x1 y0 -r B2n3/S23-q`; encoding: 1,342 variables, 416,480 clauses. |

### Observations

- On the tutorial-scale instances both tools succeed, and factoriosrc is around
  two orders of magnitude faster in wall time. Part of the gap is structural:
  LLS regenerates the CNF in Python on every run (roughly 0.3–0.5 s even for
  ~35–43k clauses, and ~4 s for the 64×64 instance's 489k clauses), while
  factoriosrc's precomputed rule tables need no per-instance encoding. The
  solver-only times still favor factoriosrc on these instances.
- On the larger windows measured here, kissat did not solve within the 60-second
  protocol while factoriosrc's plain chronological search solved in ~1.3 s. The
  one instance LLS solved outside the protocol took ~587 s of solver time
  against factoriosrc's 1.245 s. For this project's first-result search regime,
  a direct CNF encoding with a state-of-the-art CDCL solver is far behind the
  specialized search: the n-ary transition constraints inflate the clause count
  (416k clauses for 1,071 undetermined cells on the INT rule), and the generic
  encoding cannot exploit the descriptor propagation or the front optimization.
- This does not mean the CDCL experiments here are pointless. The benchmark
  table above shows the opposite regime: on very large, shallow searches such as
  the 64×64 period-1 case, factoriosrc's own backjumping and nogood database
  turn a >60 s search into milliseconds — the same situation in which generic
  CDCL excels. The gap to LLS is in the encoding and the propagation structure,
  not in the value of conflict-directed search itself.
- Caveats: single runs of first-result searches; instances at the same nominal
  box are not identical (see the boundary-handling difference above); LLS and
  factoriosrc support different feature sets (partial rules, Generations,
  higher-range neighborhoods, search-order control), so the comparison covers
  only the overlapping subset; and kissat's heuristics are tuned for hard
  unsatisfiability proofs, which these satisfiable first-result searches are not.
