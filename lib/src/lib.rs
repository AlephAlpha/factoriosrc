//! A library for searching for patterns in Factorio cellular automata.
//!
//! More documentation will be added later.

#![warn(clippy::missing_const_for_fn)]
#![warn(clippy::use_self)]
#![warn(missing_docs)]

mod cell;
mod config;
mod error;
mod rule;
mod search;
mod world;

pub use config::{Config, NewState, SearchOrder, Symmetry};
pub use error::{ConfigError, ParseSymmetryError};
pub use rule::{CellState, NeighborhoodType, Rule, RuleTable};
pub use world::{Coord, Status, World};
