//! Parsing rule strings of life-like and other cellular automata.
//!
//! More documentation will be added later.

#![warn(clippy::missing_const_for_fn)]
#![warn(missing_docs)]

mod rule;

pub use rule::{Neighbor, NeighborhoodType, Rule};
