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
- totalistic, non-hexagonal neighborhoods

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

The safe default for a future known-cells feature is therefore:

- disable the stronger front optimization when arbitrary known cells are present
- or re-enable it only after proving that the known-cell set is compatible with the chosen front

### More rules

If factoriosrc starts supporting `B0`, Generations, or other multi-state rule families, the notion
of an empty front cell stops being equivalent to dead. At that point, `front_count` will need to
track emptiness, not just dead-vs-non-dead.

There is also a rule-symmetry assumption in the current implementation. The rules that factoriosrc
currently supports are fully symmetric on the square grid, so their symmetry can be described by
the dihedral group `D8`. That is why `init_front()` only has to reason about the pattern's
symmetry and transformation, not the rule's own symmetry.

If future support includes rule families whose symmetry is smaller than `D8`, such as hexagonal
rules or fully asymmetric rules, rule symmetry becomes another part of the front proof. In that
world, front pruning can no longer assume that every reflection or rotation used by the current
argument preserves the rule itself.

### Custom symmetries, transformations, and search orders

Any extension in these areas should avoid adding more ad hoc cases directly into `init_front()`.
The important question is always the same: does this configuration preserve the translation or
reflection argument that makes the chosen front non-empty without loss of generality?

If the answer is not obviously yes, fall back to the weaker definition first and add a proof and a
test before tightening it again.