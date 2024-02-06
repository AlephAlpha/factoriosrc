use crate::{
    error::ConfigError,
    rule::MAX_NEIGHBORHOOD_SIZE,
    symmetry::{Symmetry, Transformation},
};
use ca_rules2::{Neighborhood, NeighborhoodType, Rule};
#[cfg(feature = "clap")]
use clap::{Args, ValueEnum};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::str::FromStr;

/// Search order.
///
/// This is used to determine how we find the next unknown cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
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
    Diagonal,
}

/// How to guess the state of an unknown cell.
///
/// The default is [`Dead`](NewState::Dead).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
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
    /// The probability of each state is 50%.
    #[cfg_attr(feature = "clap", value(alias = "r"))]
    Random,
}

/// The configuration of the world.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "clap", derive(Args))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Config {
    /// The rule string of the cellular automaton.
    ///
    /// Currently, the program supports the following rules:
    ///
    /// - [Outer-totalistic Life-like rules](https://conwaylife.com/wiki/Life-like_cellular_automaton).
    ///   Both Moore and von Neumann neighborhoods are supported.
    ///
    /// - [Higher-range outer-totalistic Life-like rules](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton).
    ///   Currently, the program only supports Moore, von Neumann, and cross neighborhoods.
    ///   The size of the neighborhood must be at most 24.
    ///   Rules with more than 2 states are not supported.
    ///
    /// Rules whose birth conditions contain `0` are not supported.
    ///
    /// The default rule is [factorio (R3,C2,S2,B3,N+)](https://conwaylife.com/forums/viewtopic.php?f=11&t=6166).
    #[cfg_attr(feature = "clap", arg(short, long, default_value = "R3,C2,S2,B3,N+"))]
    pub rule_str: String,

    /// Width of the world.
    pub width: u32,

    /// Height of the world.
    pub height: u32,

    /// Period of the pattern.
    #[cfg_attr(feature = "clap", arg(default_value = "1"))]
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
        arg(short = 'x', long, allow_negative_numbers = true, default_value = "0")
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
        arg(short = 'y', long, allow_negative_numbers = true, default_value = "0")
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

    /// Search order.
    ///
    /// [`None`] means that the search order is automatically determined.
    #[cfg_attr(feature = "clap", arg(short = 'o', long, value_enum))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub search_order: Option<SearchOrder>,

    /// How to guess the state of an unknown cell.
    ///
    /// The default is [`Dead`](NewState::Dead).
    #[cfg_attr(feature = "clap", arg(short, long, value_enum, default_value = "dead"))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub new_state: NewState,

    /// Random seed for guessing the state of an unknown cell.
    ///
    /// Only used if [`new_state`](Config::new_state) is [`Random`](NewState::Random).
    ///
    /// If this is [`None`], then the seed is randomly generated.
    #[cfg_attr(feature = "clap", arg(long))]
    #[cfg_attr(feature = "serde", serde(default))]
    pub seed: Option<u64>,

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
    /// If this is [`true`], when a solution with population `p` is found, then
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
            seed: None,
            max_population: None,
            reduce_max_population: false,
        }
    }

    /// Set horizontal and vertical translations.
    ///
    /// See [`dx`](Config::dx) and [`dy`](Config::dy) for more details.
    #[inline]
    pub const fn with_translations(mut self, dx: i32, dy: i32) -> Self {
        self.dx = dx;
        self.dy = dy;
        self
    }

    /// Set the diagonal width.
    ///
    /// See [`diagonal_width`](Config::diagonal_width) for more details.
    #[inline]
    pub const fn with_diagonal_width(mut self, diagonal_width: u32) -> Self {
        self.diagonal_width = Some(diagonal_width);
        self
    }

    /// Set the symmetry.
    ///
    /// See [`symmetry`](Config::symmetry) for more details.
    #[inline]
    pub const fn with_symmetry(mut self, symmetry: Symmetry) -> Self {
        self.symmetry = symmetry;
        self
    }

    /// Set the transformation.
    ///
    /// See [`transformation`](Config::transformation) for more details.
    #[inline]
    pub const fn with_transformation(mut self, transformation: Transformation) -> Self {
        self.transformation = transformation;
        self
    }

    /// Set the search order.
    ///
    /// See [`search_order`](Config::search_order) for more details.
    #[inline]
    pub const fn with_search_order(mut self, search_order: SearchOrder) -> Self {
        self.search_order = Some(search_order);
        self
    }

    /// Set how to guess the state of an unknown cell.
    ///
    /// See [`new_state`](Config::new_state) for more details.
    #[inline]
    pub const fn with_new_state(mut self, new_state: NewState) -> Self {
        self.new_state = new_state;
        self
    }

    /// Set the random seed for guessing the state of an unknown cell.
    ///
    /// See [`seed`](Config::seed) for more details.
    #[inline]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = Some(seed);
        self
    }

    /// Set the upper bound of the population of the pattern.
    ///
    /// See [`max_population`](Config::max_population) for more details.
    #[inline]
    pub const fn with_max_population(mut self, max_population: usize) -> Self {
        self.max_population = Some(max_population);
        self
    }

    /// Enable reducing the upper bound of the population when a solution is found.
    ///
    /// See [`reduce_max_population`](Config::reduce_max_population) for more details.
    #[inline]
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
    ///   Both Moore and von Neumann neighborhoods are supported.
    /// - [Higher-range outer-totalistic Life-like rules](https://conwaylife.com/wiki/Higher-range_outer-totalistic_cellular_automaton).
    ///   Currently, the program only supports Moore, von Neumann, and cross neighborhoods.
    ///   The size of the neighborhood must be at most 24.
    ///   Rules with more than 2 states are not supported.
    ///
    /// Rules whose birth conditions contain `0` are not supported.
    #[inline]
    pub fn parse_rule(&self) -> Result<Rule, ConfigError> {
        let rule = Rule::from_str(&self.rule_str).map_err(|_| ConfigError::InvalidRule)?;

        if rule.contains_b0() {
            return Err(ConfigError::UnsupportedRule);
        }

        if !matches!(rule.neighborhood, Neighborhood::Totalistic(neighborhood_type, _) if neighborhood_type != NeighborhoodType::Hexagonal)
        {
            return Err(ConfigError::UnsupportedRule);
        }

        let neighborhood_size = rule.neighborhood_size();

        if neighborhood_size > MAX_NEIGHBORHOOD_SIZE {
            return Err(ConfigError::UnsupportedRule);
        }

        Ok(rule)
    }

    /// Check whether the configuration is valid,
    /// and find a search order if it is not specified.
    pub fn check(mut self) -> Result<Self, ConfigError> {
        self.parse_rule()?;

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
                (self.width + 1) / 2
            } else {
                self.width
            };

            // If the world is symmetric with respect to vertical reflection,
            // we only need to search the upper half of the world.
            let height = if self.transformation == Transformation::S0
                || Transformation::S0.is_element_of(self.symmetry)
            {
                (self.height + 1) / 2
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

        Ok(self)
    }
}
