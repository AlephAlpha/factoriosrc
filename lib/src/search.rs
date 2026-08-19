use rand::RngExt;

use crate::{
    cell::{LifeCell, Reason},
    config::NewState,
    rule::{CellState, CheckResult, Implication},
    world::{Status, World},
};

/// The maximum number of cells that a lookahead probe may set before it stops.
const MAX_PROBE_DEDUCTIONS: usize = 256;

/// The result of a guess.
enum GuessResult {
    /// A guess was made, and the search continues.
    Guessed,
    /// All cells are known, so a solution was found.
    Solved,
    /// Lookahead found that no state of the next unknown cell is possible,
    /// so the search should backtrack.
    Conflict,
}

impl World {
    /// Check the neighborhood descriptor for a cell to see what it implies.
    ///
    /// It may deduce the state of some related cells, or find a conflict.
    ///
    /// If a conflict is found, return [`None`].
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    #[inline(always)]
    unsafe fn check_descriptor(&mut self, cell: &LifeCell) -> Option<()> {
        unsafe {
            // For a Generations rule, the exact states of the cell and its
            // successor matter, so a different check is needed.
            if self.rule.is_generations() {
                return self.check_generations(cell);
            }

            let result = self.rule.implies(cell.descriptor());

            // The descriptor does not imply anything.
            //
            // For a non-totalistic rule, the flags are empty when the states of
            // the individual unknown neighbors are not deduced, but some of them
            // may still be forced to be dead or alive.
            if result.is_empty() {
                return Some(());
            }

            // A conflict was found.
            if result.flags().contains(Implication::Conflict) {
                return None;
            }

            self.check_descriptor_implied(cell, result)
        }
    }

    /// Check the implication of a neighborhood descriptor that is not empty.
    ///
    /// This is the slow path of [`check_descriptor`](World::check_descriptor),
    /// handling the implications that require setting the states of some cells.
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    #[inline(never)]
    unsafe fn check_descriptor_implied(
        &mut self,
        cell: &LifeCell,
        result: CheckResult,
    ) -> Option<()> {
        unsafe {
            // The descriptor implies that the successor is dead or alive.
            //
            // In this case, the successor was unknown, so there is no implication about the cell
            // itself or its neighbors. So we can return early.
            if result
                .flags()
                .intersects(Implication::SuccessorDead | Implication::SuccessorAlive)
                && let Some(successor) = cell.successor.as_ref()
            {
                let state = if result.flags().contains(Implication::SuccessorAlive) {
                    CellState::Alive
                } else {
                    CellState::Dead
                };

                self.set_cell(successor, state, Reason::Deduced);

                return Some(());
            }

            // The descriptor implies that the current cell is dead or alive.
            if result
                .flags()
                .intersects(Implication::CurrentDead | Implication::CurrentAlive)
            {
                let state = if result.flags().contains(Implication::CurrentAlive) {
                    CellState::Alive
                } else {
                    CellState::Dead
                };

                self.set_cell(cell, state, Reason::Deduced);
            }

            // The descriptor implies that all unknown neighbors are dead or alive.
            if result
                .flags()
                .intersects(Implication::NeighborhoodDead | Implication::NeighborhoodAlive)
            {
                let state = if result.flags().contains(Implication::NeighborhoodAlive) {
                    CellState::Alive
                } else {
                    CellState::Dead
                };

                for i in 0..cell.neighborhood_len {
                    // Safety: the neighbors are in the same world as the cell.
                    if let Some(neighbor) = cell.neighborhood[i].as_ref()
                        && neighbor.state().is_none()
                    {
                        self.set_cell(neighbor, state, Reason::Deduced);
                    }
                }
            }

            // For a non-totalistic rule, set the individual unknown neighbors
            // that are forced to be dead or alive.
            //
            // The "all unknown neighbors are dead or alive" implication above is too strong
            // for a non-totalistic rule, where the arrangement of the neighbors matters.
            if result.forced() != 0 {
                self.set_forced_neighbors(cell, result);
            }

            Some(())
        }
    }

    /// Check the neighborhood descriptor of a cell for a Generations rule.
    ///
    /// In a Generations rule, a cell in a dying state transitions to the next
    /// state in each generation, regardless of the rule. Only a dead cell and
    /// an alive cell follow the underlying 2-state rule, which is what the
    /// lookup table describes. This check handles the exact states of the cell
    /// and its successor, and applies the lookup table only when it is sound to
    /// do so.
    ///
    /// In particular, the deductions of the lookup table assume that the cell
    /// and its neighbors are dead or alive, not dying, so:
    ///
    /// - the successor of a cell can only be deduced when the cell itself is
    ///   known to be dead or alive;
    /// - the state of the cell can only be deduced from the successor, or
    ///   deduced to be dead or alive when the successor is known to be alive;
    /// - a neighbor can only be deduced to be alive, because a neighbor in a
    ///   dying state would also behave as dead;
    /// - a conflict can only be trusted when the successor is known to be
    ///   alive, because a cell in the last dying state would otherwise be a
    ///   valid candidate.
    ///
    /// If a conflict is found, return [`None`].
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    #[inline(always)]
    unsafe fn check_generations(&mut self, cell: &LifeCell) -> Option<()> {
        unsafe {
            let num_states = self.rule.num_states();
            let state = cell.state();
            let successor_state = cell.successor_state.get();

            match state {
                // A dying cell always transitions to the next state.
                Some(CellState::Dying(i)) => {
                    let expected =
                        CellState::from_number(((i as u16 + 1) % num_states as u16) as u8);
                    match successor_state {
                        Some(successor) if successor == expected => Some(()),
                        Some(_) => None,
                        None => {
                            if let Some(successor) = cell.successor.as_ref() {
                                self.set_cell(successor, expected, Reason::Deduced);
                            }
                            Some(())
                        }
                    }
                }

                // A dead cell can never have a dying successor.
                Some(CellState::Dead) if matches!(successor_state, Some(CellState::Dying(_))) => {
                    None
                }

                // An alive cell can only have an alive successor or a successor
                // in the first dying state.
                Some(CellState::Alive)
                    if matches!(successor_state, Some(CellState::Dead))
                        || matches!(successor_state, Some(CellState::Dying(i)) if i != 2) =>
                {
                    None
                }

                // The state of the cell is unknown.
                None => match successor_state {
                    // The cell must be dead, or in the last dying state, which
                    // transitions to dead. If a dead cell would be born in this
                    // neighborhood, the cell must be in the last dying state.
                    Some(CellState::Dead) => {
                        if self
                            .rule
                            .implies(cell.descriptor())
                            .flags()
                            .contains(Implication::CurrentAlive)
                        {
                            self.set_cell(
                                cell,
                                CellState::from_number(num_states - 1),
                                Reason::Deduced,
                            );
                        }
                        Some(())
                    }

                    // The cell must be dead or alive, as determined by the rule.
                    Some(CellState::Alive) => {
                        let result = self.rule.implies(cell.descriptor());
                        if result.flags().contains(Implication::Conflict) {
                            return None;
                        }
                        if result.flags().contains(Implication::CurrentAlive) {
                            self.set_cell(cell, CellState::Alive, Reason::Deduced);
                        } else if result.flags().contains(Implication::CurrentDead) {
                            self.set_cell(cell, CellState::Dead, Reason::Deduced);
                        }
                        if result.flags().contains(Implication::NeighborhoodAlive) {
                            for i in 0..cell.neighborhood_len {
                                // Safety: the neighbors are in the same world as the cell.
                                if let Some(neighbor) = cell.neighborhood[i].as_ref()
                                    && neighbor.state().is_none()
                                {
                                    self.set_cell(neighbor, CellState::Alive, Reason::Deduced);
                                }
                            }
                        }
                        Some(())
                    }

                    // The cell must be in the previous dying state.
                    Some(CellState::Dying(i)) => {
                        self.set_cell(cell, CellState::from_number(i - 1), Reason::Deduced);
                        Some(())
                    }

                    // Nothing can be deduced from the successor.
                    None => Some(()),
                },

                // The cell is dead or alive, and the successor is dead, alive, or unknown.
                _ => {
                    let result = self.rule.implies(cell.descriptor());

                    // The descriptor does not imply anything.
                    if result.is_empty() {
                        return Some(());
                    }

                    // A conflict was found.
                    if result.flags().contains(Implication::Conflict) {
                        return None;
                    }

                    self.check_generations_implied(cell, state, result)
                }
            }
        }
    }

    /// Check a non-empty implication of a neighborhood descriptor for a
    /// Generations rule, when the cell itself is dead or alive.
    ///
    /// This is the slow path of [`check_generations`](World::check_generations),
    /// handling the implications that require setting the states of some cells.
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    #[inline(never)]
    unsafe fn check_generations_implied(
        &mut self,
        cell: &LifeCell,
        state: Option<CellState>,
        result: CheckResult,
    ) -> Option<()> {
        unsafe {
            // The descriptor implies that the successor is dead or alive.
            //
            // In this case, the successor was unknown, so there is no implication about
            // the cell itself or its neighbors. So we can return early.
            if result
                .flags()
                .intersects(Implication::SuccessorDead | Implication::SuccessorAlive)
                && let Some(successor) = cell.successor.as_ref()
            {
                let state = if result.flags().contains(Implication::SuccessorAlive) {
                    CellState::Alive
                } else if matches!(state, Some(CellState::Alive)) {
                    // An alive cell that does not survive enters the first dying state.
                    CellState::from_number(2)
                } else {
                    CellState::Dead
                };

                self.set_cell(successor, state, Reason::Deduced);

                return Some(());
            }

            // The descriptor implies that all unknown neighbors are alive.
            //
            // There is no "all unknown neighbors are dead" implication here: a neighbor
            // in a dying state would also behave as dead.
            if result.flags().contains(Implication::NeighborhoodAlive) {
                for i in 0..cell.neighborhood_len {
                    // Safety: the neighbors are in the same world as the cell.
                    if let Some(neighbor) = cell.neighborhood[i].as_ref()
                        && neighbor.state().is_none()
                    {
                        self.set_cell(neighbor, CellState::Alive, Reason::Deduced);
                    }
                }
            }

            // For a non-totalistic rule, set the individual unknown neighbors
            // that are forced to be alive.
            //
            // The neighbors forced to be dead are ignored: a neighbor in a
            // dying state would also behave as dead.
            if result.forced() != 0 {
                let flags = result.flags();
                let mut forced = result.forced();
                for i in 0..self.rule.neighborhood_size {
                    if forced & (0b10 << (2 * i)) != 0 {
                        forced &= !(0b11 << (2 * i));
                    }
                }
                if forced != 0 {
                    self.set_forced_neighbors(cell, CheckResult::new(flags, forced));
                }
            }

            Some(())
        }
    }

    /// Set the individual unknown neighbors of a cell that are forced to be
    /// dead or alive.
    ///
    /// This is only meaningful for non-totalistic rules, where the arrangement
    /// of the neighbors matters.
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    unsafe fn set_forced_neighbors(&mut self, cell: &LifeCell, result: CheckResult) {
        unsafe {
            let mut forced = result.forced();

            while forced != 0 {
                let i = (forced.trailing_zeros() >> 1) as usize;
                forced &= !(0b11 << (2 * i));

                if let Some(state) = result.forced_neighbor(i)
                    && let Some(neighbor) = cell.neighborhood[i].as_ref()
                    && neighbor.state().is_none()
                {
                    self.set_cell(neighbor, state, Reason::Deduced);
                }
            }
        }
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
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    #[inline]
    unsafe fn check_affected(&mut self, cell: &LifeCell) -> Option<()> {
        unsafe {
            // Check if the front becomes empty.
            if self.front_count == 0 {
                return None;
            }

            // Check if the population is too large.
            if self.max_population.is_some() && self.below_max == 0 {
                return None;
            }

            // Deduce the state of some cells by symmetry.
            let state = cell.state().unwrap();
            for i in 0..cell.symmetry.len() {
                let symmetry = &*cell.symmetry[i];
                match symmetry.state() {
                    None => self.set_cell(symmetry, state, Reason::Deduced),
                    Some(symmetry_state) if symmetry_state != state => return None,
                    Some(_) => {}
                }
            }

            // Check the neighborhood descriptor of the cell itself.
            self.check_descriptor(cell)?;

            // Check the neighborhood descriptor of the predecessor.
            if let Some(predecessor) = cell.predecessor.as_ref() {
                self.check_descriptor(predecessor)?;
            }

            // Check the neighborhood descriptors of the neighbors.
            //
            // For a totalistic rule, the non-null neighbors are packed to the
            // front of the array, so no null checks are needed.
            if self.rule.is_totalistic() {
                for i in 0..cell.neighborhood_len {
                    // Safety: the neighbors are in the same world as the cell.
                    let neighbor = &*cell.neighborhood[i];
                    self.check_descriptor(neighbor)?;
                }
            } else {
                for i in 0..cell.neighborhood_len {
                    // Safety: the neighbors are in the same world as the cell.
                    if let Some(neighbor) = cell.neighborhood[i].as_ref() {
                        self.check_descriptor(neighbor)?;
                    }
                }
            }

            Some(())
        }
    }

    /// Check all cells in the stack that have not been checked yet.
    ///
    /// If a conflict is found, return [`None`].
    fn check_stack(&mut self) -> Option<()> {
        self.check_stack_with_cap(None)
    }

    /// Check all cells in the stack that have not been checked yet.
    ///
    /// If a conflict is found, return [`None`].
    ///
    /// If `cap` is [`Some`], stop checking after `cap` cells have been set
    /// since the beginning of the call, even if there are more cells to check.
    fn check_stack_with_cap(&mut self, cap: Option<usize>) -> Option<()> {
        let stack_len = self.stack.len();

        while self.stack_index < self.stack.len() {
            if cap.is_some_and(|cap| self.stack.len() - stack_len > cap) {
                break;
            }

            unsafe {
                let cell = &*self.stack[self.stack_index].0;
                self.check_affected(cell)?;
                self.stack_index += 1;
            }
        }

        Some(())
    }

    /// Backtrack to the last cell whose state was chosen as a guess,
    /// and try another state for it.
    ///
    /// For a 2-state rule, the cell is set to the opposite state.
    /// For a Generations rule, the cell is set to the next state, until all
    /// states have been tried.
    ///
    /// Return the status of the search after backtracking:
    /// - If this goes back to the time before the search started, return [`NoSolution`](Status::NoSolution).
    /// - Otherwise, return [`Running`](Status::Running).
    fn backtrack(&mut self) -> Status {
        while let Some((cell, reason)) = self.stack.pop() {
            unsafe {
                let cell = &*cell;
                match reason {
                    Reason::Known => break,
                    Reason::Deduced => self.unset_cell(cell),
                    Reason::Guessed => {
                        let state = cell.state().unwrap();
                        self.stack_index = self.stack.len();
                        self.start = cell.next;
                        self.unset_cell(cell);

                        if self.rule.is_generations() {
                            let next = CellState::from_number(
                                ((state.number() as u16 + 1) % self.rule.num_states() as u16) as u8,
                            );
                            self.set_cell(
                                cell,
                                next,
                                Reason::TryAnother(self.rule.num_states() - 2),
                            );
                        } else {
                            self.set_cell(cell, !state, Reason::Deduced);
                        }
                        return Status::Running;
                    }
                    Reason::TryAnother(n) => {
                        let state = cell.state().unwrap();
                        self.stack_index = self.stack.len();
                        self.start = cell.next;
                        self.unset_cell(cell);

                        let next = CellState::from_number(
                            ((state.number() as u16 + 1) % self.rule.num_states() as u16) as u8,
                        );
                        let reason = if n == 1 {
                            Reason::Deduced
                        } else {
                            Reason::TryAnother(n - 1)
                        };
                        self.set_cell(cell, next, reason);
                        return Status::Running;
                    }
                }
            }
        }

        Status::NoSolution
    }

    /// Find a cell whose state is unknown, and make a guess.
    ///
    /// If lookahead is enabled for a 2-state rule, the two states of the cell
    /// are probed first, and the result determines the state to guess, or
    /// whether the search should backtrack.
    fn guess(&mut self) -> GuessResult {
        unsafe {
            while let Some(cell) = self.start.as_ref() {
                if cell.state().is_none() {
                    // If lookahead is enabled for a 2-state rule, probe both
                    // states of the cell before guessing.
                    if self.config.lookahead && !self.rule.is_generations() {
                        match self.probe(cell) {
                            Some(state) => {
                                self.set_cell(cell, state, Reason::Guessed);
                                self.start = cell.next;
                                return GuessResult::Guessed;
                            }
                            // Neither state is possible: the current partial
                            // assignment is contradictory.
                            None => return GuessResult::Conflict,
                        }
                    }

                    // If phase saving is enabled and the cell has been set
                    // before, guess its last state first.
                    let state = if self.config.phase_saving
                        && let Some(phase) = cell.phase.get()
                    {
                        phase
                    } else {
                        match self.config.new_state {
                            NewState::Alive => CellState::Alive,
                            NewState::Dead => CellState::Dead,
                            NewState::Random => {
                                if self.rule.is_generations() {
                                    CellState::from_number(
                                        self.rng.random_range(0..self.rule.num_states()),
                                    )
                                } else {
                                    self.rng.random()
                                }
                            }
                        }
                    };
                    self.set_cell(cell, state, Reason::Guessed);
                    self.start = cell.next;
                    return GuessResult::Guessed;
                }
                self.start = cell.next;
            }
        }

        GuessResult::Solved
    }

    /// Probe the two states of a cell to see which one is better to guess.
    ///
    /// For each state, the cell is temporarily set to that state, and the
    /// propagation is run until the queue is empty or
    /// [`MAX_PROBE_DEDUCTIONS`] cells have been set. The probe is then rolled
    /// back.
    ///
    /// Return the state that should be guessed:
    /// - If one state leads to a conflict, return the other state.
    /// - If both states are consistent, return the state that led to more
    ///   deductions (dead on ties).
    /// - If both states lead to a conflict, return [`None`], meaning that the
    ///   current partial assignment is contradictory.
    ///
    /// This is only called for 2-state rules.
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`, and its state must be
    /// unknown. Otherwise the behavior is undefined.
    unsafe fn probe(&mut self, cell: &LifeCell) -> Option<CellState> {
        let mut scores = [0usize; 2];
        let mut conflict = [false; 2];

        let states = [CellState::Dead, CellState::Alive];
        for (i, &state) in states.iter().enumerate() {
            let stack_len = self.stack.len();
            let stack_index = self.stack_index;

            self.in_probe = true;
            unsafe {
                self.set_cell(cell, state, Reason::Guessed);
            }
            let ok = self
                .check_stack_with_cap(Some(MAX_PROBE_DEDUCTIONS))
                .is_some();
            self.in_probe = false;

            let score = self.stack.len() - stack_len;

            // Roll back the probe.
            while self.stack.len() > stack_len {
                unsafe {
                    let probe_cell = &*self.stack.pop().unwrap().0;
                    self.unset_cell(probe_cell);
                }
            }
            self.stack_index = stack_index;

            scores[i] = score;
            conflict[i] = !ok;
        }

        if conflict[0] && conflict[1] {
            None
        } else if conflict[0] {
            Some(CellState::Alive)
        } else if conflict[1] {
            Some(CellState::Dead)
        } else if scores[1] > scores[0] {
            Some(CellState::Alive)
        } else {
            Some(CellState::Dead)
        }
    }

    /// One step of the search.
    ///
    /// Check all cells in the stack that have not been checked yet,
    /// backtrack if a conflict is found, and make a guess if all cells are checked.
    fn step(&mut self) -> Status {
        if self.check_stack().is_some() {
            // All cells have been checked.
            match self.guess() {
                // A guess was made.
                GuessResult::Guessed => Status::Running,
                // All cells are known.
                GuessResult::Solved => Status::Solved,
                // Lookahead found that the current partial assignment is
                // contradictory.
                GuessResult::Conflict => self.backtrack(),
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
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
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
                    self.config.max_population = self.max_population;
                    self.below_max = self
                        .population
                        .iter()
                        .filter(|&&pop| pop < population)
                        .count();
                }
                self.backtrack()
            }
            Status::NoSolution => Status::NoSolution,
            _ => Status::Running,
        };

        while status == Status::Running && max_steps.is_none_or(|max_steps| steps < max_steps) {
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
