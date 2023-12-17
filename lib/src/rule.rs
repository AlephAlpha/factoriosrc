use crate::error::RuleError;
#[cfg(feature = "clap")]
use clap::ValueEnum;
use enumflags2::{bitflags, BitFlags};
use std::{
    fmt::{self, Debug, Formatter},
    ops::Not,
};

/// The state of a known cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
pub enum CellState {
    /// The cell is dead.
    #[cfg_attr(feature = "clap", value(name = "dead", aliases = ["d", "0"]))]
    Dead = 0b01,

    /// The cell is alive.
    #[cfg_attr(feature = "clap", value(name = "alive", aliases = ["a", "1"]))]
    Alive = 0b10,
}

impl Not for CellState {
    type Output = Self;

    #[inline]
    fn not(self) -> Self::Output {
        match self {
            Self::Dead => Self::Alive,
            Self::Alive => Self::Dead,
        }
    }
}

/// Currently, the numbers of living and dead neighbors are represented by 4-bit integers
/// in the neighborhood descriptor. So the neighborhood size is limited to 15.
pub const MAX_NEIGHBORHOOD_SIZE: usize = 15;

/// The neighborhood descriptor.
///
/// A 12-bit integer value that represents the state of a cell and its neighbors.
///
/// - The first 4 bits represent the number of known dead cells in the neighborhood.
/// - The next 4 bits represent the number of known alive cells in the neighborhood.
/// - The next 2 bits represent the state of the successor cell.
/// - The last 2 bits represent the state of the current cell.
///
/// In the last 4 bits, 0b01 means dead, 0b10 means alive, and 0b00 means unknown.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Descriptor(pub(crate) u16);

impl Debug for Descriptor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let dead = (self.0 >> 8) & 0b1111;
        let alive = (self.0 >> 4) & 0b1111;
        let successor = (self.0 >> 2) & 0b11;
        let current = self.0 & 0b11;

        let successor = match successor {
            0b00 => None,
            0b01 => Some(CellState::Dead),
            0b10 => Some(CellState::Alive),
            _ => unreachable!(),
        };

        let current = match current {
            0b00 => None,
            0b01 => Some(CellState::Dead),
            0b10 => Some(CellState::Alive),
            _ => unreachable!(),
        };

        f.debug_struct("Descriptor")
            .field("dead", &dead)
            .field("alive", &alive)
            .field("successor", &successor)
            .field("current", &current)
            .field("value", &format_args!("{:#014b}", self.0))
            .finish()
    }
}

impl Descriptor {
    pub(crate) fn new(
        dead: usize,
        alive: usize,
        successor: impl Into<Option<CellState>>,
        current: impl Into<Option<CellState>>,
    ) -> Self {
        debug_assert!(dead + alive <= MAX_NEIGHBORHOOD_SIZE);

        let dead = dead as u16;
        let alive = alive as u16;
        let successor = successor.into().map_or(0, |state| state as u16);
        let current = current.into().map_or(0, |state| state as u16);
        Self(dead << 8 | alive << 4 | successor << 2 | current)
    }

    pub(crate) fn increment_dead(&mut self) {
        debug_assert!((self.0 >> 8) & 0b1111 < MAX_NEIGHBORHOOD_SIZE as u16);
        self.0 += 1 << 8;
    }

    pub(crate) fn increment_alive(&mut self) {
        debug_assert!((self.0 >> 4) & 0b1111 < MAX_NEIGHBORHOOD_SIZE as u16);
        self.0 += 1 << 4;
    }

    pub(crate) fn decrement_dead(&mut self) {
        debug_assert!((self.0 >> 8) & 0b1111 > 0);
        self.0 -= 1 << 8;
    }

    pub(crate) fn decrement_alive(&mut self) {
        debug_assert!((self.0 >> 4) & 0b1111 > 0);
        self.0 -= 1 << 4;
    }

    /// If the successor cell is unknown, sets it to some state.
    ///
    /// If the successor cell is known, sets it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn set_successor(&mut self, state: CellState) {
        debug_assert!((self.0 >> 2) & 0b11 == 0b00 || (self.0 >> 2) & 0b11 == state as u16);
        self.0 ^= (state as u16) << 2;
    }

    /// If the current cell is unknown, sets it to some state.
    ///
    /// If the current cell is known, sets it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn set_current(&mut self, state: CellState) {
        debug_assert!(self.0 & 0b11 == 0b00 || self.0 & 0b11 == state as u16);
        self.0 ^= state as u16;
    }
}

/// Possible implications of a neighborhood descriptor.
#[bitflags]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Implication {
    /// A conflict has occurred.
    Conflict,

    /// The successor cell should be alive.
    SuccessorAlive,

    /// The successor cell should be dead.
    SuccessorDead,

    /// The current cell should be alive.
    CurrentAlive,

    /// The current cell should be dead.
    CurrentDead,

    /// All unknown cells in the neighborhood should be alive.
    NeighborhoodAlive,

    /// All unknown cells in the neighborhood should be dead.
    NeighborhoodDead,
}

/// The neighborhood type of a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum NeighborhoodType {
    /// The Moore neighborhood.
    Moore,

    /// The von Neumann neighborhood.
    VonNeumann,

    /// The cross neighborhood.
    Cross,
}

impl NeighborhoodType {
    /// Gets the offsets of the neighbors.
    pub fn offsets(self, radius: usize) -> Vec<(isize, isize)> {
        let radius = radius as isize;
        match self {
            Self::Moore => {
                let mut offsets = Vec::new();
                for x in -radius..=radius {
                    for y in -radius..=radius {
                        if x != 0 || y != 0 {
                            offsets.push((x, y));
                        }
                    }
                }
                offsets.sort();
                offsets
            }
            Self::VonNeumann => {
                let mut offsets = Vec::new();
                for x in -radius..=radius {
                    for y in -radius..=radius {
                        if x.abs() + y.abs() <= radius && (x != 0 || y != 0) {
                            offsets.push((x, y));
                        }
                    }
                }
                offsets.sort();
                offsets
            }
            Self::Cross => {
                let mut offsets = Vec::new();
                for x in -radius..=radius {
                    if x != 0 {
                        offsets.push((x, 0));
                    }
                }
                for y in -radius..=radius {
                    if y != 0 {
                        offsets.push((0, y));
                    }
                }
                offsets.sort();
                offsets
            }
        }
    }
}

/// A enum of all supported rules.
///
/// Currently only two rules are supported: Factorio (`R3,C2,S2,B3,N+`),
/// and Conway's Game of Life (`B3/S23`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "clap", derive(ValueEnum))]
pub enum Rule {
    /// Factorio (`R3,C2,S2,B3,N+`).
    Factorio,

    /// Conway's Game of Life (`B3/S23`).
    Life,
}

impl Rule {
    /// Creates a rule table from a rule.
    pub fn table(self) -> RuleTable {
        match self {
            Self::Factorio => RuleTable::factorio(),
            Self::Life => RuleTable::life(),
        }
    }
}

/// The lookup table and other information of a totalistic rule.
///
/// In a totalistic rule, the state of a cell is determined by the state of itself and
/// the number of living neighbors.
///
/// Currently, the numbers of living and dead neighbors are represented by 4-bit integers
/// in the neighborhood descriptor. So the neighborhood size is limited to 15.
#[derive(Clone)]
pub struct RuleTable {
    /// Name of the rule.
    pub(crate) name: String,

    /// The size of the neighborhood.
    pub(crate) neighborhood_size: usize,

    /// The offsets of the neighbors.
    pub(crate) offsets: Vec<(isize, isize)>,

    /// The radius of the neighborhood.
    pub(crate) radius: usize,

    /// The lookup table.
    table: [BitFlags<Implication>; 1 << 12],
}

impl Debug for RuleTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rule")
            .field("name", &self.name)
            .field("neighborhood_size", &self.neighborhood_size)
            .field("offsets", &self.offsets)
            .field("radius", &self.radius)
            .finish_non_exhaustive()
    }
}

impl RuleTable {
    /// Creates and initializes a rule table.
    ///
    /// - `born` is the list of numbers of living neighbors that cause a dead cell to come to life.
    /// - `survive` is the list of numbers of living neighbors that cause a living cell to stay alive.
    pub fn new(
        name: impl Into<String>,
        neighborhood_type: NeighborhoodType,
        radius: usize,
        born: &[usize],
        survive: &[usize],
    ) -> Result<Self, RuleError> {
        let offsets = neighborhood_type.offsets(radius);

        let neighborhood_size = offsets.len();

        if neighborhood_size > MAX_NEIGHBORHOOD_SIZE {
            return Err(RuleError::NeighborhoodTooLarge);
        }

        let table = [BitFlags::empty(); 1 << 12];
        let mut rule = Self {
            name: name.into(),
            neighborhood_size,
            offsets,
            radius,
            table,
        };
        rule.init(born, survive);
        Ok(rule)
    }

    /// The Factorio rule.
    ///
    /// In this rule, a cell has 12 neighbors in a cross shape of radius 3.
    /// - A dead cell comes to life if it has exactly 3 living neighbors.
    /// - A living cell stays alive if it has exactly 2 living neighbors.
    fn factorio() -> Self {
        Self::new("R3,C2,S2,B3,N+", NeighborhoodType::Cross, 3, &[3], &[2]).unwrap()
    }

    /// Conway's Game of Life.
    ///
    /// In this rule, a cell has 8 neighbors in a Moore neighborhood of radius 1.
    /// - A dead cell comes to life if it has exactly 3 living neighbors.
    /// - A living cell stays alive if it has exactly 2 or 3 living neighbors.
    fn life() -> Self {
        Self::new("B3/S23", NeighborhoodType::Moore, 1, &[3], &[2, 3]).unwrap()
    }

    /// Initializes the lookup table.
    fn init(&mut self, born: &[usize], survive: &[usize]) {
        self.deduce_successor(born, survive);
        self.deduce_conflict();
        self.deduce_current();
        self.deduce_neighborhood();
    }

    /// Deduces the implication of the successor cell.
    fn deduce_successor(&mut self, born: &[usize], survive: &[usize]) {
        // When all neighbors are known, the successor cell can be deduced directly from the rule.
        for dead in 0..=self.neighborhood_size {
            let alive = self.neighborhood_size - dead;

            // When the current cell is dead.
            let descriptor_dead = Descriptor::new(dead, alive, None, CellState::Dead);
            self.table[descriptor_dead.0 as usize] |= if born.contains(&alive) {
                Implication::SuccessorAlive
            } else {
                Implication::SuccessorDead
            };

            // When the current cell is alive.
            let descriptor_alive = Descriptor::new(dead, alive, None, CellState::Alive);
            self.table[descriptor_alive.0 as usize] |= if survive.contains(&alive) {
                Implication::SuccessorAlive
            } else {
                Implication::SuccessorDead
            };

            // When the current cell is unknown.
            // In this case, the successor cell can still be deduced to be dead, if the number of living
            // neighbors is neither in `born` nor in `survive`.
            let descriptor_unknown = Descriptor::new(dead, alive, None, None);
            if !born.contains(&alive) && !survive.contains(&alive) {
                self.table[descriptor_unknown.0 as usize] |= Implication::SuccessorDead;
            }
        }

        // Deduce for the case when some neighbors are unknown.
        //
        // If setting an unknown neighbor to both dead and alive leads to the same implication, then
        // we can deduce that the successor cell should be in that state.
        for unknown in 1..=self.neighborhood_size {
            for dead in 0..=self.neighborhood_size - unknown {
                let alive = self.neighborhood_size - dead - unknown;

                for current in [None, Some(CellState::Dead), Some(CellState::Alive)] {
                    let descriptor = Descriptor::new(dead, alive, None, current);
                    let one_more_dead = Descriptor::new(dead + 1, alive, None, current);
                    let one_more_alive = Descriptor::new(dead, alive + 1, None, current);

                    if self.implies(one_more_dead) == self.implies(one_more_alive) {
                        self.table[descriptor.0 as usize] = self.implies(one_more_dead);
                    }
                }
            }
        }
    }

    /// Deduces conflicts.
    fn deduce_conflict(&mut self) {
        // A conflict occurs when the successor cell is known but different from the deduced value.
        for dead in 0..=self.neighborhood_size {
            for alive in 0..=self.neighborhood_size - dead {
                for current in [None, Some(CellState::Dead), Some(CellState::Alive)] {
                    // First set the successor cell to be unknown.
                    let descriptor = Descriptor::new(dead, alive, None, current);
                    let implication = self.implies(descriptor);

                    // If the successor cell is deduced to be alive, then it should not be dead.
                    if implication.contains(Implication::SuccessorAlive) {
                        let descriptor_dead =
                            Descriptor::new(dead, alive, CellState::Dead, current);
                        self.table[descriptor_dead.0 as usize] = Implication::Conflict.into();
                    }

                    // If the successor cell is deduced to be dead, then it should not be alive.
                    if implication.contains(Implication::SuccessorDead) {
                        let descriptor_alive =
                            Descriptor::new(dead, alive, CellState::Alive, current);
                        self.table[descriptor_alive.0 as usize] = Implication::Conflict.into();
                    }
                }
            }
        }
    }

    /// Deduces the implication of the current cell.
    fn deduce_current(&mut self) {
        // If setting the current cell to some state leads to a conflict, then it should be in the
        // opposite state.
        for dead in 0..=self.neighborhood_size {
            for alive in 0..=self.neighborhood_size - dead {
                for successor in [CellState::Dead, CellState::Alive] {
                    let descriptor = Descriptor::new(dead, alive, successor, None);
                    let current_dead = Descriptor::new(dead, alive, successor, CellState::Dead);
                    let current_alive = Descriptor::new(dead, alive, successor, CellState::Alive);

                    if self.implies(current_dead).contains(Implication::Conflict) {
                        self.table[descriptor.0 as usize] |= Implication::CurrentAlive;
                    }

                    if self.implies(current_alive).contains(Implication::Conflict) {
                        self.table[descriptor.0 as usize] |= Implication::CurrentDead;
                    }
                }
            }
        }
    }

    /// Deduces the implication of the neighborhood.
    fn deduce_neighborhood(&mut self) {
        // If setting an unknown neighbor to some state leads to a conflict, then all unknown
        // neighbors should be in the opposite state.
        for unknown in 1..=self.neighborhood_size {
            for dead in 0..=self.neighborhood_size - unknown {
                let alive = self.neighborhood_size - dead - unknown;

                for successor in [CellState::Dead, CellState::Alive] {
                    for current in [None, Some(CellState::Dead), Some(CellState::Alive)] {
                        let descriptor = Descriptor::new(dead, alive, successor, current);
                        let one_more_dead = Descriptor::new(dead + 1, alive, successor, current);
                        let one_more_alive = Descriptor::new(dead, alive + 1, successor, current);

                        if self.implies(one_more_dead).contains(Implication::Conflict) {
                            self.table[descriptor.0 as usize] |= Implication::NeighborhoodAlive;
                        }

                        if self.implies(one_more_alive).contains(Implication::Conflict) {
                            self.table[descriptor.0 as usize] |= Implication::NeighborhoodDead;
                        }
                    }
                }
            }
        }
    }

    /// Finds the implication of a neighborhood descriptor.
    pub(crate) fn implies(&self, descriptor: Descriptor) -> BitFlags<Implication> {
        self.table[descriptor.0 as usize]
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_offsets() {
        assert_eq!(
            NeighborhoodType::Moore.offsets(1),
            [
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, -1),
                (0, 1),
                (1, -1),
                (1, 0),
                (1, 1)
            ]
        );

        assert_eq!(
            NeighborhoodType::VonNeumann.offsets(2),
            [
                (-2, 0),
                (-1, -1),
                (-1, 0),
                (-1, 1),
                (0, -2),
                (0, -1),
                (0, 1),
                (0, 2),
                (1, -1),
                (1, 0),
                (1, 1),
                (2, 0)
            ]
        );

        assert_eq!(
            NeighborhoodType::Cross.offsets(3),
            [
                (-3, 0),
                (-2, 0),
                (-1, 0),
                (0, -3),
                (0, -2),
                (0, -1),
                (0, 1),
                (0, 2),
                (0, 3),
                (1, 0),
                (2, 0),
                (3, 0)
            ]
        );
    }
}
