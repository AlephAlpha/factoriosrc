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

    /// The period is not a multiple of the background period.
    ///
    /// For a B0 rule, the cells outside the search range are assumed to follow
    /// a uniform background of the given period, and the period of the
    /// searched pattern must be a multiple of it.
    #[error("The period must be a multiple of the background period ({0}) for a B0 rule")]
    InvalidPeriod(u32),

    /// The world is not a square when it should be.
    #[error("The world is not a square when it should be")]
    NotSquare,

    /// The world has a diagonal width when it should not.
    #[error("The world has a diagonal width when it should not")]
    HasDiagonalWidth,

    /// The translations do not satisfy the symmetry.
    #[error("The translations do not satisfy the symmetry")]
    InvalidTranslation,

    /// The symmetry is not compatible with the rule.
    #[error("The symmetry is not compatible with the rule")]
    SymmetryIncompatibleWithRule,

    /// The transformation is not compatible with the rule.
    #[error("The transformation is not compatible with the rule")]
    TransformationIncompatibleWithRule,

    /// Backjumping is enabled for a Generations rule.
    #[error("Backjumping is only supported for rules with 2 states")]
    BackjumpUnsupported,

    /// Lookahead is enabled for a Generations rule.
    #[error("Lookahead is only supported for rules with 2 states")]
    LookaheadUnsupported,

    /// A known cell is outside the world.
    #[error("A known cell is outside the world")]
    InvalidKnownCell,

    /// Known cell constraints conflict with each other.
    #[error("Known cell constraints conflict with each other")]
    ConflictingKnownCells,
}

/// An error that can occur when deserializing a [`World`].
#[cfg(feature = "serde")]
#[derive(Clone, Copy, Debug, Error)]
pub enum SerdeError {
    /// The configuration is invalid.
    #[error("The configuration is invalid: {0}")]
    InvalidConfig(#[from] ConfigError),

    /// The index of a cell is out of bounds.
    #[error("The index of a cell is out of bounds")]
    OutOfBounds,

    /// The stack is invalid.
    #[error("The stack is invalid")]
    InvalidStack,
}
