#[cfg(feature = "documented")]
use crate::Config;
use crate::{NewState, SearchOrder, Status};
#[cfg(feature = "documented")]
use documented::DocumentedFields;

/// Shared help keys for [`Config`] fields.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ConfigHelpField {
    /// The rule string field.
    RuleString,
    /// The width field.
    Width,
    /// The height field.
    Height,
    /// The period field.
    Period,
    /// The horizontal translation field.
    Dx,
    /// The vertical translation field.
    Dy,
    /// The diagonal width field.
    DiagonalWidth,
    /// The symmetry field.
    Symmetry,
    /// The transformation field.
    Transformation,
    /// The search-order field.
    SearchOrder,
    /// The new-state field.
    NewState,
    /// The phase-saving field.
    PhaseSaving,
    /// The lookahead field.
    Lookahead,
    /// The backjump field.
    Backjump,
    /// The nogood field.
    Nogood,
    /// The random-seed field.
    Seed,
    /// The known-cells field.
    KnownCells,
    /// The max-population field.
    MaxPopulation,
    /// The reduce-max-population field.
    ReduceMaxPopulation,
}

impl ConfigHelpField {
    /// Iterate over all shared config help fields.
    #[inline]
    pub fn iter() -> impl Iterator<Item = Self> {
        [
            Self::RuleString,
            Self::Width,
            Self::Height,
            Self::Period,
            Self::Dx,
            Self::Dy,
            Self::DiagonalWidth,
            Self::Symmetry,
            Self::Transformation,
            Self::SearchOrder,
            Self::NewState,
            Self::PhaseSaving,
            Self::Lookahead,
            Self::Backjump,
            Self::Nogood,
            Self::Seed,
            Self::KnownCells,
            Self::MaxPopulation,
            Self::ReduceMaxPopulation,
        ]
        .into_iter()
    }

    /// Whether this field belongs to the experimental group.
    #[inline]
    pub const fn is_experimental(self) -> bool {
        matches!(
            self,
            Self::PhaseSaving | Self::Lookahead | Self::Backjump | Self::Nogood
        )
    }

    /// A UI-friendly field label.
    #[inline]
    pub const fn label(self) -> &'static str {
        match self {
            Self::RuleString => "Rule",
            Self::Width => "Width",
            Self::Height => "Height",
            Self::Period => "Period",
            Self::Dx => "DX",
            Self::Dy => "DY",
            Self::DiagonalWidth => "Diagonal width",
            Self::Symmetry => "Symmetry",
            Self::Transformation => "Transformation",
            Self::SearchOrder => "Search order",
            Self::NewState => "New state",
            Self::PhaseSaving => "Phase saving",
            Self::Lookahead => "Lookahead",
            Self::Backjump => "Backjump",
            Self::Nogood => "Nogood",
            Self::Seed => "Seed",
            Self::KnownCells => "Known cells",
            Self::MaxPopulation => "Max population",
            Self::ReduceMaxPopulation => "Reduce max population",
        }
    }

    /// A tooltip-safe short help string.
    #[inline]
    pub const fn short_help(self) -> &'static str {
        match self {
            Self::RuleString => {
                "Cellular automaton rule in Life-like, higher-range totalistic, or Generations syntax."
            }
            Self::Width => "Width of the search world in cells.",
            Self::Height => "Height of the search world in cells.",
            Self::Period => "Number of generations in the repeating cycle.",
            Self::Dx => "Horizontal translation applied over one full period.",
            Self::Dy => "Vertical translation applied over one full period.",
            Self::DiagonalWidth => {
                "Optional diagonal band that constrains cells outside it to be in the background state."
            }
            Self::Symmetry => "Required symmetry of the searched pattern.",
            Self::Transformation => "Transformation applied before translation each period.",
            Self::SearchOrder => {
                "Traversal order for unresolved cells. Auto usually picks a sensible default."
            }
            Self::NewState => "How unknown cells are guessed during search.",
            Self::PhaseSaving => {
                "Experimental: remember each cell's last state and guess it first. May help or hurt depending on the rule and new-state strategy."
            }
            Self::Lookahead => {
                "Experimental: probe both states of an unknown cell before guessing it. Only for 2-state rules. May help or hurt."
            }
            Self::Backjump => {
                "Experimental: CDCL-style conflict analysis with non-chronological backtracking. Only for 2-state rules. Usually slower than the default backtracking."
            }
            Self::Nogood => {
                "Experimental: remember learned forbidden patterns and use them to reject guesses. Enables Backjump. Only for 2-state rules."
            }
            Self::Seed => "Random seed used only when New state is random.",
            Self::KnownCells => "Pinned alive/dead cells that must hold before the search starts.",
            Self::MaxPopulation => "Optional upper bound on the population.",
            Self::ReduceMaxPopulation => {
                "Tighten the population bound whenever a smaller solution is found."
            }
        }
    }

    #[cfg(feature = "documented")]
    const fn field_name(self) -> &'static str {
        match self {
            Self::RuleString => "rule_str",
            Self::Width => "width",
            Self::Height => "height",
            Self::Period => "period",
            Self::Dx => "dx",
            Self::Dy => "dy",
            Self::DiagonalWidth => "diagonal_width",
            Self::Symmetry => "symmetry",
            Self::Transformation => "transformation",
            Self::SearchOrder => "search_order",
            Self::NewState => "new_state",
            Self::PhaseSaving => "phase_saving",
            Self::Lookahead => "lookahead",
            Self::Backjump => "backjump",
            Self::Nogood => "nogood",
            Self::Seed => "seed",
            Self::KnownCells => "known_cells",
            Self::MaxPopulation => "max_population",
            Self::ReduceMaxPopulation => "reduce_max_population",
        }
    }

    /// The long help sourced from the documented field docs.
    #[cfg(feature = "documented")]
    #[inline]
    pub fn long_help(self) -> &'static str {
        Config::get_field_docs(self.field_name()).expect("missing Config field docs")
    }
}

impl Status {
    /// A tooltip-safe short help string.
    #[inline]
    pub const fn short_help(self) -> &'static str {
        match self {
            Self::NotStarted => "Not started yet.",
            Self::Running => "Searching...",
            Self::Solved => "A solution was found.",
            Self::NoSolution => "No more solutions.",
        }
    }
}

/// Shared help keys for search-app runtime controls.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchControlHelpField {
    /// The world-growth toggle.
    IncreaseWorldSize,
    /// The continue-after-solution toggle.
    NoStop,
    /// The result-export template field.
    ExportResults,
    /// The step-size field.
    Step,
}

impl SearchControlHelpField {
    /// A UI-friendly field label.
    #[inline]
    pub const fn label(self) -> &'static str {
        match self {
            Self::IncreaseWorldSize => "Increase size",
            Self::NoStop => "No stop",
            Self::ExportResults => "Export results",
            Self::Step => "Step",
        }
    }

    /// A tooltip-safe short help string.
    #[inline]
    pub const fn short_help(self) -> &'static str {
        match self {
            Self::IncreaseWorldSize => {
                "Restart with a slightly larger world after an exhausted search."
            }
            Self::NoStop => "Keep searching after the first solution instead of pausing.",
            Self::ExportResults => {
                "Save every found solution to compact RLE files. A file-name template with \
                 {rule}, {width}, {height}, {period}, {dx}, {dy}, {symmetry}, {transformation}, \
                 {index}, {generation}, and {population} placeholders. Leave empty to disable."
            }
            Self::Step => "Display and update interval in search steps.",
        }
    }
}

impl SearchOrder {
    /// A tooltip-safe short help string.
    #[inline]
    pub const fn short_help(self) -> &'static str {
        match self {
            Self::RowFirst => "Search unresolved cells row by row.",
            Self::ColumnFirst => "Search unresolved cells column by column.",
            Self::Diagonal => {
                "Search unresolved cells in diagonal order. Useful for diagonal spaceships."
            }
        }
    }

    /// The long help sourced from the documented variant docs.
    #[cfg(feature = "documented")]
    #[inline]
    pub const fn long_help(self) -> &'static str {
        match self {
            Self::RowFirst => Self::FIELD_DOCS[0],
            Self::ColumnFirst => Self::FIELD_DOCS[1],
            Self::Diagonal => Self::FIELD_DOCS[2],
        }
    }
}

impl NewState {
    /// A tooltip-safe short help string.
    #[inline]
    pub const fn short_help(self) -> &'static str {
        match self {
            Self::Alive => "Guess that unknown cells are alive.",
            Self::Dead => "Guess that unknown cells are dead.",
            Self::Random => "Guess unknown cells randomly.",
        }
    }

    /// The long help sourced from the documented variant docs.
    #[cfg(feature = "documented")]
    #[inline]
    pub const fn long_help(self) -> &'static str {
        match self {
            Self::Alive => Self::FIELD_DOCS[0],
            Self::Dead => Self::FIELD_DOCS[1],
            Self::Random => Self::FIELD_DOCS[2],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_short_help_is_concise() {
        assert_eq!(
            ConfigHelpField::KnownCells.short_help(),
            "Pinned alive/dead cells that must hold before the search starts."
        );
    }

    #[test]
    fn search_order_short_help_covers_diagonal() {
        assert!(SearchOrder::Diagonal.short_help().contains("diagonal"));
    }

    #[test]
    fn config_field_iteration_covers_core_fields() {
        let fields: Vec<_> = ConfigHelpField::iter().collect();
        assert!(fields.contains(&ConfigHelpField::RuleString));
        assert!(fields.contains(&ConfigHelpField::KnownCells));
        assert_eq!(fields.len(), 19);
    }

    #[test]
    fn experimental_group_covers_sat_heuristics() {
        let experimental: Vec<_> = ConfigHelpField::iter()
            .filter(|field| field.is_experimental())
            .collect();
        assert_eq!(
            experimental,
            vec![
                ConfigHelpField::PhaseSaving,
                ConfigHelpField::Lookahead,
                ConfigHelpField::Backjump,
                ConfigHelpField::Nogood,
            ]
        );
    }

    #[cfg(feature = "documented")]
    #[test]
    fn long_help_uses_documented_docs() {
        assert!(
            ConfigHelpField::RuleString
                .long_help()
                .contains("Currently, the program supports")
        );
        assert!(
            SearchOrder::Diagonal
                .long_help()
                .contains("This is useful for finding diagonal spaceships.")
        );
    }
}
