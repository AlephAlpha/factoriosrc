use crate::rule::{CellState, Descriptor, MAX_NEIGHBORHOOD_SIZE};
use std::cell::Cell;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// The reason why a cell is set to a state.
///
/// When serialized, [`Known`](Reason::Known), [`Deduced`](Reason::Deduced),
/// and [`Guessed`](Reason::Guessed) are represented by the strings `"k"`,
/// `"d"`, and `"g"`, and [`TryAnother`](Reason::TryAnother) is represented by
/// the number of states that have not been tried yet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// The state is known from the configuration before the search.
    Known,

    /// The state is deduced from some other cells.
    Deduced,

    /// The state is chosen as a guess.
    Guessed,

    /// A guessed state was rejected, and the cell is set to another state.
    ///
    /// The field is the number of states that have not been tried yet.
    /// Only used in Generations rules.
    TryAnother(u8),
}

#[cfg(feature = "serde")]
impl Serialize for Reason {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Known => serializer.serialize_str("k"),
            Self::Deduced => serializer.serialize_str("d"),
            Self::Guessed => serializer.serialize_str("g"),
            Self::TryAnother(n) => serializer.serialize_u8(*n),
        }
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Reason {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        struct ReasonVisitor;

        impl serde::de::Visitor<'_> for ReasonVisitor {
            type Value = Reason;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str(r#"the string "k", "d", or "g", or a number of untried states"#)
            }

            fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                match v {
                    "k" => Ok(Reason::Known),
                    "d" => Ok(Reason::Deduced),
                    "g" => Ok(Reason::Guessed),
                    _ => Err(serde::de::Error::custom("invalid reason")),
                }
            }

            fn visit_u64<E>(self, v: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u8::try_from(v)
                    .map(Reason::TryAnother)
                    .map_err(serde::de::Error::custom)
            }

            fn visit_i64<E>(self, v: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u8::try_from(v)
                    .map(Reason::TryAnother)
                    .map_err(serde::de::Error::custom)
            }
        }

        deserializer.deserialize_any(ReasonVisitor)
    }
}

/// A cell in the cellular automaton.
///
/// The name `LifeCell` is used to avoid confusion with the [`Cell`] type in `std::cell`.
///
/// The fields are ordered (and the layout is fixed with `repr(C)`) so that
/// the fields accessed in the hot paths of the search are in the first cache
/// lines of the cell.
///
/// # Safety
///
/// This struct contains raw pointers. It is safe to use as long as the following invariants are
/// maintained:
///
/// - Raw pointers in the `symmetry` vector should be non-null.
/// - Other raw pointers may be null.
/// - When a pointer is non-null, it must point to a cell in the same [`World`].
/// - The pointers in the `neighborhood` array may be null; when non-null,
///   they must point to cells in the same [`World`].
#[derive(Debug)]
#[repr(C)]
pub struct LifeCell {
    /// The neighborhood descriptor of the cell.
    pub(crate) descriptor: Cell<Descriptor>,

    /// The state of the cell.
    ///
    /// [`None`] means the cell is unknown.
    pub(crate) state: Cell<Option<CellState>>,

    /// The state of the successor of the cell.
    ///
    /// [`None`] if the successor is unknown. This is a cached copy of the
    /// state of the successor cell, so that checking the cell does not need
    /// to dereference the successor. It is kept in sync in
    /// [`update_successor`](LifeCell::update_successor).
    pub(crate) successor_state: Cell<Option<CellState>>,

    /// The reason why this cell was set to its current state.
    ///
    /// [`None`] if the cell is unknown.
    pub(crate) reason: Cell<Option<Reason>>,

    /// Whether the cell is on the front, i.e. the first row or column, depending on the search order.
    ///
    /// This is used to ensure that the front is always non-empty.
    pub(crate) is_front: bool,

    /// The generation of the cell.
    pub(crate) generation: i32,

    /// The predecessor of the cell.
    pub(crate) predecessor: *const Self,

    /// The successor of the cell.
    pub(crate) successor: *const Self,

    /// The number of entries to iterate over in [`neighborhood`](LifeCell::neighborhood).
    ///
    /// For a totalistic rule, this is the number of non-null neighbors.
    /// For a non-totalistic rule, this is the size of the neighborhood,
    /// and the entries may be null.
    pub(crate) neighborhood_len: usize,

    /// The next cell to be searched according to the search order.
    pub(crate) next: *const Self,

    /// Cells that are known to be equal to this cell because of the symmetry.
    ///
    /// The pointers in this vector should be non-null.
    pub(crate) symmetry: Vec<*const Self>,

    /// The neighborhood of the cell.
    ///
    /// For a totalistic rule, the non-null entries are packed to the front of
    /// the array, and the entries in `0..neighborhood_len` are all non-null.
    /// This is sound because the state updates of a totalistic rule do not
    /// depend on the position of the neighbor in the array.
    ///
    /// For a non-totalistic rule, the entries keep their original positions,
    /// which may be interleaved with null entries.
    pub(crate) neighborhood: [*const Self; MAX_NEIGHBORHOOD_SIZE],
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
            successor_state: Cell::new(None),
            predecessor: std::ptr::null(),
            successor: std::ptr::null(),
            neighborhood: [std::ptr::null(); MAX_NEIGHBORHOOD_SIZE],
            neighborhood_len: 0,
            symmetry: Vec::new(),
            next: std::ptr::null(),
            is_front: false,
            reason: Cell::new(None),
        }
    }

    /// Get the state of the cell.
    pub(crate) const fn state(&self) -> Option<CellState> {
        self.state.get()
    }

    /// Get the neighborhood descriptor of the cell.
    pub(crate) const fn descriptor(&self) -> Descriptor {
        self.descriptor.get()
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

        let successor_state = self.successor_state.get();
        self.successor_state.set(if successor_state.is_none() {
            Some(state)
        } else {
            None
        });
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
