//! A library for searching for patterns in Factorio cellular automata.
//!
//! More documentation will be added later.

#![warn(missing_docs)]
#![warn(clippy::nursery)]
#![warn(clippy::unnested_or_patterns)]
#![warn(clippy::uninlined_format_args)]

mod cell;
mod config;
mod error;
mod export;
mod help;
mod nogood;
mod rule;
mod search;
mod world;

pub use ca_symmetry::{Symmetry, Transformation, TranslationCondition};
pub use cell::Reason;
pub use config::{Config, KnownCell, NewState, SearchOrder};
pub use error::ConfigError;
#[cfg(not(target_arch = "wasm32"))]
pub use export::save_generation;
pub use export::{DEFAULT_EXPORT_TEMPLATE, ExportError, ExportFields, Template, TemplateError};
pub use help::{ConfigHelpField, SearchControlHelpField};
pub use nogood::{NogoodDb, NogoodStats};
pub use rule::{CellState, RuleTable};
pub use world::{Coord, Status, World};
