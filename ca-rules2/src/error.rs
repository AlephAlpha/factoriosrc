use thiserror::Error;

/// An error that can occur when generating a list of neighbors.
#[derive(Clone, Copy, Debug, Error)]
pub enum NeighborError {
    /// The radius is too large.
    #[error("The radius is too large")]
    RadiusTooLarge,
    /// The neighborhood size is too large.
    #[error("The neighborhood size is too large")]
    NeighborhoodTooLarge,
}

/// An error that can occur when parsing a rule string.
#[derive(Clone, Copy, Debug, Error)]
pub enum RuleStringError {
    /// The syntax of the rule string is invalid.
    #[error("The syntax of the rule string is invalid")]
    InvalidSyntax,
    /// The birth or survival condition is invalid.
    #[error("The birth or survival condition is invalid")]
    InvalidCondition,
    /// The number of states is smaller than 2.
    #[error("The number of states is smaller than 2")]
    InvalidNumberOfStates,
}
