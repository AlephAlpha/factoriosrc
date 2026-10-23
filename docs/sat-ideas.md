# SAT-Inspired Search in factoriosrc

> **Scope.** This is a concise implementation note, not a general survey of
> SAT solving. It records the SAT-inspired mechanisms that exist in the
> repository, their semantic limits, and a small set of possible directions.
> The code is the source of truth; this document explains it rather than
> defining it.

`factoriosrc` is not a CNF SAT solver. It searches finite cellular-automaton
worlds with incremental neighborhood descriptors, symmetry deductions, and a
fixed search-order chain. SAT terminology is useful for naming some of the
mechanisms, but a direct CDCL analogy is not valid at every boundary.

## Status And Options

The experimental options are disabled by default. They are fields of `Config`
and are available through the current frontends.

| Mechanism | Status | Option | Current implementation | Main limit |
| --- | --- | --- | --- | --- |
| Descriptor propagation and chronological search | Implemented | default | Checks affected descriptors incrementally, then backtracks through the search chain. | This is local table propagation, not watched-literal BCP. |
| Phase saving | Implemented | `--phase-saving` | Tries the last real state of a cell before `new_state`. | A polarity heuristic only; it does not select a different cell. |
| Lookahead | Implemented | `--lookahead` | Probes `Dead` and `Alive` for the next branch candidate and keeps the better polarity. | Two-state rules only; the probe is bounded and does not select a variable. |
| Conflict analysis and backjumping | Implemented | `--backjump` | Performs 1-UIP-style analysis for selected local conflicts and resumes above the learned conflict level. The analysis machinery is also enabled by `--nogood`. | Two-state rules only; global and some non-local failures backtrack chronologically. |
| Exact-position nogood learning | Implemented | `--nogood` | Persists analyzed forbidden assignments and propagates them while the current `World` remains alive. With only `--nogood` the search resumes chronologically; `--nogood --backjump` adds the non-chronological resumption. | Two-state rules only; entries use absolute cell indices and cannot be transferred. |
| Chronological nogood learning | Implemented | `--nogood` (without `--backjump`) | Learns analyzed conflicts but resumes with the ordinary chronological backtracking, isolating the value of the learned patterns from the backjump resumption. | Two-state rules only; pays the analysis and database maintenance without the non-chronological resumption. |
| Machinery cost guard | Implemented | `--nogood-guard` | Enables nogood learning (not backjumping) and suspends the experimental machinery when its measured work is too high. | Opt-in and heuristic; it is not a proof of wall-time improvement. |
| Conflict activity branching | Implemented | `--activity` | Chooses the most active unknown cell in a window of eight search-chain cells. | Changes branching order only; the window remains deliberately small. |
| Restarts | Implemented | `--restart` | Performs Luby-scheduled restarts over conflicts. A restart pops the trail to the known cells and resumes from the head of the search order while keeping learned nogoods, saved phases, and activity. | Two-state rules, because restart enables the nogood database; suppressed after the first reported solution unless `reduce_max_population` is enabled. |
| Relative or cross-world nogoods | Rejected | none | A translation-relative prototype was measured and removed. Cross-world reuse would build on the same relative representation. | Watch-based translated matching was far slower than exact-position nogoods; see [Measured Alternatives](#measured-alternatives-not-kept). |
| Component caching and a direct CNF encoding | Proposed | none | Not implemented. | These would be separate search strategies, not drop-in replacements. |

The option implications and rule restrictions are enforced by
`Config::check()`:

- `nogood_guard` enables `nogood`; `restart` also enables `nogood`, because a
  deterministic restart without learned clauses would re-enter the same path;
  `backjump` stays independent. `--nogood` learns and resumes chronologically,
  while `--nogood --backjump` is the non-chronological combination, and
  `--nogood-guard --backjump` is the guarded version of it.
- `lookahead`, `backjump`, and `nogood` are rejected for rules with more than
  two states. `restart` is rejected for the same reason, since it enables
  `nogood`.
- `phase_saving` and `activity` also apply to Generations rules.
- `nogood_capacity` changes the database budget; it is not a separate search
  method. `restart_interval` changes the restart schedule; it is not a
  separate search method either.

Use the following terms consistently:

- **Implemented** means that a code path exists. It does not imply that it is
  faster than the default search.
- **Tested** means that repository tests cover the stated behavior for some
  configurations. It is not an exhaustive proof over all rules and options.
- **Measured** is reserved for current factoriosrc results recorded in the
  canonical benchmark table near the end of this document. Archived external
  comparisons are labeled separately and are not current measurements.
- **Proposed** means that no current search mode implements the idea.
- **Rejected** means that a prototype was implemented and measured, then
  removed; it is not current behavior.

## Search Model

### World And Cells

`World` represents a finite `(x, y, t)` search box and a periodic cycle of
`period` generations. The configured box contains the cells that the search may
branch on. The allocated cell graph also contains a neighborhood-radius padding
ring and cells fixed by the background, symmetry, or known-cell constraints.

`World::canonicalize_coord()` maps generations outside `0..period` back into
the cycle while applying the configured transformation and translation. A
successor or predecessor can therefore point to another generation or to a
cell outside the searchable box. Outside cells are treated as known background
states.

Each `LifeCell` contains, directly or indirectly:

- an optional state;
- a descriptor for its current state, successor state, and neighborhood;
- predecessor and successor pointers;
- symmetry-equivalent cells; and
- a `next` pointer forming the branching chain.

`RuleTable::implies()` uses a precomputed table. For totalistic rules the
descriptor stores neighbor counts; for non-totalistic rules it stores the
known and unknown arrangement of neighbors. The table can report a conflict,
deduce a successor or current state, or force neighbors.

### One Search Step

The main loop is `World::search()` and its internal `step()`:

1. `set_cell()` applies an assignment and updates descriptors, successor
   caches, front and population counters, the trail, and any enabled heuristic
   state. Symmetry deductions are made when the propagation queue is checked.
2. `check_stack()` treats the unchecked suffix of the trail as a propagation
   queue. `check_affected()` checks pending nogoods, global constraints,
   symmetry, and the descriptors of the changed cell, its predecessor, and its
   neighbors.
3. Generations rules use a separate check because dying states advance
   deterministically and are only equivalent to dead in the underlying
   two-state neighborhood rule.
4. When propagation reaches a fixed point, `guess()` chooses the next unknown
   cell. The default cell is the head of the `next` chain; `activity` may choose
   another cell in its small window. `lookahead`, when enabled, chooses the
   polarity before the real assignment.
5. A local conflict is analyzed when `backjump` or `nogood` is active and the
   machinery is not suspended. The resumption policy is selected by
   `backjump`: it either backjumps above the learned conflict level or calls
   the ordinary `backtrack()`, which removes assignments until another state
   can be tried.
6. While the search is still running, a due restart pops every search
   assignment back to the known cells and resumes at the head of the search
   order. Learned nogoods, saved phases, and activity are kept; see
   [Restarts](#restarts).
7. When all searchable cells are assigned, `check_period()` applies the current
   smaller-period divisor check. If it passes, `World::search()` reports
   `Solved`; otherwise the assignment is backtracked. A later call continues
   enumeration.

### SAT And Constraint-Programming Analogies

| factoriosrc mechanism | Closest analogy | Difference that matters |
| --- | --- | --- |
| Descriptor checks and the trail queue | Unit propagation | One descriptor is an n-ary transition constraint, not a CNF clause. |
| `next`-chain branching | DPLL branching | The order is spatial and is also tied to the front optimization. |
| `Reason`, `TrailMeta`, and `Antecedent` | Trail and implication graph | Detailed antecedents exist only when the conflict analysis machinery is enabled (`backjump` or `nogood`). |
| `World::analyze()` | 1-UIP conflict analysis | Reasons are reconstructed from mutable descriptors and can become stale. |
| `NogoodDb` | Learned-clause database | Entries are absolute, world-local forbidden assignments. |
| `activity` | VSIDS-style branching | Only a fixed local window is reordered; it is not a full SAT variable heuristic. |
| `World::restart` and the Luby schedule | CDCL restarts | A restart keeps local search state (phases, activity, nogoods) rather than only a clause database, and it is suppressed in enumeration after the first reported solution. |

## Semantic Boundaries

SAT-inspired changes must preserve the following constraints. The supported rule
families are defined by `Config::parse_rule()`; currently the app accepts at
most 255 states, totalistic neighborhoods of size at most 24, and
non-totalistic neighborhoods of size at most 8. The complete family and
geometry description is kept in [`docs/front.md`](front.md) and
`AGENTS.md`, not duplicated here.

### Local Constraints Are N-ary

A transition relates a cell, its successor, and a neighborhood that can contain
many cells. A conflict is therefore closer to a hypergraph constraint than to a
Boolean clause. A learned nogood is sound only for the exact assignments and
assumptions used by the current analysis.

### Global And Complete-Assignment Constraints

`front_count` rejects an empty non-trivial front, and `below_max` rejects a
population bound that no generation can satisfy. Both are maintained
incrementally but are reported as `Confl::Global`; they do not enter conflict
analysis. The front proof depends on translation and reflection invariants and
is documented in [`docs/front.md`](front.md).

The period test is performed only after a complete assignment. It is also not a
local descriptor conflict. A future learning scheme must handle these
constraints separately instead of treating every failure as an ordinary clause.

### Background, Padding, And B0

For a rule without `B0`, the background is dead. For a `B0` rule without the
maximum survival condition, the background cycles through all rule states; its
period is the number of states. For a rule with both `B0` and `S-max`, the
background is permanently alive. `Config::check()` requires the searched period
to be a multiple of the background period.

For `B0`, "empty" means equal to the background state at that generation, not
necessarily dead. In the `B0` plus `S-max` case, the population counters count
dead cells because they are the deviations from the alive background.

Padding cells, diagonal-width boundaries, known cells, and the background are
part of the current world semantics. A deduction near one of these boundaries
is not automatically valid at another coordinate, in a different-sized world,
or at another background phase. This is the main reason the current nogood
database does not normalize entries by translation or symmetry.

### Generations Rules

A Generations rule has `Dead`, `Alive`, and one or more dying states. A dying
state advances deterministically. The neighborhood descriptor treats dying
states as dead for the underlying two-state rule, but the full deduction logic
must still respect the exact current and successor states. This creates an
intentional asymmetry: a deduction that is valid for `Alive` or `Dead` is not
automatically valid for every dying state.

For this reason, lookahead, conflict analysis, and nogood learning are rejected
for Generations rules rather than silently approximated. Phase saving and
activity only change branching and remain available.

### Enumeration And Persistence

The normal use case is enumeration, not merely finding one satisfying
assignment. After a solution, the next call to `search()` resumes the same
search and may lower `max_population` when `reduce_max_population` is enabled.
Generation rotations and other equivalent encodings may still be reported as
separate solutions when the configuration permits them. Restarts do not change
this contract: they are suppressed after the first reported solution unless
`reduce_max_population` is enabled. See [Restarts](#restarts).

The serialized `World` stores the ordinary configuration, stack, visible search
state, and random generator. It does not store `TrailMeta`, the nogood database,
activity values, guard state, or the original antecedent graph. Phases of cells
that had already been unset are also lost. Loading therefore may change the
heuristic path and learned information, but not the intended search semantics.
`increase_world_size()` rebuilds the world and loses the current search state as
well as all learned nogoods.

## Conflict Analysis And Backjumping

Backjumping is the first CDCL-inspired experiment. The conflict analysis
machinery it shares with nogood learning is enabled by `Config::backjump` or
`Config::nogood`, and is limited to two-state rules. The non-chronological
resumption itself is selected by `Config::backjump` alone; with only
`Config::nogood`, the analyzed conflicts are learned into the database but
resumed chronologically.

### Trail Metadata

`Reason` remains a small state-setting category: `Known`, `Deduced`, `Guessed`,
or `TryAnother`. When the conflict analysis machinery is enabled, `World` keeps
a parallel `TrailMeta` entry containing:

- the decision level;
- whether the entry is the level's decision carrier;
- whether the carrier is a chronological flip; and
- an optional `Antecedent`.

The antecedent is one of:

- `Descriptor(source)`, for a rule-table deduction;
- `Symmetry(source)`, for a symmetry deduction; or
- `Clause { start, len }`, for a deduction made by a learned clause.

The literals of a clause antecedent live in a world-local reason arena that the
live trail owns, not inline in the trail entry, so recording a firing or a
learned unit does not allocate. The arena is compacted from the live trail
entries when it has accumulated too much garbage.

Descriptor antecedents are reconstructed when needed from the source descriptor.
Only cells that were earlier on the trail than the deduction are included. This
position filter is necessary because a later assignment may have changed the
descriptor and cannot be retroactively treated as a cause.

### Analysis And Resumption

For `Confl::Rule`, `Confl::Symmetry`, or an active `Confl::Nogood`,
`World::analyze()`:

1. Seeds the conflict with the literals that directly participate in it.
2. Resolves current-level literals through their antecedents until one
   current-level literal remains, the 1-UIP.
3. Falls back to chronological backtracking if no usable 1-UIP exists or a
   learned-clause reason is stale.
4. Pops to the highest decision level of the remaining literals, while
   rechecking descriptors affected by the popped cells.
5. Reassigns the 1-UIP cell to the opposite state with a temporary clause
   antecedent.

Steps 4 and 5 are the non-chronological resumption of `Config::backjump`. With
`--nogood` alone, the analysis still resolves to the 1-UIP (steps 1-2) but
skips the resumption of steps 4-5: the learned nogood is recorded in the
persistent database, and the ordinary chronological backtracking resumes the
search; the temporary clause reason of step 5 is not created.

The temporary clause exists on the current trail. It becomes persistent only
when `nogood` is enabled. Global conflicts, a failed two-state lookahead, and a
failed complete-assignment period check use chronological backtracking.

### Enumeration Guard

Chronological backtracking uses a flip carrier to mark that the first branch of
a decision has been exhausted. Popping that carrier or restoring its first
state would re-enter already enumerated solutions. Analysis therefore clamps
its target above the deepest flip carrier. If the 1-UIP or target reaches that
carrier, it learns the conflict when possible and then falls back to
chronological backtracking. This protocol is part of correctness, not merely a
performance heuristic.

Backjumping without a persistent database can revisit similar conflicts after
backtracking. It is therefore kept opt-in; the existence of conflict analysis
does not imply a speedup.

## Exact-Position Nogood Learning

`--nogood` stores the forbidden assignments produced by conflict analysis, and
enables the analysis machinery itself. It does not enable backjumping: the
search resumes chronologically unless `--backjump` is set as well. The
chronological mode is the direct test of the learned patterns, separated from
the re-check and resumption costs of the backjump.

### Representation And Lifetime

A nogood is a sorted list of `(absolute cell index, state)` literals. All listed
assignments must not hold simultaneously in any solution. The learned entry
contains the rejected state of the 1-UIP and the states of the other learned
clause literals.

The database indexes entries by their literals and rejects verbatim duplicates.
It is valid only inside the current `World`: it is cleared by world creation,
save/load reconstruction, and `increase_world_size()`. It is not normalized by
translation, generation rotation, symmetry, rule, or world size.

### Propagation

For every real set or unset, `NogoodDb` maintains the number of literals that do
not currently match. The index makes this incremental update possible.

- When one unknown literal remains, the database forces that cell away from its
  recorded state. The assignment receives an `Antecedent::Clause`.
- When all literals match, `check_affected()` reports `Confl::Nogood` after
  revalidating the match.
- When chronological backtracking tries the opposite state of a two-state
  guess, an indexed completion query can skip a branch that would immediately
  complete a nogood. The query reads the unmatched-literal counter of each
  candidate and evaluates only the entries that are at most one literal short
  of a match, so the candidate cap bounds the entries actually examined
  instead of the whole bucket.
- Lookahead probes do not update the database counters; their assignments and
  rollback are temporary.

The propagation path is important: a nogood can be completed by deductions, not
only by a final guess. A stale clause reason is ignored by conflict analysis and
causes a chronological fallback.

### Measured Alternatives (Not Kept)

A standard two-watched-literal variant of the database was implemented and
measured with the release-build protocol of
[`docs/capacity-study.md`](capacity-study.md). It cut the per-assignment
maintenance sharply (on the `c1` row, from about `1.5e9` index bucket updates
to about `6.5e7` watch visits), but two-watched-literal propagation does not
re-propagate every unit after backtracking the way the per-literal counters do.
The step counts rose from 442k to 855k on `c1`, from 2.06M to 3.72M on `c5`,
and from 653k to 776k on `c7`, and the wall time rose on all three, so the
counter scheme was kept.

A recursive clause-minimization pass shortened the learned clauses by only
about 6% (the average length fell from 33.0 to 31.2 on `c1`) while the analysis
cost grew about fivefold: `analysis_scanned` rose from 27.5M to 150M on `c1`
and from 497M to 2.35B on `c1 --backjump`. The wall time rose 15-21% on `c1`,
`c5`, and `c7`, so the pass was not kept.

Ranking the eviction by LBD as a tie-break after recency and uses was also not
kept: repeated release runs were consistently slower (about +3.2% on `c1`,
+1.1% on `c7`, and +1.2% on `c5`), even where it lowered the step count
slightly. None of these variants is in the repository.

Translated, translation-relative nogoods were prototyped behind a temporary
`--relative-nogood` flag and removed. The prototype canonicalized each clause
by the offsets from its lexicographically smallest `(generation, y, x)` cell
and created placements lazily from two watched literals. It kept a clause only
when its analysis used no level-0 cell or symmetry deduction and the
transformation was the identity. Differential tests on small worlds matched the
plain solution set. In an ad-hoc release run of `B3/S23 20 20 2 -n r --seed 1`
(first solution), `--nogood` took 0.28 s and 202,471 steps, while the relative
mode took 32.3 s and 410,916 steps and created and evicted about 41.5M
placements. On `B2n3/S23-q 30 9 4 -x 1 -n a` it exceeded 180 s (canonical
`--nogood`: 8.49 s), and on `B3/S23 7 7 2 -n a --no-stop` it exceeded 60 s
(3.06 s). Watch discovery creates too many speculative placements, so a
fundamentally cheaper indexing scheme would be needed; the mode is not kept,
and cross-world nogoods are not pursued because they depend on the same
relative representation.

### Capacity And Eviction

The current implementation uses these limits:

| Item | Current behavior |
| --- | --- |
| Automatic capacity | `max(2048, 4 * world_size)`, including the padding ring. |
| Explicit capacity | `Config::nogood_capacity` overrides the automatic value. |
| Learned-entry length | Entries longer than 96 literals are rejected. |
| Completion-query work | At most 512 index entries are visited, and at most 64 of the filtered candidates are evaluated. |
| Reduction | When the capacity is reached, the worst half is evicted. |
| Ranking | Older last-use epoch first, then fewer uses, then older entry id. |
| LBD | Recorded for diagnostics, but not used for propagation or eviction. |

The capacity and query bounds may reduce pruning, but they must not change the
solution set. The capacity is also a maintenance budget: every real assignment
walks the relevant index bucket, so an unnecessarily large database can cost
more time even when it lowers the search-step count. A capacity sweep over the
canonical workloads, a size ladder, and the larger cases is recorded in
[`docs/capacity-study.md`](capacity-study.md).

`World::search_stats()`, `World::nogood_stats()`, `World::nogood_top()`, and
`World::search_steps()` expose diagnostic counters. Non-TUI JSON output includes
these counters when requested; they do not affect the search.

### Cost Guard

`--nogood-guard` enables the database (not backjumping), then monitors a proxy
for the experimental machinery's work:

`queued_cells + analysis_scanned + bucket_updates`.

Here `queued_cells` includes propagation work after backjumps, while
`analysis_scanned` and `bucket_updates` measure the two other machinery paths.
Without backjumping, the same budget bounds the chronological learning
instead.

Every 1024 search steps, the guard compares this work with a budget of 512
operations per step. If the budget is exceeded, it disables conflict analysis
and nogood maintenance, so the search falls back to chronological backtracking.
After a cooldown that grows with repeated suspensions, it re-enables the
machinery and rebuilds the database counters. The current base cooldown is
`2^18` steps and the backoff exponent is capped at 6.

The guard is deterministic, depends on operation counters rather than wall time,
and is not serialized. It is disabled unless `nogood_guard` is explicitly set.

## Restarts

`--restart` enables Luby-scheduled restarts. A restart pops every search
assignment back to the known cells, resets the decision levels and flip
carriers, and resumes at the head of the search order. It keeps the learned
nogood database, the saved phases, the conflict activity, and the random
generator. The known cells, background, and symmetry deductions stay in place,
while the front and population counters are restored by the ordinary
`unset_cell()` path.

The schedule is the Luby sequence `1, 1, 2, 1, 1, 2, 4, ...`, multiplied by a
base interval measured in conflicts. A conflict is a failed propagation check,
a failed lookahead probe, or a failed complete-assignment period check. The
default base interval is 16 conflicts; `--restart-interval` overrides it.
Restarts stop after 512 restarts in the current search phase, after which the
search continues chronologically. The final segment is therefore an ordinary
complete search, so `NoSolution` remains reachable and a restart sequence
cannot prevent exhaustion. The schedule and the restart counter are heuristic
state and are not serialized; `reported_solutions` is serialized so that the
enumeration gating below survives save/load.

`Config::check()` makes `--restart` imply `--nogood`: a deterministic restart
without learned clauses would re-enter the same path. The option therefore
inherits the two-state restriction of the nogood database. Restarts are also
deferred while `--nogood-guard` has suspended the machinery, because a restart
without active learned clauses has no pruning to make progress.

### Enumeration And Population Bounds

A restart before the first reported solution is safe for enumeration. It may
discard flip carriers that marked exhausted branches, but those branches
contain no reported solution, so re-entering them only repeats work. Once a
solution has been reported, the flip carriers protect the enumeration from
reporting it again. The implementation therefore stops restarting after the
first reported solution unless `reduce_max_population` is enabled.

With `reduce_max_population`, each solution lowers the bound to `p - 1`, where
`p` is the minimum population of that solution. Every previously reported
solution is strictly above the new bound, so a restart cannot report one of
them again, and the search under the new bound remains complete. The first
solution and the sequence of record-low solutions may differ from a run
without restarts, but the enumerated solution set and count, and the final
minimum population, are unchanged. Each time the bound is tightened, the Luby
schedule and the restart budget start over, because the bound defines a new
search problem.

The canonical benchmark now records restart combinations and one minimization
case. The results are mixed: pairing restart with phase saving is the strongest
restart mode on the two hard first-solution rows, while `--nogood` remains the
best choice for the minimizing case. See the canonical tables below for the
measured values.

## Branching Heuristics

### Phase Saving

With `phase_saving`, `LifeCell::phase` remembers the last non-probe assignment
to a cell, whether it came from a guess, deduction, or known configuration.
Unsetting a cell does not clear the phase. If no phase exists, `new_state` is
used. A lookahead probe is run before phase saving is consulted and does not
update the phase.

### Lookahead

For the branch candidate selected by the ordinary chain or by `activity`,
`probe()` temporarily tries `Dead` and `Alive` separately. Each probe propagates
up to 256 newly set cells and then rolls back completely. If one polarity
conflicts, the other is chosen; if both survive, the polarity with more
deductions is chosen; ties choose `Dead`. If both conflict, ordinary
backtracking is used.

Lookahead does not choose a different cell and is rejected for Generations
rules.

### Activity Branching

With `activity`, conflicts bump the cells that participate in them. An analyzed
conflict bumps the 1-UIP and learned-clause literals and applies the geometric
bump/decay path. A chronological local conflict bumps its seed cells. The next
branch is selected from the first eight unknown cells of the search chain; ties
preserve chain order. The cursor remains at the earliest unknown cell so that a
later selected branch cannot skip earlier cells.

Activity changes branching order only and is available for Generations rules.
Its values are heuristic state and are not serialized.

## Possible Directions

These are proposals, not current features. Any implementation should first add
a differential correctness test and then use the canonical benchmark table.
Chronological nogood learning was implemented from this list: `--nogood`
without `--backjump` is the isolated learning mode, and it is measured in the
canonical benchmark tables below. Restarts were implemented from the same list
and are described in [Restarts](#restarts); their canonical measurements are
recorded in the restart and minimization tables below.

1. **Stronger consistency across descriptors.** Adjacent descriptors share
   cells and successors. A bounded on-demand consistency check could compose
   facts that one descriptor cannot derive alone.
2. **Learning for Generations.** Possible encodings include learning only in the
   dead/alive layer or using multi-valued variables. They must preserve dying
   transitions and `TryAnother` enumeration semantics.
3. **Database quality policies.** A protected short-clause tier, an LBD-aware
   ranking, or an online capacity policy may be useful, but the current policy
   is recency-based and the LBD is diagnostic only.
4. **Component caching and CNF comparison.** These are separate strategies. A
   CNF implementation would be an external baseline, not a replacement for
   descriptor propagation without an encoding and boundary comparison.

## External CNF Baseline: Logic Life Search

[Logic Life Search](https://gitlab.com/OscarCunningham/logic-life-search) (LLS)
is a useful reference for the alternative approach: encode the search as CNF
and let a general-purpose SAT solver do the propagation and learning. This
comparison is kept as an archived external snapshot, not as part of the
recurring factoriosrc benchmark table. It was not rerun during this rewrite.

The recorded setup used LLS commit `ecf6c24`, Python 3.14.7, kissat 4.0.4,
`--background vacuum`, single runs, and a 60-second timeout on the same machine
as the earlier factoriosrc measurements. The factoriosrc values below are old
reference values and should not be read as the result of the next unified
benchmark run.

### Encoding Differences

- LLS creates `p + 1` generations and constrains generation `p` to equal a
  translated generation 0. factoriosrc stores `p` generations and maps the
  successor of the last generation through `World::canonicalize_coord()`.
- The two encodings differ at a drifting box boundary. LLS fixes translated
  phase-0 cells that leave the box to the background; factoriosrc's wraparound
  constrains the opposite edge of the last generation. Equal-looking boxes are
  therefore not always the same search problem.
- LLS has no equivalent of factoriosrc's non-empty-front constraint. In
  particular, LLS can report the trivial all-dead period-1 pattern where
  factoriosrc deliberately rejects it.
- The comparison covers only the overlapping, non-`B0`, two-state cases. Rule
  support, boundary semantics, symmetry handling, and search controls are not
  identical.

### Archived Results

The numbers are wall-clock time for the complete command unless noted. The
factoriosrc result includes the plain chronological search, except for the
period-1 row where the old experimental range is shown for context.

| Search problem | LLS | factoriosrc | Interpretation |
| --- | ---: | ---: | --- |
| `B3/S23` tutorial c/3 ship, 16x6 | 1.14 s | 0.009 s | Both tools found a tutorial solution. |
| `B3/S23` symmetric c/3 ship, 17x12 | 15.7 s | 0.105 s | Both tools found a solution, but heuristics chose different populations. |
| `B3/S23` c/4 ship, 26x8 | >60 s; 587 s in a later run | 1.245 s | The compared boxes also differ at the drifting boundary. |
| `B2n3/S23-q` c/4 ship, 30x9 | >60 s | 4.213 s | The CNF encoding reached about 416,000 clauses. |
| `B3/S23` period 1, 64x64 | 4.07 s for all-dead | >60 s plain; 0.014-1.854 s with old experimental modes | Not the same task because LLS accepts the trivial pattern. |

The comparison suggests two separate conclusions. First, on the measured
tutorial and larger first-result cases, the specialized search was much faster:
its rule tables are precomputed and it avoids regenerating a large CNF for each
instance. Second, the success of backjumping and nogoods on shallow searches
does not make the CNF approach irrelevant. It shows that conflict-directed
search can help; the larger gap is in the encoding, descriptor propagation, and
front semantics rather than in the general idea of learning from conflicts.

The snapshot has important limits: it consists of single runs, some nominally
equal boxes are semantically different, the tools support different features,
and kissat is optimized mainly for difficult unsatisfiable proofs while these
tests stop at a satisfying first result. Any new claim should use a fresh,
matched protocol rather than extending this table one row at a time.

## Verification And Maintenance

### Source Map

| Concern | Source of truth |
| --- | --- |
| Supported rules, option implications, and automatic search order | `lib/src/config.rs` |
| Cell graph, front, global counters, persistence, and public diagnostics | `lib/src/world.rs` |
| Propagation, branching, probing, restarts, backtracking, and analysis | `lib/src/search.rs` |
| States, descriptors, and rule-table implications | `lib/src/rule.rs` |
| Reasons and antecedents | `lib/src/cell.rs` |
| Nogood representation, propagation, statistics, and eviction | `lib/src/nogood.rs` |
| Front invariant and its proof obligations | [`docs/front.md`](front.md) |

When changing one of these implementations:

- Update the status table and the relevant detailed section together.
- Compare both the solution set and the number of enumerated solutions with the
  default search. A set comparison alone can hide duplicate enumeration.
- Include B0 backgrounds, symmetry and transformation, known cells, population
  bounds, `reduce_max_population`, save/load, and world growth when the change
  affects learning or backtracking.
- For changes to unsafe search internals, run the Miri check required by
  [`AGENTS.md`](../AGENTS.md).
- Use paths and symbol names instead of line numbers. Keep this document about
  current behavior; put implementation history in git rather than in a growing
  research log.

Do not describe an experiment as a performance improvement without a reproducible
command, commit, build profile, environment, stopping condition, and result.
The canonical factoriosrc benchmark section below is the only recurring
benchmark table. The LLS section is a separately labeled archived comparison and
should only be changed after rerunning the complete external protocol.

## Canonical Benchmark Table

This is the current unified benchmark. It was measured on 2026-10-20 with a
release build (`cargo build --release`) at commit `22b2aca` on Linux x86-64
(24 cores, 31 GiB RAM), with single runs, up to six concurrent single-threaded
processes, a 600-second per-cell timeout, and the default `new_state` unless
the case says otherwise. Each cell runs
`target/release/factoriosrc-tui new --no-tui --format json <case> <options>`;
the reported time is the program's `elapsed_secs` at the stopping condition.
`>600 s` means the cell hit the timeout and `N/A` means `Config::check()`
rejects the option for that rule. Every option and combination, including the
restart combinations, produced the same solution count as the plain search on
the two enumeration rows: 10 on the 10th-solution row and 9,537 on the
exhaustion row. Step counts are deterministic for fixed-seed runs and are
available in the JSON output.

`--nogood` resumes chronologically; `--nogood --backjump` is the
non-chronological combination, and `--nogood-guard --backjump` adds the cost
guard to it. `--restart` enables `--nogood` implicitly, so the restart table
also carries the single flag. The tables are split into single flags, learning
combinations, branching combinations, restart combinations, and a
minimization case.

Fill future reruns in place; do not append a new date-specific table.

### Single Flags

| Case | Stopping condition | Plain | `--phase-saving` | `--lookahead` | `--activity` | `--backjump` | `--nogood` | `--nogood-guard` |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 26 8 4 -y 1 -n a` | First solution | 1.29 s | 3.94 s | 5.86 s | 1.53 s | 44.6 s | 1.73 s | 1.41 s |
| `B3/S23 64 64 1 -n a` | First solution | >600 s | 1.98 s | 0.046 s | 0.023 s | 0.019 s | 5.87 s | 5.87 s |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` | First solution | 2.39 s | 3.46 s | N/A | 2.64 s | N/A | N/A | N/A |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2- -n a` | First solution | 73.7 s | 14.4 s | 33.3 s | 63.5 s | >600 s | 379 s | 79.1 s |
| `B2n3/S23-q 30 9 4 -x 1 -n a` | First solution | 4.25 s | 3.61 s | 4.11 s | 2.71 s | 434 s | 8.49 s | 4.71 s |
| `B3/S23 20 20 2 -n r --seed 1 --no-stop` | Through the 10th solution | 6.09 s | 0.937 s | 100 s | 2.14 s | >600 s | 0.280 s | 556 s |
| `B3/S23 7 7 2 -n a --no-stop` | Exhaustion (9,537 solutions) | 0.669 s | 0.703 s | 1.02 s | 0.684 s | 2.51 s | 3.06 s | 1.68 s |

### Learning Combinations

| Case | Stopping condition | `--nogood --backjump` | `--nogood-guard --backjump` | `--activity --nogood` | `--activity --nogood --backjump` | `--phase-saving --nogood` | `--phase-saving --nogood --backjump` |
| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 26 8 4 -y 1 -n a` | First solution | 3.05 s | 1.20 s | 1.96 s | 3.62 s | 6.04 s | 10.5 s |
| `B3/S23 64 64 1 -n a` | First solution | 0.0069 s | 0.0068 s | 0.714 s | 0.0097 s | 0.0082 s | 0.0060 s |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` | First solution | N/A | N/A | N/A | N/A | N/A | N/A |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2- -n a` | First solution | >600 s | 79.4 s | 381 s | >600 s | 417 s | 519 s |
| `B2n3/S23-q 30 9 4 -x 1 -n a` | First solution | 17.0 s | 4.76 s | 4.82 s | 5.61 s | 4.75 s | 11.9 s |
| `B3/S23 20 20 2 -n r --seed 1 --no-stop` | Through the 10th solution | 0.441 s | 11.9 s | 0.070 s | 0.083 s | 8.94 s | 0.116 s |
| `B3/S23 7 7 2 -n a --no-stop` | Exhaustion (9,537 solutions) | 3.29 s | 1.69 s | 3.14 s | 3.44 s | 3.04 s | 3.23 s |

### Branching Combinations

| Case | Stopping condition | `--phase-saving --activity` | `--activity --backjump` | `--phase-saving --backjump` |
| --- | --- | ---: | ---: | ---: |
| `B3/S23 26 8 4 -y 1 -n a` | First solution | 1.33 s | 34.8 s | >600 s |
| `B3/S23 64 64 1 -n a` | First solution | >600 s | 0.019 s | 0.0093 s |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` | First solution | 4.05 s | N/A | N/A |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2- -n a` | First solution | 55.2 s | >600 s | >600 s |
| `B2n3/S23-q 30 9 4 -x 1 -n a` | First solution | 4.11 s | >600 s | >600 s |
| `B3/S23 20 20 2 -n r --seed 1 --no-stop` | Through the 10th solution | 16.0 s | >600 s | >600 s |
| `B3/S23 7 7 2 -n a --no-stop` | Exhaustion (9,537 solutions) | 0.744 s | 3.35 s | 2.63 s |

### Restart Combinations

| Case | Stopping condition | `--restart` | `--restart --phase-saving` | `--restart --activity` | `--restart --backjump` | `--restart --nogood-guard` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 26 8 4 -y 1 -n a` | First solution | 2.06 s | 6.08 s | 1.96 s | 3.59 s | 2.89 s |
| `B3/S23 64 64 1 -n a` | First solution | 0.0038 s | 0.0048 s | 0.023 s | 0.0049 s | 0.0050 s |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` | First solution | N/A | N/A | N/A | N/A | N/A |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2- -n a` | First solution | 382 s | 19.3 s | 301 s | >600 s | >600 s |
| `B2n3/S23-q 30 9 4 -x 1 -n a` | First solution | 8.85 s | 1.48 s | 5.23 s | 16.6 s | 244 s |
| `B3/S23 20 20 2 -n r --seed 1 --no-stop` | Through the 10th solution | 1.05 s | 0.718 s | 233 s | 0.369 s | 146 s |
| `B3/S23 7 7 2 -n a --no-stop` | Exhaustion (9,537 solutions) | 3.12 s | 3.06 s | 3.16 s | 3.28 s | 1.86 s |

### Minimization

| Case | Stopping condition | Plain | `--nogood` | `--phase-saving` | `--restart` | `--restart --phase-saving` |
| --- | --- | ---: | ---: | ---: | ---: | ---: |
| `B3/S23 16 16 2 -n r --seed 1 --reduce-max-population --no-stop` | Minimum (exhaustion under decreasing bounds) | >600 s (best 83) | 16.3 s | 45.2 s | 297 s | 244 s |

Every completed minimization cell reached the same minimum population, 3; the
plain cell timed out after finding population 83 while looking for a smaller
solution. The times are the final `elapsed_secs` at `NoSolution`, not the time
of an intermediate solution. The JSON output exposes `min_population`,
`max_population`, the effective `restart_interval`, and the `conflicts` and
`restarts` counters, so these cells can be reproduced from the raw output.

Adding `--backjump` to a learning pair changes the resumption policy, not the
semantics, and lands near the corresponding chronological pair
(`--activity --nogood --backjump` is 0.083 s against 0.070 s for
`--activity --nogood` on `B3/S23 20 20 2`, 5.61 s against 4.82 s on
`B2n3/S23-q`, and 3.62 s against 1.96 s on `B3/S23 26 8 4`).

The central comparison is `--nogood` against `--nogood --backjump`.
Chronological learning is faster on five of the six rows where both run:
`B3/S23 26 8 4` 1.73 s against 3.05 s, `R3,C2,S2,B3,N+` 379 s against
a timeout, `B2n3/S23-q` 8.49 s against 17.0 s, and `B3/S23 20 20 2` 0.280 s
against 0.441 s, plus the exhaustion row where it is 3.06 s against 3.29 s.
The exception is `B3/S23 64 64 1`, where the non-chronological resumption is
essential: `--nogood --backjump` finishes in 0.0069 s while `--nogood` needs
5.87 s. Non-chronological resumption is therefore not merely a cost of the
previous design; it is the only mode that solves that case quickly.

The guard is less reliable in the chronological mode. `--nogood-guard` is the
best experimental single flag on `B3/S23 26 8 4` (1.41 s, against 1.73 s for
`--nogood` and 1.29 s for plain), close to the plain search on
`R3,C2,S2,B3,N+` (79.1 s against 73.7 s) and on `B2n3/S23-q` (4.71 s
against 4.25 s), but it collapses on `B3/S23 20 20 2` (556 s against
0.280 s for unguarded `--nogood`, and 6.09 s for plain): a suspension changes
the search path, and the resumed machinery then re-enters a much worse
region. `--nogood-guard --backjump` keeps the old guarded behavior: it is
79.4 s on the hard `R3,C2,S2,B3,N+` row and 1.69 s on the exhaustion row.

Among the learning combinations, `--activity --nogood` is the best choice on
`B3/S23 20 20 2` (0.070 s) and matches the plain search on `B2n3/S23-q`
(4.82 s against 4.25 s), but it is much slower than plain on
`R3,C2,S2,B3,N+` (381 s against 73.7 s). `--phase-saving --nogood` is the
worst of the chrono learning modes on `B3/S23 26 8 4` (6.04 s against 1.73 s)
and `B3/S23 20 20 2` (8.94 s against 0.280 s), although its backjump version
is among the fastest there (0.116 s).

The branching combinations reproduce their known profiles. Combining the two
branching heuristics still hurts `B3/S23 64 64 1` (`--phase-saving --activity`
times out while both parts finish in under 2 s). Adding backjumping to a
branching heuristic remains bad: `--activity --backjump` improves
`B3/S23 26 8 4` over backjumping alone (34.8 s against 44.6 s) but times out
on three rows, and `--phase-saving --backjump` even times out on
`B3/S23 26 8 4`, where every other option finishes.

The restart table is mixed. `--restart --phase-saving` is the strongest
restart mode on the hard `R3,C2,S2,B3,N+` row (19.3 s against 73.7 s for plain
and 382 s for restart alone), although plain `--phase-saving` is faster still
(14.4 s); on `B2n3/S23-q` it is the fastest cell overall (1.48 s against
4.25 s for plain). `--restart` alone is the fastest cell on
`B3/S23 64 64 1` (0.0038 s, ahead of `--nogood --backjump` at 0.0069 s).
Phase saving is decisive on the hard rows but costs on `B3/S23 26 8 4`, where
restart alone is 2.06 s against 6.08 s for the pair. The same pair is slower
than `--nogood` on `B3/S23 26 8 4` (6.08 s against 1.73 s), and on the
10th-solution row it is slower than `--activity --nogood` (0.718 s against
0.070 s). Restart alone stays near the nogood cost on the hard
`R3,C2,S2,B3,N+` row (382 s). The other pairings are less consistent:
`--restart --activity` beats
restart alone on `R3,C2,S2,B3,N+` and `B2n3/S23-q` but collapses on
`B3/S23 20 20 2` (233 s); `--restart --backjump` is fast on
`B3/S23 64 64 1` and the 10th-solution row (0.0049 s and 0.369 s) but times
out on `R3,C2,S2,B3,N+`; and `--restart --nogood-guard` is slow on
`B2n3/S23-q` and `B3/S23 20 20 2` (244 s and 146 s). On the exhaustive world
every restart mode costs 1.86-3.28 s against 0.669 s for plain, like the other
learning modes.

The minimization row is the case most relevant to the intended use, and
restart does not pay off there in this run. `--nogood` reaches the minimum in
16.3 s; `--phase-saving` needs 45.2 s;
`--restart` and `--restart --phase-saving` need 297 s and 244 s; the plain
search times out after finding population 83. Every completed cell reached the
same minimum population, 3, so the restart gating preserves the final answer,
but the restart overhead dominates the tightening sequence.

On the exhaustive small world, every learning mode costs 1.69-3.44 s against
0.669 s for the plain search: the analysis and database maintenance are pure
overhead when the search is already cheap. The restart modes cost 1.86-3.28 s
for the same reason, and all of them enumerate exactly 9,537 solutions. The
`--nogood` pairs still carry the two-state restriction, which is why the
Generations row is `N/A` for them; `--restart` is `N/A` there for the same
reason. `--phase-saving --lookahead` is deliberately not listed: `guess()`
consults the lookahead probe before phase saving, so phase saving is never
read and the pair is behaviorally identical to `--lookahead` alone.

The nogood rows use the automatic capacity unless a future benchmark explicitly
records another value. Capacity sweeps and one-off profiling belong in the
experiment notes or git history, not in additional tables in this document.
