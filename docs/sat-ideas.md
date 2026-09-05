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
four experimental switches are available from the CLI as
`--phase-saving`, `--lookahead`, `--backjump`, and `--nogood`; the TUI and egui
frontends expose the same options.

| Technique | Current status | Scope and important limits |
| --- | --- | --- |
| Local propagation and chronological search | Implemented by default | The baseline search uses the fixed `next` chain, incremental descriptors, and chronological backtracking. |
| Phase saving | Implemented, opt-in | Remembers the last real state of a cell and tries it first. Works with supported two-state and Generations rules. |
| Lookahead | Implemented, opt-in | Probes both states of the next cell and chooses a polarity. Two-state rules only; it does not choose a different cell. |
| Conflict analysis and backjumping | Implemented, opt-in | A 1-UIP-style analysis for local rule, symmetry, and learned-nogood conflicts. Two-state rules only. |
| Exact-position nogood database | Implemented, opt-in | Learns from successful local conflict analysis and propagates learned forbidden patterns. Nogoods use absolute cell indices and are valid only in the current `World`. Two-state rules only. |
| VSIDS-style activity | Not implemented | The current branching cell still comes from the fixed search-order chain. |
| Translated or cross-size nogoods | Not implemented | The current database is not normalized to relative coordinates. |
| Dynamic cell selection for lookahead | Not implemented | Lookahead only changes the state tried for the next cell. |
| Restarts, component caching, and CNF encoding | Not implemented | These remain possible future experiments, not current search modes. |

`--nogood` enables `backjump` implicitly in `Config::check()`. `lookahead`,
`backjump`, and `nogood` are rejected for Generations rules because their
current reasoning is defined only for the two-state layer. Phase saving has no
such restriction.

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
Raw solution counts are not always a suitable differential oracle because the
search can report equivalent encodings, such as generation rotations, more
than once.

### Unsafe hot paths and persistence

`LifeCell` and the graph of raw pointers in `World` are performance-sensitive
unsafe code. Changes to `lib/src/cell.rs`, `lib/src/world.rs`, or
`lib/src/search.rs` require the Miri check described in `AGENTS.md`.

The serialized `World` stores the configuration, ordinary search stack, and
visible search state, but not `TrailMeta`, decision-level arrays, or the
nogood database. Loading replays stack assignments without reconstructing the
original antecedent graph. Phase history for unset cells is also not
serialized. This affects performance and heuristic state, not the intended
semantics of a completed search.

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
- whether the entry is a decision carrier; and
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
has exactly one reasonless decision entry.

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
4. Pop the trail to the highest decision level represented by the remaining
   literals.
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

### Correctness status

The repository contains explicit solution-set and invariant tests for
backjumping, including:

- ordinary, non-totalistic, B0, symmetry, and transformation configurations;
- deeper searches and max-population searches;
- `reduce_max_population`;
- the backjump trail metadata invariant; and
- combinations with lookahead and the nogood database.

The comparison oracle is the set of serialized solutions, not the number of
times the search happens to reach them. These tests establish the behavior of
the checked configurations; they are not an exhaustive configuration matrix
and they do not establish a performance improvement. Backjumping remains
opt-in because conflict analysis can cost more than chronological search when
learned information is not retained across backtracking.

## Exact-Position Nogood Learning

The nogood database is the current implementation of clause-learning memory.
It is enabled by `Config::nogood` and automatically enables backjumping.

### Representation and lifetime

A learned entry is a bounded list of `(absolute cell index, state)` literals.
The first literal is the rejected state of the 1-UIP cell; the remaining
literals are the states in the learned clause. The database indexes every
literal so that a matching entry can be found without scanning all entries.

The current implementation deliberately uses absolute indices:

- it does not store relative coordinates;
- it does not canonicalize by translation, symmetry, or generation;
- it cannot transfer entries between positions or world sizes; and
- it is cleared when a world is loaded or rebuilt, including
  `increase_world_size()`.

The database is persistent only across backtracking inside one `World`. It
does not persist across save/load or world growth.

The current implementation constants are intentionally modest and bounded:

| Limit | Current value | Purpose |
| --- | ---: | --- |
| Database capacity | `1 << 16` entries | When full, the older half is evicted and the index is rebuilt. |
| Literals per nogood | `16` | Avoids indexing very large learned patterns. |
| Candidates checked by one indexed query | `64` | Bounds work for a popular `(cell, state)` bucket. |

These are implementation limits, not correctness assumptions. Missing a
candidate because of the query cap loses pruning but must not change the
solution set.

### Propagation-level firing

Each database entry maintains a counter of literals whose cells currently hold
the recorded states. Real `set_cell()` and `unset_cell()` operations update
the counters through the `(cell, state)` index.

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

### Correctness status

The repository tests exercise learning and propagation directly and compare
the solution sets of default and nogood-enabled searches. The covered cases
include ordinary, non-totalistic, B0, symmetry, transformation,
max-population, `reduce_max_population`, save/load, world growth, and feature
combinations. These tests establish the checked configurations, not the
performance of the database or the safety of a future translated mode.

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

### Activity-based variable selection

VSIDS-style activity is not implemented. The current `guess()` always follows
the `next` chain. A future activity heuristic must be evaluated against the
front optimization and the locality of descriptor propagation; replacing the
spatial order globally is not automatically a win. A constrained local
reordering or an explicit experimental mode would be safer first steps.

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

- compare the **sets** of enumerated solutions with the default search rather
  than relying only on raw counts;
- include B0/background behavior, symmetry and transformation, population
  bounds, `reduce_max_population`, and option combinations when the change
  affects learning or backtracking;
- remember that save/load and `increase_world_size()` intentionally discard
  learned nogoods and the original conflict-analysis metadata; and
- run Miri for unsafe search-internal changes as specified in `AGENTS.md`.

Use file paths and symbol names in this note instead of line numbers. When an
implementation detail changes, update the status table and the relevant
section together. Do not describe an experiment as a performance improvement
without a reproducible command, commit, build profile, environment, stopping
condition, and result.

## Benchmark Table

### Measurement protocol

The results below were collected on 2026-09-05 from a release build of the
current working tree:

- Base revision: `6c097b4`, with the dependency refresh already present in
  `Cargo.lock` and `egui/Cargo.toml`. No search implementation was changed for
  this benchmark run.
- Build command: `cargo build --release`.
- Machine: 12th Gen Intel Core i9-12900KS, 24 logical CPUs, Linux
  `7.1.9-1-MANJARO`.
- Toolchain: `rustc 1.98.1`, `cargo 1.98.1`.
- Each cell is one fresh process with no warmup or repeated samples. The
  command shape was `target/release/factoriosrc-tui new --no-tui --format json`
  followed by the case arguments and, where applicable, one experimental flag.
- Every process had the same hard timeout of **60 seconds**. `>60 s` means the
  process was killed by that timeout. Every run that completed returned
  `Solved`; unsupported Generations options were not run.
- The JSON format was used only to capture the terminal status reliably. These
  are first-result timings, not full enumeration timings.

This protocol is intentionally different from `just bench`, which uses
hyperfine warmups and no timeout.

| Case | Plain | `--phase-saving` | `--lookahead` | `--backjump` | `--nogood` | Notes |
| --- | ---: | ---: | ---: | ---: | ---: | --- |
| `B3/S23 26 8 4 -y 1 -n a` | 1.265 s | 3.806 s | 5.810 s | 38.126 s | 12.145 s | Two-state rule; all runs solved. |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` | 2.169 s | 3.176 s | N/A | N/A | N/A | Generations rule; `Config::check()` rejects the three two-state-only options. |
| `B3/S23 64 64 1 -n a` | >60 s | 1.899 s | 0.047 s | 0.023 s | 0.013 s | Large two-state case; completed runs solved. |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2-` | 28.445 s | 14.053 s | 32.235 s | >60 s | >60 s | Factorio rule; backjump and nogood timed out. |
| `B2n3/S23-q 30 9 4 -x 1` | 1.288 s | 2.790 s | 4.096 s | >60 s | >60 s | Requested INT case; backjump and nogood timed out. |

These measurements are a baseline for future iterations, not a general claim
that any option is better. For a later rerun, record the date, revision,
dependency state, release profile, CPU, operating system, toolchain, timeout,
warmup/repetition policy, random seed, stopping condition, and whether the run
measures a first result or full enumeration.

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
| `B3/S23 26 8 4 -y 1 -n a` (c/4 ship) | >60 s (timeout); a follow-up run without the timeout solved it in ~587 s wall / 586.6 s solver | 1.265 s (plain) | `lls -c -b 26 8 -s p4 x0 y1`; also >60 s with `-b 26 9`. LLS encoding: 1,015 variables, 116,442 clauses; its 53-cell ship differs from factoriosrc's edge-touching 57-cell one. |
| `B3/S23 64 64 1 -n a` (period 1) | 4.07 s / 0.092 s, but the solution is the all-dead pattern; with `-p ">=100"`: >60 s (a dry run did not even finish encoding within 180 s) | >60 s (plain); 0.013–1.899 s with experimental modes | LLS has no non-empty requirement; the ≥100-population attempt died in LLS's cardinality encoding, not in the solver. |
| `B2n3/S23-q 30 9 4 -x 1` (INT c/4 ship) | >60 s (timeout; also >60 s with `-b 31 9`) | 1.288 s (plain) | `lls -c -b 30 9 -s p4 x1 y0 -r B2n3/S23-q`; encoding: 1,342 variables, 416,480 clauses. |

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
  against factoriosrc's 1.265 s. For this project's first-result search regime,
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
