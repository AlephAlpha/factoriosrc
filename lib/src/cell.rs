use crate::rule::{CellState, Descriptor, MAX_NEIGHBORHOOD_SIZE};
use std::cell::{Cell, RefCell};

/// A cell in the cellular automaton.
///
/// The name `LifeCell` is used to avoid confusion with the [`Cell`] type in `std::cell`.
#[derive(Debug, Clone, Default)]
pub(crate) struct LifeCell<'a> {
    /// The state of the cell.
    ///
    /// `None` means the cell is unknown.
    pub(crate) state: Cell<Option<CellState>>,

    /// The neighborhood descriptor of the cell.
    pub(crate) descriptor: Cell<Descriptor>,

    /// The predecessor of the cell.
    pub(crate) predecessor: Cell<Option<&'a LifeCell<'a>>>,

    /// The successor of the cell.
    pub(crate) successor: Cell<Option<&'a LifeCell<'a>>>,

    /// The neighborhood of the cell.
    pub(crate) neighborhood: [Cell<Option<&'a LifeCell<'a>>>; MAX_NEIGHBORHOOD_SIZE],

    /// Cells that are known to be equal to this cell because of the symmetry.
    pub(crate) symmetry: RefCell<Vec<&'a LifeCell<'a>>>,

    /// The next cell to be searched according to the search order.
    pub(crate) next: Cell<Option<&'a LifeCell<'a>>>,

    /// Whether the cell is on the front, i.e. the first row or column, depending on the search order.
    ///
    /// This is used to ensure that the front is always non-empty.
    pub(crate) is_front: Cell<bool>,
}

impl<'a> LifeCell<'a> {
    /// Get the state of the cell.
    pub(crate) fn state(&self) -> Option<CellState> {
        self.state.get()
    }

    /// Get the neighborhood descriptor of the cell.
    pub(crate) fn descriptor(&self) -> Descriptor {
        self.descriptor.get()
    }

    /// Whether the cell is on the front, i.e. the first row or column, depending on the search order.
    ///
    /// This is used to ensure that the front is always non-empty.
    pub(crate) fn is_front(&self) -> bool {
        self.is_front.get()
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
