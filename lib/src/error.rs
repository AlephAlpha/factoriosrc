use thiserror::Error;

/// An error that can occur when initializing the search from a configuration.
#[derive(Clone, Copy, Debug, Error)]
pub enum ConfigError {
    /// The rule string is invalid.
    #[error("The rule string is invalid")]
    InvalidRule,

    /// The rule is not supported.
    #[error("The rule is not supported")]
    UnsupportedRule,

    /// The width, height, period, or diagonal width is zero.
    #[error("The width, height, period, or diagonal width is zero")]
    InvalidSize,

    /// The population upper bound is zero.
    #[error("The population upper bound is zero")]
    InvalidMaxPopulation,

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
