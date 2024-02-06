use crate::rule::{CellState, Descriptor, MAX_NEIGHBORHOOD_SIZE};
use std::cell::Cell;

/// A cell in the cellular automaton.
///
/// The name `LifeCell` is used to avoid confusion with the [`Cell`] type in `std::cell`.
///
/// # Safety
///
/// This struct contains raw pointers. It is safe to use as long as the following invariants are
/// maintained:
///
/// - Raw pointers in the `neighborhood` array are non-null.
/// - Other raw pointers may be null.
/// - When a pointer is non-null, it must point to a cell in the same [`World`].
#[derive(Debug)]
pub(crate) struct LifeCell {
    /// The generation of the cell.
    pub(crate) generation: i32,

    /// The state of the cell.
    ///
    /// [`None`] means the cell is unknown.
    pub(crate) state: Cell<Option<CellState>>,

    /// The neighborhood descriptor of the cell.
    pub(crate) descriptor: Cell<Descriptor>,

    /// The predecessor of the cell.
    pub(crate) predecessor: *const LifeCell,

    /// The successor of the cell.
    pub(crate) successor: *const LifeCell,

    /// The neighborhood of the cell.
    pub(crate) neighborhood: [*const LifeCell; MAX_NEIGHBORHOOD_SIZE],

    /// Cells that are known to be equal to this cell because of the symmetry.
    ///
    /// The pointers in this vector should be non-null.
    pub(crate) symmetry: Vec<*const LifeCell>,

    /// The next cell to be searched according to the search order.
    pub(crate) next: *const LifeCell,

    /// Whether the cell is on the front, i.e. the first row or column, depending on the search order.
    ///
    /// This is used to ensure that the front is always non-empty.
    pub(crate) is_front: bool,
}

impl LifeCell {
    /// Create a new cell in the given generation.
    ///
    /// Other fields are initialized to their default values.
    pub(crate) fn new(generation: i32) -> Self {
        Self {
            generation,
            state: Cell::new(None),
            descriptor: Cell::default(),
            predecessor: std::ptr::null(),
            successor: std::ptr::null(),
            neighborhood: [std::ptr::null(); MAX_NEIGHBORHOOD_SIZE],
            symmetry: Vec::new(),
            next: std::ptr::null(),
            is_front: false,
        }
    }

    /// Get the state of the cell.
    pub(crate) fn state(&self) -> Option<CellState> {
        self.state.get()
    }

    /// Get the neighborhood descriptor of the cell.
    pub(crate) fn descriptor(&self) -> Descriptor {
        self.descriptor.get()
    }

    /// Update the neighborhood descriptor to increment the number of dead neighbors.
    pub(crate) fn increment_dead(&self) {
        let mut descriptor = self.descriptor.get();
        descriptor.increment_dead();
        self.descriptor.set(descriptor);
    }

    /// Update the neighborhood descriptor to increment the number of living neighbors.
    pub(crate) fn increment_alive(&self) {
        let mut descriptor = self.descriptor.get();
        descriptor.increment_alive();
        self.descriptor.set(descriptor);
    }

    /// Update the neighborhood descriptor to decrement the number of dead neighbors.
    pub(crate) fn decrement_dead(&self) {
        let mut descriptor = self.descriptor.get();
        descriptor.decrement_dead();
        self.descriptor.set(descriptor);
    }

    /// Update the neighborhood descriptor to decrement the number of living neighbors.
    pub(crate) fn decrement_alive(&self) {
        let mut descriptor = self.descriptor.get();
        descriptor.decrement_alive();
        self.descriptor.set(descriptor);
    }

    /// Update the state of the successor cell in the neighborhood descriptor.
    ///
    /// If the successor cell is unknown, set it to some state.
    ///
    /// If the successor cell is known, set it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn update_successor(&self, state: CellState) {
        let mut descriptor = self.descriptor.get();
        descriptor.update_successor(state);
        self.descriptor.set(descriptor);
    }

    /// Update the state of the current cell in the neighborhood descriptor.
    ///
    /// If the current cell is unknown, set it to some state.
    ///
    /// If the current cell is known, set it to unknown. In this case,
    /// the `state` argument should be equal to its current state.
    pub(crate) fn update_current(&self, state: CellState) {
        let mut descriptor = self.descriptor.get();
        descriptor.update_current(state);
        self.descriptor.set(descriptor);
    }
}
