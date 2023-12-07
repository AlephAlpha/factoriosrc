//! Currently Factorio (R3,C2,S2,B3,N+) is the only supported rule.
//!
//! In this rule, a cell has 12 neighbors in a cross shape.
//! - A dead cell comes to life if it has exactly 3 living neighbors.
//! - A living cell stays alive if it has exactly 2 living neighbors.

use enumflags2::{bitflags, BitFlags};
use std::{
    fmt::{self, Debug, Formatter},
    ops::Not,
};

/// The state of a known cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CellState {
    /// The cell is dead.
    Dead = 0b01,

    /// The cell is alive.
    Alive = 0b10,
}

impl Not for CellState {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Self::Dead => Self::Alive,
            Self::Alive => Self::Dead,
        }
    }
}

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

        assert!(dead + alive <= Factorio::NEIGHBORHOOD_SIZE as u16);
        assert!(successor < 3 && current < 3);

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

        write!(
            f,
            "Descriptor {{ dead: {}, alive: {}, successor: {:?}, current: {:?}, value: {:b} }}",
            dead, alive, successor, current, self.0
        )
    }
}

impl Descriptor {
    pub(crate) fn new(
        dead: usize,
        alive: usize,
        successor: impl Into<Option<CellState>>,
        current: impl Into<Option<CellState>>,
    ) -> Self {
        assert!(dead + alive <= Factorio::NEIGHBORHOOD_SIZE);

        let dead = dead as u16;
        let alive = alive as u16;
        let successor = successor.into().map_or(0, |state| state as u16);
        let current = current.into().map_or(0, |state| state as u16);
        Self(dead << 8 | alive << 4 | successor << 2 | current)
    }

    pub(crate) fn increment_dead(&mut self) {
        debug_assert!((self.0 >> 8) & 0b1111 < 12);
        self.0 += 1 << 8;
    }

    pub(crate) fn increment_alive(&mut self) {
        debug_assert!((self.0 >> 4) & 0b1111 < 12);
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
        debug_assert!((self.0 >> 2) & 0b11 < 3);
        self.0 ^= (state as u16) << 2;
    }

    /// If the current cell is unknown, sets it to some state.
    ///
    /// If the current cell is known, sets it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn set_current(&mut self, state: CellState) {
        debug_assert!(self.0 & 0b11 < 3);
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

/// The Factorio rule.
///
/// In this rule, a cell has 12 neighbors in a cross shape.
/// - A dead cell comes to life if it has exactly 3 living neighbors.
/// - A living cell stays alive if it has exactly 2 living neighbors.
///
/// This struct contains a lookup table for all possible neighborhood descriptors.
#[derive(Clone)]
pub struct Factorio {
    table: [BitFlags<Implication>; 1 << 12],
}

impl Factorio {
    /// In Factorio, a cell has 12 neighbors in a cross shape.
    pub const NEIGHBORHOOD_SIZE: usize = 12;

    /// Offsets of the neighbors.
    pub const OFFSETS: [(isize, isize); Self::NEIGHBORHOOD_SIZE] = [
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
        (3, 0),
    ];

    /// Radius of the neighborhood.
    pub const RADIUS: usize = 3;

    /// Name of the rule.
    pub const NAME: &'static str = "R3,C2,S2,B3,N+";

    /// Creates a new Factorio object and initializes the lookup table.
    pub fn new() -> Self {
        let table = [BitFlags::empty(); 1 << 12];
        let mut rule = Self { table };
        rule.init();
        rule
    }

    /// Initializes the lookup table.
    fn init(&mut self) {
        self.deduce_successor();
        self.deduce_conflict();
        self.deduce_current();
        self.deduce_neighborhood();
    }

    /// Deduces the implication of the successor cell.
    fn deduce_successor(&mut self) {
        // When all neighbors are known, the successor cell can be deduced directly from the rule.
        for dead in 0..=Self::NEIGHBORHOOD_SIZE {
            let alive = Self::NEIGHBORHOOD_SIZE - dead;

            // When the current cell is dead.
            let descriptor_dead = Descriptor::new(dead, alive, None, CellState::Dead);
            self.table[descriptor_dead.0 as usize] |= if alive == 3 {
                Implication::SuccessorAlive
            } else {
                Implication::SuccessorDead
            };

            // When the current cell is alive.
            let descriptor_alive = Descriptor::new(dead, alive, None, CellState::Alive);
            self.table[descriptor_alive.0 as usize] |= if alive == 2 {
                Implication::SuccessorAlive
            } else {
                Implication::SuccessorDead
            };

            // When the current cell is unknown.
            // In this case, the successor cell can still be deduced to be dead, if the number of living
            // neighbors is neither 2 nor 3.
            let descriptor_unknown = Descriptor::new(dead, alive, None, None);
            if alive != 2 && alive != 3 {
                self.table[descriptor_unknown.0 as usize] |= Implication::SuccessorDead;
            }
        }

        // Deduce for the case when some neighbors are unknown.
        //
        // If setting an unknown neighbor to both dead and alive leads to the same implication, then
        // we can deduce that the successor cell should be in that state.
        for unknown in 1..=Self::NEIGHBORHOOD_SIZE {
            for dead in 0..=Self::NEIGHBORHOOD_SIZE - unknown {
                let alive = Self::NEIGHBORHOOD_SIZE - dead - unknown;

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
        for dead in 0..=Self::NEIGHBORHOOD_SIZE {
            for alive in 0..=Self::NEIGHBORHOOD_SIZE - dead {
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
        for dead in 0..=Self::NEIGHBORHOOD_SIZE {
            for alive in 0..=Self::NEIGHBORHOOD_SIZE - dead {
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
        for unknown in 1..=Self::NEIGHBORHOOD_SIZE {
            for dead in 0..=Self::NEIGHBORHOOD_SIZE - unknown {
                let alive = Self::NEIGHBORHOOD_SIZE - dead - unknown;

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
