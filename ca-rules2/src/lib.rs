//! A crate for parsing and working with cellular automata rules.
//!
//! Currently, the following kinds of rules are supported:
//!
//! - [Life-like rules](https://conwaylife.com/wiki/Life-like_cellular_automaton),
//!   see [`parse_life_like`].
//! - [Isotropic non-totalistic (INT) rules](https://conwaylife.com/wiki/Isotropic_non-totalistic_rule),
//!   see [`parse_int_life`] and [`parse_int_hex`].
//! - [Generations rules](https://conwaylife.com/wiki/Generations), see
//!   [`parse_generations`].
//! - [HROT rules](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton),
//!   see [`parse_hrot`].
//! - [Non-isotropic rules](https://conwaylife.com/wiki/Non-isotropic_rule),
//!   see [`parse_map`].
//!
//! [`parse_rule`] supports all of the above, and chooses the right kind of
//! rule automatically.

#![warn(missing_docs)]
#![warn(clippy::nursery)]

mod error;
mod int;
mod parse;
mod rule;

pub use error::{NeighborError, ParseRuleError};
pub use parse::{
    parse_generations, parse_hrot, parse_int_hex, parse_int_life, parse_life_like, parse_map,
    parse_rule,
};
pub use rule::{Neighbor, Neighborhood, NeighborhoodType, Rule};
