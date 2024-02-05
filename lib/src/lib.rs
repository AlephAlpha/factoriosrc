//! A library for searching for patterns in Factorio cellular automata.
//!
//! More documentation will be added later.

#![warn(missing_docs)]
#![warn(clippy::nursery)]
#![warn(clippy::unnested_or_patterns)]
#![warn(clippy::uninlined_format_args)]
#![allow(clippy::redundant_pub_crate)]

mod cell;
mod config;
mod error;
mod rule;
mod search;
mod symmetry;
mod world;

pub use config::{Config, NewState, SearchOrder};
pub use error::ConfigError;
pub use rule::{CellState, RuleTable};
pub use symmetry::{Symmetry, Transformation};
pub use world::{Coord, Status, World};
