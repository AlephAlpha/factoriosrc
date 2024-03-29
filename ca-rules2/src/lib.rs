//! A crate for parsing and working with cellular automata rules.
//!
//! More documentation will be added later.

#![warn(missing_docs)]
#![warn(clippy::nursery)]

mod error;
mod parse;
mod rule;

pub use error::{NeighborError, ParseRuleError};
pub use parse::{parse_generations, parse_hrot, parse_life_like, parse_rule};
pub use rule::{Neighbor, Neighborhood, NeighborhoodType, Rule};
