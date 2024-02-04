use crate::{parse_rule, NeighborError, ParseRuleError};
use std::str::FromStr;

/// The coordinates of a neighbor and its weight.
///
/// See the documentation of [`Rule`] for more information.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Neighbor {
    /// The coordinates of the neighbor relative to the center cell.
    pub coord: (i32, i32),
    /// The weight of the neighbor.
    ///
    /// For totalistic rules, this number is always 1.
    pub weight: u64,
}

impl Neighbor {
    /// Creates a new neighbor from its coordinates and weight.
    pub const fn new(coord: (i32, i32), weight: u64) -> Self {
        Self { coord, weight }
    }

    /// Creates a list of neighbors from a list of coordinates,
    /// where each neighbor has weight 1.
    pub fn from_coords(coords: impl IntoIterator<Item = (i32, i32)>) -> Vec<Self> {
        coords
            .into_iter()
            .map(|coord| Self::new(coord, 1))
            .collect()
    }

    /// Creates a list of neighbors from a list of coordinates,
    /// where the `i`-th neighbor has weight `2^i`.
    ///
    /// This is useful for non-totalistic rules.
    ///
    /// # Errors
    ///
    /// The number of neighbors must be at most 64. Otherwise, an error is returned.
    pub fn from_coords_non_totalistic(
        coords: impl IntoIterator<Item = (i32, i32)>,
    ) -> Result<Vec<Self>, NeighborError> {
        coords
            .into_iter()
            .enumerate()
            .map(|(i, coord)| {
                if i < 64 {
                    Ok(Self::new(coord, 1 << i))
                } else {
                    Err(NeighborError::NeighborhoodTooLarge)
                }
            })
            .collect()
    }

    /// Creates a list of neighbors from a neighborhood type and a radius.
    ///
    /// If `is_totalistic` is `true`, all neighbors have weight 1.
    ///
    /// If `is_totalistic` is `false`, the weight of the `i`-th neighbor is `2^i`.
    ///
    /// # Errors
    ///
    /// Returns an error if the radius is too large.
    ///
    /// If `is_totalistic` is `false`, the number of neighbors must be at most 64.
    /// This means that the maximum radius allowed is `3` for [`Moore`](NeighborhoodType::Moore),
    /// `5` for [`VonNeumann`](NeighborhoodType::VonNeumann),
    /// `16` for [`Cross`](NeighborhoodType::Cross),
    /// and `4` for [`Hexagonal`](NeighborhoodType::Hexagonal).
    ///
    /// When `is_totalistic` is `true`, the radius should be at most [`i32::MAX`].
    pub fn from_neighborhood_type(
        neighborhood_type: NeighborhoodType,
        radius: u32,
        is_totalistic: bool,
    ) -> Result<Vec<Self>, NeighborError> {
        neighborhood_type.neighbors(radius, is_totalistic)
    }
}

/// Predefined neighborhood types.
///
/// This enum is non-exhaustive. More neighborhood types may be added in the future.
///
/// Please see [Golly's documentation](https://golly.sourceforge.io/Help/Algorithms/Larger_than_Life.html)
/// for more information.
#[non_exhaustive]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NeighborhoodType {
    /// The Moore neighborhood.
    ///
    /// For example, the Moore neighborhood of radius 2 is:
    ///
    /// ```text
    /// # # # # #
    /// # # # # #
    /// # # O # #
    /// # # # # #
    /// # # # # #
    /// ```
    Moore,

    /// The von Neumann neighborhood.
    ///
    /// For example, the von Neumann neighborhood of radius 2 is:
    ///
    /// ```text
    /// . . # . .
    /// . # # # .
    /// # # O # #
    /// . # # # .
    /// . . # . .
    /// ```
    VonNeumann,

    /// The cross neighborhood.
    ///
    /// For example, the cross neighborhood of radius 2 is:
    ///
    /// ```text
    /// . . # . .
    /// . . # . .
    /// # # O # #
    /// . . # . .
    /// . . # . .
    /// ```
    Cross,

    /// The hash neighborhood.
    ///
    /// For example, the hash neighborhood of radius 2 is:
    ///
    /// ```text
    /// . # . # .
    /// # # # # #
    /// . # O # .
    /// # # # # #
    /// . # . # .
    /// ```
    Hash,

    /// The hexagonal neighborhood, emulated on a square grid.
    ///
    /// For example, the hexagonal neighborhood of radius 2 is:
    ///
    /// ```text
    /// # # # . .
    /// # # # # .
    /// # # O # #
    /// . # # # #
    /// . . # # #
    /// ```
    Hexagonal,
}

impl NeighborhoodType {
    /// Gets the number of neighbors given a radius.
    ///
    /// The number of neighbors is:
    /// - `4 * radius * (radius + 1)` for [`Moore`](NeighborhoodType::Moore),
    /// - `2 * radius * (radius + 1)` for [`VonNeumann`](NeighborhoodType::VonNeumann),
    /// - `4 * radius` for [`Cross`](NeighborhoodType::Cross),
    /// - `8 * radius` for [`Hash`](NeighborhoodType::Hash),
    /// - `3 * radius * (radius + 1)` for [`Hexagonal`](NeighborhoodType::Hexagonal).
    pub const fn size(self, radius: u32) -> usize {
        (match self {
            Self::Moore => 4 * radius * (radius + 1),
            Self::VonNeumann => 2 * radius * (radius + 1),
            Self::Cross => 4 * radius,
            Self::Hash => 8 * radius,
            Self::Hexagonal => 3 * radius * (radius + 1),
        }) as usize
    }

    /// Gets a list of coordinates from a neighborhood type and a radius.
    ///
    /// The coordinates are relative to the center cell.
    pub fn neighbor_coords(self, radius: u32) -> Vec<(i32, i32)> {
        let size = self.size(radius);
        let radius = radius as i32;

        let mut coords = Vec::with_capacity(size);

        match self {
            Self::Moore => {
                for x in -radius..=radius {
                    for y in -radius..=radius {
                        if x != 0 || y != 0 {
                            coords.push((x, y));
                        }
                    }
                }
            }
            Self::VonNeumann => {
                for x in -radius..=radius {
                    let max_y = radius - x.abs();
                    for y in -max_y..=max_y {
                        if x != 0 || y != 0 {
                            coords.push((x, y));
                        }
                    }
                }
            }
            Self::Cross => {
                for x in -radius..0 {
                    coords.push((x, 0));
                }
                for y in -radius..=radius {
                    if y != 0 {
                        coords.push((0, y));
                    }
                }
                for x in 1..=radius {
                    coords.push((x, 0));
                }
            }
            Self::Hash => {
                for x in -radius..=radius {
                    for y in -radius..=radius {
                        if x.abs() == 1 || y.abs() == 1 {
                            coords.push((x, y));
                        }
                    }
                }
            }
            Self::Hexagonal => {
                for x in -radius..=radius {
                    let min_y = (x - radius).max(-radius);
                    let max_y = (x + radius).min(radius);
                    for y in min_y..=max_y {
                        if x != 0 || y != 0 {
                            coords.push((x, y));
                        }
                    }
                }
            }
        };
        coords
    }

    /// Gets a list of [`Neighbor`]s from a neighborhood type and a radius.
    ///
    /// If `is_totalistic` is `true`, all neighbors have weight 1.
    ///
    /// If `is_totalistic` is `false`, the weight of the `i`-th neighbor is `2^i`.
    ///
    /// # Errors
    ///
    /// Returns an error if the radius is too large.
    ///
    /// If `is_totalistic` is `false`, the number of neighbors must be at most 64.
    /// This means that the maximum radius allowed is `3` for [`Moore`](NeighborhoodType::Moore),
    /// `5` for [`VonNeumann`](NeighborhoodType::VonNeumann),
    /// `16` for [`Cross`](NeighborhoodType::Cross),
    /// and `4` for [`Hexagonal`](NeighborhoodType::Hexagonal).
    ///
    /// When `is_totalistic` is `true`, the radius should be at most [`i32::MAX`].
    pub fn neighbors(
        self,
        radius: u32,
        is_totalistic: bool,
    ) -> Result<Vec<Neighbor>, NeighborError> {
        if radius > i32::MAX as u32 {
            return Err(NeighborError::RadiusTooLarge);
        }

        let size = self.size(radius);

        if !is_totalistic && size > 64 {
            return Err(NeighborError::RadiusTooLarge);
        }

        let coords = self.neighbor_coords(radius);

        if is_totalistic {
            Ok(Neighbor::from_coords(coords))
        } else {
            Neighbor::from_coords_non_totalistic(coords)
        }
    }
}

/// The shape of a neighborhood.
///
/// In a usual Life-like rule, the neighborhood of a cell is simply the 8 cells surrounding it. This is called
/// the Moore neighborhood of radius 1. In each generation, the state of a cell is determined by the states of
/// itself and its neighbors in the previous generation.
///
/// This struct generalizes the neighborhood to any shape and any radius. The neighborhood can be specified as a
/// predefined [`NeighborhoodType`] (e.g. Moore, von Neumann, etc.) and a radius, or as a custom list of coordinates.
///
/// This struct also supports three different ways to interpret the [`birth`](Rule::birth) and [`survival`](Rule::survival)
/// conditions: totalistic, non-totalistic, and weighted.
///
/// In a `[Rule]`, the [`birth`](Rule::birth) and [`survival`](Rule::survival) conditions are specified as a list
/// of integers, representing the sum of weights of neighbors in the "live" state. Interpretation of the conditions
/// are defined as follows:
///
/// - For totalistic neighborhoods, the integers in the conditions list represent the number of live neighbors.
/// - For non-totalistic neighborhoods, if the neighborhood has `n` neighbors, we can view the state of the
///   neighborhood as a binary number with `n` bits. The `i`-th bit is 1 if the `i`-th neighbor is in the "live"
///   state, and 0 otherwise. The integers in the conditions list represent the value of this binary number.
/// - For weighted neighborhoods, each neighbor is assigned a weight, and the integers in the conditions list
///   represent the sum of weights of live neighbors.
///
/// Please refer to the documentation of the [`Rule`] struct for more information.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Neighborhood {
    /// A totalistic neighborhood, specified by a [`NeighborhoodType`] and a radius.
    Totalistic(NeighborhoodType, u32),
    /// A non-totalistic neighborhood, specified a [`NeighborhoodType`] and a radius.
    Nontotalistic(NeighborhoodType, u32),
    /// A custom totalistic neighborhood, specified by a list of coordinates.
    CustomTotalistic(Vec<(i32, i32)>),
    /// A custom non-totalistic neighborhood, specified by a list of coordinates.
    CustomNontotalistic(Vec<(i32, i32)>),
    /// A custom weighted neighborhood, specified by a list of [`Neighbor`]s.
    CustomWeighted(Vec<Neighbor>),
}

impl Neighborhood {
    /// Gets a list of coordinates from the neighborhood.
    ///
    /// The coordinates are relative to the center cell.
    pub fn neighbor_coords(&self) -> Vec<(i32, i32)> {
        match self {
            Self::Totalistic(neighborhood_type, radius)
            | Self::Nontotalistic(neighborhood_type, radius) => {
                neighborhood_type.neighbor_coords(*radius)
            }
            Self::CustomTotalistic(coords) | Self::CustomNontotalistic(coords) => coords.clone(),
            Self::CustomWeighted(neighbors) => {
                neighbors.iter().map(|neighbor| neighbor.coord).collect()
            }
        }
    }

    /// Gets a list of [`Neighbor`]s from a neighborhood shape.
    ///
    /// If the neighborhood is totalistic, all neighbors have weight 1.
    ///
    /// If the neighborhood is non-totalistic, the weight of the `i`-th neighbor is `2^i`.
    ///
    /// # Errors
    ///
    /// Returns an error if the radius is too large.
    ///
    /// If the neighborhood is non-totalistic, the number of neighbors must be at most 64.
    /// This means that the maximum radius allowed is `3` for [`Moore`](NeighborhoodType::Moore),
    /// `5` for [`VonNeumann`](NeighborhoodType::VonNeumann),
    /// `16` for [`Cross`](NeighborhoodType::Cross),
    /// and `4` for [`Hexagonal`](NeighborhoodType::Hexagonal).
    ///
    /// When the neighborhood is totalistic, the radius should be at most [`i32::MAX`].
    ///
    /// # Examples
    ///
    /// For example, Conway's Game of Life has the Moore neighborhood of radius 1.
    ///
    /// ```rust
    /// # use ca_rules2::{Neighbor, Neighborhood, NeighborhoodType};
    /// let neighborhood = Neighborhood::Totalistic(NeighborhoodType::Moore, 1);
    /// let neighbors = vec![
    ///     Neighbor { coord: (-1, -1), weight: 1 },
    ///     Neighbor { coord: (-1,  0), weight: 1 },
    ///     Neighbor { coord: (-1,  1), weight: 1 },
    ///     Neighbor { coord: ( 0, -1), weight: 1 },
    ///     Neighbor { coord: ( 0,  1), weight: 1 },
    ///     Neighbor { coord: ( 1, -1), weight: 1 },
    ///     Neighbor { coord: ( 1,  0), weight: 1 },
    ///     Neighbor { coord: ( 1,  1), weight: 1 },
    /// ];
    /// assert_eq!(neighborhood.neighbors().unwrap(), neighbors);
    /// ```
    pub fn neighbors(&self) -> Result<Vec<Neighbor>, NeighborError> {
        match self {
            Self::Totalistic(neighborhood_type, radius) => {
                neighborhood_type.neighbors(*radius, true)
            }
            Self::Nontotalistic(neighborhood_type, radius) => {
                neighborhood_type.neighbors(*radius, false)
            }
            Self::CustomTotalistic(coords) => Ok(Neighbor::from_coords(coords.iter().copied())),
            Self::CustomNontotalistic(coords) => {
                Neighbor::from_coords_non_totalistic(coords.iter().copied())
            }
            Self::CustomWeighted(neighbors) => Ok(neighbors.clone()),
        }
    }

    /// Whether the neighborhood is totalistic.
    pub const fn is_totalistic(&self) -> bool {
        matches!(self, Self::Totalistic(_, _) | Self::CustomTotalistic(_))
    }

    /// Whether the neighborhood is non-totalistic.
    pub const fn is_nontotalistic(&self) -> bool {
        matches!(
            self,
            Self::Nontotalistic(_, _) | Self::CustomNontotalistic(_)
        )
    }

    /// Number of neighbors.
    pub fn size(&self) -> usize {
        match self {
            Self::Totalistic(neighborhood_type, radius)
            | Self::Nontotalistic(neighborhood_type, radius) => neighborhood_type.size(*radius),
            Self::CustomTotalistic(coords) | Self::CustomNontotalistic(coords) => coords.len(),
            Self::CustomWeighted(neighbors) => neighbors.len(),
        }
    }

    /// Radius of the neighborhood.
    ///
    /// For custom neighborhoods, the radius is the maximum Chebyshev distance between the center cell and its neighbors.
    ///
    /// The Chebyshev distance between two points `(x1, y1)` and `(x2, y2)` is `max(|x1 - x2|, |y1 - y2|)`.
    pub fn radius(&self) -> u32 {
        match self {
            Self::Totalistic(_, radius) | Self::Nontotalistic(_, radius) => *radius,
            Self::CustomTotalistic(coords) | Self::CustomNontotalistic(coords) => coords
                .iter()
                .map(|(x, y)| x.abs().max(y.abs()))
                .max()
                .unwrap_or(0)
                as u32,
            Self::CustomWeighted(neighbors) => neighbors
                .iter()
                .map(|neighbor| neighbor.coord.0.abs().max(neighbor.coord.1.abs()))
                .max()
                .unwrap_or(0) as u32,
        }
    }

    /// Maximum possible value for a birth or survival condition.
    ///
    /// For totalistic neighborhoods, this is the number of neighbors.
    ///
    /// For non-totalistic neighborhoods, this is `2^n`, where `n` is the number of neighbors.
    ///
    /// For weighted neighborhoods, this is the sum of weights of neighbors.
    pub fn max_condition(&self) -> u64 {
        match self {
            Self::Totalistic(_, _) | Self::CustomTotalistic(_) => self.size() as u64,
            Self::Nontotalistic(_, _) | Self::CustomNontotalistic(_) => 1 << self.size(),
            Self::CustomWeighted(neighbors) => {
                neighbors.iter().map(|neighbor| neighbor.weight).sum()
            }
        }
    }
}

/// A cellular automaton rule.
///
/// # Rules
///
/// This struct is intended to represent a [Generations](https://www.conwaylife.com/wiki/Generations) rule.
/// It is a generalization of the Life-like rules, with possibly more states.
///
/// A Generations rule has at least 2 states:
///
/// - A "dead" state, represented by the number 0.
/// - A "live" state, represented by the number 1.
/// - Possibly some "dying" states, represented by numbers greater than 1.
///
/// In each generation, a cell transitions from one state to another according to the following rules:
///
/// - A cell in the "dead" state will:
///    - Transition to the "live" state if it satisfies the [`birth`](Rule::birth) conditions.
///    - Remain in the "dead" state otherwise.
/// - A cell in the "live" state will:
///    - Remain in the "live" state if it satisfies the [`survival`](Rule::survival) conditions.
///    - Otherwise, transition to the next "dying" state, or the "dead" state if there are only 2 states.
/// - A cell in a "dying" state will transition to the next "dying" state, or the "dead" state if it is
///  already in the last "dying" state.
///
/// When the number of states is 2, there are no "dying" states, and the rule is equivalent to a Life-like rule.
///
/// The number of states is specified by the [`states`](Rule::states) field. It must be at least 2.
///
/// # Neighborhood
///
/// In a usual Life-like rule, the neighborhood of a cell is simply the 8 cells surrounding it. This is called
/// the Moore neighborhood of radius 1. In each generation, the state of a cell is determined by the states of
/// itself and its neighbors in the previous generation.
///
/// This struct generalizes the neighborhood to any shape and any radius. The neighborhood can be specified as a
/// predefined [`NeighborhoodType`] (e.g. Moore, von Neumann, etc.) and a radius, or as a custom list of coordinates.
///
/// This struct also supports three different ways to interpret the [`birth`](Rule::birth) and [`survival`](Rule::survival)
/// conditions: totalistic, non-totalistic, and weighted.
///
/// These information is specified by the [`neighborhood`](Rule::neighborhood) field. Please refer to the documentation
/// of the [`Neighborhood`] enum for more information.
///
/// # Birth and Survival Conditions
///
/// The [`birth`](Rule::birth) and [`survival`](Rule::survival) conditions are specified as a list of integers,
/// representing the sum of weights of neighbors in the "live" state. No distinction is made between "dying"
/// states and the "dead" state when calculating the sum.
///
/// Interpretation of the conditions depends on the neighborhood:
///
/// - For totalistic neighborhoods, the integers in the conditions list represent the number of live neighbors.
/// - For non-totalistic neighborhoods, if the neighborhood has `n` neighbors, we can view the state of the
///   neighborhood as a binary number with `n` bits. The `i`-th bit is 1 if the `i`-th neighbor is in the "live"
///   state, and 0 otherwise. The integers in the conditions list represent the value of this binary number.
/// - For weighted neighborhoods, each neighbor is assigned a weight, and the integers in the conditions list
///   represent the sum of weights of live neighbors.
///
/// Totalistic rules and non-totalistic rules can be seen as special cases of weighted rules. In a totalistic rule,
/// all neighbors have weight 1. In a non-totalistic rule, the weight of the `i`-th neighbor is `2^i`.
///
/// # Examples
///
/// For example, Conway's Game of Life has the Moore neighborhood of radius 1. It is a totalistic rule, so
/// it can be represented as follows:
///
/// ```rust
/// # use ca_rules2::{Neighborhood, NeighborhoodType, Rule};
/// let rule = Rule {
///    states: 2,
///    neighborhood: Neighborhood::Totalistic(NeighborhoodType::Moore, 1),
///    birth: vec![3],
///    survival: vec![2, 3],
/// };
/// ```
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Rule {
    /// The number of states.
    ///
    /// It must be at least 2.
    pub states: u64,
    /// The neighborhood.
    pub neighborhood: Neighborhood,
    /// Birth conditions.
    pub birth: Vec<u64>,
    /// Survival conditions.
    pub survival: Vec<u64>,
}

impl Rule {
    /// Whether the rule is totalistic.
    pub const fn is_totalistic(&self) -> bool {
        self.neighborhood.is_totalistic()
    }

    /// Number of neighbors.
    pub fn neighborhood_size(&self) -> usize {
        self.neighborhood.size()
    }

    /// Radius of the neighborhood.
    pub fn radius(&self) -> u32 {
        self.neighborhood.radius()
    }

    /// The list of coordinates of the neighbors.
    ///
    /// The coordinates are relative to the center cell.
    pub fn neighbor_coords(&self) -> Vec<(i32, i32)> {
        self.neighborhood.neighbor_coords()
    }

    /// Whether the birth conditions contain 0.
    ///
    /// In this case, a dead cell can be born even if it has no live neighbors.
    /// This needs to be handled separately in many use cases.
    ///
    /// For example, a cellular automaton simulator that supports infinite grids may assume that
    /// all "background" cells are dead. However, if the birth conditions contain 0, these dead
    /// cells may become alive in the next generation, which breaks the assumption.
    pub fn contains_b0(&self) -> bool {
        self.birth.contains(&0)
    }

    /// Checks whether the birth and survival conditions are valid.
    ///
    /// These conditions should not contain any number greater than the maximum possible value.
    ///
    /// See the documentation of the [`Neighborhood::max_condition`] for more information.
    pub fn check_conditions(&self) -> bool {
        let max_condition = self.neighborhood.max_condition();

        self.birth.iter().all(|&n| n <= max_condition)
            && self.survival.iter().all(|&n| n <= max_condition)
    }
}

impl FromStr for Rule {
    type Err = ParseRuleError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        parse_rule(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALL_NEIGHBORHOOD_TYPES: [NeighborhoodType; 5] = [
        NeighborhoodType::Moore,
        NeighborhoodType::VonNeumann,
        NeighborhoodType::Cross,
        NeighborhoodType::Hexagonal,
        NeighborhoodType::Hexagonal,
    ];

    #[test]
    fn test_neighborhood_type() {
        let moore = NeighborhoodType::Moore.neighbors(1, true).unwrap();
        assert_eq!(
            moore,
            vec![
                Neighbor::new((-1, -1), 1),
                Neighbor::new((-1, 0), 1),
                Neighbor::new((-1, 1), 1),
                Neighbor::new((0, -1), 1),
                Neighbor::new((0, 1), 1),
                Neighbor::new((1, -1), 1),
                Neighbor::new((1, 0), 1),
                Neighbor::new((1, 1), 1),
            ]
        );

        let von_neumann = NeighborhoodType::VonNeumann.neighbors(1, true).unwrap();
        assert_eq!(
            von_neumann,
            vec![
                Neighbor::new((-1, 0), 1),
                Neighbor::new((0, -1), 1),
                Neighbor::new((0, 1), 1),
                Neighbor::new((1, 0), 1),
            ]
        );

        let cross = NeighborhoodType::Cross.neighbors(1, true).unwrap();
        assert_eq!(
            cross,
            vec![
                Neighbor::new((-1, 0), 1),
                Neighbor::new((0, -1), 1),
                Neighbor::new((0, 1), 1),
                Neighbor::new((1, 0), 1),
            ]
        );

        let hash = NeighborhoodType::Hash.neighbors(1, true).unwrap();
        assert_eq!(
            hash,
            vec![
                Neighbor::new((-1, -1), 1),
                Neighbor::new((-1, 0), 1),
                Neighbor::new((-1, 1), 1),
                Neighbor::new((0, -1), 1),
                Neighbor::new((0, 1), 1),
                Neighbor::new((1, -1), 1),
                Neighbor::new((1, 0), 1),
                Neighbor::new((1, 1), 1),
            ]
        );

        let hexagonal = NeighborhoodType::Hexagonal.neighbors(1, true).unwrap();
        assert_eq!(
            hexagonal,
            vec![
                Neighbor::new((-1, -1), 1),
                Neighbor::new((-1, 0), 1),
                Neighbor::new((0, -1), 1),
                Neighbor::new((0, 1), 1),
                Neighbor::new((1, 0), 1),
                Neighbor::new((1, 1), 1),
            ]
        );

        for r in 1..5 {
            for neighborhood_type in ALL_NEIGHBORHOOD_TYPES {
                let rule = Rule {
                    states: 2,
                    neighborhood: Neighborhood::Totalistic(neighborhood_type, r),
                    birth: Vec::new(),
                    survival: Vec::new(),
                };

                let neighbors = rule.neighbor_coords();

                assert_eq!(neighbors.len(), rule.neighborhood_size());

                let custom = Neighborhood::CustomTotalistic(neighbors);
                assert_eq!(custom.radius(), r);
            }
        }
    }
}
