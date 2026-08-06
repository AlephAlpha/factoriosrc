use crate::{cell::LifeCell, error::ConfigError};
use ca_rules2::{Neighborhood, Rule};
use enumflags2::{BitFlags, bitflags};
use rand::{
    Rng, RngExt,
    distr::{Distribution, StandardUniform},
};
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Debug, Formatter},
    ops::Not,
};

/// The state of a known cell.
///
/// The states are numbered from 0. State 0 is [`Dead`](CellState::Dead),
/// state 1 is [`Alive`](CellState::Alive), and the states `2..num_states - 1` are
/// dying states, where `num_states` is the number of states of the rule.
///
/// When serialized, the dead and alive states are represented by the strings
/// `"0"` and `"1"`, and a dying state is represented by its state number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CellState {
    /// The cell is dead.
    Dead,

    /// The cell is alive.
    Alive,

    /// The cell is in a dying state.
    ///
    /// In each generation, a cell in the `i`-th dying state transitions to the
    /// `(i + 1)`-th state, or to the dead state if `i` is the last dying state.
    Dying(u8),
}

#[cfg(feature = "serde")]
impl Serialize for CellState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Dead => serializer.serialize_str("0"),
            Self::Alive => serializer.serialize_str("1"),
            Self::Dying(i) => serializer.serialize_u8(*i),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for CellState {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct CellStateVisitor;

        impl serde::de::Visitor<'_> for CellStateVisitor {
            type Value = CellState;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"the string "0" or "1", or a state number"#)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "0" => Ok(CellState::Dead),
                    "1" => Ok(CellState::Alive),
                    _ => v
                        .parse::<u8>()
                        .map(CellState::from_number)
                        .map_err(serde::de::Error::custom),
                }
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u8::try_from(v)
                    .map(CellState::from_number)
                    .map_err(serde::de::Error::custom)
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u8::try_from(v)
                    .map(CellState::from_number)
                    .map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(CellStateVisitor)
    }
}

impl CellState {
    /// Create a state from its number.
    ///
    /// State 0 is dead, state 1 is alive, and the other states are dying states.
    #[inline]
    pub const fn from_number(n: u8) -> Self {
        match n {
            0 => Self::Dead,
            1 => Self::Alive,
            _ => Self::Dying(n),
        }
    }

    /// The number of the state.
    #[inline]
    pub const fn number(self) -> u8 {
        match self {
            Self::Dead => 0,
            Self::Alive => 1,
            Self::Dying(i) => i,
        }
    }

    /// The state of the cell in the underlying 2-state rule.
    ///
    /// This is the encoding used in the neighborhood descriptor: `0b01` means
    /// dead (or dying, which is dead for the underlying rule), and `0b10` means
    /// alive.
    #[inline]
    pub(crate) const fn base_code(self) -> u64 {
        match self {
            Self::Dead | Self::Dying(_) => 0b01,
            Self::Alive => 0b10,
        }
    }
}

impl Not for CellState {
    type Output = Self;

    #[inline]
    fn not(self) -> Self::Output {
        match self {
            Self::Alive => Self::Dead,
            _ => Self::Alive,
        }
    }
}

impl Distribution<CellState> for StandardUniform {
    #[inline]
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> CellState {
        match rng.random_range(0..2) {
            0 => CellState::Dead,
            1 => CellState::Alive,
            _ => unreachable!(),
        }
    }
}

/// Currently the maximum neighborhood size is 24.
pub const MAX_NEIGHBORHOOD_SIZE: usize = 24;

/// The maximum neighborhood size of an isotropic non-totalistic rule.
///
/// This is 8, which is the size of the range-1 Moore neighborhood, and also
/// covers the range-1 hexagonal neighborhood (6 cells). The lookup table of a
/// non-totalistic rule has `2^(2n + 4)` entries for a neighborhood of size `n`,
/// so it is not feasible to support larger neighborhoods.
pub const INT_MAX_NEIGHBORHOOD_SIZE: usize = 8;

/// The neighborhood descriptor.
///
/// An integer value that represents the state of a cell, its successor, and its neighborhood.
///
/// For a totalistic rule, it contains the numbers of dead and alive neighbors.
/// For an isotropic non-totalistic rule, it contains the bit masks of the alive
/// and unknown neighbors.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct Descriptor(pub(crate) u64);

impl Debug for Descriptor {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("Descriptor")
            .field("dead", &self.dead())
            .field("alive", &self.alive())
            .field("successor", &self.successor())
            .field("current", &self.current())
            .field("value", &format_args!("{:#032b}", self.0))
            .finish()
    }
}

impl Descriptor {
    /// The number of bits used to represent the number of living or dead neighbors.
    const NEIGHBOR_COUNT_BITS: usize = 6;

    /// A bit mask for the number of living or dead neighbors.
    const NEIGHBOR_COUNT_MASK: u64 = (1 << Self::NEIGHBOR_COUNT_BITS) - 1;

    /// The number of bits used to represent the state of the successor cell.
    const SUCCESSOR_BITS: usize = 2;

    /// A bit mask for the state of the successor or current cell.
    const STATE_MASK: u64 = (1 << Self::SUCCESSOR_BITS) - 1;

    /// The amount to shift to get the state of the current cell.
    const CURRENT_SHIFT: usize = 0;

    /// The amount to shift to get the state of the successor cell.
    const SUCCESSOR_SHIFT: usize = Self::SUCCESSOR_BITS;

    /// The amount to shift to get the number of living neighbors.
    const ALIVE_SHIFT: usize = Self::SUCCESSOR_BITS + Self::SUCCESSOR_BITS;

    /// The amount to shift to get the number of dead neighbors.
    const DEAD_SHIFT: usize = Self::NEIGHBOR_COUNT_BITS + Self::ALIVE_SHIFT;

    /// The total number of bits used to represent the neighborhood descriptor.
    const BITS: usize = Self::DEAD_SHIFT + Self::NEIGHBOR_COUNT_BITS;

    /// The amount to shift to get the first bit of the state of a neighbor of a
    /// non-totalistic rule.
    ///
    /// The states of the neighbors are stored in the bits
    /// `NEIGHBOR_STATE_SHIFT + 2i` and `NEIGHBOR_STATE_SHIFT + 2i + 1` for the
    /// `i`-th neighbor: `0b00` means unknown, `0b01` means alive, and `0b10`
    /// means dead.
    const NEIGHBOR_STATE_SHIFT: usize = Self::ALIVE_SHIFT;

    /// Get the number of dead neighbors.
    const fn dead(self) -> u16 {
        ((self.0 >> Self::DEAD_SHIFT) & Self::NEIGHBOR_COUNT_MASK) as u16
    }

    /// Get the number of living neighbors.
    const fn alive(self) -> u16 {
        ((self.0 >> Self::ALIVE_SHIFT) & Self::NEIGHBOR_COUNT_MASK) as u16
    }

    /// Get the state of the successor cell.
    const fn successor(self) -> Option<CellState> {
        match (self.0 >> Self::SUCCESSOR_SHIFT) & Self::STATE_MASK {
            0b00 => None,
            0b01 => Some(CellState::Dead),
            0b10 => Some(CellState::Alive),
            _ => unreachable!(),
        }
    }

    /// Get the state of the current cell.
    const fn current(self) -> Option<CellState> {
        match (self.0 >> Self::CURRENT_SHIFT) & Self::STATE_MASK {
            0b00 => None,
            0b01 => Some(CellState::Dead),
            0b10 => Some(CellState::Alive),
            _ => unreachable!(),
        }
    }

    /// Create a neighborhood descriptor from the number of dead and alive neighbors,
    /// and the states of the successor and current cells.
    pub(crate) fn new(
        dead: usize,
        alive: usize,
        successor: impl Into<Option<CellState>>,
        current: impl Into<Option<CellState>>,
    ) -> Self {
        debug_assert!(dead + alive <= MAX_NEIGHBORHOOD_SIZE);

        let dead = dead as u64;
        let alive = alive as u64;
        let successor = successor.into().map_or(0, |state| state.base_code());
        let current = current.into().map_or(0, |state| state.base_code());
        Self(
            dead << Self::DEAD_SHIFT
                | alive << Self::ALIVE_SHIFT
                | successor << Self::SUCCESSOR_SHIFT
                | current << Self::CURRENT_SHIFT,
        )
    }

    /// Increment the number of dead neighbors.
    pub(crate) fn increment_dead(&mut self) {
        debug_assert!(self.dead() < MAX_NEIGHBORHOOD_SIZE as u16);
        self.0 += 1 << Self::DEAD_SHIFT;
    }

    /// Increment the number of living neighbors.
    pub(crate) fn increment_alive(&mut self) {
        debug_assert!(self.alive() < MAX_NEIGHBORHOOD_SIZE as u16);
        self.0 += 1 << Self::ALIVE_SHIFT;
    }

    /// Decrement the number of dead neighbors.
    pub(crate) fn decrement_dead(&mut self) {
        debug_assert!(self.dead() > 0);
        self.0 -= 1 << Self::DEAD_SHIFT;
    }

    /// Decrement the number of living neighbors.
    pub(crate) fn decrement_alive(&mut self) {
        debug_assert!(self.alive() > 0);
        self.0 -= 1 << Self::ALIVE_SHIFT;
    }

    /// If the successor cell is unknown, set it to some state.
    ///
    /// If the successor cell is known, set it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn update_successor(&mut self, state: CellState) {
        debug_assert!(
            self.successor().is_none()
                || self.successor().unwrap().base_code() == state.base_code()
        );
        self.0 ^= state.base_code() << 2;
    }

    /// If the current cell is unknown, set it to some state.
    ///
    /// If the current cell is known, set it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn update_current(&mut self, state: CellState) {
        debug_assert!(
            self.current().is_none() || self.current().unwrap().base_code() == state.base_code()
        );
        self.0 ^= state.base_code();
    }
}

/// Possible implications of a neighborhood descriptor.
#[allow(clippy::use_self)]
#[bitflags]
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Implication {
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

/// The result of checking a neighborhood descriptor.
///
/// The low 8 bits are the implications for the cell, its successor and its
/// neighbors, and the remaining bits are the states that the individual
/// neighbors are forced to, 2 bits for each neighbor: `0b01` means alive, and
/// `0b10` means dead. The forced states are only meaningful for non-totalistic
/// rules; for totalistic rules they are always 0.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct CheckResult(u32);

impl CheckResult {
    /// Create a result from the implications and the forced states.
    #[inline]
    pub(crate) fn new(flags: BitFlags<Implication>, forced: u16) -> Self {
        Self(flags.bits() as u32 | (forced as u32) << 8)
    }

    /// Whether the descriptor implies nothing.
    #[inline]
    pub(crate) const fn is_empty(&self) -> bool {
        self.0 == 0
    }

    /// The implications for the cell, its successor and its neighbors.
    #[inline]
    pub(crate) fn flags(&self) -> BitFlags<Implication> {
        BitFlags::from_bits_truncate((self.0 & 0xff) as u8)
    }

    /// The states that the individual neighbors are forced to.
    ///
    /// For each neighbor `i`, the two bits `2i` and `2i + 1` (counting from the
    /// least significant bit) form the forced state of the `i`-th neighbor:
    /// `0b01` means alive, and `0b10` means dead.
    ///
    /// This is only meaningful for non-totalistic rules. For totalistic rules,
    /// this is always 0.
    #[inline]
    pub(crate) const fn forced(&self) -> u16 {
        (self.0 >> 8) as u16
    }

    /// The state that the `i`-th neighbor is forced to, if any.
    #[inline]
    pub(crate) const fn forced_neighbor(&self, i: usize) -> Option<CellState> {
        match (self.forced() >> (2 * i)) & 0b11 {
            0b01 => Some(CellState::Alive),
            0b10 => Some(CellState::Dead),
            _ => None,
        }
    }
}

/// The lookup table and other information of a rule.
///
/// For a totalistic rule, the state of a cell is determined by the state of itself and
/// the number of living neighbors. For an isotropic non-totalistic rule, it is determined
/// by the states of the individual neighbors.
///
/// Currently, the size of the neighborhood of a totalistic rule is limited to 24,
/// and the size of the neighborhood of a non-totalistic rule is limited to
/// [`INT_MAX_NEIGHBORHOOD_SIZE`].
#[derive(Clone)]
pub struct RuleTable {
    /// The size of the neighborhood.
    pub(crate) neighborhood_size: usize,

    /// The offsets of the neighbors.
    pub(crate) offsets: Vec<(i32, i32)>,

    /// The radius of the neighborhood.
    pub(crate) radius: u32,

    /// The lookup table.
    impl_: RuleTableImpl,

    /// The token of the state "alive" of the `i`-th neighbor.
    ///
    /// The `i`-th element is the bit of the descriptor of the `i`-th neighbor
    /// that marks the cell itself as alive. Toggling this bit changes the state
    /// of the cell from unknown to alive, or from alive to unknown.
    alive_tokens: [u64; MAX_NEIGHBORHOOD_SIZE],

    /// The token of the state "dead" of the `i`-th neighbor.
    ///
    /// See [`alive_tokens`](RuleTable::alive_tokens).
    dead_tokens: [u64; MAX_NEIGHBORHOOD_SIZE],

    /// The number of states of the rule.
    ///
    /// For a Generations rule, this is at least 3. A cell in a dying state
    /// transitions to the next state in each generation, regardless of the rule.
    pub(crate) num_states: u8,
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

/// The lookup table of a rule.
#[derive(Clone)]
enum RuleTableImpl {
    /// The lookup table of a totalistic rule.
    ///
    /// The index of the table is the whole descriptor, which contains the numbers
    /// of dead and alive neighbors, and the states of the current and successor cells.
    Count(CountTable),

    /// The lookup table of an isotropic non-totalistic rule.
    ///
    /// The index of the table is the descriptor masked to `2^(4 + 2n) - 1`, where
    /// `n` is the size of the neighborhood. The descriptor contains the bit masks
    /// of the alive and unknown neighbors, and the states of the current and
    /// successor cells.
    Mask(MaskTable),
}

impl RuleTable {
    /// Create and initialize a rule table from a [`Rule`].
    pub fn new(rule: &Rule) -> Result<Self, ConfigError> {
        if rule.contains_b0() || rule.states > 255 {
            return Err(ConfigError::UnsupportedRule);
        }

        let neighborhood_size = rule.neighborhood_size();

        if neighborhood_size > MAX_NEIGHBORHOOD_SIZE {
            return Err(ConfigError::UnsupportedRule);
        }

        let offsets = rule.neighbor_coords();
        let radius = rule.radius();

        // Find the index of the opposite offset of each offset.
        //
        // When a neighbor of a cell is set to a state, the bit of the neighbor's
        // descriptor that corresponds to the cell should be updated.
        let mut back_index = Vec::with_capacity(neighborhood_size);
        for &(ox, oy) in &offsets {
            let index = offsets.iter().position(|&(x, y)| (x, y) == (-ox, -oy));
            let Some(index) = index else {
                return Err(ConfigError::UnsupportedRule);
            };
            back_index.push(index);
        }

        let mut alive_tokens = [0; MAX_NEIGHBORHOOD_SIZE];
        let mut dead_tokens = [0; MAX_NEIGHBORHOOD_SIZE];
        for i in 0..neighborhood_size {
            alive_tokens[i] = 1 << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * back_index[i]);
            dead_tokens[i] = 1 << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * back_index[i] + 1);
        }

        let impl_ = match &rule.neighborhood {
            Neighborhood::Totalistic(_, _) if neighborhood_size <= MAX_NEIGHBORHOOD_SIZE => {
                RuleTableImpl::Count(CountTable::new(
                    neighborhood_size,
                    &rule.birth,
                    &rule.survival,
                ))
            }
            Neighborhood::Nontotalistic(_, _) if neighborhood_size <= INT_MAX_NEIGHBORHOOD_SIZE => {
                RuleTableImpl::Mask(MaskTable::new(
                    neighborhood_size,
                    &rule.birth,
                    &rule.survival,
                ))
            }
            _ => return Err(ConfigError::UnsupportedRule),
        };

        Ok(Self {
            neighborhood_size,
            offsets,
            radius,
            impl_,
            alive_tokens,
            dead_tokens,
            num_states: rule.states as u8,
        })
    }

    /// The number of states of the rule.
    #[inline]
    pub const fn num_states(&self) -> u8 {
        self.num_states
    }

    /// Whether the rule is a Generations rule.
    ///
    /// A Generations rule has at least 3 states. A cell in a dying state
    /// transitions to the next state in each generation, regardless of the rule.
    #[inline]
    pub const fn is_generations(&self) -> bool {
        self.num_states > 2
    }

    /// Find the implication of a neighborhood descriptor.
    pub(crate) fn implies(&self, descriptor: Descriptor) -> CheckResult {
        match &self.impl_ {
            RuleTableImpl::Count(count) => CheckResult::new(count.implies(descriptor), 0),
            RuleTableImpl::Mask(mask) => mask.implies(descriptor),
        }
    }

    /// Update the descriptor of a cell when a neighbor is set to a state.
    ///
    /// # Safety
    ///
    /// The neighbor must be in the same world as the cell.
    /// Otherwise the behavior is undefined.
    pub(crate) fn set_neighbor(&self, neighbor: &LifeCell, i: usize, state: CellState) {
        let mut descriptor = neighbor.descriptor.get();

        match &self.impl_ {
            RuleTableImpl::Count(_) => match state {
                CellState::Dead | CellState::Dying(_) => descriptor.increment_dead(),
                CellState::Alive => descriptor.increment_alive(),
            },
            RuleTableImpl::Mask(_) => {
                let token = match state {
                    CellState::Dead | CellState::Dying(_) => self.dead_tokens[i],
                    CellState::Alive => self.alive_tokens[i],
                };
                descriptor.0 ^= token;
            }
        }

        neighbor.descriptor.set(descriptor);
    }

    /// Update the descriptor of a cell when a neighbor is set to unknown.
    ///
    /// # Safety
    ///
    /// The neighbor must be in the same world as the cell.
    /// Otherwise the behavior is undefined.
    pub(crate) fn unset_neighbor(&self, neighbor: &LifeCell, i: usize, state: CellState) {
        let mut descriptor = neighbor.descriptor.get();

        match &self.impl_ {
            RuleTableImpl::Count(_) => match state {
                CellState::Dead | CellState::Dying(_) => descriptor.decrement_dead(),
                CellState::Alive => descriptor.decrement_alive(),
            },
            RuleTableImpl::Mask(_) => {
                let token = match state {
                    CellState::Dead | CellState::Dying(_) => self.dead_tokens[i],
                    CellState::Alive => self.alive_tokens[i],
                };
                descriptor.0 ^= token;
            }
        }

        neighbor.descriptor.set(descriptor);
    }

    /// Update the descriptor of a cell when one of its neighbors is outside the world.
    ///
    /// The neighbor is permanently dead.
    pub(crate) fn set_outside_neighbor(&self, cell: &LifeCell, i: usize) {
        let mut descriptor = cell.descriptor.get();

        match &self.impl_ {
            RuleTableImpl::Count(_) => descriptor.increment_dead(),
            RuleTableImpl::Mask(_) => {
                // The state of the neighbor is stored in the bits
                // `NEIGHBOR_STATE_SHIFT + 2i` and `NEIGHBOR_STATE_SHIFT + 2i + 1`.
                descriptor.0 |= 1 << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * i + 1);
            }
        }

        cell.descriptor.set(descriptor);
    }
}

/// The lookup table of a totalistic rule.
///
/// In a totalistic rule, the state of a cell is determined by the state of itself and
/// the number of living neighbors.
///
/// Currently, the neighborhood size is limited to 24.
#[derive(Clone)]
struct CountTable {
    /// The lookup table.
    table: Vec<BitFlags<Implication>>,
}

impl CountTable {
    /// Create and initialize a lookup table.
    fn new(neighborhood_size: usize, birth: &[u64], survival: &[u64]) -> Self {
        let mut table = Self {
            table: vec![BitFlags::empty(); 1 << Descriptor::BITS],
        };
        table.deduce_successor(neighborhood_size, birth, survival);
        table.deduce_conflict(neighborhood_size);
        table.deduce_current(neighborhood_size);
        table.deduce_neighborhood(neighborhood_size);
        table
    }

    /// Find the implication of a neighborhood descriptor.
    fn implies(&self, descriptor: Descriptor) -> BitFlags<Implication> {
        self.table[descriptor.0 as usize]
    }

    /// Deduce the implication of the successor cell.
    fn deduce_successor(&mut self, neighborhood_size: usize, birth: &[u64], survival: &[u64]) {
        // When all neighbors are known, the successor cell can be deduced directly from the rule.
        for dead in 0..=neighborhood_size {
            let alive = neighborhood_size - dead;

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
            // In this case, the successor cell can still be deduced, if it should be in the same state
            // regardless of whether the current cell is dead or alive.
            let descriptor_unknown = Descriptor::new(dead, alive, None, None);
            if birth.contains(&(alive as u64)) && survival.contains(&(alive as u64)) {
                self.table[descriptor_unknown.0 as usize] |= Implication::SuccessorAlive;
            }
            if !birth.contains(&(alive as u64)) && !survival.contains(&(alive as u64)) {
                self.table[descriptor_unknown.0 as usize] |= Implication::SuccessorDead;
            }
        }

        // Deduce for the case when some neighbors are unknown.
        //
        // If setting an unknown neighbor to both dead and alive leads to the same implication, then
        // we can deduce that the successor cell should be in that state.
        for unknown in 1..=neighborhood_size {
            for dead in 0..=neighborhood_size - unknown {
                let alive = neighborhood_size - dead - unknown;

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
    fn deduce_conflict(&mut self, neighborhood_size: usize) {
        // A conflict occurs when the successor cell is known but different from the deduced value.
        for dead in 0..=neighborhood_size {
            for alive in 0..=neighborhood_size - dead {
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
    fn deduce_current(&mut self, neighborhood_size: usize) {
        // If setting the current cell to some state leads to a conflict, then it should be in the
        // opposite state.
        for dead in 0..=neighborhood_size {
            for alive in 0..=neighborhood_size - dead {
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
    fn deduce_neighborhood(&mut self, neighborhood_size: usize) {
        // If setting an unknown neighbor to some state leads to a conflict, then all unknown
        // neighbors should be in the opposite state.
        for unknown in 1..=neighborhood_size {
            for dead in 0..=neighborhood_size - unknown {
                let alive = neighborhood_size - dead - unknown;

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
}

/// The lookup table of an isotropic non-totalistic rule.
///
/// In a non-totalistic rule, the state of a cell is determined by the states of the
/// individual neighbors, not just their number. The table is indexed by the descriptor,
/// which contains the bit masks of the alive and unknown neighbors, and the states of
/// the current and successor cells.
///
/// The low 8 bits of each entry are the [`BitFlags<Implication>`], and the remaining
/// bits are the states that the individual neighbors are forced to, 2 bits for each
/// neighbor: `0b01` means alive, and `0b10` means dead.
#[derive(Clone)]
struct MaskTable {
    /// The lookup table.
    table: Vec<u32>,

    /// The bit mask used to index the lookup table.
    index_mask: u32,
}

impl MaskTable {
    /// Create and initialize a lookup table.
    ///
    /// For a neighborhood of size `n`, the index of the table is
    /// `alive | unknown << n | current | successor << 2`, where `alive` and `unknown`
    /// are the bit masks of the alive and unknown neighbors.
    fn new(neighborhood_size: usize, birth: &[u64], survival: &[u64]) -> Self {
        let index_mask = (1 << (4 + 2 * neighborhood_size)) - 1;
        let mut table = Self {
            table: vec![0; 1 << (4 + 2 * neighborhood_size)],
            index_mask,
        };

        // Remove duplicate masks, so that we can count the number of matching
        // completions by counting the matching masks.
        let mut birth = birth.to_vec();
        birth.sort_unstable();
        birth.dedup();
        let mut survival = survival.to_vec();
        survival.sort_unstable();
        survival.dedup();

        let full_mask = (1 << neighborhood_size) - 1;

        // For each pair of masks of alive and unknown neighbors, fill the entries
        // of all the descriptors with the given states of the current and successor cells.
        for alive in 0..=full_mask {
            let complement = full_mask ^ alive;
            let mut unknown = complement;
            loop {
                table.fill_entry(
                    neighborhood_size,
                    alive as u64,
                    unknown as u64,
                    &birth,
                    &survival,
                );

                if unknown == 0 {
                    break;
                }
                unknown = (unknown - 1) & complement;
            }
        }

        table.deduce_forced_neighbors(neighborhood_size);

        table
    }

    /// Find the implication of a neighborhood descriptor.
    fn implies(&self, descriptor: Descriptor) -> CheckResult {
        CheckResult(self.table[descriptor.0 as usize & self.index_mask as usize])
    }

    /// The flags of a table entry.
    fn flags_at(&self, index: usize) -> BitFlags<Implication> {
        BitFlags::from_bits_truncate((self.table[index] & 0xff) as u8)
    }

    /// Fill the implications of all descriptors with the given masks of alive
    /// and unknown neighbors.
    fn fill_entry(&mut self, n: usize, alive: u64, unknown: u64, birth: &[u64], survival: &[u64]) {
        let range = alive | unknown;
        let total = 1 << unknown.count_ones();

        // The possible states of the successor cell.
        //
        // The `0b01` bit means that the successor cell can be dead, and the `0b10`
        // bit means that it can be alive.
        let reachable = |conditions: &[u64]| -> u8 {
            let matching = conditions
                .iter()
                .filter(|&&mask| mask & !range == 0 && mask & alive == alive)
                .count() as u64;
            let mut reachable = 0;
            if matching > 0 {
                reachable |= 0b10;
            }
            if matching < total {
                reachable |= 0b01;
            }
            reachable
        };

        // When the current cell is dead, the successor cell is determined by the birth conditions.
        let reachable_dead = reachable(birth);

        // When the current cell is alive, the successor cell is determined by the survival conditions.
        let reachable_alive = reachable(survival);

        let base = Self::states(n, alive, unknown);

        // When the current cell is known.
        self.fill_known_current(base | CellState::Dead.base_code(), reachable_dead);
        self.fill_known_current(base | CellState::Alive.base_code(), reachable_alive);

        // When the current cell is unknown.
        self.fill_unknown_current(base, reachable_dead, reachable_alive);
    }

    /// The bits of the states of the neighbors in the descriptor, given the
    /// masks of the alive and unknown neighbors.
    fn states(n: usize, alive: u64, unknown: u64) -> u64 {
        let mut states = 0;
        for i in 0..n {
            let state = if alive & (1 << i) != 0 {
                0b01
            } else if unknown & (1 << i) == 0 {
                0b10
            } else {
                0b00
            };
            states |= state << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * i);
        }
        states
    }

    /// Fill the implications of the descriptors with the given masks of alive and
    /// unknown neighbors and a known state of the current cell.
    fn fill_known_current(&mut self, base: u64, reachable: u8) {
        // When the successor cell is unknown.
        self.table[base as usize] = (if reachable == 0b10 {
            Implication::SuccessorAlive.into()
        } else if reachable == 0b01 {
            Implication::SuccessorDead.into()
        } else {
            BitFlags::empty()
        })
        .bits() as u32;

        // When the successor cell is known to be dead.
        self.table[(base | (CellState::Dead.base_code()) << 2) as usize] = (if reachable & 0b01 == 0
        {
            Implication::Conflict.into()
        } else {
            BitFlags::empty()
        })
        .bits() as u32;

        // When the successor cell is known to be alive.
        self.table[(base | (CellState::Alive.base_code()) << 2) as usize] =
            (if reachable & 0b10 == 0 {
                Implication::Conflict.into()
            } else {
                BitFlags::empty()
            })
            .bits() as u32;
    }

    /// Fill the implications of the descriptors with the given masks of alive and
    /// unknown neighbors and an unknown state of the current cell.
    fn fill_unknown_current(&mut self, base: u64, reachable_dead: u8, reachable_alive: u8) {
        let combined = reachable_dead | reachable_alive;

        // When the successor cell is unknown.
        self.table[base as usize] = (if combined == 0b10 {
            Implication::SuccessorAlive.into()
        } else if combined == 0b01 {
            Implication::SuccessorDead.into()
        } else {
            BitFlags::empty()
        })
        .bits() as u32;

        // When the successor cell is known to be dead.
        self.table[(base | (CellState::Dead.base_code()) << 2) as usize] =
            Self::current_flags(reachable_dead, reachable_alive, 0b01).bits() as u32;

        // When the successor cell is known to be alive.
        self.table[(base | (CellState::Alive.base_code()) << 2) as usize] =
            Self::current_flags(reachable_dead, reachable_alive, 0b10).bits() as u32;
    }

    /// The implications of the current cell when the successor cell is known.
    ///
    /// The `successor_state` argument is the bit of the possible states of the
    /// successor cell that corresponds to its known state.
    fn current_flags(
        reachable_dead: u8,
        reachable_alive: u8,
        successor_state: u8,
    ) -> BitFlags<Implication> {
        // If no state of the current cell can produce the known state of the successor
        // cell, a conflict has occurred.
        if reachable_dead & successor_state == 0 && reachable_alive & successor_state == 0 {
            return Implication::Conflict.into();
        }

        let mut flags = BitFlags::empty();

        // If the current cell were dead, the successor cell could not be in the known state.
        if reachable_dead & successor_state == 0 {
            flags |= Implication::CurrentAlive;
        }

        // If the current cell were alive, the successor cell could not be in the known state.
        if reachable_alive & successor_state == 0 {
            flags |= Implication::CurrentDead;
        }

        flags
    }

    /// Deduce the states that the individual neighbors are forced to.
    ///
    /// For each unknown neighbor of each descriptor, the state of the neighbor
    /// is forced to alive or dead if one of the two possible states would lead
    /// to a conflict. The forced states are stored in the table entries, 2 bits
    /// for each neighbor, starting from the 8-th bit.
    fn deduce_forced_neighbors(&mut self, neighborhood_size: usize) {
        let n = neighborhood_size;
        let full_mask = (1 << n) - 1;

        for alive in 0..=full_mask {
            let complement = full_mask ^ alive;
            let mut unknown = complement;
            loop {
                if unknown != 0 {
                    let base = Self::states(n, alive as u64, unknown as u64);

                    for i in 0..n {
                        let bit = 1 << i;
                        if unknown & bit == 0 {
                            continue;
                        }

                        // The two bits of the state of the `i`-th neighbor.
                        let field = 0b11 << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * i);
                        // The state of the `i`-th neighbor when it is set to dead.
                        let dead_field = 0b10 << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * i);
                        // The state of the `i`-th neighbor when it is set to alive.
                        let alive_field = 0b01 << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * i);

                        let dead_index = base & !field | dead_field;
                        let alive_index = base & !field | alive_field;

                        // For each state of the current cell and each known state
                        // of the successor cell.
                        for current in 0..=2 {
                            for successor in 1..=2 {
                                let index = base | current | successor << 2;
                                let dead_possible = !self
                                    .flags_at((dead_index | current | successor << 2) as usize)
                                    .contains(Implication::Conflict);
                                let alive_possible = !self
                                    .flags_at((alive_index | current | successor << 2) as usize)
                                    .contains(Implication::Conflict);

                                let forced = match (dead_possible, alive_possible) {
                                    (false, true) => 0b01, // forced alive
                                    (true, false) => 0b10, // forced dead
                                    _ => 0,
                                };

                                self.table[index as usize] |= forced << (8 + 2 * i);
                            }
                        }
                    }
                }

                if unknown == 0 {
                    break;
                }
                unknown = (unknown - 1) & complement;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ca_rules2::parse_rule;

    /// Create a descriptor of a range-1 Moore neighborhood with the given masks
    /// of alive and unknown neighbors and the states of the current and successor cells.
    const fn moore_descriptor(
        alive: u64,
        unknown: u64,
        current: u64,
        successor: u64,
    ) -> Descriptor {
        let mut value = current | successor << 2;
        let mut i = 0;
        while i < 8 {
            let state = if alive & (1 << i) != 0 {
                0b01
            } else if unknown & (1 << i) == 0 {
                0b10
            } else {
                0b00
            };
            value |= state << (Descriptor::NEIGHBOR_STATE_SHIFT + 2 * i);
            i += 1;
        }
        Descriptor(value)
    }

    #[test]
    fn test_mask_table_successor() {
        let rule = parse_rule("B2a/S12").unwrap();
        let table = RuleTable::new(&rule).unwrap();

        // A dead cell with two adjacent living neighbors is born.
        let descriptor = moore_descriptor(0x03, 0, CellState::Dead.base_code(), 0);
        assert!(
            table
                .implies(descriptor)
                .flags()
                .contains(Implication::SuccessorAlive)
        );

        // A dead cell with two non-adjacent living neighbors is not born.
        let descriptor = moore_descriptor(0x0a, 0, CellState::Dead.base_code(), 0);
        assert!(
            table
                .implies(descriptor)
                .flags()
                .contains(Implication::SuccessorDead)
        );

        // An alive cell with one living neighbor survives.
        let descriptor = moore_descriptor(0x01, 0, CellState::Alive.base_code(), 0);
        assert!(
            table
                .implies(descriptor)
                .flags()
                .contains(Implication::SuccessorAlive)
        );

        // An alive cell with no living neighbors dies.
        let descriptor = moore_descriptor(0, 0, CellState::Alive.base_code(), 0);
        assert!(
            table
                .implies(descriptor)
                .flags()
                .contains(Implication::SuccessorDead)
        );

        // If the successor cell is known to be dead, a birth pattern is a conflict.
        let descriptor = moore_descriptor(
            0x03,
            0,
            CellState::Dead.base_code(),
            CellState::Dead.base_code(),
        );
        assert!(
            table
                .implies(descriptor)
                .flags()
                .contains(Implication::Conflict)
        );
    }

    #[test]
    fn test_mask_table_current() {
        let rule = parse_rule("B2a/S12").unwrap();
        let table = RuleTable::new(&rule).unwrap();

        // If the successor cell is known to be alive, and the neighborhood has two
        // non-adjacent living neighbors, then the current cell cannot be dead
        // (a dead cell with such a neighborhood is not born).
        let descriptor = moore_descriptor(0x0a, 0, 0, CellState::Alive.base_code());
        assert!(
            table
                .implies(descriptor)
                .flags()
                .contains(Implication::CurrentAlive)
        );

        // If the successor cell is known to be dead, and the neighborhood has two
        // non-adjacent living neighbors, then the current cell cannot be alive
        // (an alive cell with such a neighborhood survives).
        let descriptor = moore_descriptor(0x0a, 0, 0, CellState::Dead.base_code());
        assert!(
            table
                .implies(descriptor)
                .flags()
                .contains(Implication::CurrentDead)
        );
    }

    #[test]
    fn test_mask_table_forced_neighbor() {
        let rule = parse_rule("B2a/S12").unwrap();
        let table = RuleTable::new(&rule).unwrap();

        // The neighborhood has two adjacent living neighbors, and the successor cell
        // is known to be alive. The third neighbor must be dead, because a pattern
        // with three living neighbors is not born.
        let descriptor = moore_descriptor(
            0x03,
            0x04,
            CellState::Dead.base_code(),
            CellState::Alive.base_code(),
        );
        assert_eq!(
            table.implies(descriptor).forced_neighbor(2),
            Some(CellState::Dead)
        );

        // If the successor cell is known to be dead, the third neighbor must be alive,
        // because the only way to avoid the birth is to have three living neighbors.
        let descriptor = moore_descriptor(
            0x03,
            0x04,
            CellState::Dead.base_code(),
            CellState::Dead.base_code(),
        );
        assert_eq!(
            table.implies(descriptor).forced_neighbor(2),
            Some(CellState::Alive)
        );

        // If the successor cell is unknown, no neighbor is forced.
        let descriptor = moore_descriptor(0x03, 0x04, CellState::Dead.base_code(), 0);
        assert_eq!(table.implies(descriptor).forced_neighbor(2), None);
    }
}
