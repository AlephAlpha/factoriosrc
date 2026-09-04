# SAT Solver Techniques for factoriosrc

A living experiment log for bringing SAT solver techniques (CDCL and friends) into
factoriosrc. It maps factoriosrc's algorithm onto SAT vocabulary, records what has been
tried and with what outcome, distills the lessons from failed attempts, and plans the next
steps.

- Every **implemented** item is an opt-in experiment behind a `Config` flag; nothing is on by
  default. `Config::check()` (`lib/src/config.rs`) is the source of truth for which
  flag/rule combinations are accepted.
- Correctness claims are backed by differential enumeration tests against the default search
  (solution-set equality) and by Miri for the unsafe internals.
- Exact benchmark numbers live only in [Consolidated benchmarks](#consolidated-benchmarks);
  the idea sections carry only qualitative verdicts.

## Status at a glance

| # | Direction | Part | Status | Flag | Verdict |
| --- | --- | --- | --- | --- | --- |
| 1 | Conflict analysis + backjumping | antecedent recording, 1-UIP analysis | implemented | `--backjump` | Net loss on small/medium searches (re-treading); decisive win on very large searches |
| 2 | Nogood database | exact-position db + propagation-level firing | implemented | `--nogood` (implies `--backjump`) | Recovers most of the re-treading loss; plain search wins typical solving searches but loses on the deep oscillator case |
| 2 | Nogood database | translated templates + cross-size transfer | implemented | `--nogood-translate` (implies `--nogood`) | Sound after the margin rules (iterations 6-7); instantiation re-enabled, transfer verified against real solution sets |
| 2 | Nogood database | propagation-integrated template matching | shelved (measured) | — | Translated completions are real but unaffordable to catch at completion time; see idea 2 |
| 3 | Phase saving | — | implemented | `--phase-saving` | Mixed; helps on the factorio rule, hurts when a fixed `new_state` already fits |
| 3 | VSIDS-style activity | — | idea only | — | |
| 4 | Lookahead probing | polarity selection | implemented | `--lookahead` | Big win on `B3/S23` default-strategy searches; clear loss on the factorio rule |
| 4 | Lookahead probing | cell selection | idea only | — | Needs a search-order refactor to stay sound |
| 5 | Cross-neighborhood consistency | — | idea only | — | |
| 6 | Multi-valued encoding for Generations | — | idea only | — | |
| 7 | Restarts | — | not recommended (for now) | — | |

## Maintaining this document

This log is edited by humans and AI agents alike. To keep it readable, follow these rules:

- Each idea section keeps the same shape: **Idea**, **Status**, mechanics, **Lessons
  learned**, **What remains** (the exact subsections vary a little where the idea needs it).
  Append new iterations as new entries; do not rewrite history.
- Keep the **Status at a glance** table in sync with `Config::check()` and the CLI flags in
  the same change that touches them (see AGENTS.md).
- Keep lessons from failed or reverted attempts — the general lesson, not the blow-by-blow.
  Drop details that no longer reproduce (typo'd benchmark commands, superseded behaviors that
  `Config::check` now rejects, one-off hardware notes).
- Record exact benchmark numbers only in the consolidated benchmark section, stamped with
  date and build. Keep prose qualitative; delete stale numbers rather than letting them rot.
- Correctness claims must state how they were verified (differential enumeration tests,
  Miri, ...).
- Refer to code as `path/file.rs` + item name, without line numbers (they rot).

## Background

### The current algorithm is DPLL with strong propagation

factoriosrc's core is a constraint satisfaction search over a three-dimensional (x, y,
generation) periodic cell grid. The search loop lives in `lib/src/search.rs`:

- `World::search` calls `step()` in a loop; `step()` first propagates (`check_stack()`), then,
  if there is no conflict, branches (`guess()`).
- **Propagation**: `set_cell` (`lib/src/world.rs`) pushes the cell onto the `stack`; the part
  of the stack after `stack_index` is the propagation queue. `check_affected` checks the
  descriptors of the set cell, its neighbors, and its predecessor. `RuleTable::implies`
  (`lib/src/rule.rs`) looks up a precomputed table and may deduce the successor, the current
  cell, all unknown neighbors (totalistic rules), or individual neighbors (non-totalistic
  rules, the `forced` bits).
- **Branching**: `guess()` walks the `next` chain — a fixed spatial order built by
  `init_next` — picks the next unknown cell, and sets it to Alive, Dead, or a random state
  according to `Config::new_state`.
- **Backtracking**: `backtrack()` pops the stack; at a `Reason::Guessed` cell it flips the
  state (2-state rules) or tries the next state in the cycle (Generations,
  `Reason::TryAnother`).
- A full assignment is checked by `check_period` (rejection of smaller periods), then the
  search backtracks to enumerate the next solution.

Correspondence with SAT vocabulary:

| factoriosrc mechanism | SAT solver counterpart |
| --- | --- |
| `check_affected` + the `stack`/`stack_index` propagation queue | BCP (unit propagation); the incremental descriptor updates are similar to watchlists |
| The precomputed `RuleTable` | A precompiled failed-literal / unit-propagation closure |
| `guess()` walking the `next` chain | DPLL-style branching in a fixed order |
| `backtrack()` undoing the latest guess | Chronological backtracking |
| `Reason` (`lib/src/cell.rs`) | Records the decision level, but (before idea 1) not the antecedent |

So the current algorithm is roughly DPLL with strong unit propagation. What is missing is
exactly the core of CDCL: conflict analysis, non-chronological backtracking (backjumping),
clause learning, restarts, and phase/activity heuristics.

### Structural differences that decide what transfers

- **The constraints are n-ary.** For each cell, successor = f(current, neighborhood), where
  the neighborhood can have up to 24 cells. The implication graph is a hypergraph: the
  antecedent of a deduction can span the cell itself, its successor, and several neighbors. A
  learned nogood is an n-ary forbidden condition, not a binary clause.
- **Global constraints.** The non-empty front (`front_count == 0`), the population bound
  (`below_max == 0`), and the period check (`check_period`) are not local constraints.
  Conflicts from them cannot be learned; conflict analysis must exclude them.
- **Periodicity / translation invariance.** The world is periodic in x, y, and t (see
  `World::canonicalize_coord`). A learned nogood can be normalized to relative coordinates
  and reused at any position, any generation, and even across world sizes — a unique
  advantage of CA search over SAT, and the basis of idea 2.
- **Generations rules are multi-valued.** Branching cycles through the states
  (`Reason::TryAnother`), and the deductions of the underlying 2-state rule are asymmetric:
  neighbors can be deduced alive but not dead (see `check_generations_implied`). Learning and
  backjumping must preserve this asymmetry.
- **The goal is to enumerate all solutions.** factoriosrc does not stop at the first
  solution; it keeps enumerating (combined with `reduce_max_population` for optimization
  searches, and "search for the next solution" in the GUIs). This affects restart-based
  techniques.
- **Unsafe hot paths.** Changes to `LifeCell`/`World` touch unsafe code (`lib/src/cell.rs`,
  `lib/src/world.rs`, `lib/src/search.rs`) and need Miri verification (see AGENTS.md).

## Idea 1: Antecedent recording, conflict analysis, backjumping

**Idea.** Attach to every deduction its *antecedent* — the (cell, state) facts it was derived
from. On a conflict, resolve backwards through the antecedents to the 1-UIP (first unique
implication point) and backjump there, flipping it (2-state rules) or trying the next state
(Generations). This is standard CDCL conflict analysis plus backjumping, except that the
implication graph is a hypergraph with n-ary nodes.

**Status: implemented** as an opt-in experiment. `Config::backjump` (CLI `--backjump`;
toggles in the TUI and egui UIs; 2-state rules only, per `Config::check`) enables both the
**recording layer** (antecedents and decision levels — also the foundation of idea 2) and the
full 1-UIP analysis with non-chronological backtracking. Correctness is verified by a
differential enumeration matrix against the default search (8 rules × 2–3 world sizes ×
periods × 3 search orders × 6 symmetries × 6 transformations × 3 new-state strategies =
16,200 configurations).

**How it works.**

- The antecedent of a deduction is the known part of the *source cell's* descriptor — the
  cell whose neighborhood lookup produced the deduction (`Antecedent::Descriptor`); a
  symmetry deduction points at the mirrored cell. The exact antecedent set is recovered by
  walking the known cells of that descriptor, excluding the deduced cell, and filtering to
  cells whose stack positions precede the deduction. The position filter is
  soundness-critical: a cell set after the deduction cannot have been part of its reason (if
  it had changed the deduction, the search would have conflicted when it was set).
- The search stack carries parallel `TrailMeta` (level, decision flag, antecedent) kept in
  sync at every pop site (`backtrack`, the lookahead probe rollback), so the implication
  graph of the current partial assignment is always reconstructible.
- Recording costs a few percent when enabled and nothing when disabled (the code path is
  unchanged; the side structures are empty).

**Lessons learned.**

1. **Chronological flips break CDCL's one-decision-per-level invariant.** When the search
   backtracks chronologically, the popped guess is re-set to its opposite state as a
   reasonless "deduction". These flips are now recorded as *decision carriers*: each occupies
   a whole level, `current_level` counts carriers rather than guesses, and every level above
   the root has exactly one reasonless decision. Resolving a reasonless literal "with an
   empty reason" derived invalid nogoods — this was the mistake that made the first
   implementation incomplete (it enumerated a strict subset of solutions) and had to be
   reverted; the differential test caught it.
2. **Trail order is not search-chain order.** The `next` chain is a fixed spatial order that
   differs from the trail order, and a backjump truncation pops trail entries whose chain
   positions lie *before* the resumption point — the search then never visits them again and
   once reported `Solved` with unknown cells. The fix: rank cells in chain order, resume at
   the chain-earliest popped cell, and re-check the trail entries whose descriptors were
   changed by the pops (a targeted re-queue, not a full re-check).
3. **Learned-clause reasons go stale.** The reason of a backjump flip is only valid while its
   cells still hold the recorded values; the analysis must verify the recorded stack
   positions and fall back to chronological backtracking otherwise.
4. **Without a persistent nogood database, conflict analysis re-treads closed branches.**
   Every learned clause is discarded as soon as its cells pop, so the analysis regularly
   re-explores already-closed branches (on a tiny configuration the same solution was
   re-found 16 times). The result is a net loss on small/medium searches — orders of
   magnitude slower on typical benchmarks — while the recording layer alone is cheap.
5. **Very large searches are the exception.** On a large world with shallow local cascades
   (`B3/S23 64 64 1 -n a`), conflicts are deep and the jump distance dwarfs the
   re-propagated work: backjumping finds a pattern in milliseconds where the default search
   does not finish in 90+ seconds. This matches the old rlifesrc observation that
   backjumping pays off mainly for large still lifes. Note that backjumping, lookahead, and
   phase saving all find *different* solutions there within seconds — which heuristic wins
   matters when the tool is used to discover examples rather than to enumerate.
6. **Differential testing is the safety net that matters.** Compare solution *sets*, not raw
   counts (counts differ legitimately — a pattern and its generation rotation are both valid
   finds). The differential matrix caught the first attempt's incompleteness and the
   chain-resumption corner case above.

**What remains.** The flag stays opt-in for experimentation. For typical small/medium
searches, idea 1 is useful mainly as the foundation of idea 2; its own payoff is the
large-search regime. Open question: the analysis overhead per conflict is what makes the
factorio-rule profile a DNF under the CDCL machinery where the default search finishes —
reducing it has not been attempted.

## Idea 2: A nogood database (clause learning for CA)

**Idea.** The CA version of CDCL clause learning. A learned nogood is a set of relative
coordinates plus states that cannot be extended to a solution — essentially a forbidden local
pattern. Because CA search is translation invariant, a nogood can be reused at any position,
any generation, and potentially across world sizes; cross-size reuse in particular may be the
single biggest practical win among the ideas here.

**Status: implemented** as opt-in experiments, in `lib/src/nogood.rs` (`NogoodDb`):

- `Config::nogood` (CLI `--nogood`; toggles in both UIs; 2-state rules only) — the
  exact-position database with propagation-level firing; implicitly enables `--backjump`.
- `Config::nogood_translate` (CLI `--nogood-translate`; 2-state only) — translated templates
  and cross-size transfer; implicitly enables `--nogood`.

Five iterations so far; the planned propagation-integrated template matching was
investigated with a measurement probe and shelved (iteration 5).

**The core that pays: exact-position nogoods with propagation-level firing.**

- Every successful conflict analysis records its 1-UIP cell with its rejected state plus the
  literals of the learned clause (capped at `MAX_NOGOOD_LITERALS`; larger patterns are not
  worth their index entries). Nogoods are stored by absolute cell indices and indexed by every
  (cell, state) pair, so a candidate guess finds the nogoods it would complete without
  scanning the database. The database is bounded (the oldest half is evicted when full), and
  queries examine at most `MAX_QUERY_CANDIDATES` candidates per index bucket.
- Each entry maintains a counter of how many of its literals currently hold, updated
  incrementally through the existing (cell, state) index in `set_cell`/`unset_cell`. When an
  entry is *one literal short*, its remaining unknown cell is forced away from the recorded
  state during propagation — unit propagation on the learned nogoods — justified by an
  `Antecedent::Clause` built from the other literals at fire time. When *all* literals hold
  (reachable when a wrong-state cell is unset and later re-set to the recorded state, skipping
  the one-short window), a pending `Confl::Nogood` conflict is queued and **re-validated when
  consumed**, because the propagation queue can empty right after the match and the step can
  end with a direct backtrack that pops matched cells before the next check sees the flag.
- Lookahead probes are fully excluded from the counters, including their rollback, which must
  run while `in_probe` is still set so that the skipped updates stay symmetric.

**Iterations.**

1. **Guess-time checks only** (superseded). The database was consulted only in `guess()`
   before choosing a state, and at the chronological backtrack flip. Correct, and it recovered
   roughly half of what backjumping loses on small enumeration workloads — but it rescued none
   of the backjumping losses. Instrumentation showed why: a guess-time check only fires when
   the last literal of a forbidden pattern happens to be the cell being guessed; patterns
   completed by deduction are never intercepted.
2. **Propagation-level firing** (the current core, described above). This is the mechanism
   that recovers the re-treading loss: it is consistently far cheaper than backjumping
   alone, it shrinks backjumping's enumeration work by one to two orders of magnitude, and
   it is the only CDCL configuration that beats the plain chronological search on the deep
   oscillator workload — though on typical solving searches the plain search still wins.
3. **Anchor classes and translatable templates.** Every learned nogood records *what its
   derivation relied upon* (`Anchor`), collected while the analysis walks the implication
   chain (see "What a nogood may rely on" below). Eligible nogoods are additionally stored as
   templates in frame coordinates; behind `Config::nogood_translate`, `guess()` checks
   translated alignments of a template against the current assignment. Sound and verified,
   but guess-time template checks repeat the iteration-1 lesson: template hits are rare
   compared to firings of concrete entries, and the per-guess probe cost dominates — a net
   loss on single-size solving searches. A later seed sweep showed that the oscillator
   workload's apparent exception was trajectory luck, not systematic pruning: a translate
   run with no template hits is exactly the plain nogood run plus probe overhead, and on
   one seed the reordering turned a solve into a DNF. The templates earn their keep as the
   data structure for cross-size transfer.
4. **Cross-size transfer.** Free templates and single-edge-pinned templates survive
   `increase_world_size` without coordinate remapping (free coordinates are relative; left/top
   pins are absolute; right/bottom pins are stored as distances from the edge), and are
   *instantiated* as concrete entries at every alignment that fits the new world (bounded;
   only concrete entries take part in propagation firing). Mirror-pair, both-edges-pinned,
   user-known, and diagonal-band templates are dropped. On growth workflows (exhaust-and-grow)
   this cuts search calls by roughly a quarter compared to the plain database, but wall time
   roughly doubles — the instantiation work plus the guess-time probing cost more than the
   pruned calls save. Fully verified; the economics are not there yet.
5. **Transfer-only templates** (the current code). The planned propagation-integrated
   template matching was investigated first with a measurement probe and then shelved (see
   the shelving analysis below), and the guess-time template checks were removed instead:
   templates are now built at learn time, handed over through `increase_world_size`, and
   instantiated as concrete entries there — and they no longer affect the search within one
   world. `--nogood-translate` is therefore exactly `--nogood` plus template bookkeeping on
   single-size workloads (verified: identical step counts on every benchmark), and its value
   is confined to the growth workflow, where instantiation prunes a fifth of the steps at
   8x8 and — with the margin rules of iteration 7 — pays for itself. The template
   deduplication query also got the same candidate cap as the other queries (it was an
   unbounded scan over a popular state bucket).
6. **Watched-literal maintenance: attempted and reverted.** Profiling attributed ~66% of the
   nogood run time to the matched-literal counter maintenance (`on_set` 34% + `on_unset`
   32%: every set and unset of a cell walks the full index bucket of every nogood
   containing it), so a Minisat-style watched-literal scheme was implemented: two watched
   literals per entry with blocker-hint caches, an O(1)-per-unset update, and repair hooks
   for watchers left stale by completed nogoods. It was sound by construction (every
   firing and full match is re-validated against the live world), single-size traces
   matched, and Miri passed — but on the benchmarks it **regressed 3–14x** (oscillator
   `20 20 2`: 779k steps / 6.9 s became > 240 s; spaceship: 1.03M → 3.4M steps, 13 s →
   55 s): the unit window is only detected through a watched literal, so near-complete
   nogoods are caught as conflicts instead of firing earlier, and the loss of
   propagation-level forcing cascades into huge trajectory differences. **Reverted**; the
   counter scheme stays. The `ClauseReason` hardening from the experiment was kept: the
   recorded literals now also carry their *state*, closing a latent hole where a recycled
   stack position could make a stale learned-clause reason pass the position-only check.
7. **The anchor-path soundness hole was root-caused and fixed with margin rules.** The
   verification of iteration 6 established that instantiated templates could prune real
   solutions; the deep-dive of iteration 7 traced the reason: a learned nogood's
   forbidden-ness depends on the rule-consistency **context** of its cloud (the cloud plus
   radius), and that context can differ between the source world and a translated
   alignment — a cloud that was boundary-adjacent at the source (with the boundary's
   background values feeding the cloud's deductions) has a different constraint network
   than the same cloud at an interior alignment of a larger world. The anchor's pin flags
   only record the boundary facts the resolution chain *walked*, not the geometry of the
   cloud itself (the trace showed a {left,top}-anchored clause whose cloud was
   bottom-adjacent at the 3x3 source, with no bottom flag). Two complementary rules now
   close it:
   - **Build time** (`build_template` returns `None`): the cloud's span plus the rule
     radius must be interior at the source on every unpinned side; a pinned side may
     touch the boundary (the pin certifies that interaction), but the opposite side must
     stay interior. Templates failing this are not stored.
   - **Instantiation time** (`instantiate_templates`): the translated alignment must keep
     the same interiority on the equivalent sides of the target world.
   Soundness is now verified mechanically: a closure checker compares every instantiated
   entry against the real solution sets of the grown world, and the differential growth
   tests pass again with instantiation enabled. The instantiation margins also make the
   machinery's behavior world-size-aware: small worlds reject most templates (their clouds
   are boundary-bound), and candidates become transferable as the world grows. The
   previously disabled `--nogood-translate` instantiation is re-enabled with these rules.

**Lessons learned.**

1. **Memory pays only when it participates in propagation.** Guess-time interception cannot
   see patterns completed by deductions — the failure of iteration 1, and again of the
   guess-time template checks in iteration 3. The probe of iteration 5 confirmed that the
   missing interceptions exist — the assignment routinely contains fully completed
   translated patterns — but also showed that catching them at completion time is out of
   reach at this library scale (see the shelving analysis below).
2. **Forced assignments must be decision carriers, not clause-justified deductions.** The
   first version justified nogood-forced guesses with an `Antecedent::Clause` built from the
   blocking nogood; as soon as such a cell was popped, the clause went stale, and every
   analysis walking through the cell fell back to chronological backtracking — the spaceship
   benchmark went from seconds to effectively never finishing. Guess-time forcing is now
   recorded as a decision carrier (a conservative-sound stop point for resolution);
   propagation firings keep clause antecedents, whose staleness falls back safely.
3. **Bound query cost.** A popular anchor cell shares its index bucket with many nogoods;
   cap candidates per query, and build the "other literals" vector only after a candidate
   fully matches.
4. **Re-validate pending conflicts when consumed** (see the firing description above).
5. **Track what a nogood relies on before translating it** (see below). For the
   exact-position mode none of the filters are needed — everything it relies on holds
   throughout one world; for translated or cross-size modes they are all mandatory.
6. **Memory pays where the plain search is redundant.** A seed sweep on the oscillator
   workload showed that plain nogood beats the default search on every seed tried
   (4–60x): random guessing makes the plain search revisit equivalent subspaces (tens to
   hundreds of millions of steps) that propagation firing deduplicates to a few hundred
   thousand. On directed searches (the spaceship case) the same machinery removes only
   ~6x of the steps — far less than its per-step cost — and loses. The same sweep showed
   that the translate variant's extra win on one seed was trajectory luck: template hits
   are rare (0–453 per run), and a run with none is exactly the plain nogood run plus
   probe overhead.
7. **Measure before redesigning.** The planned propagation-integrated template matching
   was probed before being implemented. The probe showed that the uncaught completions
   are real and plentiful (on the spaceship workload the assignment typically contains
   hundreds of fully completed translated patterns at any time), but that no affordable
   mechanism can notice them at the moment they complete. The redesign was shelved and
   replaced by removing the inert guess-time checks — a fraction of the planned cost, and
   a strictly faster program.

**What a nogood may rely on** — reference for the translation/transfer filters. Three
mechanisms make a nogood boundary-dependent (relying on "outside the world is the background
state", set by `init_known`), and the first two are invisible in the nogood's literals:

1. **Known-cell literals.** A literal cell with `Reason::Known` (a padding cell, a cell whose
   translated predecessor left the world, a user known cell, a diagonal-width cell) relies on
   that forcing.
2. **Baked descriptor bits.** Padding-frame cells have their missing neighbors baked into the
   descriptor as the background state (`set_outside_neighbor`, null successors), and the
   conflict seed collects only the *known real cells* of the descriptor. A conflict found on
   such a cell can depend on the background without any Known literal appearing in the
   nogood. Every `Antecedent::Descriptor` source on the resolution chain must therefore be an
   *interior* cell — inside `[0,w) × [0,h)` with non-null predecessor and successor, which
   implies a complete neighborhood, since the padding exactly covers the radius.
3. **Excluded level-0 literals.** The analysis drops level-0 literals from the learned clause
   because they always hold; but for a persistent nogood, dropping a conjunct makes the
   forbidden pattern *stronger*. If any level-0 fact was relied upon during the resolution,
   the nogood must be tagged as config-local.

Additionally, nogoods derived from `Confl::Symmetry` are not translatable: the mirror mapping
depends on the absolute world size (e.g. `D2H` uses `y ↦ h−1−y`); they are valid only within
one world size.

The anchor scheme of iteration 3 classifies all of this: background-forced boundary cells and
baked descriptors pin the corresponding edge; user known cells and the diagonal band make the
nogood position-fixed; untracked level-0 facts and rotation/diagonal symmetry pairs make it
world-local; mirror pairs (`S0`/`S2`) allow sliding along their axis only; nogoods relying on
none of these are free. Deductions justified by learned clauses inherit their anchors, so
reliance propagates through resolutions. On the spaceship benchmark the distribution is
roughly 45% free, 54% edge-pinned, under 1% local — most knowledge is at least partially
translatable, and edge pins survive world growth.

**Why propagation-integrated template matching was shelved** — the analysis that killed
the planned iteration 5, recorded so that it is not re-derived:

- *The potential is real.* A temporary probe (a full library scan every 8192nd cell set)
  counted translated template alignments — excluding the learned ones — that were fully
  matched by the current assignment, i.e. forbidden patterns completed by deductions that
  neither the concrete-entry machinery nor the guess-time check could see. The assignment
  contained such patterns at essentially every sample point: on average ~1.4 concurrent on
  the oscillator workloads and ~350 on the spaceship workload (exact numbers in the
  consolidated benchmarks section). Catching a completion at the moment it happens would
  prune the doomed region immediately instead of waiting for the search to stumble out of
  it.
- *The cost is out of reach.* Detecting a completion when its last literal is set requires
  finding all (template, alignment) pairs that contain the just-set (cell, state). Concrete
  entries have that index because their positions are absolute; translated templates do
  not, and there is no way to build one — every alignment of every template could match
  any cell. The fallback is scanning the library: on the spaceship workload roughly 27k
  templates × ~800 alignments × ~6 literals ≈ 140M operations per scan, against a per-set
  budget of ~100 operations — three orders of magnitude too expensive to run often enough
  to matter. A per-alignment watcher scheme (the watched-literal idea) does not escape
  this: the watchers still have to be *created*, and creation requires the same scan.
- *The escape hatches are speculative.* A much smaller hot-set library (activity-ranked,
  tens of templates instead of tens of thousands) would make periodic scans affordable,
  at the price of most of the knowledge; a fundamentally different index would have to
  exploit structure that relative-coordinate templates do not have. Neither has a
  concrete design.

**Not yet tried / skipped.**

- Canonicalizing nogoods under `Config::symmetry` (a minimal representative per equivalence
  class) could improve the hit rate — untested.
- Clause-database-style management (eviction by activity/LBD) — currently only oldest-half
  eviction plus query caps.
- A static library of rule-specific forbidden patterns (e.g. "this 2x2 patch can never be
  all-alive") was considered as an easy start and skipped: dynamic learning subsumes it.

**What remains.** The honest summary of the five iterations: *persistent memory works and
pays for itself only when it participates in propagation*, and the only affordable
participation is through concrete entries with exact positions. Translated knowledge is
cheap to store but unaffordable to match at this library scale (see the shelving analysis
above). The measured bottleneck of the CDCL machinery on solving searches is the
per-conflict analysis overhead, not the memory — reducing that is a separate direction
(idea 1, "What remains").

## Idea 3: Phase saving and decision heuristics

### Phase saving — implemented

`Config::phase_saving` (CLI `--phase-saving`; toggles in both UIs; off by default). Each cell
remembers the last state it was set to (`LifeCell::phase`, written in `World::set_cell`) and
`guess()` prefers that state. This replaces the global `Config::new_state` policy per cell;
it is only a heuristic, so it is equally valid when enumerating multiple solutions. On
save/load, the phases of the cells on the stack are rebuilt by replaying `set_cell`; phases
of unset cells are lost, which only affects the heuristic.

**Verdict** (qualitative, from the consolidated benchmarks, 2026-08-31):

- Helps on the factorio rule `R3,C2,S2,B3,N+` — the rule this project is built around:
  about 2x faster on the 50x10 case (a different, also valid solution), neutral on the
  larger 50x12 case (the same solution).
- 1.5–3x slower on the other solving searches tried (the Generations control, the spaceship
  cases, the INT rule, the oscillator search). The effect depends on the fixed `new_state`
  strategy: when that strategy already fits the task, phase saving interferes.
- Solves the very large 64x64 case in seconds where the default search hits the time limit.
- Kept as opt-in. Follow-ups: restrict it to certain rules or new-state strategies, or
  combine it with probing (idea 4).

### VSIDS-style activity — idea only

Give cells that participate in conflicts a score bump, and branch on the most active cell.
Not implemented; deliberately left out of the phase-saving experiment so the two heuristics
could be evaluated separately. The risk: the fixed spatial order of the `next` chain is an
important synergy with the front optimization and local propagation (the front argument in
`docs/front.md` does not depend on the guess order, but the order affects propagation
efficiency). Safe approaches: activity only within a local window of the current order, or an
experimental switch to compare against the fixed order.

> Aside: R3 searches show a sharp difficulty cliff — some D2- period-3 cases are
> instantaneous (a structural no-solution argument), while period-4 or C1 cases can exceed 15
> minutes. Pick factorio-rule benchmarks carefully.

## Idea 4: Probing before branching (lookahead)

**Idea.** The CA version of SatZ-style lookahead / DLIS. The lookup tables already embed one
level of failed literals — `Implication::NeighborhoodAlive/Dead` (`lib/src/rule.rs`) are
exactly "setting an unknown neighbor to some state leads to a conflict", and the `forced`
bits of non-totalistic rules are the same for individual neighbors. Probing goes one level
deeper: before branching on a cell, try its candidate states, propagate for a bounded number
of steps, and use the outcome to choose. A conflict found while probing is a free
failed-literal prune. The world is small, `check_affected` is cheap, and everything is local,
which fits factoriosrc well.

**Status: implemented** (polarity selection only; the cell-selection variant is not).
`Config::lookahead` (CLI `--lookahead`; toggles in both UIs; off by default; 2-state rules
only — `Config::check` rejects Generations, since the dying states have no probe analogue).
`World::probe` (`lib/src/search.rs`) temporarily sets the next unknown cell to each state,
propagates with a cap of `MAX_PROBE_DEDUCTIONS` deductions, scores the probe, and rolls it
back (reusing the existing set/unset + stack machinery); the search then guesses the better
state, or the only non-conflicting one. `World::in_probe` keeps probes from corrupting phase
saving when both are enabled.

**Verdict** (qualitative, from the consolidated benchmarks, 2026-08-31):

- Dramatic win on the `B3/S23` dead-first search (about 7x faster, and it finds the same
  pattern as the alive-first search): probes immediately discover failed literals and pick
  alive first, avoiding long dead branches.
- Also solves the very large 64x64 case in under a tenth of a second (a different pattern).
- Clear loss on the default factorio-rule searches, and about 3x slower on the INT-rule and
  oscillator searches. Lowering the probe cap did not change this (earlier runs), so the
  overhead is the fixed per-guess probe cost, not the propagation depth.
- Kept as opt-in because of the large potential win. Follow-ups: probe less often (e.g. only
  every k-th guess, or only when the preferred state differs from the default guess), probe a
  single state instead of both, or only probe small neighborhoods where the check is cheap.
- The cell-selection variant (using probe scores to choose *which* cell to branch on, not
  just which state) is not implemented; it needs a small search-order refactor to stay sound.

## Idea 5: Consistency across overlapping neighborhoods

Propagation currently only reasons inside a single cell's descriptor. The descriptors of two
adjacent cells share variables (shared neighbors + the successor chain), and together they
can deduce things that neither table can alone — the analogue of 2-consistency in CP, or
local resolution in SAT preprocessing.

- Fully precomputing this is infeasible (two neighborhoods give about 2^(2n) entries). A
  practical approach is to check "descriptor pairs" encountered during propagation on the
  fly, with a small cache (transposition-table style).
- A generative variant, analogous to BVE (bounded variable elimination): fold the
  deterministic dying chains of Generations rules (they are already deterministic
  propagation, so they can be eliminated like pseudo-variables).

## Idea 6: Multi-valued encoding for Generations rules

- `Reason::TryAnother` branches in a cycle over the states, while CDCL techniques live in the
  Boolean domain.
- Options: treat the dying chain as deterministic propagation (it already is) and restrict
  learning and backjumping to the dead/alive base layer; or encode Generations rules in
  Boolean variables (one variable per cell per state, with exactly-one constraints), so that
  the 2-state CDCL machinery applies unchanged.
- Either way, preserve the asymmetry of deduction (only alive can be deduced).

## Idea 7: Restarts (Luby sequence) — not recommended for now

- Restarts help find the first solution, but factoriosrc's typical use case is enumerating
  all solutions, where restarts only repeat work.
- They only make sense together with idea 2 (nogood memory) and phase saving, and they
  interact badly with save/load and incremental world growth.
- Reconsider only after the CDCL-style techniques above are mature.

## Other loose ideas

- **Component caching** (SAT's component caching): split the unknown cells into independent
  components by constraint dependency and solve each separately. The constraint graph is dense
  in periodic searches, so components are rare; a large world with many known cells might be
  an exception.
- **Cube-and-conquer**: split the search space into cubes to solve in parallel, for future
  parallel search (the egui frontend already runs the search on a background thread).
- **Encode as CNF and compare against an off-the-shelf SAT solver**: periodic pattern search
  can be encoded directly (one variable per cell per generation plus transition constraints).
  This could serve as a baseline for the ceiling of what a SAT-style approach can achieve —
  first measured via LLS (see the SAT comparison in the consolidated benchmarks): factoriosrc
  wins 25–280x on its home turf, generic SAT wins the redundant oscillator regime, and the
  64x64 row shows factoriosrc's own CDCL machinery beating kissat when enabled. An own
  encoder for higher-range rules (counting-network decomposition of the 24-input transition)
  remains the open SAT question.
- Borrowing from row-by-row searchers (e.g. qfind) is a separate direction (already in the
  README) and out of scope here.

## Consolidated benchmarks

> The numbers in earlier revisions of this document came from several one-off runs with
> slightly different setups and have been retired. The table below is from one unified
> re-measurement run. Re-run the whole table with the same driver and time limit instead of
> patching single cells, and keep exact numbers in this section only.

Setup: 2026-08-31, release build, rustc 1.98.0, Intel i9-12900KS, single run per cell.
Driver: the in-repo example `lib/examples/bench` (`cargo run --release -p factoriosrc-lib
--example bench -- <case>`); it accepts the same shapes as the TUI CLI. Timing starts after
the world and its rule table are built, so it measures the search itself. Uniform time
limit: 240 s per run, chosen so that the slowest known-finite configuration still finishes.
Work metric: search steps (calls to `World::search(1)`). Raw solution counts are not
comparable across strategies (the search may legitimately re-find solutions; correctness is
solution-set equality, verified by the differential tests), so they are not reported.

| Case | Why this case | default | `--backjump` | `--nogood` | `--nogood-translate` | `--phase-saving` | `--lookahead` |
| --- | --- | --- | --- | --- | --- | --- | --- |
| `B3/S23 26 8 4 -y 1 -n a` | spaceship search; the re-treading case | 1.31 s | 41.8 s | 13.7 s | 13.7 s | 3.99 s | 6.14 s |
| `B3/S23 26 8 4 -y 1` | same, default (dead-first) `new_state` | 42.3 s | DNF | DNF | DNF | 43.9 s | 6.13 s |
| `B3/S23 64 64 1 -n a` | very large world; where CDCL shines | DNF | **20 ms** | **7 ms** | 6 ms | 2.08 s | 47 ms |
| `B3/S23 4 4 2` (enumerate all) | enumeration work, shallow | <1 ms · 389 steps | 3 ms · 2,352 | 2 ms · 563 | 1 ms · 563 | <1 ms · 389 | <1 ms · 343 |
| `B3/S23 5 5 2` (enumerate all) | enumeration work, deep | 3 ms · 4,649 | 196 ms · 306,064 | 31 ms · 12,246 | 33 ms · 12,246 | 3 ms · 4,649 | 2 ms · 3,777 |
| `3457/357/5 20 16 7 -x 3 -s D2- -n a` | Generations control | 2.30 s | n/a | n/a | n/a | 3.36 s | n/a |
| `R3,C2,S2,B3,N+ 50 10 4 -x 2 -s D2-` | factorio rule, solves | 29.3 s | DNF | DNF | DNF | 14.6 s | 34.1 s |
| `R3,C2,S2,B3,N+ 50 12 3 -x 1 -s D2-` | factorio rule, large | 150.2 s | DNF | DNF | DNF | 145.5 s | DNF |
| `B2n3/S23-q 30 9 4 -x 1` | isotropic non-totalistic rule | 1.27 s | DNF | DNF | DNF | 2.77 s | 4.05 s |
| `B3/S23 20 20 2 -n r --seed 42` | deep period-2 oscillator search (random guessing) | 35.4 s | DNF | **7.59 s** | 7.06 s | 77.9 s | 108.6 s |
| `B3/S23 4 4 1` exhaust-and-grow to 8x8 | growth workflow for cross-size transfer | **5.5 s** | 30.9 s + DNF at 8x8 | 167 s | 146 s | — | — |

DNF = the uniform 240 s limit was exceeded; `n/a` = the flag/rule combination is rejected by
`Config::check`; `—` = not applicable to this workflow. Enumeration and growth cells show
`time · steps`. Growth row details: the default search exhausted every size up to 8x8 (8x8:
38.4M steps, 4.7 s); nogood also exhausted every size (8x8: 10.2M steps in 147 s);
nogood-translate exhausted every size too (8x8: 8.35M steps in 127 s); backjump exhausted
up to 7x8 and hit the limit at 8x8 (467M steps without finishing).

Which solution was found (pops = population of the first solution found; "same" = the
identical pattern):

- Spaceship row: all six configurations find the same pattern (pop 57).
- Dead-first row: default and phase-saving the same pattern (pop 53); lookahead the same
  pattern as the spaceship row (pop 57).
- 64x64 row: backjump, nogood, and nogood-translate the same pattern (pop 1848);
  phase-saving pop 1821; lookahead pop 1419 — three different patterns.
- Generations row: default pop 72, phase-saving pop 78.
- Factorio rows: 50x10 — default pop 72, phase-saving pop 82, lookahead pop 84; 50x12 —
  default and phase-saving the same pattern (pop 124).
- INT row: default and lookahead the same pattern (pop 66); phase-saving pop 43.
- Oscillator row: all five configurations find different patterns (pops 164 / 155 / 167 /
  159 / 140) — expected with random guessing, and a reminder that a heuristic change also
  changes *which* example the tool finds.

Observations:

- **The CDCL profile is remarkably uniform on solving searches**: on the factorio rule, the
  INT rule, and the dead-first spaceship case, backjumping, nogoods, and templates all hit
  the 240 s limit where the default search finishes. The analysis overhead per conflict
  dominates there, not the lack of memory (idea 1, lesson 4).
- **Very large searches remain the CDCL showcase** (64x64 row): the default search made no
  progress in 240 s (2.6B steps), while backjumping solved in 21 ms and the nogood database
  in 7 ms — the nogood firing turns the jumps themselves into propagation.
- **Deep period-2 searches with random guessing are the one measured regime where idea 2
  beats the plain search** (oscillator row): nogood is 4.7x faster than default, and the
  seed sweep below shows the win is systematic across seeds — random guessing makes the
  plain search so redundant that propagation firing's step reduction dwarfs its per-step
  cost. (The translate variant's apparent 9.6x win on this row in the original run was
  trajectory luck of the since-removed guess-time template checks.)
- **Template transfer now pays on growth workflows** (growth row): nogood-translate takes
  ~18% fewer steps at 8x8 and finishes the whole growth sequence faster than plain nogood
  (146 s vs 167 s) — the margin rules made the transferred knowledge sound and profitable,
  and also cut the template-building overhead (the single-size translate column is now
  within noise of `--nogood`, e.g. 13.7 s vs 13.1 s on the spaceship). The plain
  chronological search still beats both by an order of magnitude.
- **Phase saving** is about 2x faster on the factorio-rule 50x10 case, neutral on 50x12
  (same solution), 1.5–3x slower on the other solving searches, and it solves the 64x64
  case in 2.1 s.
- **Lookahead** reproduces its dramatic win on the dead-first spaceship case (6.9x faster;
  it finds the same pattern as the alive-first search) and loses ~1.2–3.2x on the factorio,
  INT, and oscillator searches.

Seed sweep on the oscillator case (default / `--nogood` / `--nogood-translate`, seeds 1, 2,
3, 42), run to separate mechanism from trajectory luck. The translate cells are from the
post-iteration-5 re-measurement, where translate is exactly `--nogood` plus template
bookkeeping; the pre-iteration-5 translate runs — with the since-removed guess-time checks —
measured 1.1 s (seed 1), DNF (seed 2), 5.6 s (seed 3), and 3.3 s (seed 42), pure trajectory
luck of that mechanism:

| Seed | default | `--nogood` | `--nogood-translate` |
| --- | --- | --- | --- |
| 1 | 6.0 s · 42.8M steps | 2.1 s · 414k | 2.19 s · 414k |
| 2 | 10.3 s · 72.5M steps | 1.6 s · 272k | 1.67 s · 272k |
| 3 | 125.8 s · 836M steps | 5.2 s · 773k | 5.35 s · 773k |
| 42 | 35.4 s · 252.9M steps | 6.9 s · 780k | 7.03 s · 780k |

`--nogood` is faster than the default search on every seed (the step reduction is 60–1000x
on a highly redundant random-guessing search). After iteration 5, `--nogood-translate`
reproduces the `--nogood` trajectory exactly.

Investigation probe for the shelved propagation-integrated template matching (temporary
instrumentation, since removed): a full template-library scan every 8192nd cell set counted
translated alignments — excluding the learned ones — that were *fully* matched by the
current assignment. Oscillator seed 42: 758 matches over 547 samples (~1.4 concurrent);
oscillator seed 2: 9,043 over 5,211 samples (~1.7); spaceship: 461,832 over 1,308 samples
(~350 concurrent). The assignment therefore routinely contains forbidden translated
patterns that nothing catches — but scanning the library costs ~140M operations on the
spaceship workload against a ~100-operation per-set budget (see the shelving analysis in
idea 2), so the probe's instrumentation was removed rather than turned into a mechanism.

### SAT comparison: LLS + off-the-shelf solvers (2026-08-31)

Logic Life Search (Python, version 6, git `ec6c24`) encodes a search as CNF and
calls an off-the-shelf solver; this measures the generic-SAT ceiling on the Life-like cases
that LLS can express (no higher-range rules, no Generations, no enumeration, no world
growth). Setup: same machine and 240 s limit as the table above; solver kissat (Debian
build); `--background vacuum` (border cells dead, matching factoriosrc's semantics — the
LLS default `possible_strobing` would change the problem); `-c 0 2` for exact period 4;
`-p ">0"` for the still-life case; default transition encodings (`-M 1` for Life, `-M 2`
otherwise, both others tried on the ship case). Note the formulation difference: LLS builds
a fixed window of p+1 generations with the last equal to the first translated by (dx, dy),
while factoriosrc builds p generations whose successor wraps into the first — so a ship's
LLS box must be taller/wider than factoriosrc's by roughly (p−1)·|displacement| (the ship
row uses 26x11/26x12 for factoriosrc's 26x8). Wall time includes CNF generation in Python;
solver time is LLS's own report.

| Case | CNF (vars / clauses) | LLS + kissat | factoriosrc best | factoriosrc default |
| --- | --- | --- | --- | --- |
| 25P3H: `16 6 3 -y 1` | 369 / 34,412 | 1.11 s (+0.3 s encode) | 4 ms (dead-first) | 4 ms |
| Brain: `17 12 3 -y 1 -s D2\|` | 424 / 43,369 | 2.49 s | 48 ms (dead-first) | 102 ms |
| ship: `26 11..12 4 -y 1` | 1,405 / 86k–447k (per `-M`) | DNF ×3 encodings | 1.31 s (`-n a`) | 42.3 s |
| oscillator: `20 20 2` | 1,201 / 130,954 | **0.83 s** | 7.6 s (`--nogood`) | 35.4 s |
| still life: `64 64 1` | 12,287 / 509,802 | 5.2 s | **21 ms** (`--backjump`) | DNF |
| INT: `B2n3/S23-q 33 9 4 -x 1` | 1,477 / 461,102 | DNF | 1.27 s | 1.27 s |

Observations:

- **The tutorial's times are outdated.** The LLS tutorial reports 57.5 s for the Brain
  search; with the current kissat it is 2.5 s. Any conclusion drawn from the tutorial's
  numbers (including the doubt that motivated this comparison) needs re-measurement — which
  is what this table is.
- **factoriosrc wins decisively on its home turf.** On small/medium spaceship and INT-rule
  searches it is 25–280x faster than LLS+kissat. The specialized machinery — the precomputed
  rule table (which is worth far more per decision than generic unit propagation over a CNF
  encoding), the front optimization, and the fixed search order — has no CNF analogue.
- **SAT wins the redundant regime.** The 20x20 oscillator is the one case where generic SAT
  beats every factoriosrc configuration (0.83 s vs 7.6 s for `--nogood`, 35.4 s default).
  This is the same redundancy story as the seed sweep above — kissat learns clauses over the
  whole box without paying factoriosrc's per-step interpreter cost — and it confirms that
  factoriosrc's remaining disadvantage there is engineering overhead, not architecture.
- **The 64x64 split is the most instructive row.** factoriosrc's default search DNFs,
  `--backjump`/`--nogood` solve it in 7–21 ms — beating kissat's 5.2 s by ~250x — yet those
  same flags lose on every small/medium search. The table-propagation-plus-conflict-analysis
  combination is genuinely stronger than a state-of-the-art SAT solver on this encoding when
  its overhead does not dominate.
- **Ship searches are LLS's weak spot.** The p+1 fixed-window formulation makes a 26-wide
  period-4 ship hard for kissat under all three transition encodings, where factoriosrc's
  moving-window formulation solves it in seconds.
- **Verdict on the SAT path:** as a replacement for factoriosrc's core, no. As a backend
  for specific regimes (redundant oscillator-type searches, UNSAT proving, possibly an
  optimization-search frontend), a hybrid could make sense. The remaining open SAT question
  is a hand-written encoding for higher-range rules (LLS cannot express them, and the 24-input
  transition constraint needs a counting-network decomposition); the ship and INT results
  above temper expectations. Note also: LLS's minisat backend crashes on the Debian build's
  output format; running minisat standalone on the dumped 25P3H CNF took 0.97 s — comparable
  to kissat's 1.11 s on this small instance.

## Suggested next steps

1. **Reduce the per-conflict analysis overhead** — the uniform DNF profile of the CDCL
   flags on solving searches (factorio rule, INT rule, dead-first spaceship case) is an
   overhead problem, not a memory problem (idea 1, "What remains"). This is the biggest
   unaddressed lever in the benchmark table, and the SAT comparison reinforces it: the one
   regime where generic SAT wins (the oscillator) is the one where factoriosrc's per-step
   interpreter cost, not its architecture, is the limiter.
2. **Idea 2 follow-ups (optional, speculative)**: the probe quantified a large pool of
   uncaught translated-pattern completions (see idea 2); closing it needs a fundamentally
   cheaper translated-match index — e.g. a small activity-ranked hot set with periodic
   doom scans — or accepting exact-position memory as the ceiling. No concrete design
   exists; do not attempt without one.
3. **Idea 3: VSIDS activity** as an experimental switch compared against the fixed order
   (mind the fixed-order synergy with the front optimization).
4. **Idea 4: cheaper probing** (probe less often, one state, or only cheap neighborhoods);
   the cell-selection variant needs a small search-order refactor to stay sound.
5. **SAT follow-ups (optional)**: an own CNF encoder for higher-range rules (counting-network
   decomposition) would settle whether the factorio rule is tractable for generic SAT;
   a hybrid "SAT backend for redundant/UNSAT regimes" is conceivable but has no concrete
   design yet. Learn from Minisat/Kissat with a shopping list tied to the analysis-overhead
   bottleneck (allocation-free `analyze()`, clause minimization, LBD-based database
   reduction, restart policies).
6. Ideas 5 and 6 as needed; idea 7 last, only once nogood memory and phase saving are
   mature.

## Checklist before implementing

- Conflicts from global constraints (`front_count`, `below_max`, `check_period`) are not
  learnable and must be marked specially in conflict analysis. See `docs/front.md` for the
  reasoning framework.
- Any unsafe change in `lib/src/world.rs`, `lib/src/search.rs`, or `lib/src/cell.rs` requires
  Miri: `just init`, then `cargo +nightly miri test test_miri`.
- New fields or states that need persistence must be synced with `WorldSerde`
  (`lib/src/world.rs`) and the TUI/egui save formats (which are not interchangeable, see
  AGENTS.md).
- `Config::check()` is the single source of truth for validation; if a new search strategy
  affects the supported rules, change `Config` first, then the UIs and docs — and update the
  status table of this document in the same change.
- Validate each idea separately for the combination of "enumerate all solutions +
  `reduce_max_population` + incrementally larger worlds": is the learned information still
  valid after backtracking and after rebuilding the world?
- For the CDCL machinery in particular, validate by comparing the *sets* of enumerated
  solutions against the default search (raw solution counts differ even without backjumping,
  since the search may legitimately re-find a solution, e.g. a pattern and its generation
  rotation). This differential test caught the incompleteness bug in the first 1-UIP attempt.
