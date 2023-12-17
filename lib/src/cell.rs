use crate::rule::{CellState, Descriptor, MAX_NEIGHBORHOOD_SIZE};
use std::default::Default;

/// A cell in the cellular automaton.
#[derive(Debug, Clone)]
pub(crate) struct LifeCell {
    /// The state of the cell.
    ///
    /// `None` means the cell is unknown.
    pub(crate) state: Option<CellState>,

    /// The neighborhood descriptor of the cell.
    pub(crate) descriptor: Descriptor,

    /// The predecessor of the cell.
    pub(crate) predecessor: Option<CellId>,

    /// The successor of the cell.
    pub(crate) successor: Option<CellId>,

    /// The neighborhood of the cell.
    pub(crate) neighborhood: [Option<CellId>; MAX_NEIGHBORHOOD_SIZE],

    /// Cells that are known to be equal to this cell because of the symmetry.
    pub(crate) symmetry: Vec<CellId>,

    /// The next cell to be searched according to the search order.
    pub(crate) next: Option<CellId>,

    /// Whether the cell is on the front, i.e. the first row or column, depending on the search order.
    ///
    /// This is used to ensure that the front is always non-empty.
    pub(crate) is_front: bool,
}

impl Default for LifeCell {
    fn default() -> Self {
        Self {
            state: None,
            descriptor: Descriptor::default(),
            predecessor: None,
            successor: None,
            neighborhood: [None; MAX_NEIGHBORHOOD_SIZE],
            symmetry: Vec::new(),
            next: None,
            is_front: false,
        }
    }
}

/// The identifier of a cell.
///
/// Currently, it is just the index of the cell in the cell list.
///
/// In the future, it may be changed to a non-null pointer to the cell,
/// using some unsafe code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct CellId(pub(crate) usize);
