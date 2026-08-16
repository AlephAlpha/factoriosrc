# SAT Solver Techniques for factoriosrc

> **Status**: This note is a collection of ideas and a preliminary analysis. **Nothing has been
> implemented yet.** Every direction below is at the "worth trying" stage; the concrete designs,
> data structures, and integration with the existing code all remain to be discussed and
> experimented with.

## Background and purpose

factoriosrc's core is a constraint satisfaction search over a three-dimensional (x, y, generation)
periodic cell grid. The README's todo list contains "Taking inspiration from SAT solvers (the CDCL
algorithm) and constraint programming". This note maps factoriosrc's current algorithm onto the
vocabulary of SAT solvers, points out the gaps, and lists some directions worth exploring. Each
direction only discusses the idea, the expected payoff, and the risks; it does not propose a
concrete implementation.

## Where we are: factoriosrc is already a DPLL solver

Overview of the search loop:

- `World::search` (`lib/src/search.rs:624`) calls `step()` in a loop;
- `step()` (`lib/src/search.rs:561`) first calls `check_stack()` (propagation), then, if there is
  no conflict, `guess()` (branching);
- Propagation: `set_cell` (`lib/src/world.rs:708`) pushes the cell onto the `stack`; the part of
  the stack after `stack_index` forms a queue of cells to check (`lib/src/world.rs:106-118`).
  `check_affected` (`lib/src/search.rs:396`) checks the descriptors of the set cell itself, its
  neighbors, and its predecessor;
- Deduction: `RuleTable::implies` (`lib/src/rule.rs:583`) looks up a precomputed table by the
  descriptor and returns a `CheckResult` (`lib/src/rule.rs:376`), which may deduce the state of
  the successor, the current cell, all unknown neighbors (totalistic rules), or individual
  neighbors (non-totalistic rules, see the `forced` bits);
- Branching: `guess()` (`lib/src/search.rs:529`) walks the `next` chain (a fixed spatial order,
  built by `init_next`, `lib/src/world.rs:506`), picks the next unknown cell, and sets it to
  Alive, Dead, or a random state according to `Config::new_state`;
- Backtracking: `backtrack()` (`lib/src/search.rs:475`) pops the stack; when it reaches a
  `Reason::Guessed` cell, it flips the state (2-state rules) or tries the next state in the cycle
  (Generations rules, `Reason::TryAnother`);
- When a full assignment is found, `check_period` (`lib/src/search.rs:581`) rejects patterns of a
  smaller period, then the search backtracks to enumerate the next solution.

Correspondence table:

| factoriosrc mechanism | SAT solver counterpart |
| --- | --- |
| `check_affected` + the `stack`/`stack_index` propagation queue | BCP (unit propagation); the incremental descriptor updates are similar to watchlists |
| The precomputed `RuleTable` | A precompiled failed-literal / unit-propagation closure |
| `guess()` walking the `next` chain | DPLL-style branching in a fixed order |
| `backtrack()` undoing the latest guess | Chronological backtracking |
| `Reason` (`lib/src/cell.rs:14`) | Records the decision level, but **not the antecedent** |

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
  `lib/src/search.rs:286`). Learning and backjumping must preserve this asymmetry.
- **The goal is to enumerate all solutions.** factoriosrc does not stop at the first solution; it
  keeps enumerating (combined with `reduce_max_population` for optimization searches, and
  "search for the next solution" in the GUIs). This affects the applicability of restart-based
  techniques.
- **Unsafe hot paths.** Changes to `LifeCell`/`World` touch unsafe code (`lib/src/cell.rs`,
  `lib/src/world.rs`, `lib/src/search.rs`) and need Miri verification (see AGENTS.md).

## Idea 1: Record antecedents + conflict analysis + non-chronological backtracking

The core CDCL idea, and the foundation for most of the others.

### What the antecedent is

When `check_descriptor_implied` (`lib/src/search.rs:60`) deduces the state of a successor,
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

## Idea 2: Forbidden pattern memory (a nogood database)

The CA version of CDCL clause learning. A learned nogood is a set of relative coordinates plus
states that cannot be extended to a solution; it is essentially a forbidden local pattern.

### Normalization

A nogood is stored in relative coordinates, so it can be translated to any position and any
generation. Canonicalizing nogoods with `Config::symmetry` (picking a minimal representative of
each equivalence class) can significantly improve the hit rate.

### Reuse across world sizes

`increase_world_size` (`lib/src/world.rs:1039`) rebuilds the `World` from scratch, throwing away
all search experience. But a nogood learned in a smaller world remains valid in a larger world as
long as it does not rely on the "outside the world is dead" boundary assumption. This fits
factoriosrc's typical workflow of gradually enlarging the world while searching, and may be the
single biggest practical win among the ideas here.

### Mind the boundary conditions

Cells at the boundary of a small world are forced dead (`init_known`, `lib/src/world.rs:598`). A
nogood that relies on this is not valid in a larger world. Filter by "at least the rule radius
away from the boundary", or record for each nogood whether it uses the boundary-dead assumption.

### Database management

Analogous to the clause databases of modern SAT solvers: evict by activity/LBD to bound memory
use. As a start, one could store only minimal conflict sets and evict the oldest entries when
full.

### An easier start

Precompute "rule-specific static forbidden patterns" that do not depend on the instance, e.g.
"this 2x2 patch can never be all-alive under this rule". This is a small static nogood library
that can be used to evaluate the hit rate and query cost before building the full machinery.

## Idea 3: Phase saving and decision heuristics

Lowest risk, a good first experiment.

### Phase saving (the MiniSat idea)

Each cell remembers the last state tried or deduced for it, and prefers that state when it is
guessed again. This replaces the global `Config::new_state` policy of Alive/Dead/Random. The
change is concentrated in `guess()`, barely touches data structures, and is equally valid when
enumerating multiple solutions (it is only a heuristic, not a correctness issue).

### VSIDS-style activity

Give cells that participate in conflicts a score bump, and branch on the most active cell. The
risk: the fixed spatial order of the `next` chain is an important synergy with the front
optimization and local propagation (the front argument in `docs/front.md` does not depend on the
guess order, but the order affects propagation efficiency). A safe approach is to use activity
only within a local window of the current order, or to make it an experimental switch to compare
against the fixed order.

## Idea 4: Probing before branching (lookahead)

The CA version of SatZ-style lookahead / DLIS.

- The current lookup tables already embed one level of failed literals:
  `Implication::NeighborhoodAlive/Dead` (`lib/src/rule.rs:360-366`) are exactly "setting an
  unknown neighbor to some state leads to a conflict", precomputed; the `forced` bits of
  non-totalistic rules are the same idea for individual neighbors.
- **One level deeper**: before branching, probe candidate cells — set, propagate for k steps,
  count deductions/check for conflicts, unset. Use the score to rank candidate cells. A conflict
  found while probing is a free failed-literal prune.
- The world is small, `check_affected` is cheap, and everything is local, which fits factoriosrc
  well.
- This needs a snapshot/rollback mechanism: the existing set/unset + stack machinery can be
  reused, or probing can run on a separate small stack.

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

1. Idea 3 (phase saving): purely incremental, zero structural risk; first measure the payoff of
   heuristics at this scale;
2. The foundation of idea 1: attach antecedents to `Reason::Deduced` (record only, do not change
   the algorithm yet) — all later CDCL-style techniques depend on it;
3. Idea 4 (probing-based ranking): an easier win on top of that foundation than full conflict
   analysis;
4. The full idea 1 (1-UIP backjumping) → idea 2 (cross-size nogoods): the long-term goals;
5. Ideas 5 and 6 as needed; idea 7 last.

## Things to re-check before implementing

- Conflicts from global constraints (front_count, below_max, check_period) are not learnable and
  must be marked specially in conflict analysis. See `docs/front.md` for the reasoning framework.
- Any unsafe change in `lib/src/world.rs`, `lib/src/search.rs`, or `lib/src/cell.rs` requires
  `cargo +nightly miri test test_miri`.
- New fields or states that need persistence must be synced with `WorldSerde`
  (`lib/src/world.rs:1062`) and the TUI/egui save formats (which are not interchangeable, see
  AGENTS.md).
- `Config::check()` (`lib/src/config.rs`) is the single source of truth for validation; if a new
  search strategy affects the supported rules, change `Config` first, then the UIs and docs.
- Each idea must be validated separately for the combination of "enumerate all solutions +
  reduce_max_population + incrementally larger worlds": is the learned information still valid
  after backtracking and after rebuilding the world?
