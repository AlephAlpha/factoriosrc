//! A crate for parsing and working with cellular automata rules.
//!
//! More documentation will be added later.

#![warn(clippy::missing_const_for_fn)]
#![warn(missing_docs)]

mod error;
mod parse;
mod rule;

pub use error::{NeighborError, ParseRuleError};
pub use parse::{parse_generations, parse_hrot, parse_life_like};
pub use rule::{Neighbor, NeighborhoodType, Rule};
