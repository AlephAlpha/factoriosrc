mod cell;
mod config;
mod error;
mod rule;
mod search;
mod world;

pub use config::{Config, SearchOrder, Symmetry};
pub use error::{ConfigError, ParseSymmetryError};
pub use rule::CellState;
pub use world::{Coord, Status, World};
