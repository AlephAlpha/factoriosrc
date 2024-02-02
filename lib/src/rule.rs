use crate::error::ConfigError;
use ca_rules2::{Neighborhood, NeighborhoodType, Rule};
use enumflags2::{bitflags, BitFlags};
use rand::{
    distributions::{Distribution, Standard},
    Rng,
};
use std::{
    fmt::{self, Debug, Formatter},
    ops::Not,
};

/// The state of a known cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellState {
    /// The cell is dead.
    Dead = 0b01,

    /// The cell is alive.
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

impl Distribution<CellState> for Standard {
    #[inline]
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> CellState {
        match rng.gen_range(0..2) {
            0 => CellState::Dead,
            1 => CellState::Alive,
            _ => unreachable!(),
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
    /// Create a neighborhood descriptor from the number of dead and alive neighbors,
    /// and the states of the successor and current cells.
    fn new(
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

    /// Increment the number of dead neighbors.
    pub(crate) fn increment_dead(&mut self) {
        debug_assert!((self.0 >> 8) & 0b1111 < MAX_NEIGHBORHOOD_SIZE as u16);
        self.0 += 1 << 8;
    }

    /// Increment the number of living neighbors.
    pub(crate) fn increment_alive(&mut self) {
        debug_assert!((self.0 >> 4) & 0b1111 < MAX_NEIGHBORHOOD_SIZE as u16);
        self.0 += 1 << 4;
    }

    /// Decrement the number of dead neighbors.
    pub(crate) fn decrement_dead(&mut self) {
        debug_assert!((self.0 >> 8) & 0b1111 > 0);
        self.0 -= 1 << 8;
    }

    /// Decrement the number of living neighbors.
    pub(crate) fn decrement_alive(&mut self) {
        debug_assert!((self.0 >> 4) & 0b1111 > 0);
        self.0 -= 1 << 4;
    }

    /// If the successor cell is unknown, set it to some state.
    ///
    /// If the successor cell is known, set it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn update_successor(&mut self, state: CellState) {
        debug_assert!((self.0 >> 2) & 0b11 == 0b00 || (self.0 >> 2) & 0b11 == state as u16);
        self.0 ^= (state as u16) << 2;
    }

    /// If the current cell is unknown, set it to some state.
    ///
    /// If the current cell is known, set it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn update_current(&mut self, state: CellState) {
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

/// The lookup table and other information of a totalistic rule.
///
/// In a totalistic rule, the state of a cell is determined by the state of itself and
/// the number of living neighbors.
///
/// Currently, the numbers of living and dead neighbors are represented by 4-bit integers
/// in the neighborhood descriptor. So the neighborhood size is limited to 15.
#[derive(Clone)]
pub struct RuleTable {
    /// The size of the neighborhood.
    pub(crate) neighborhood_size: usize,

    /// The offsets of the neighbors.
    pub(crate) offsets: Vec<(i32, i32)>,

    /// The radius of the neighborhood.
    pub(crate) radius: u32,

    /// The lookup table.
    table: [BitFlags<Implication>; 1 << 12],
}

impl Debug for RuleTable {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Rule")
            .field("neighborhood_size", &self.neighborhood_size)
            .field("offsets", &self.offsets)
            .field("radius", &self.radius)
            .finish_non_exhaustive()
    }
}

impl RuleTable {
    /// Create and initialize a rule table from a [`Rule`].
    pub fn new(rule: Rule) -> Result<Self, ConfigError> {
        if !matches!(rule.neighborhood, Neighborhood::Totalistic(neighborhood_type, _) if neighborhood_type != NeighborhoodType::Hexagonal)
        {
            return Err(ConfigError::UnsupportedRule);
        }

        let neighborhood_size = rule.neighborhood_size();

        if neighborhood_size > MAX_NEIGHBORHOOD_SIZE {
            return Err(ConfigError::UnsupportedRule);
        }

        let offsets = rule.neighbor_coords();
        let radius = rule.radius();

        let table: [BitFlags<Implication, u8>; 4096] = [BitFlags::empty(); 1 << 12];
        let mut rule_table = Self {
            neighborhood_size,
            offsets,
            radius,
            table,
        };
        rule_table.init(&rule.birth, &rule.survival);
        Ok(rule_table)
    }

    /// Initialize the lookup table.
    fn init(&mut self, birth: &[u64], survival: &[u64]) {
        self.deduce_successor(birth, survival);
        self.deduce_conflict();
        self.deduce_current();
        self.deduce_neighborhood();
    }

    /// Deduce the implication of the successor cell.
    fn deduce_successor(&mut self, birth: &[u64], survival: &[u64]) {
        // When all neighbors are known, the successor cell can be deduced directly from the rule.
        for dead in 0..=self.neighborhood_size {
            let alive = self.neighborhood_size - dead;

            // When the current cell is dead.
            let descriptor_dead = Descriptor::new(dead, alive, None, CellState::Dead);
            self.table[descriptor_dead.0 as usize] |= if birth.contains(&(alive as u64)) {
                Implication::SuccessorAlive
            } else {
                Implication::SuccessorDead
            };

            // When the current cell is alive.
            let descriptor_alive = Descriptor::new(dead, alive, None, CellState::Alive);
            self.table[descriptor_alive.0 as usize] |= if survival.contains(&(alive as u64)) {
                Implication::SuccessorAlive
            } else {
                Implication::SuccessorDead
            };

            // When the current cell is unknown.
            // In this case, the successor cell can still be deduced to be dead, if the number of living
            // neighbors is neither in `birth` nor in `survival`.
            let descriptor_unknown = Descriptor::new(dead, alive, None, None);
            if !birth.contains(&(alive as u64)) && !survival.contains(&(alive as u64)) {
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

    /// Deduce conflicts.
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

    /// Deduce the implication of the current cell.
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

    /// Deduce the implication of the neighborhood.
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

    /// Find the implication of a neighborhood descriptor.
    pub(crate) const fn implies(&self, descriptor: Descriptor) -> BitFlags<Implication> {
        self.table[descriptor.0 as usize]
    }
}
