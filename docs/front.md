# Front Optimization

This note documents the non-empty front optimization implemented by `World::init_front()`
and `front_count` in `factoriosrc-lib`.

The original rlifesrc discussion is in issue #81:
https://github.com/AlephAlpha/rlifesrc/issues/81

It is intentionally narrower than the original rlifesrc design discussion. The goal here is to
explain the invariants that the current code relies on, so future work on more rules, known
cells, symmetries, transformations, or custom search orders has a concrete checklist.

## Scope

This document describes the current factoriosrc implementation, not every rule family that
rlifesrc discusses.

At the app level, `Config::parse_rule()` currently accepts only:

- rules with 2 states, or Generations rules with up to 255 states
- totalistic neighborhoods with size at most 24
- isotropic non-totalistic rules with a range-1 Moore or hexagonal neighborhood (size at most 8)
- non-isotropic (MAP) rules with a range-1 Moore, von Neumann, or hexagonal neighborhood

`B0` rules are supported: the cells outside the search range are assumed to follow a uniform
periodic background, and a front cell is empty exactly when it is equal to that background state.
See ["B0 rules"](#b0-rules) below for details.

Within that subset, a front cell is empty exactly when it is in the background state. That lets the
current code use one simple counter: `front_count` is the number of front cells that are still
unknown or not in the background state. Dying cells are counted as non-empty, which is
conservative but sound for Generations rules.

## Why the optimization works

If the search constraints are invariant under a translation toward the search front, then any
solution whose front is entirely empty can be shifted one step toward the front and still satisfy
the same constraints.

That makes the all-empty front redundant, so the search can reject it immediately. In the current
implementation, that rejection happens in `check_affected()`: when `front_count` reaches zero, the
current partial assignment is discarded.

If the relevant invariance argument does not hold, factoriosrc falls back to treating the whole
first generation as the front instead of enabling the stronger optimization.

## How factoriosrc chooses the front

`init_front()` first tries to prove that the current search setup supports the stronger front
optimization. If that proof fails, it marks the entire first generation as front.

### Row-first search

Row-first front pruning is enabled only when all of the following hold:

- the symmetry is a subgroup of `D2H`
- the transformation is an element of `D2H`
- there is no diagonal width

When it is enabled:

- if `dx == 0`, only the left half of the front row is used
- if `dx == 0` and `dy >= 0`, the front is row `max(dy, 1) - 1` of generation `0`
- otherwise, the front is row `0` across all generations

### Column-first search

Column-first front pruning is enabled only when all of the following hold:

- the symmetry is a subgroup of `D2V`
- the transformation is an element of `D2V`
- there is no diagonal width

When it is enabled:

- if `dy == 0`, only the top half of the front column is used
- if `dy == 0` and `dx >= 0`, the front is column `max(dx, 1) - 1` of generation `0`
- otherwise, the front is column `0` across all generations

### Diagonal search

Diagonal front pruning is enabled only when all of the following hold:

- the symmetry is a subgroup of `D2D`
- the transformation is an element of `D2D`

When it is enabled:

- if `dx == dy` and `dx >= 0`, the front is row `max(dy, 1) - 1` of generation `0`
- otherwise, the front is row `0` across all generations plus column `0` across all generations
- if `dx != dy`, the column part starts at `y = 1` to avoid double-counting `(0, 0, t)`

### Fallback

If none of the cases above apply, factoriosrc marks every cell in generation `0` as front.

This is weaker, but it avoids relying on a translation or reflection argument that the current
configuration does not satisfy.

## `front_count` is not the same as the number of front cells

`init_front()` marks cells with `is_front`, but `front_count` tracks only front cells that are not
yet proven empty.

That means:

- `init_known()` may immediately reduce `front_count` if some front cells are forced to be dead
- `set_cell()` decrements `front_count` only when a front cell becomes dead
- `unset_cell()` increments `front_count` only when undoing a dead assignment on a front cell

This is why `front_count` is the value used for pruning, while `is_front` is just the structural
definition of the front.

## B0 rules

A rule contains `B0` when a dead cell with no living neighbors becomes alive in the next
generation. In that case, the cells outside the search range cannot be assumed to be dead, since
they would all become alive in the next generation. Instead, they are assumed to follow a uniform
periodic background, which is a periodic orbit of the rule:

- for a rule without `B0`, the background is always dead (period 1);
- for a rule with `B0` but not the maximum survival condition (`S-max`, e.g. `S8`), the
  background cycles through all the states of the rule (period `num_states`; for a 2-state rule,
  it alternates between dead and alive);
- for a rule with both `B0` and `S-max`, the background is always alive (period 1).

The period of the searched pattern must be a multiple of the background period, otherwise the
pattern cannot be embedded in an infinite periodic universe. This is checked by `Config::check()`.

For a `B0` rule, "empty" means being equal to the background state of that generation, so a front
cell is not necessarily dead. `front_count` is decremented exactly when a front cell is set to its
background state.

### The front covers the first `background_period` generations

The strengthened fronts that are restricted to the first generation (the `dy >= 0` row-first case,
the `dx >= 0` column-first case, and the `dx == dy` diagonal case) rely on the generation-rotation
argument: a pattern whose front is empty can be rotated in time so that a front cell becomes
non-empty on the first generation.

For a `B0` rule, rotating the pattern in time changes the phase of the background, so a rotated
pattern is only valid if the background is constant (i.e. the rule has `S-max`). Otherwise, the
first `background_period` generations are used instead of the first generation: a pattern whose
front is empty on the first `background_period` generations is empty on every generation, so it
can be shifted toward the front. This matches rlifesrc's `fn_is_front` (`t < max_t` with
`max_t = gen()` for `B0` rules).

For a rule with both `B0` and `S-max`, the background is constant, so the generation-rotation
argument holds and the front covers only the first generation, like for a rule without `B0`.

### The fallback is still sound for `B0` rules

The fallback front (the whole first generation) is used when the translation or reflection
argument for the stronger fronts does not apply. For a `B0` rule, a pattern whose first generation
is entirely in the background state evolves into the pure background (the unique orbit through the
background state, since the period is a multiple of the background period), so the whole pattern is
trivial. The fallback therefore rejects only the trivial pattern, and remains sound.

## What future features need to preserve

### Known cells

Known cells are fixed to absolute coordinates. That breaks the translation invariance that the
front optimization relies on, unless the known-cell constraint itself is closed under the same
translation or reflection argument.

The current implementation takes the conservative route:

- any non-empty known-cell set disables the stronger front optimization
- the search falls back to treating the whole first generation as front

This is intentionally conservative. A future implementation may re-enable stronger front pruning
for specific known-cell sets, but only after proving that the chosen constraints are compatible
with the relevant translation or reflection argument.

### More rules

factoriosrc now supports Generations rules with up to 255 states, and `B0` rules (including
`B0S8` rules, where the background is alive). The notion of an empty front cell is defined by the
background state: `front_count` is decremented exactly when a front cell is set to its background
state (which is the dead state for a rule without `B0`). A front cell in a dying state is still
counted as non-empty, so the front optimization is weaker for patterns that contain dying cells,
but it never rejects a valid pattern. Since a dying cell can never become alive, this is still
sound.

There is also a rule-symmetry assumption in the current implementation. Most rules that
factoriosrc currently supports are fully symmetric on the square grid, so their symmetry can be
described by the dihedral group `D8`. That is why `init_front()` only has to reason about the
pattern's symmetry and transformation, not the rule's own symmetry.

However, isotropic non-totalistic rules may have a smaller symmetry group than `D8` (for example,
a rule with a single anisotropic birth class), and hexagonal rules are invariant only under `R0`,
`R2`, `S1`, and `S3`. For such rules, rule symmetry is part of the front proof. The front pruning
cannot assume that every reflection or rotation used by the current argument preserves the rule
itself.

The groundwork for this is already in place:

- `ca-rules2` computes the symmetry group of a rule via `Rule::symmetry_elements()`.
- `Config::check()` rejects configurations whose pattern symmetry or transformation is not a
  subgroup (element) of the rule's symmetry group. This guarantees that any reflection or
  rotation used by the search is compatible with the rule.
- `init_front()` additionally checks the rule's symmetry group directly, so that a `World`
  built with a rule of smaller symmetry falls back to the weaker front definition instead of
  relying on an argument that does not hold:
  - the row-first halved front (used when `dx == 0`) requires the rule to be invariant under
    the horizontal reflection `S2`,
  - the column-first halved front (used when `dy == 0`) requires the rule to be invariant under
    the vertical reflection `S0`,
  - the diagonal front with `dx == dy` requires the rule to be invariant under the diagonal
    reflection `S1`.

Since every rule currently accepted by `Config::parse_rule()` is invariant under the whole of
`D8` (or, for hexagonal rules, under the symmetries of the hexagonal grid), these conditions only
disable the front optimization when the user chooses a pattern symmetry or transformation that is
not compatible with the rule.

### Custom symmetries, transformations, and search orders

Any extension in these areas should avoid adding more ad hoc cases directly into `init_front()`.
The important question is always the same: does this configuration preserve the translation or
reflection argument that makes the chosen front non-empty without loss of generality?

If the answer is not obviously yes, fall back to the weaker definition first and add a proof and a
test before tightening it again.