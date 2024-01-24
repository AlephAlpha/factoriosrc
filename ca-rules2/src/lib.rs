//! Parsing rule strings of life-like and other cellular automata.
//!
//! More documentation will be added later.

#![warn(clippy::missing_const_for_fn)]
#![warn(missing_docs)]

mod error;
mod parse;
mod rule;

pub use error::{NeighborError, RuleStringError};
pub use parse::{parse_generations, parse_life_like};
pub use rule::{Neighbor, NeighborhoodType, Rule};
