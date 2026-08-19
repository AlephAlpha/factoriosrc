use crate::{
    error::ConfigError,
    rule::{CellState, INT_MAX_NEIGHBORHOOD_SIZE, MAX_NEIGHBORHOOD_SIZE},
};
use ca_rules2::{Neighborhood, Rule};
use ca_symmetry::{Symmetry, Transformation};
#[cfg(feature = "clap")]
use clap::{Args, ValueEnum};
#[cfg(feature = "documented")]
use documented::{Documented, DocumentedFields};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, str::FromStr};
use strum::{Display, EnumIter, EnumString, IntoEnumIterator};

/// Search order.
///
/// This is used to determine how we find the next unknown cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Display, EnumIter, EnumString)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
#[cfg_attr(feature = "documented", derive(Documented, DocumentedFields))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum SearchOrder {
    /// Search in row-major order.
    ///
    /// ```text
    /// 1 2 3
    /// 4 5 6
    /// 7 8 9
    /// ```
    #[cfg_attr(feature = "clap", value(name = "row", alias = "r"))]
    #[cfg_attr(feature = "serde", serde(rename = "row"))]
    #[strum(serialize = "row")]
    RowFirst,

    /// Search in column-major order.
    ///
    /// ```text
    /// 1 4 7
    /// 2 5 8
    /// 3 6 9
    /// ```
    #[cfg_attr(feature = "clap", value(name = "column", alias = "c"))]
    #[cfg_attr(feature = "serde", serde(rename = "column"))]
    #[strum(serialize = "column")]
    ColumnFirst,

    /// Search in diagonal order.
    ///
    /// ```text
    /// 1 3 6
    /// 2 5 8
    /// 4 7 9
    /// ```
    ///
    /// This is useful for finding diagonal spaceships.
    ///
    /// This requires the world to be square.
    #[cfg_attr(feature = "clap", value(name = "diagonal", alias = "d"))]
    #[cfg_attr(feature = "serde", serde(rename = "diagonal"))]
    #[strum(serialize = "diagonal")]
    Diagonal,
}

impl SearchOrder {
    /// An iterator over all possible search orders.
    #[inline]
    pub fn iter() -> impl Iterator<Item = Self> {
        <Self as IntoEnumIterator>::iter()
    }
}

/// How to guess the state of an unknown cell.
///
/// The default is [`Dead`](NewState::Dead).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Display, EnumIter, EnumString)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
#[cfg_attr(feature = "documented", derive(Documented, DocumentedFields))]
#[cfg_attr(
    feature = "serde",
    derive(Serialize, Deserialize),
    serde(rename_all = "lowercase")
)]
pub enum NewState {
    /// Guess that the cell is alive.
    #[cfg_attr(feature = "clap", value(alias = "a"))]
    Alive,

    /// Guess that the cell is dead.
    #[default]
    #[cfg_attr(feature = "clap", value(alias = "d"))]
    Dead,

    /// Make a random guess.
    ///
    /// The probability of each state is 50% for a rule with 2 states.
    /// For a Generations rule, the probability of each state is `1 / num_states`.
    #[cfg_attr(feature = "clap", value(alias = "r"))]
    Random,
}

impl NewState {
    /// An iterator over all possible [`NewState`]s.
    #[inline]
    pub fn iter() -> impl Iterator<Item = Self> {
        <Self as IntoEnumIterator>::iter()
    }
}

/// A cell whose state is fixed before the search starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct KnownCell {
    /// The horizontal coordinate of the cell.
    pub x: u32,

    /// The vertical coordinate of the cell.
    pub y: u32,

    /// The generation of the cell.
    pub t: u32,

    /// The fixed state of the cell.
    pub state: CellState,
}

impl KnownCell {
    /// Create a known cell at the given coordinates.
    #[inline]
    pub const fn new(x: u32, y: u32, t: u32, state: CellState) -> Self {
        Self { x, y, t, state }
    }
}

/// The configuration of the world.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(Args))]
#[cfg_attr(feature = "documented", derive(Documented, DocumentedFields))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Config {
    /// The rule string of the cellular automaton.
    ///
    /// Currently, the program supports the following rules:
    ///
    /// - [Outer-totalistic Life-like rules](https://conwaylife.com/wiki/Life-like_cellular_automaton).
    ///   Moore, von Neumann, and hexagonal neighborhoods are supported.
    ///
    /// - [Higher-range outer-totalistic Life-like rules](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton).
    ///   Currently, the program only supports Moore, von Neumann, cross, hash, and hexagonal
    ///   neighborhoods.
    ///   The size of the neighborhood must be at most 24.
    ///
    /// - [Isotropic non-totalistic rules](https://conwaylife.com/wiki/Isotropic_non-totalistic_rule).
    ///   Both the range-1 Moore neighborhood and the range-1 hexagonal neighborhood
    ///   (emulated on a square grid) are supported. Hexagonal isotropic non-totalistic
    ///   rules must be written with the class letters `o`, `m`, and `p`, so that they
    ///   are recognized as isotropic non-totalistic rules. Hexagonal rules written
    ///   without class letters are treated as (outer-)totalistic rules.
    ///
    /// - [Generations rules](https://conwaylife.com/wiki/Generations),
    ///   with at most 255 states. All the neighborhoods above are supported.
    ///
    /// Rules whose birth conditions contain `0` are not supported.
    ///
    /// The default rule is [factorio (R3,C2,S2,B3,N+)](https://conwaylife.com/forums/viewtopic.php?f=11&t=6166).
    #[cfg_attr(feature = "clap", arg(short, long, default_value = "R3,C2,S2,B3,N+"))]
    pub rule_str: String,

    /// Width of the search world in cells.
    #[cfg_attr(feature = "clap", arg(default_value_t = 0))]
    pub width: u32,

    /// Height of the search world in cells.
    #[cfg_attr(feature = "clap", arg(default_value_t = 0))]
    pub height: u32,

    /// Number of generations in the repeating cycle.
    #[cfg_attr(feature = "clap", arg(default_value_t = 1))]
    pub period: u32,

    /// Horizontal translation of the world.
    ///
    /// The pattern is translated by `dx` cells to the left in each period.
    ///
    /// In other words, if the period is `p`, then a cell at position `(x, y)`
    /// on the `p`-th generation should have the same state as a cell at position
    /// `(x + dx, y + dy)` on the 0-th generation.
    #[cfg_attr(
        feature = "clap",
        arg(short = 'x', long, allow_negative_numbers = true, default_value_t = 0)
    )]
    #[cfg_attr(feature = "serde", serde(default))]
    pub dx: i32,

    /// Vertical translation of the world.
    ///
    /// The pattern is translated by `dy` cells upwards in each period.
    ///
    /// In other words, if the period is `p`, then a cell at position `(x, y)`
    /// on the `p`-th generation should have the same state as a cell at position
    /// `(x + dx, y + dy)` on the 0-th generation.
    #[cfg_attr(
        feature = "clap",
        arg(short = 'y', long, allow_negative_numbers = true, default_value_t = 0)
    )]
    #[cfg_attr(feature = "serde", serde(default))]
    pub dy: i32,

    /// Diagonal width of the world.
    ///
    /// If the diagonal width is `n`, then cells at positions `(x, y)`
    /// where `abs(x - y) >= n` are always dead.
    ///
    /// This is useful for finding diagonal spaceships.
    ///
    /// If this is not [`None`], then the world must be square.
    #[cfg_attr(feature = "clap", arg(short, long))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub diagonal_width: Option<u32>,

    /// Symmetry of the pattern.
    ///
    /// There are 10 possible symmetries, corresponding to the 10 subgroups of the
    /// [dihedral group _D_<sub>8</sub>](https://en.wikipedia.org/wiki/Dihedral_group).
    ///
    /// Some symmetries require the world to be square.
    /// Some require the world to have no diagonal width.
    /// Some require the world to have no translation.
    ///
    /// The notation is borrowed from the Oscar Cunningham's
    /// [Logic Life Search](https://github.com/OscarCunningham/logic-life-search).
    #[cfg_attr(feature = "clap", arg(short, long, value_enum, default_value = "C1"))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub symmetry: Symmetry,

    /// Transformation of the pattern.
    ///
    /// There are 8 possible transformations, corresponding to the 8 elements of the
    /// [dihedral group D8](https://en.wikipedia.org/wiki/Dihedral_group).
    ///
    /// In each period, the pattern is first transformed according to the transformation,
    /// then translated according to [`dx`](crate::Config::dx) and [`dy`](crate::Config::dy).
    ///
    /// In other words, if the period is `p`, and the transformation maps `(x, y)` to
    /// `(x', y')`, then the cell at position `(x', y')` on the `p`-th generation should
    /// have the same state as the cell at position `(x + dx, y + dy)` on the 0-th
    /// generation.
    ///
    /// Some transformations require the world to be square.
    /// Some require the world to have no diagonal width.
    /// Some require the world to have no translation.
    ///
    /// The notation is based on the notation used in group theory.
    #[cfg_attr(feature = "clap", arg(short, long, value_enum, default_value = "R0"))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub transformation: Transformation,

    /// Traversal order for unresolved cells.
    ///
    /// [`None`] means that the search order is chosen automatically.
    #[cfg_attr(feature = "clap", arg(short = 'o', long, value_enum))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub search_order: Option<SearchOrder>,

    /// How to guess the state of an unknown cell.
    ///
    /// The default is [`Dead`](NewState::Dead).
    #[cfg_attr(feature = "clap", arg(short, long, value_enum, default_value = "dead"))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub new_state: NewState,

    /// Whether to remember the last state of each cell and guess it first.
    ///
    /// When this is `true`, each cell remembers the last state it was set to
    /// (by guessing, deduction, or from the configuration), and the next time
    /// the cell is guessed, the remembered state is tried first.
    ///
    /// This is an experimental heuristic inspired by the phase saving
    /// heuristic of SAT solvers; it does not always help. Whether it helps
    /// depends on the rule and the [`new_state`](Config::new_state) strategy:
    /// on the default rule `R3,C2,S2,B3,N+` it gives a small speedup (about
    /// 5%) with the default dead strategy and a larger one (about 2x) with
    /// the random strategy, but it can also slow the search down for other
    /// rules and strategies. The default is `false`.
    #[cfg_attr(feature = "clap", arg(long))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub phase_saving: bool,

    /// Whether to probe the states of an unknown cell before guessing it.
    ///
    /// When this is `true`, before guessing the state of an unknown cell,
    /// both possible states are temporarily set and propagated for a bounded
    /// number of deductions, and the probe is then rolled back. If a state
    /// leads to a conflict, the other state is guessed; if both states lead
    /// to a conflict, the search backtracks; otherwise, the state that led to
    /// more deductions is guessed first.
    ///
    /// This is an experimental heuristic inspired by the lookahead / failed
    /// literal technique of SAT solvers; it does not always help. It only
    /// applies to rules with 2 states. The default is `false`.
    #[cfg_attr(feature = "clap", arg(long))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub lookahead: bool,

    /// Random seed for guessing the state of an unknown cell.
    ///
    /// This is only used when [`new_state`](Config::new_state) is [`Random`](NewState::Random).
    ///
    /// If this is [`None`], then the seed is randomly generated.
    #[cfg_attr(feature = "clap", arg(long))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub seed: Option<u64>,

    /// Cells whose states are fixed before the search starts.
    ///
    /// Coordinates are absolute positions in the search world.
    /// All known cells must lie inside the current world bounds.
    ///
    /// Repeated coordinates with the same state are deduplicated during validation.
    /// Repeated coordinates with different states are rejected.
    #[cfg_attr(feature = "clap", arg(skip))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub known_cells: Vec<KnownCell>,

    /// Upper bound of the population of the pattern.
    ///
    /// If the period is greater than 1, then this is the upper bound of the minimum population
    /// among all the generations.
    ///
    /// If this is [`None`], then the population is not bounded.
    #[cfg_attr(feature = "clap", arg(short, long))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub max_population: Option<usize>,

    /// Whether to reduce the upper bound of the population when a solution is found.
    ///
    /// If this is `true`, when a solution with population `p` is found, then
    /// [`max_population`](Config::max_population) will be set to `p - 1`.
    ///
    /// This is useful for finding the smallest possible pattern.
    #[cfg_attr(feature = "clap", arg(long))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub reduce_max_population: bool,
}

impl Config {
    /// Create a new configuration.
    #[inline]
    pub fn new(rule_str: &str, width: u32, height: u32, period: u32) -> Self {
        Self {
            rule_str: rule_str.to_string(),
            width,
            height,
            period,
            dx: 0,
            dy: 0,
            diagonal_width: None,
            symmetry: Symmetry::C1,
            transformation: Transformation::R0,
            search_order: None,
            new_state: NewState::Dead,
            phase_saving: false,
            lookahead: false,
            seed: None,
            known_cells: Vec::new(),
            max_population: None,
            reduce_max_population: false,
        }
    }

    /// Set horizontal and vertical translations.
    ///
    /// See [`dx`](Config::dx) and [`dy`](Config::dy) for more details.
    #[inline]
    #[must_use]
    pub const fn with_translations(mut self, dx: i32, dy: i32) -> Self {
        self.dx = dx;
        self.dy = dy;
        self
    }

    /// Set the diagonal width.
    ///
    /// See [`diagonal_width`](Config::diagonal_width) for more details.
    #[inline]
    #[must_use]
    pub const fn with_diagonal_width(mut self, diagonal_width: u32) -> Self {
        self.diagonal_width = Some(diagonal_width);
        self
    }

    /// Set the symmetry.
    ///
    /// See [`symmetry`](Config::symmetry) for more details.
    #[inline]
    #[must_use]
    pub const fn with_symmetry(mut self, symmetry: Symmetry) -> Self {
        self.symmetry = symmetry;
        self
    }

    /// Set the transformation.
    ///
    /// See [`transformation`](Config::transformation) for more details.
    #[inline]
    #[must_use]
    pub const fn with_transformation(mut self, transformation: Transformation) -> Self {
        self.transformation = transformation;
        self
    }

    /// Set the search order.
    ///
    /// See [`search_order`](Config::search_order) for more details.
    #[inline]
    #[must_use]
    pub const fn with_search_order(mut self, search_order: SearchOrder) -> Self {
        self.search_order = Some(search_order);
        self
    }

    /// Set how to guess the state of an unknown cell.
    ///
    /// See [`new_state`](Config::new_state) for more details.
    #[inline]
    #[must_use]
    pub const fn with_new_state(mut self, new_state: NewState) -> Self {
        self.new_state = new_state;
        self
    }

    /// Enable remembering the last state of each cell and guessing it first.
    ///
    /// See [`phase_saving`](Config::phase_saving) for more details.
    #[inline]
    #[must_use]
    pub const fn with_phase_saving(mut self) -> Self {
        self.phase_saving = true;
        self
    }

    /// Enable probing the states of an unknown cell before guessing it.
    ///
    /// See [`lookahead`](Config::lookahead) for more details.
    #[inline]
    #[must_use]
    pub const fn with_lookahead(mut self) -> Self {
        self.lookahead = true;
        self
    }

    /// Set the random seed for guessing the state of an unknown cell.
    ///
    /// See [`seed`](Config::seed) for more details.
    #[inline]
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Add a known cell to the configuration.
    #[inline]
    #[must_use]
    pub fn with_known_cell(mut self, known_cell: KnownCell) -> Self {
        self.known_cells.push(known_cell);
        self
    }

    /// Add multiple known cells to the configuration.
    #[inline]
    #[must_use]
    pub fn with_known_cells<I>(mut self, known_cells: I) -> Self
    where
        I: IntoIterator<Item = KnownCell>,
    {
        self.known_cells.extend(known_cells);
        self
    }

    /// Set the upper bound of the population of the pattern.
    ///
    /// See [`max_population`](Config::max_population) for more details.
    #[inline]
    #[must_use]
    pub const fn with_max_population(mut self, max_population: usize) -> Self {
        self.max_population = Some(max_population);
        self
    }

    /// Enable reducing the upper bound of the population when a solution is found.
    ///
    /// See [`reduce_max_population`](Config::reduce_max_population) for more details.
    #[inline]
    #[must_use]
    pub const fn with_reduce_max_population(mut self) -> Self {
        self.reduce_max_population = true;
        self
    }

    /// Whether the configuration requires the world to be square.
    #[inline]
    pub const fn requires_square(&self) -> bool {
        self.symmetry.requires_square()
            || self.transformation.requires_square()
            || self.diagonal_width.is_some()
            || matches!(self.search_order, Some(SearchOrder::Diagonal))
    }

    /// Whether the symmetry or the transformation requires the world to have no diagonal width.
    #[inline]
    pub const fn requires_no_diagonal_width(&self) -> bool {
        self.symmetry.requires_no_diagonal_width()
            || self.transformation.requires_no_diagonal_width()
    }

    /// Whether the translation is compatible with the symmetry.
    #[inline]
    pub const fn translation_is_valid(&self) -> bool {
        self.symmetry.translation_is_valid(self.dx, self.dy)
    }

    /// Try to parse the rule string, and check whether the rule is supported.
    ///
    /// Currently, the program supports the following rules:
    /// - [Outer-totalistic Life-like rules](https://conwaylife.com/wiki/Life-like_cellular_automaton).
    ///   Moore, von Neumann, and hexagonal neighborhoods are supported.
    /// - [Higher-range outer-totalistic Life-like rules](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton).
    ///   Currently, the program only supports Moore, von Neumann, cross, hash, and hexagonal
    ///   neighborhoods.
    ///   The size of the neighborhood must be at most 24.
    /// - [Isotropic non-totalistic rules](https://conwaylife.com/wiki/Isotropic_non-totalistic_rule).
    ///   Both the range-1 Moore neighborhood and the range-1 hexagonal neighborhood
    ///   (emulated on a square grid) are supported.
    /// - [Non-isotropic rules](https://conwaylife.com/wiki/Non-isotropic_rule),
    ///   in the form of [MAP strings](https://conwaylife.com/wiki/MAP_string).
    ///   The range-1 Moore, von Neumann, and hexagonal neighborhoods are supported.
    /// - [Generations rules](https://conwaylife.com/wiki/Generations),
    ///   with at most 255 states. All the neighborhoods above are supported.
    ///
    /// Rules whose birth conditions contain `0` are not supported.
    #[inline]
    pub fn parse_rule(&self) -> Result<Rule, ConfigError> {
        let rule = Rule::from_str(&self.rule_str).map_err(|_| ConfigError::InvalidRule)?;

        if rule.contains_b0() || rule.states > 255 {
            return Err(ConfigError::UnsupportedRule);
        }

        match &rule.neighborhood {
            Neighborhood::Totalistic(_, _) if rule.neighborhood_size() <= MAX_NEIGHBORHOOD_SIZE => {
            }
            Neighborhood::Nontotalistic(_, _)
                if rule.neighborhood_size() <= INT_MAX_NEIGHBORHOOD_SIZE => {}
            _ => return Err(ConfigError::UnsupportedRule),
        }

        Ok(rule)
    }

    /// Check whether the configuration is valid,
    /// find a search order if it is not specified,
    /// and remove duplicate known cells.
    pub fn check(&mut self) -> Result<(), ConfigError> {
        let rule = self.parse_rule()?;
        check_rule_symmetry(&rule, self.symmetry, self.transformation)?;

        if self.width == 0
            || self.height == 0
            || self.period == 0
            || self.diagonal_width.is_some_and(|d| d == 0)
        {
            return Err(ConfigError::InvalidSize);
        }

        if self.max_population.is_some_and(|p| p == 0) {
            return Err(ConfigError::InvalidMaxPopulation);
        }

        let mut known_cells = BTreeMap::new();
        for known_cell in &self.known_cells {
            if known_cell.x >= self.width
                || known_cell.y >= self.height
                || known_cell.t >= self.period
                || known_cell.state.number() as u64 >= rule.states
            {
                return Err(ConfigError::InvalidKnownCell);
            }

            match known_cells.insert((known_cell.x, known_cell.y, known_cell.t), known_cell.state) {
                Some(state) if state != known_cell.state => {
                    return Err(ConfigError::ConflictingKnownCells);
                }
                _ => {}
            }
        }

        self.known_cells = known_cells
            .into_iter()
            .map(|((x, y, t), state)| KnownCell::new(x, y, t, state))
            .collect();

        if self.width != self.height && self.requires_square() {
            return Err(ConfigError::NotSquare);
        }

        if self.diagonal_width.is_some() && self.requires_no_diagonal_width() {
            return Err(ConfigError::HasDiagonalWidth);
        }

        if !self.translation_is_valid() {
            return Err(ConfigError::InvalidTranslation);
        }

        // If the search order is not specified, determine it automatically.
        if self.search_order.is_none() {
            // If the world is symmetric with respect to horizontal reflection,
            // we only need to search the left half of the world.
            let width = if self.transformation == Transformation::S2
                || Transformation::S2.is_element_of(self.symmetry)
            {
                self.width.div_ceil(2)
            } else {
                self.width
            };

            // If the world is symmetric with respect to vertical reflection,
            // we only need to search the upper half of the world.
            let height = if self.transformation == Transformation::S0
                || Transformation::S0.is_element_of(self.symmetry)
            {
                self.height.div_ceil(2)
            } else {
                self.height
            };

            // If the world is symmetric with respect to diagonal reflection,
            // we only need to search the lower triangle of the world.
            let diagonal_width = if self.transformation == Transformation::S1
                || Transformation::S1.is_element_of(self.symmetry)
            {
                self.diagonal_width.or(Some(self.width))
            } else {
                self.diagonal_width.map(|d| 2 * d + 1)
            };

            // The shortest edge should be searched first.
            let search_order = if diagonal_width.is_some_and(|d| d <= width && d <= height) {
                SearchOrder::Diagonal
            } else if width < height {
                SearchOrder::RowFirst
            } else if width > height {
                SearchOrder::ColumnFirst
            } else {
                // If the world is square, check the translations.
                if self.dx.abs() < self.dy.abs() {
                    SearchOrder::RowFirst
                } else {
                    SearchOrder::ColumnFirst
                }
            };

            self.search_order = Some(search_order);
        }

        Ok(())
    }
}

/// Check that the symmetry and the transformation are compatible with the rule.
///
/// The symmetry and the transformation must be subgroups (elements) of the symmetry
/// group of the rule. Otherwise, the search may find patterns that do not actually
/// satisfy the rule.
fn check_rule_symmetry(
    rule: &Rule,
    symmetry: Symmetry,
    transformation: Transformation,
) -> Result<(), ConfigError> {
    let rule_symmetry = rule.symmetry_elements();

    if !rule_symmetry.contains(&transformation) {
        return Err(ConfigError::TransformationIncompatibleWithRule);
    }

    if !symmetry
        .transformations()
        .all(|t| rule_symmetry.contains(&t))
    {
        return Err(ConfigError::SymmetryIncompatibleWithRule);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ca_rules2::NeighborhoodType;

    #[test]
    fn test_parse_rule_accepts_supported_rules() {
        assert!(Config::new("B3/S23", 3, 3, 1).parse_rule().is_ok());
        assert!(Config::new("R3,C2,S2,B3,N+", 5, 5, 1).parse_rule().is_ok());
        assert!(Config::new("B2a/S12", 3, 3, 1).parse_rule().is_ok());
        assert!(Config::new("B2o/S12oH", 3, 3, 1).parse_rule().is_ok());
        assert!(Config::new("B2/S34H", 3, 2, 2).parse_rule().is_ok());
        assert!(Config::new("B245/S3H", 3, 3, 1).parse_rule().is_ok());
        assert!(Config::new("MAPHmlphg", 3, 3, 1).parse_rule().is_ok());
        assert!(
            Config::new("MAPFgFoF2gXgH5oF4B+gH4A6A", 3, 3, 1)
                .parse_rule()
                .is_ok()
        );
        assert!(Config::new("MAPARYXfhZofugWaH7oaIDogBZofuhogOiAaIDogIAAgAAWaH7oaIDogGiA6ICAAIAAaIDogIAAgACAAIAAAAAAAA", 3, 3, 1).parse_rule().is_ok());
        assert!(Config::new("3457/357/5", 3, 3, 1).parse_rule().is_ok());
        assert!(Config::new("B2a/S12/3", 3, 3, 1).parse_rule().is_ok());
        assert!(Config::new("B2/S/3", 3, 3, 1).parse_rule().is_ok());
    }

    #[test]
    fn test_parse_rule_rejects_invalid_rule_strings() {
        assert!(matches!(
            Config::new("not a rule", 3, 3, 1).parse_rule(),
            Err(ConfigError::InvalidRule)
        ));
    }

    #[test]
    fn test_parse_rule_rejects_unsupported_rules() {
        for rule in [
            "B03/S23",
            "B2/S/300",
            "R3,C2,S2,B3",
            "MAP/////w==",
            "B2a/S12/256",
        ] {
            assert!(matches!(
                Config::new(rule, 3, 3, 1).parse_rule(),
                Err(ConfigError::UnsupportedRule)
            ));
        }
    }

    #[test]
    fn test_check_rejects_known_cell_states_out_of_range() {
        assert!(matches!(
            Config::new("B3/S23", 3, 3, 1)
                .with_known_cell(KnownCell::new(0, 0, 0, CellState::Dying(2)))
                .check(),
            Err(ConfigError::InvalidKnownCell)
        ));

        assert!(matches!(
            Config::new("3457/357/5", 3, 3, 1)
                .with_known_cell(KnownCell::new(0, 0, 0, CellState::Dying(5)))
                .check(),
            Err(ConfigError::InvalidKnownCell)
        ));

        assert!(
            Config::new("3457/357/5", 3, 3, 1)
                .with_known_cell(KnownCell::new(0, 0, 0, CellState::Dying(4)))
                .check()
                .is_ok()
        );
    }

    #[test]
    fn test_check_rule_symmetry() {
        // A rule that is invariant under all 8 transformations.
        let life = Rule {
            states: 2,
            neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
            birth: vec![3],
            survival: vec![2, 3],
        };
        assert!(check_rule_symmetry(&life, Symmetry::D8, Transformation::R1).is_ok());
        assert!(check_rule_symmetry(&life, Symmetry::C1, Transformation::R0).is_ok());

        // A rule that is invariant only under the identity and the diagonal reflection.
        let only_nw = Rule {
            states: 2,
            neighborhood: Neighborhood::Nontotalistic(NeighborhoodType::Moore, 1),
            birth: vec![1],
            survival: Vec::new(),
        };
        assert!(check_rule_symmetry(&only_nw, Symmetry::D2D, Transformation::S1).is_ok());
        assert!(matches!(
            check_rule_symmetry(&only_nw, Symmetry::D2H, Transformation::R0),
            Err(ConfigError::SymmetryIncompatibleWithRule)
        ));
        assert!(matches!(
            check_rule_symmetry(&only_nw, Symmetry::C1, Transformation::S0),
            Err(ConfigError::TransformationIncompatibleWithRule)
        ));
    }

    #[test]
    fn test_check_rejects_invalid_configurations() {
        assert!(matches!(
            Config::new("B3/S23", 0, 3, 1).check(),
            Err(ConfigError::InvalidSize)
        ));

        assert!(matches!(
            Config::new("B3/S23", 3, 3, 1)
                .with_max_population(0)
                .check(),
            Err(ConfigError::InvalidMaxPopulation)
        ));

        assert!(matches!(
            Config::new("B3/S23", 3, 4, 1)
                .with_search_order(SearchOrder::Diagonal)
                .check(),
            Err(ConfigError::NotSquare)
        ));

        assert!(matches!(
            Config::new("B3/S23", 4, 4, 1)
                .with_diagonal_width(2)
                .with_symmetry(Symmetry::D2H)
                .check(),
            Err(ConfigError::HasDiagonalWidth)
        ));

        assert!(matches!(
            Config::new("B3/S23", 4, 4, 1)
                .with_translations(1, 0)
                .with_symmetry(Symmetry::D2H)
                .check(),
            Err(ConfigError::InvalidTranslation)
        ));

        assert!(matches!(
            Config::new("B3/S23", 4, 4, 1)
                .with_known_cell(KnownCell::new(4, 0, 0, CellState::Alive))
                .check(),
            Err(ConfigError::InvalidKnownCell)
        ));

        assert!(matches!(
            Config::new("B3/S23", 4, 4, 1)
                .with_known_cells([
                    KnownCell::new(1, 1, 0, CellState::Alive),
                    KnownCell::new(1, 1, 0, CellState::Dead),
                ])
                .check(),
            Err(ConfigError::ConflictingKnownCells)
        ));
    }

    #[test]
    fn test_check_chooses_automatic_search_order() {
        let mut row_first = Config::new("B3/S23", 2, 5, 1);
        row_first.check().unwrap();
        assert_eq!(row_first.search_order, Some(SearchOrder::RowFirst));

        let mut column_first = Config::new("B3/S23", 5, 2, 1);
        column_first.check().unwrap();
        assert_eq!(column_first.search_order, Some(SearchOrder::ColumnFirst));

        let mut diagonal = Config::new("B3/S23", 5, 5, 1)
            .with_diagonal_width(3)
            .with_transformation(Transformation::S1);
        diagonal.check().unwrap();
        assert_eq!(diagonal.search_order, Some(SearchOrder::Diagonal));
    }

    #[test]
    fn test_check_preserves_explicit_search_order() {
        let mut config = Config::new("B3/S23", 2, 5, 1).with_search_order(SearchOrder::ColumnFirst);
        config.check().unwrap();
        assert_eq!(config.search_order, Some(SearchOrder::ColumnFirst));
    }

    #[test]
    fn test_check_deduplicates_identical_known_cells() {
        let mut config = Config::new("B3/S23", 3, 3, 1).with_known_cells([
            KnownCell::new(1, 1, 0, CellState::Alive),
            KnownCell::new(1, 1, 0, CellState::Alive),
        ]);

        config.check().unwrap();

        assert_eq!(
            config.known_cells,
            vec![KnownCell::new(1, 1, 0, CellState::Alive)]
        );
    }
}
