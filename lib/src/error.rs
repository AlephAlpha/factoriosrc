use thiserror::Error;

/// An error that can occur when parsing a symmetry.
#[derive(Clone, Copy, Debug, Error)]
#[error("Invalid symmetry")]
pub struct ParseSymmetryError;

/// An error that can occur when initializing a rule.
#[derive(Clone, Copy, Debug, Error)]
pub enum RuleError {
    /// The neighborhood size is too large.
    #[error("The neighborhood size is too large")]
    NeighborhoodTooLarge,
}

/// An error that can occur when initializing the search from a configuration.
#[derive(Clone, Copy, Debug, Error)]
pub enum ConfigError {
    /// The width, height, period, or diagonal width is zero.
    #[error("The width, height, period, or diagonal width is zero")]
    InvalidSize,

    /// The world is not a square when it should be.
    #[error("The world is not a square when it should be")]
    NotSquare,

    /// The world has a diagonal width when it should not.
    #[error("The world has a diagonal width when it should not")]
    HasDiagonalWidth,

    /// The translations do not satisfy the symmetry.
    #[error("The translations do not satisfy the symmetry")]
    InvalidTranslation,
}
