use crate::NeighborError;

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
            .map(|coord| Neighbor::new(coord, 1))
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
                    Ok(Neighbor::new(coord, 1 << i))
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
    /// When `is_totalistic` is `true`, the radius should be at most `i32::MAX`.
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
/// This is used to generate the list of [`neighbors`](Rule::neighbors) of a [`Rule`].
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
    /// When `is_totalistic` is `true`, the radius should be at most `i32::MAX`.
    pub fn neighbors(
        self,
        radius: u32,
        is_totalistic: bool,
    ) -> Result<Vec<Neighbor>, NeighborError> {
        if radius > i32::MAX as u32 {
            return Err(NeighborError::RadiusTooLarge);
        }

        let radius = radius as i32;

        let size = match self {
            NeighborhoodType::Moore => 4 * radius * (radius + 1),
            NeighborhoodType::VonNeumann => 2 * radius * (radius + 1),
            NeighborhoodType::Cross => 4 * radius,
            NeighborhoodType::Hexagonal => 3 * radius * (radius + 1),
        };

        if !is_totalistic && size > 64 {
            return Err(NeighborError::RadiusTooLarge);
        }

        let mut coords = Vec::with_capacity(size as usize);

        match self {
            NeighborhoodType::Moore => {
                for x in -radius..=radius {
                    for y in -radius..=radius {
                        if x != 0 || y != 0 {
                            coords.push((x, y));
                        }
                    }
                }
            }
            NeighborhoodType::VonNeumann => {
                for x in -radius..=radius {
                    let max_y = radius - x.abs();
                    for y in -max_y..=max_y {
                        if x != 0 || y != 0 {
                            coords.push((x, y));
                        }
                    }
                }
            }
            NeighborhoodType::Cross => {
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
            NeighborhoodType::Hexagonal => {
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

        if is_totalistic {
            Ok(Neighbor::from_coords(coords))
        } else {
            Neighbor::from_coords_non_totalistic(coords)
        }
    }
}

/// A cellular automaton rule.
///
/// # Number of states
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
/// When the number of states is 2, it is a usual Life-like rule.
///
/// The number of states is specified by the [`states`](Rule::states) field. It must be at least 2.
///
/// # Neighbors and weights
///
/// In each generation, the state of a cell is determined by the states of itself and its neighbors in the
/// previous generation. The [`neighbors`](Rule::neighbors) field specifies the coordinates of the neighbors
/// relative to the center cell, and their weights.
///
/// The [`birth`](Rule::birth) and [`survival`](Rule::survival) conditions are specified as a list of integers,
/// representing the sum of weights of neighbors in the "live" state. No distinction is made between "dying"
/// states and the "dead" state when calculating the sum.
///
/// For totalistic rules, we only need to count the number of neighbors in the "live" state. So the weight
/// of each neighbor is 1.
///
/// For non-totalistic rules, we need to know the state of each neighbor. We can assign a distinct power of 2
/// to each neighbor, so that each possible combination of neighbor states corresponds to a unique integer.
///
/// There are other possible ways to assign weights to neighbors. For example, you can assign the weights
/// according to the distance between the center cell and its neighbors.
///
/// # Examples
///
/// For example, Conway's Game of Life has the Moore neighborhood of radius 1. It is a totalistic rule, so
/// it can be represented as follows:
///
/// ```rust
/// # use ca_rules2::{Neighbor, Rule};
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
/// let rule = Rule {
///     states: 2,
///     neighbors,
///     birth: vec![3],
///     survival: vec![2, 3],
/// };
/// ```
///
/// The list of neighbors can also be generated using the [`Neighbor::from_coords`] method:
///
/// ```rust
/// # use ca_rules2::{Neighbor, Rule};
/// let neighbors = Neighbor::from_coords([
///    (-1, -1), (-1,  0), (-1,  1),
///    ( 0, -1),           ( 0,  1),
///    ( 1, -1), ( 1,  0), ( 1,  1),
/// ]);
/// let rule = Rule {
///     states: 2,
///     neighbors,
///     birth: vec![3],
///     survival: vec![2, 3],
/// };
/// ```
///
/// An even simpler way to generate the list of neighbors is to use the [`NeighborhoodType`] enum:
///
/// ```rust
/// # use ca_rules2::{NeighborhoodType, Rule};
/// let rule = Rule {
///    states: 2,
///    neighbors: NeighborhoodType::Moore.neighbors(1, true).unwrap(),
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
    /// The list of neighbors.
    pub neighbors: Vec<Neighbor>,
    /// Birth conditions.
    pub birth: Vec<u64>,
    /// Survival conditions.
    pub survival: Vec<u64>,
}

impl Rule {
    /// Whether the rule is totalistic.
    ///
    /// A rule is totalistic if all neighbors have weight 1.
    pub fn is_totalistic(&self) -> bool {
        self.neighbors.iter().all(|neighbor| neighbor.weight == 1)
    }

    /// Number of neighbors.
    pub fn neighborhood_size(&self) -> usize {
        self.neighbors.len()
    }

    /// Radius of the neighborhood.
    ///
    /// The radius is the maximum Chebyshev distance between the center cell and its neighbors.
    ///
    /// The Chebyshev distance between two points `(x1, y1)` and `(x2, y2)` is `max(|x1 - x2|, |y1 - y2|)`.
    pub fn radius(&self) -> u32 {
        self.neighbors
            .iter()
            .map(|neighbor| neighbor.coord.0.abs().max(neighbor.coord.1.abs()))
            .max()
            .unwrap_or(0) as u32
    }

    /// Checks whether the birth and survival conditions are valid.
    ///
    /// These conditions should not contain any number greater than the sum of weights of neighbors.
    pub fn check_conditions(&self) -> bool {
        let sum_of_weights = self.neighbors.iter().map(|neighbor| neighbor.weight).sum();

        self.birth.iter().all(|&n| n <= sum_of_weights)
            && self.survival.iter().all(|&n| n <= sum_of_weights)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
            assert_eq!(
                NeighborhoodType::Moore
                    .neighbors(r as u32, true)
                    .unwrap()
                    .len(),
                4 * r * (r + 1)
            );
            assert_eq!(
                NeighborhoodType::VonNeumann
                    .neighbors(r as u32, true)
                    .unwrap()
                    .len(),
                2 * r * (r + 1)
            );
            assert_eq!(
                NeighborhoodType::Cross
                    .neighbors(r as u32, true)
                    .unwrap()
                    .len(),
                4 * r
            );
            assert_eq!(
                NeighborhoodType::Hexagonal
                    .neighbors(r as u32, true)
                    .unwrap()
                    .len(),
                3 * r * (r + 1)
            );
        }
    }

    #[test]
    fn test_radius() {
        let neighborhood_types = [
            NeighborhoodType::Moore,
            NeighborhoodType::VonNeumann,
            NeighborhoodType::Cross,
            NeighborhoodType::Hexagonal,
        ];

        for r in 1..5 {
            for neighborhood_type in neighborhood_types {
                let rule = Rule {
                    states: 2,
                    neighbors: neighborhood_type.neighbors(r, true).unwrap(),
                    birth: Vec::new(),
                    survival: Vec::new(),
                };

                assert_eq!(rule.radius(), r);
            }
        }
    }
}
