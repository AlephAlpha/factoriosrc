# ca-rules2

A crate for parsing and working with cellular automata rules. This is a rewrite of [ca-rules](https://crates.io/crates/ca-rules) crate.

Currently it only supports [higher-range outer-totalistic rules](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton). These are the rules that are supported by factoriosrc.

A rule is defined by the following data:

- The number of states.
- The neighborhood.
- A list of numbers that represent the birth conditions.
- A list of numbers that represent the survival conditions.

Currently factoriosrc only supports rules with 2 states, but this crate should support rules with more states.
