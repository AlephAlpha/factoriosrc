use crate::{
    error::{ConfigError, ParseSymmetryError},
    rule::CellState,
};
use std::{
    fmt::{self, Display, Formatter},
    str::FromStr,
};

/// Symmetry of the pattern.
///
/// Some symmetries require the world to be square.
/// Some require the world to have no diagonal width.
/// Some require the world to have no translation.
///
/// The notation is borrowed from the Oscar Cunningham's
/// [Logic Life Search](https://github.com/OscarCunningham/logic-life-search).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Symmetry {
    /// No symmetry.
    #[default]
    C1,

    /// Symmetry with respect to 180-degree rotation.
    ///
    /// This requires the world to have no translation.
    C2,

    /// Symmetry with respect to 90-degree rotation.
    ///
    /// This requires the world to be square, have no diagonal width, and have no translation.
    C4,

    /// Symmetry with respect to horizontal reflection.
    ///
    /// Denoted by `D2|`.
    ///
    /// This requires the world to have no diagonal width, and have no horizontal translation.
    D2H,

    /// Symmetry with respect to vertical reflection.
    ///
    /// Denoted by `D2-`.
    ///
    /// This requires the world to have no diagonal width, and have no vertical translation.
    D2V,

    /// Symmetry with respect to diagonal reflection.
    ///
    /// Denoted by `D2\`.
    ///
    /// This requires the world to be square, and the horizontal and vertical translations to be equal.
    D2D,

    /// Symmetry with respect to antidiagonal reflection.
    ///
    /// Denoted by `D2/`.
    ///
    /// This requires the world to be square, and the horizontal and vertical translations to add up to zero.
    D2A,

    /// Symmetry with respect to both horizontal and vertical reflections.
    ///
    /// Denoted by `D4+`.
    ///
    /// This requires the world to have no diagonal width, and have no translation.
    D4O,

    /// Symmetry with respect to both diagonal and antidiagonal reflections.
    ///
    /// Denoted by `D4X`.
    ///
    /// This requires the world to be square, and have no translation.
    D4X,

    /// Symmetry with respect to all the above rotations and reflections.
    ///
    /// Requires the world to be square and have no diagonal width, and have no translation.
    D8,
}

impl FromStr for Symmetry {
    type Err = ParseSymmetryError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "C1" => Ok(Self::C1),
            "C2" => Ok(Self::C2),
            "C4" => Ok(Self::C4),
            "D2|" => Ok(Self::D2H),
            "D2-" => Ok(Self::D2V),
            "D2\\" => Ok(Self::D2D),
            "D2/" => Ok(Self::D2A),
            "D4+" => Ok(Self::D4O),
            "D4X" => Ok(Self::D4X),
            "D8" => Ok(Self::D8),
            _ => Err(ParseSymmetryError),
        }
    }
}

impl Display for Symmetry {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::C1 => write!(f, "C1"),
            Self::C2 => write!(f, "C2"),
            Self::C4 => write!(f, "C4"),
            Self::D2H => write!(f, "D2|"),
            Self::D2V => write!(f, "D2-"),
            Self::D2D => write!(f, "D2\\"),
            Self::D2A => write!(f, "D2/"),
            Self::D4O => write!(f, "D4+"),
            Self::D4X => write!(f, "D4X"),
            Self::D8 => write!(f, "D8"),
        }
    }
}

impl Symmetry {
    /// Each symmetry can be represented as a subgroup of the dihedral group D8.
    /// This function checks whether the symmetry is a subgroup of the other symmetry.
    ///
    /// For example, `D2H` is a subgroup of `D4O`.
    /// This means that if a pattern has `D4O` symmetry, it also has `D2H` symmetry.
    pub fn is_subgroup_of(self, other: Self) -> bool {
        matches!(
            (self, other),
            (Self::C1, _)
                | (
                    Self::C2,
                    Self::C2 | Self::C4 | Self::D4O | Self::D4X | Self::D8
                )
                | (Self::C4, Self::C4 | Self::D8)
                | (Self::D2H, Self::D2H | Self::D4O | Self::D8)
                | (Self::D2V, Self::D2V | Self::D4O | Self::D8)
                | (Self::D2D, Self::D2D | Self::D4X | Self::D8)
                | (Self::D2A, Self::D2A | Self::D4X | Self::D8)
                | (Self::D4O, Self::D4O | Self::D8)
                | (Self::D8, Self::D8)
        )
    }

    /// Whether the symmetry requires the world to be square.
    ///
    /// This is true for `C4`, `D2D`, `D2A`, `D4X`, and `D8`.
    pub fn requires_square(self) -> bool {
        !self.is_subgroup_of(Self::D4O)
    }

    /// Whether the symmetry requires the world to have no diagonal width.
    ///
    /// This is true for `C4`, `D2H`, `D2V`, `D4O`, and `D8`.
    pub fn requires_no_diagonal_width(self) -> bool {
        !self.is_subgroup_of(Self::D4X)
    }

    /// Whether the translation satisfies the symmetry.
    pub fn is_translation_valid(self, dx: isize, dy: isize) -> bool {
        match self {
            Self::C1 => true,
            Self::C2 => dx == 0 && dy == 0,
            Self::C4 => dx == 0 && dy == 0,
            Self::D2H => dx == 0,
            Self::D2V => dy == 0,
            Self::D2D => dx == dy,
            Self::D2A => dx == -dy,
            Self::D4O => dx == 0 && dy == 0,
            Self::D4X => dx == 0 && dy == 0,
            Self::D8 => dx == 0 && dy == 0,
        }
    }
}

/// Search order.
///
/// This is used to determine how we find the next unknown cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchOrder {
    /// Search in row-major order.
    ///
    /// ```
    /// 1 2 3
    /// 4 5 6
    /// 7 8 9
    /// ```
    RowFirst,

    /// Search in column-major order.
    ///
    /// ```
    /// 1 4 7
    /// 2 5 8
    /// 3 6 9
    /// ```
    ColumnFirst,

    /// Search in diagonal order.
    ///
    /// ```
    /// 1 3 6
    /// 2 5 8
    /// 4 7 9
    /// ```
    ///
    /// This is useful for finding diagonal spaceships.
    ///
    /// This requires the world to be square.
    Diagonal,
}

/// Configuration for the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Width of the world.
    pub width: usize,

    /// Height of the world.
    pub height: usize,

    /// Period of the pattern.
    pub period: usize,

    /// Horizontal translation of the world.
    ///
    /// The pattern is translated by `x` cells to the left in each period.
    pub dx: isize,

    /// Vertical translation of the world.
    ///
    /// The pattern is translated by `y` cells upwards in each period.
    pub dy: isize,

    /// Diagonal width of the world.
    ///
    /// If the diagonal width is `n`, then cells at positions `(x, y)`
    /// where `abs(x - y) >= n` are always dead.
    ///
    /// This is useful for finding diagonal spaceships.
    ///
    /// If this is not `None`, then the world must be square.
    pub diagonal_width: Option<usize>,

    /// Symmetry of the pattern.
    pub symmetry: Symmetry,

    /// Search order.
    ///
    /// `None` means that the search order is automatically determined.
    pub search_order: Option<SearchOrder>,

    /// The first state to try for an unknown cell.
    pub new_state: CellState,
}

impl Config {
    /// Creates a new configuration.
    pub fn new(width: usize, height: usize, period: usize) -> Self {
        Self {
            width,
            height,
            period,
            dx: 0,
            dy: 0,
            diagonal_width: None,
            symmetry: Symmetry::C1,
            search_order: None,
            new_state: CellState::Dead,
        }
    }

    /// Sets horizontal and vertical translations.
    pub fn with_translations(mut self, dx: isize, dy: isize) -> Self {
        self.dx = dx;
        self.dy = dy;
        self
    }

    /// Sets the diagonal width.
    pub fn with_diagonal_width(mut self, diagonal_width: usize) -> Self {
        self.diagonal_width = Some(diagonal_width);
        self
    }

    /// Sets the symmetry.
    pub fn with_symmetry(mut self, symmetry: Symmetry) -> Self {
        self.symmetry = symmetry;
        self
    }

    /// Sets the search order.
    pub fn with_search_order(mut self, search_order: SearchOrder) -> Self {
        self.search_order = Some(search_order);
        self
    }

    /// Sets the first state to try for an unknown cell.
    pub fn with_new_state(mut self, new_state: CellState) -> Self {
        self.new_state = new_state;
        self
    }

    /// Whether the configuration requires the world to be square.
    pub fn requires_square(&self) -> bool {
        self.symmetry.requires_square()
            || self.diagonal_width.is_some()
            || self.search_order == Some(SearchOrder::Diagonal)
    }

    /// Checks whether the configuration is valid,
    /// and finds a search order if it is not specified.
    pub fn check(mut self) -> Result<Self, ConfigError> {
        if self.width == 0
            || self.height == 0
            || self.period == 0
            || self.diagonal_width.is_some_and(|d| d == 0)
        {
            return Err(ConfigError::InvalidSize);
        }

        if self.width != self.height && self.requires_square() {
            return Err(ConfigError::NotSquare);
        }

        if self.diagonal_width.is_some() && self.symmetry.requires_no_diagonal_width() {
            return Err(ConfigError::HasDiagonalWidth);
        }

        if !self.symmetry.is_translation_valid(self.dx, self.dy) {
            return Err(ConfigError::InvalidTranslation);
        }

        // If the search order is not specified, determine it automatically.
        if self.search_order.is_none() {
            // If the world is symmetric with respect to horizontal reflection,
            // we only need to search the left half of the world.
            let width = match self.symmetry {
                Symmetry::D2H | Symmetry::D4O | Symmetry::D8 => (self.width + 1) / 2,
                _ => self.width,
            };

            // If the world is symmetric with respect to vertical reflection,
            // we only need to search the upper half of the world.
            let height = match self.symmetry {
                Symmetry::D2V | Symmetry::D4O | Symmetry::D8 => (self.height + 1) / 2,
                _ => self.height,
            };

            // If the world is symmetric with respect to diagonal reflection,
            // we only need to search the lower triangle of the world.
            let diagonal_width = match self.symmetry {
                Symmetry::D2D | Symmetry::D4X | Symmetry::D8 => self.diagonal_width,
                _ => self.diagonal_width.map(|d| 2 * d + 1),
            };

            // The shortest edge should be searched first.
            let search_order = if diagonal_width.is_some_and(|d| d < width && d < height) {
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
