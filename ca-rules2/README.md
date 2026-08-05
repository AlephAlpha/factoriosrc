# ca-rules2

A crate for parsing and working with cellular automata rules. This is a rewrite of [ca-rules](https://crates.io/crates/ca-rules) crate.

Currently it supports the following kinds of rules:

- [Higher-range outer-totalistic rules](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton).
- [Isotropic non-totalistic rules](https://conwaylife.com/wiki/Isotropic_non-totalistic_rule) with the range-1 Moore or hexagonal neighborhood.
- [Non-isotropic rules](https://conwaylife.com/wiki/Non-isotropic_rule) with the range-1 Moore, von Neumann, or hexagonal neighborhood, in the form of [MAP strings](https://conwaylife.com/wiki/MAP_string).
- [Generations rules](https://conwaylife.com/wiki/Generations).

A rule is defined by the following data:

- The number of states.
- The neighborhood.
- A list of numbers that represent the birth conditions.
- A list of numbers that represent the survival conditions.

Currently factoriosrc only supports rules with 2 states, but this crate should support rules with more states.
