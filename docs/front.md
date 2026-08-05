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

- 2-state rules
- non-`B0` rules
- totalistic neighborhoods with size at most 24
- isotropic non-totalistic rules with a range-1 Moore or hexagonal neighborhood (size at most 8)
- non-isotropic (MAP) rules with a range-1 Moore, von Neumann, or hexagonal neighborhood

Within that subset, a front cell is empty exactly when it is dead. That lets the current code use
one simple counter: `front_count` is the number of front cells that are still unknown or alive.

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

If factoriosrc starts supporting `B0`, Generations, or other multi-state rule families, the notion
of an empty front cell stops being equivalent to dead. At that point, `front_count` will need to
track emptiness, not just dead-vs-non-dead.

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