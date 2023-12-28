use rand::Rng;

use crate::{
    cell::LifeCell,
    config::NewState,
    rule::{CellState, Implication},
    world::{Reason, Status, World},
};

impl<'a> World<'a> {
    /// Check the neighborhood descriptor for a cell to see what it implies.
    ///
    /// It may deduce the state of some related cells, or find a conflict.
    ///
    /// If a conflict is found, return [`None`].
    fn check_descriptor(&mut self, cell: &'a LifeCell<'a>) -> Option<()> {
        let implication = self.rule.implies(cell.descriptor());

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
            if let Some(successor) = cell.successor.get() {
                let state = if implication.contains(Implication::SuccessorAlive) {
                    CellState::Alive
                } else {
                    CellState::Dead
                };

                self.set_cell(successor, state, Reason::Deduced);

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

            self.set_cell(cell, state, Reason::Deduced);
        }

        // The descriptor implies that all unknown neighbors are dead or alive.
        if implication.intersects(Implication::NeighborhoodDead | Implication::NeighborhoodAlive) {
            let state = if implication.contains(Implication::NeighborhoodAlive) {
                CellState::Alive
            } else {
                CellState::Dead
            };

            for i in 0..self.rule.neighborhood_size {
                if let Some(neighbor) = cell.neighborhood[i].get() {
                    if neighbor.state().is_none() {
                        self.set_cell(neighbor, state, Reason::Deduced);
                    }
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
    /// This also checks if the front becomes empty, checks if the population is too large,
    /// and deduces the state of some cells by symmetry.
    ///
    /// If a conflict is found, return [`None`].
    fn check_affected(&mut self, cell: &'a LifeCell<'a>) -> Option<()> {
        // Check if the front becomes empty.
        if self.front_count == 0 {
            return None;
        }

        // Check if the population is too large.
        if self
            .max_population
            .is_some_and(|max_population| *self.population.iter().min().unwrap() > max_population)
        {
            return None;
        }

        // Deduce the state of some cells by symmetry.
        let state = cell.state().unwrap();
        for i in 0..cell.symmetry.borrow().len() {
            let symmetry = cell.symmetry.borrow()[i];
            let symmetry_state = symmetry.state();

            if symmetry_state.is_none() {
                self.set_cell(symmetry, state, Reason::Deduced);
            } else if symmetry_state.unwrap() != state {
                return None;
            }
        }

        // Check the neighborhood descriptor of the cell itself.
        self.check_descriptor(cell)?;

        // Check the neighborhood descriptors of the neighbors.
        for i in 0..self.rule.neighborhood_size {
            if let Some(neighbor) = cell.neighborhood[i].get() {
                self.check_descriptor(neighbor)?;
            }
        }

        // Check the neighborhood descriptor of the predecessor.
        if let Some(predecessor) = cell.predecessor.get() {
            self.check_descriptor(predecessor)?;
        }

        Some(())
    }

    /// Check all cells in the stack that have not been checked yet.
    ///
    /// If a conflict is found, return [`None`].
    fn check_stack(&mut self) -> Option<()> {
        while self.stack_index < self.stack.len() {
            let cell = self.stack[self.stack_index].0;
            self.check_affected(cell)?;
            self.stack_index += 1;
        }

        Some(())
    }

    /// Backtrack to the last cell whose state was chosen as a guess,
    /// and deduce that it should be the opposite state.
    ///
    /// Return the status of the search after backtracking:
    /// - If this goes back to the time before the search started, return [`NoSolution`](Status::NoSolution).
    /// - Otherwise, return [`Running`](Status::Running).
    fn backtrack(&mut self) -> Status {
        while let Some((cell, reason)) = self.stack.pop() {
            match reason {
                Reason::Known => break,
                Reason::Deduced => self.unset_cell(cell),
                Reason::Guessed => {
                    let state = cell.state().unwrap();
                    self.stack_index = self.stack.len();
                    self.start = cell.next.get();
                    self.unset_cell(cell);
                    self.set_cell(cell, !state, Reason::Deduced);
                    return Status::Running;
                }
            }
        }

        Status::NoSolution
    }

    /// Find a cell whose state is unknown, and make a guess.
    ///
    /// If no cell is found, return [`None`].
    fn guess(&mut self) -> Option<()> {
        while let Some(cell) = self.start {
            if cell.state().is_none() {
                let state = match self.config.new_state {
                    NewState::Alive => CellState::Alive,
                    NewState::Dead => CellState::Dead,
                    NewState::Random => self.rng.gen(),
                };
                self.set_cell(cell, state, Reason::Guessed);
                self.start = cell.next.get();
                return Some(());
            } else {
                self.start = cell.next.get();
            }
        }

        None
    }

    /// One step of the search.
    ///
    /// Check all cells in the stack that have not been checked yet,
    /// backtrack if a conflict is found, and make a guess if all cells are checked.
    fn step(&mut self) -> Status {
        if self.check_stack().is_some() {
            // All cells have been checked.
            if self.guess().is_some() {
                // A guess was made.
                Status::Running
            } else {
                // All cells are known.
                Status::Solved
            }
        } else {
            // Backtrack.
            self.backtrack()
        }
    }

    /// When a pattern is found, check that its period is correct.
    ///
    /// For example, when we are searching for a period 4 oscillator,
    /// we need to exclude still lifes and period 2 oscillators.
    fn check_period(&self) -> bool {
        let (w, h, p) = (
            self.config.width as isize,
            self.config.height as isize,
            self.config.period as isize,
        );
        let dx = self.config.dx;
        let dy = self.config.dy;

        // The actual period of the pattern must be a divisor of the period we are searching for.

        'd: for d in 2..=p {
            if p % d == 0 && dx % d == 0 && dy % d == 0 {
                // Check that if the actual period is p / d.
                // If so, return false.

                let p0 = p / d;
                let dx0 = dx / d;
                let dy0 = dy / d;

                // We only need to check the cells in the first generation.
                for x in 0..w {
                    for y in 0..h {
                        let state0 = self.get_cell_state((x, y, 0));
                        let state1 = self.get_cell_state((x - dx0, y - dy0, p0));
                        if state0 != state1 {
                            continue 'd;
                        }
                    }
                }

                return false;
            }
        }

        true
    }

    /// The main loop of the search.
    ///
    /// Search for a solution, or until the maximum number of steps is reached.
    ///
    /// Update and return the search status.
    pub fn search(&mut self, max_steps: impl Into<Option<usize>>) -> Status {
        let mut steps = 0;
        let max_steps = max_steps.into();

        let mut status = match self.status {
            // If the current status is `Solved`, backtrack to find the next solution.
            Status::Solved => {
                if self.config.reduce_max_population {
                    let population = *self.population.iter().min().unwrap();
                    self.max_population = Some(population - 1);
                }
                self.backtrack()
            }
            Status::NoSolution => Status::NoSolution,
            _ => Status::Running,
        };

        while status == Status::Running && !max_steps.is_some_and(|max_steps| steps >= max_steps) {
            status = self.step();

            // If a pattern is found, check that its period is correct,
            // and backtrack if not.
            if status == Status::Solved && !self.check_period() {
                status = self.backtrack();
            }

            steps += 1;
        }

        self.status = status;

        status
    }
}
