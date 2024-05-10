#[cfg(feature = "clap")]
use clap::ValueEnum;
#[cfg(feature = "documented")]
use documented::{Documented, DocumentedFields};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::{cmp::Ordering, ops::Mul};
use strum::{Display, EnumIter, EnumString, IntoEnumIterator};

/// Geometric transformation that can be applied to a pattern.
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Display, EnumIter, EnumString)]
#[cfg_attr(feature = "clap", derive(ValueEnum), value(rename_all = "PascalCase"))]
#[cfg_attr(feature = "documented", derive(Documented, DocumentedFields))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub enum Transformation {
    /// Identity transformation.
    #[default]
    R0,

    /// 90-degree rotation (clockwise).
    ///
    /// This requires the world to be square, have no diagonal width, and have no translation.
    R1,

    /// 180-degree rotation.
    ///
    /// This requires the world to have no translation.
    R2,

    /// 270-degree rotation (clockwise).
    ///
    /// This requires the world to be square, have no diagonal width, and have no translation.
    R3,

    /// Vertical reflection.
    ///
    /// This requires the world to have no diagonal width, and have no vertical translation.
    S0,

    /// Diagonal reflection.
    ///
    /// This requires the world to be square, and the horizontal and vertical translations to be equal.
    S1,

    /// Horizontal reflection.
    ///
    /// This requires the world to have no diagonal width, and have no horizontal translation.
    S2,

    /// Antidiagonal reflection.
    ///
    /// This requires the world to be square, and the horizontal and vertical translations to add up to zero.
    S3,
}

/// The multiplication of two transformations is defined as their composition.
impl Mul for Transformation {
    type Output = Self;

    #[inline]
    fn mul(self, rhs: Self) -> Self::Output {
        self.compose(rhs)
    }
}

/// An alternative representation of a transformation.
///
/// A transformation can be represented as a pair of a type (rotation or reflection)
/// and an index (0, 1, 2, or 3).
///
/// Inverse and composition can be computed more easily using this representation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum D8 {
    R(i8),
    S(i8),
}

impl D8 {
    const fn from(transformation: Transformation) -> Self {
        match transformation {
            Transformation::R0 => Self::R(0),
            Transformation::R1 => Self::R(1),
            Transformation::R2 => Self::R(2),
            Transformation::R3 => Self::R(3),
            Transformation::S0 => Self::S(0),
            Transformation::S1 => Self::S(1),
            Transformation::S2 => Self::S(2),
            Transformation::S3 => Self::S(3),
        }
    }

    const fn into(self) -> Transformation {
        match self {
            Self::R(0) => Transformation::R0,
            Self::R(1) => Transformation::R1,
            Self::R(2) => Transformation::R2,
            Self::R(3) => Transformation::R3,
            Self::S(0) => Transformation::S0,
            Self::S(1) => Transformation::S1,
            Self::S(2) => Transformation::S2,
            Self::S(3) => Transformation::S3,
            _ => unreachable!(),
        }
    }

    const fn inverse(self) -> Self {
        match self {
            Self::R(i) => Self::R((-i) & 3),
            Self::S(i) => Self::S(i),
        }
    }

    const fn compose(self, other: Self) -> Self {
        match (self, other) {
            (Self::R(i), Self::R(j)) => Self::R((i + j) & 3),
            (Self::S(i), Self::S(j)) => Self::R((i - j) & 3),
            (Self::R(i), Self::S(j)) => Self::S((i + j) & 3),
            (Self::S(i), Self::R(j)) => Self::S((i - j) & 3),
        }
    }
}

impl Transformation {
    /// Each transformation can be represented as an element of the dihedral group D8.
    /// This function checks whether the transformation is an element of the subgroup
    /// corresponding to a given symmetry.
    ///
    /// For example, [`S0`](Transformation::S0) is a subgroup of [`D2V`](Symmetry::D2V).
    /// This means that if a pattern has [`D2V`](Symmetry::D2V) symmetry, it is invariant
    /// under the [`S0`](Transformation::S0) transformation.
    #[inline]
    pub const fn is_element_of(self, symmetry: Symmetry) -> bool {
        matches!(
            (self, symmetry),
            (Self::R0, Symmetry::C1)
                | (Self::R0 | Self::R2, Symmetry::C2)
                | (Self::R0 | Self::R1 | Self::R2 | Self::R3, Symmetry::C4)
                | (Self::R0 | Self::S0, Symmetry::D2V)
                | (Self::R0 | Self::S2, Symmetry::D2H)
                | (Self::R0 | Self::S1, Symmetry::D2D)
                | (Self::R0 | Self::S3, Symmetry::D2A)
                | (Self::R0 | Self::R2 | Self::S0 | Self::S2, Symmetry::D4O)
                | (Self::R0 | Self::R2 | Self::S1 | Self::S3, Symmetry::D4X)
                | (_, Symmetry::D8)
        )
    }

    /// Whether the transformation requires the world to be square.
    ///
    /// This is true for `R1`, `R3`, `S1`, and `S3`.
    #[inline]
    pub const fn requires_square(self) -> bool {
        !self.is_element_of(Symmetry::D4O)
    }

    /// Whether the transformation requires the world to have no diagonal width.
    ///
    /// This is true for `R1`, `R3`, `S2`, `S3`, and `S0`.
    #[inline]
    pub const fn requires_no_diagonal_width(self) -> bool {
        !self.is_element_of(Symmetry::D4X)
    }

    /// The inverse of the transformation.
    #[inline]
    #[must_use]
    pub const fn inverse(self) -> Self {
        D8::from(self).inverse().into()
    }

    /// The composition of two transformations.
    #[inline]
    #[must_use]
    pub const fn compose(self, other: Self) -> Self {
        D8::from(self).compose(D8::from(other)).into()
    }

    /// Apply the transformation to the given coordinates, using `(0, 0)` as the center.
    #[inline]
    pub const fn apply(self, x: i32, y: i32) -> (i32, i32) {
        match self {
            Self::R0 => (x, y),
            Self::R1 => (-y, x),
            Self::R2 => (-x, -y),
            Self::R3 => (y, -x),
            Self::S0 => (x, -y),
            Self::S1 => (y, x),
            Self::S2 => (-x, y),
            Self::S3 => (-y, -x),
        }
    }

    /// Given a world size, apply the transformation to the given coordinates,
    /// using the center of the world as the center.
    ///
    /// If the transformation requires the world to be square, but the world is not square,
    /// the result is not guaranteed to be correct.
    #[inline]
    pub const fn apply_with_size(self, x: i32, y: i32, width: i32, height: i32) -> (i32, i32) {
        match self {
            Self::R0 => (x, y),
            Self::R1 => (height - y - 1, x),
            Self::R2 => (width - x - 1, height - y - 1),
            Self::R3 => (y, width - x - 1),
            Self::S0 => (x, height - y - 1),
            Self::S1 => (y, x),
            Self::S2 => (width - x - 1, y),
            Self::S3 => (height - y - 1, width - x - 1),
        }
    }

    /// An iterator over all possible transformations.
    #[inline]
    pub fn iter() -> impl Iterator<Item = Self> {
        <Self as IntoEnumIterator>::iter()
    }
}

/// Symmetry of a pattern.
///
/// There are 10 possible symmetries, corresponding to the 10 subgroups of the
/// [dihedral group D8](https://en.wikipedia.org/wiki/Dihedral_group).
///
/// Some symmetries require the world to be square.
/// Some require the world to have no diagonal width.
/// Some require the world to have no translation.
///
/// The notation is borrowed from the Oscar Cunningham's
/// [Logic Life Search](https://github.com/OscarCunningham/logic-life-search).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Display, EnumIter, EnumString)]
#[cfg_attr(feature = "clap", derive(ValueEnum), value(rename_all = "PascalCase"))]
#[cfg_attr(feature = "documented", derive(Documented, DocumentedFields))]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
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
    #[strum(serialize = "D2|")]
    #[cfg_attr(feature = "clap", value(name = "D2|"))]
    #[cfg_attr(feature = "serde", serde(rename = "D2|"))]
    D2H,

    /// Symmetry with respect to vertical reflection.
    ///
    /// Denoted by `D2-`.
    ///
    /// This requires the world to have no diagonal width, and have no vertical translation.
    #[strum(serialize = "D2-")]
    #[cfg_attr(feature = "clap", value(name = "D2-"))]
    #[cfg_attr(feature = "serde", serde(rename = "D2-"))]
    D2V,

    /// Symmetry with respect to diagonal reflection.
    ///
    /// Denoted by `D2\`.
    ///
    /// This requires the world to be square, and the horizontal and vertical translations to be equal.
    #[strum(serialize = "D2\\")]
    #[cfg_attr(feature = "clap", value(name = "D2\\"))]
    #[cfg_attr(feature = "serde", serde(rename = "D2\\"))]
    D2D,

    /// Symmetry with respect to antidiagonal reflection.
    ///
    /// Denoted by `D2/`.
    ///
    /// This requires the world to be square, and the horizontal and vertical translations to add up to zero.
    #[strum(serialize = "D2/")]
    #[cfg_attr(feature = "clap", value(name = "D2/"))]
    #[cfg_attr(feature = "serde", serde(rename = "D2/"))]
    D2A,

    /// Symmetry with respect to both horizontal and vertical reflections.
    ///
    /// Denoted by `D4+`.
    ///
    /// This requires the world to have no diagonal width, and have no translation.
    #[strum(serialize = "D4+")]
    #[cfg_attr(feature = "clap", value(name = "D4+"))]
    #[cfg_attr(feature = "serde", serde(rename = "D4+"))]
    D4O,

    /// Symmetry with respect to both diagonal and antidiagonal reflections.
    ///
    /// This requires the world to be square, and have no translation.
    D4X,

    /// Symmetry with respect to all the above rotations and reflections.
    ///
    /// This requires the world to be square and have no diagonal width, and have no translation.
    D8,
}

/// The partial order of symmetries is defined by the subgroup relation.
impl PartialOrd for Symmetry {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        if *self == *other {
            Some(Ordering::Equal)
        } else if self.is_subgroup_of(*other) {
            Some(Ordering::Less)
        } else if other.is_subgroup_of(*self) {
            Some(Ordering::Greater)
        } else {
            None
        }
    }
}

impl Symmetry {
    /// Each symmetry can be represented as a subgroup of the dihedral group D8.
    /// This function checks whether the symmetry is a subgroup of the other symmetry.
    ///
    /// For example, [`D2H`](Symmetry::D2H) is a subgroup of [`D4O`](Symmetry::D4O).
    /// This means that if a pattern has [`D4O`](Symmetry::D4O) symmetry, it also has
    /// [`D2H`](Symmetry::D2H) symmetry.
    #[inline]
    pub const fn is_subgroup_of(self, other: Self) -> bool {
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
                | (Self::D4X, Self::D4X | Self::D8)
                | (Self::D8, Self::D8)
        )
    }

    /// Whether the symmetry requires the world to be square.
    ///
    /// This is true for `C4`, `D2D`, `D2A`, `D4X`, and `D8`.
    #[inline]
    pub const fn requires_square(self) -> bool {
        !self.is_subgroup_of(Self::D4O)
    }

    /// Whether the symmetry requires the world to have no diagonal width.
    ///
    /// This is true for `C4`, `D2H`, `D2V`, `D4O`, and `D8`.
    #[inline]
    pub const fn requires_no_diagonal_width(self) -> bool {
        !self.is_subgroup_of(Self::D4X)
    }

    /// The condition that a translation must satisfy to be compatible with the symmetry.
    #[inline]
    pub const fn translation_condition(self) -> TranslationCondition {
        match self {
            Self::C1 => TranslationCondition::Any,
            Self::D2H => TranslationCondition::NoHorizontal,
            Self::D2V => TranslationCondition::NoVertical,
            Self::D2D => TranslationCondition::Diagonal,
            Self::D2A => TranslationCondition::Antidiagonal,
            _ => TranslationCondition::NoTranslation,
        }
    }

    /// Whether the translation is compatible with the symmetry.
    ///
    /// A translation is compatible with the symmetry if it commutes with all the transformations
    /// that are elements of the symmetry.
    #[inline]
    pub const fn translation_is_valid(self, dx: i32, dy: i32) -> bool {
        match self.translation_condition() {
            TranslationCondition::Any => true,
            TranslationCondition::NoHorizontal => dx == 0,
            TranslationCondition::NoVertical => dy == 0,
            TranslationCondition::NoTranslation => dx == 0 && dy == 0,
            TranslationCondition::Diagonal => dx == dy,
            TranslationCondition::Antidiagonal => dx == -dy,
        }
    }

    /// An iterator over all possible symmetries.
    #[inline]
    pub fn iter() -> impl Iterator<Item = Self> {
        <Self as IntoEnumIterator>::iter()
    }

    /// An iterator over the transformations that are elements of the symmetry.
    #[inline]
    pub fn transformations(self) -> impl Iterator<Item = Transformation> {
        Transformation::iter().filter(move |&t| t.is_element_of(self))
    }
}

/// Conditions that a translation must satisfy to be compatible with a symmetry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TranslationCondition {
    /// All translations are compatible with the symmetry.
    Any,
    /// `dx` must be zero.
    NoHorizontal,
    /// `dy` must be zero.
    NoVertical,
    /// Both `dx` and `dy` must be zero.
    NoTranslation,
    /// `dx` must be equal to `dy`.
    Diagonal,
    /// `dx` must be equal to `-dy`.
    Antidiagonal,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transformation_compose() {
        let (x, y) = (1, 2);

        for t in Transformation::iter() {
            assert_eq!(t.inverse().compose(t), Transformation::R0);
        }

        for t1 in Transformation::iter() {
            for t2 in Transformation::iter() {
                let (x1, y1) = t2.apply(x, y);
                assert_eq!(t1.compose(t2).apply(x, y), t1.apply(x1, y1));
            }
        }
    }

    #[test]
    fn test_symmetry_subgroup() {
        for s1 in Symmetry::iter() {
            for s2 in Symmetry::iter() {
                assert_eq!(
                    s1.is_subgroup_of(s2),
                    s1.transformations().all(|t| t.is_element_of(s2))
                );
            }
        }
    }

    #[test]
    fn test_symmetry_conditions() {
        for s in Symmetry::iter() {
            assert_eq!(
                s.requires_square(),
                s.transformations().any(|t| t.requires_square())
            );

            assert_eq!(
                s.requires_no_diagonal_width(),
                s.transformations().any(|t| t.requires_no_diagonal_width())
            );

            for dx in -1..=1 {
                for dy in -1..=1 {
                    assert_eq!(
                        s.translation_is_valid(dx, dy),
                        s.transformations().all(|t| {
                            let (x, y) = (10, 20);
                            let (x1, y1) = t.apply(x, y);
                            t.apply(x + dx, y + dy) == (x1 + dx, y1 + dy)
                        })
                    );
                }
            }
        }
    }
}
