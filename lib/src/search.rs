use crate::{
    cell::CellId,
    rule::{CellState, Implication},
    world::{Reason, Status, World},
};

impl World {
    /// Check the neighborhood descriptor for a cell to see what it implies.
    ///
    /// It may deduce the state of some related cells, or find a conflict.
    ///
    /// If a conflict is found, return `None`.
    fn check_descriptor(&mut self, id: CellId) -> Option<()> {
        let descriptor = self.get_cell(id).descriptor;

        let implication = self.rule.implies(descriptor);

        // The descriptor does not imply anything.
        if implication.is_empty() {
            return Some(());
        }

        // A conflict was found.
        if implication.contains(Implication::Conflict) {
            return None;
        }

        // The descriptor implies that the successor is dead or alive.
        //
        // In this case, the successor was unknown, so there is no implication about the cell
        // itself or its neighbors. So we can return early.
        if implication.intersects(Implication::SuccessorDead | Implication::SuccessorAlive) {
            let state = if implication.contains(Implication::SuccessorAlive) {
                CellState::Alive
            } else {
                CellState::Dead
            };

            if let Some(successor_id) = self.get_cell(id).successor {
                self.set_cell(successor_id, state, Reason::Deduced);

                return Some(());
            }
        }

        // The descriptor implies that the current cell is dead or alive.
        if implication.intersects(Implication::CurrentDead | Implication::CurrentAlive) {
            let state = if implication.contains(Implication::CurrentAlive) {
                CellState::Alive
            } else {
                CellState::Dead
            };

            self.set_cell(id, state, Reason::Deduced);
        }

        // The descriptor implies that all unknown neighbors are dead or alive.
        if implication.intersects(Implication::NeighborhoodDead | Implication::NeighborhoodAlive) {
            let state = if implication.contains(Implication::NeighborhoodAlive) {
                CellState::Alive
            } else {
                CellState::Dead
            };

            for neighbor_id in self.get_cell(id).neighborhood.into_iter().flatten() {
                if self.get_cell(neighbor_id).state.is_none() {
                    self.set_cell(neighbor_id, state, Reason::Deduced);
                }
            }
        }

        Some(())
    }

    /// Check the neighborhood descriptor of a cell, its neighbors, and its predecessor.
    ///
    /// When the state of a cell is set, these are all the cells whose descriptors
    /// may be affected.
    ///
    /// This also checks if the front becomes empty, and deduces the state of some cells
    /// by symmetry.
    ///
    /// If a conflict is found, return `None`.
    fn check_affected(&mut self, id: CellId) -> Option<()> {
        if self.front_count == 0 {
            return None;
        }

        let state = self.get_cell(id).state.unwrap();
        for i in 0..self.get_cell(id).symmetry.len() {
            let symmetry_id = self.get_cell(id).symmetry[i];
            let symmetry_state = self.get_cell(symmetry_id).state;

            if symmetry_state.is_none() {
                self.set_cell(symmetry_id, state, Reason::Deduced);
            } else if symmetry_state.unwrap() != state {
                return None;
            }
        }

        self.check_descriptor(id)?;

        for neighbor_id in self.get_cell(id).neighborhood.into_iter().flatten() {
            self.check_descriptor(neighbor_id)?;
        }

        if let Some(predecessor_id) = self.get_cell(id).predecessor {
            self.check_descriptor(predecessor_id)?;
        }

        Some(())
    }

    /// Check all cells in the stack that have not been checked yet.
    ///
    /// If a conflict is found, return `None`.
    fn check_stack(&mut self) -> Option<()> {
        while self.stack_index < self.stack.len() {
            let id = self.stack[self.stack_index].0;
            self.check_affected(id)?;
            self.stack_index += 1;
        }

        Some(())
    }

    /// Backtrack to the last cell whose state was chosen as a guess,
    /// and deduce that it should be the opposite state.
    ///
    /// If this goes back to the time before the search started, return `None`.
    fn backtrack(&mut self) -> Option<()> {
        while let Some((id, reason)) = self.stack.pop() {
            match reason {
                Reason::Known => break,
                Reason::Deduced => self.unset_cell(id),
                Reason::Guessed => {
                    let state = self.get_cell(id).state.unwrap();
                    self.stack_index = self.stack.len();
                    self.start = self.get_cell(id).next;
                    self.unset_cell(id);
                    self.set_cell(id, !state, Reason::Deduced);
                    return Some(());
                }
            }
        }

        None
    }

    /// Find a cell whose state is unknown, and make a guess.
    ///
    /// If no cell is found, return `None`.
    fn guess(&mut self) -> Option<()> {
        while let Some(id) = self.start {
            if self.get_cell(id).state.is_none() {
                let state = self.config.new_state;
                self.set_cell(id, state, Reason::Guessed);
                self.start = self.get_cell(id).next;
                return Some(());
            } else {
                self.start = self.get_cell(id).next;
            }
        }

        None
    }

    /// One step of the search.
    ///
    /// Check all cells in the stack that have not been checked yet,
    /// backtrack if a conflict is found, and make a guess if all cells are checked.
    fn step(&mut self) -> Status {
        if let Some(()) = self.check_stack() {
            // All cells have been checked.
            if let Some(()) = self.guess() {
                // A guess was made.
                Status::Running
            } else {
                // All cells are known.
                Status::Solved
            }
        } else {
            // Backtrack.
            if let Some(()) = self.backtrack() {
                // Try the other state.
                Status::Running
            } else {
                // The search has failed.
                Status::Unsolvable
            }
        }
    }

    /// The main loop of the search.
    ///
    /// Search for a solution, or until the maximum number of steps is reached.
    ///
    /// Update and return the search status.
    pub fn search(&mut self, max_steps: impl Into<Option<usize>>) -> Status {
        let mut steps = 0;
        let max_steps = max_steps.into();
        let mut status = Status::Running;

        while !max_steps.is_some_and(|max_steps| steps >= max_steps) {
            status = self.step();
            steps += 1;

            if status != Status::Running {
                break;
            }
        }

        self.status = status;

        status
    }
}
