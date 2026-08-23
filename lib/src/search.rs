use rand::RngExt;

use crate::{
    cell::{Antecedent, LifeCell, Reason},
    config::NewState,
    rule::{CellState, CheckResult, Implication},
    world::{Confl, Status, World},
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

                self.set_cell(
                    successor,
                    state,
                    Reason::Deduced,
                    Some(Antecedent::Descriptor(cell)),
                    false,
                );

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

                self.set_cell(
                    cell,
                    state,
                    Reason::Deduced,
                    Some(Antecedent::Descriptor(cell)),
                    false,
                );
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
                        self.set_cell(
                            neighbor,
                            state,
                            Reason::Deduced,
                            Some(Antecedent::Descriptor(cell)),
                            false,
                        );
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
                                self.set_cell(successor, expected, Reason::Deduced, None, false);
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
                                None,
                                false,
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
                            self.set_cell(cell, CellState::Alive, Reason::Deduced, None, false);
                        } else if result.flags().contains(Implication::CurrentDead) {
                            self.set_cell(cell, CellState::Dead, Reason::Deduced, None, false);
                        }
                        if result.flags().contains(Implication::NeighborhoodAlive) {
                            for i in 0..cell.neighborhood_len {
                                // Safety: the neighbors are in the same world as the cell.
                                if let Some(neighbor) = cell.neighborhood[i].as_ref()
                                    && neighbor.state().is_none()
                                {
                                    self.set_cell(
                                        neighbor,
                                        CellState::Alive,
                                        Reason::Deduced,
                                        None,
                                        false,
                                    );
                                }
                            }
                        }
                        Some(())
                    }

                    // The cell must be in the previous dying state.
                    Some(CellState::Dying(i)) => {
                        self.set_cell(
                            cell,
                            CellState::from_number(i - 1),
                            Reason::Deduced,
                            None,
                            false,
                        );
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

                self.set_cell(successor, state, Reason::Deduced, None, false);

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
                        self.set_cell(neighbor, CellState::Alive, Reason::Deduced, None, false);
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
                    self.set_cell(
                        neighbor,
                        state,
                        Reason::Deduced,
                        Some(Antecedent::Descriptor(cell)),
                        false,
                    );
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
    /// If a conflict is found, return the reason as an [`Err`].
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    #[inline]
    unsafe fn check_affected(&mut self, cell: &LifeCell) -> Result<(), Confl> {
        unsafe {
            // Check if the front becomes empty.
            if self.front_count == 0 {
                return Err(Confl::Global);
            }

            // Check if the population is too large.
            if self.max_population.is_some() && self.below_max == 0 {
                return Err(Confl::Global);
            }

            // Deduce the state of some cells by symmetry.
            let state = cell.state().unwrap();
            for i in 0..cell.symmetry.len() {
                let symmetry = &*cell.symmetry[i];
                match symmetry.state() {
                    None => self.set_cell(
                        symmetry,
                        state,
                        Reason::Deduced,
                        Some(Antecedent::Symmetry(cell)),
                        false,
                    ),
                    Some(symmetry_state) if symmetry_state != state => {
                        return Err(Confl::Symmetry(cell, symmetry));
                    }
                    Some(_) => {}
                }
            }

            // Check the neighborhood descriptor of the cell itself.
            self.check_descriptor(cell).ok_or(Confl::Rule(cell))?;

            // Check the neighborhood descriptor of the predecessor.
            if let Some(predecessor) = cell.predecessor.as_ref() {
                self.check_descriptor(predecessor)
                    .ok_or(Confl::Rule(predecessor))?;
            }

            // Check the neighborhood descriptors of the neighbors.
            //
            // For a totalistic rule, the non-null neighbors are packed to the
            // front of the array, so no null checks are needed.
            if self.rule.is_totalistic() {
                for i in 0..cell.neighborhood_len {
                    // Safety: the neighbors are in the same world as the cell.
                    let neighbor = &*cell.neighborhood[i];
                    self.check_descriptor(neighbor)
                        .ok_or(Confl::Rule(neighbor))?;
                }
            } else {
                for i in 0..cell.neighborhood_len {
                    // Safety: the neighbors are in the same world as the cell.
                    if let Some(neighbor) = cell.neighborhood[i].as_ref() {
                        self.check_descriptor(neighbor)
                            .ok_or(Confl::Rule(neighbor))?;
                    }
                }
            }

            Ok(())
        }
    }

    /// Check all cells in the stack that have not been checked yet.
    ///
    /// If a conflict is found, return the reason as an [`Err`].
    fn check_stack(&mut self) -> Result<(), Confl> {
        self.check_stack_with_cap(None)
    }

    /// Check all cells in the stack that have not been checked yet.
    ///
    /// If a conflict is found, return the reason as an [`Err`].
    ///
    /// If `cap` is [`Some`], stop checking after `cap` cells have been set
    /// since the beginning of the call, even if there are more cells to check.
    fn check_stack_with_cap(&mut self, cap: Option<usize>) -> Result<(), Confl> {
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

        Ok(())
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
                self.pop_meta();
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
                                None,
                                false,
                            );
                        } else {
                            self.set_cell(cell, !state, Reason::Deduced, None, true);
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
                        self.set_cell(cell, next, reason, None, false);
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
                    //
                    // The `Config::check` rejects lookahead for Generations
                    // rules, so this condition is only a defense in depth.
                    if self.config.lookahead && !self.rule.is_generations() {
                        match self.probe(cell) {
                            Some(state) => {
                                self.set_cell(cell, state, Reason::Guessed, None, true);
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
                    self.set_cell(cell, state, Reason::Guessed, None, true);
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
                self.set_cell(cell, state, Reason::Guessed, None, true);
            }
            let ok = self
                .check_stack_with_cap(Some(MAX_PROBE_DEDUCTIONS))
                .is_ok();
            self.in_probe = false;

            let score = self.stack.len() - stack_len;

            // Roll back the probe.
            while self.stack.len() > stack_len {
                let (probe_cell, _) = self.stack.pop().unwrap();
                unsafe {
                    self.pop_meta();
                    self.unset_cell(&*probe_cell);
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
        match self.check_stack() {
            Ok(()) => {
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
            }
            // A conflict was found.
            //
            // If backjumping is enabled and the conflict is a local one, it is
            // analyzed and the search backjumps to the decision that caused it;
            // otherwise the search backtracks chronologically.
            Err(confl) => match confl {
                Confl::Rule(_) | Confl::Symmetry(_, _) if self.config.backjump => {
                    self.analyze(confl)
                }
                _ => self.backtrack(),
            },
        }
    }

    /// Analyze a local conflict and backjump to the decision that caused it.
    ///
    /// This is the CA analogue of CDCL conflict analysis. The conflicting
    /// literal set is resolved backwards through the antecedents of the
    /// deductions (see [`Antecedent`](crate::cell::Antecedent)), until exactly
    /// one literal remains at the current decision level: the first unique
    /// implication point (1-UIP). The search then pops the trail down to the
    /// highest level of the remaining literals, and re-sets the 1-UIP cell to
    /// the opposite state, justified by the learned clause.
    ///
    /// A literal with no antecedent (a guess, or the flip of a guess by
    /// [`backtrack`](World::backtrack)) is resolved by simply removing it, like
    /// a decision in SAT.
    ///
    /// If the conflict cannot be analyzed (no 1-UIP is found), the search
    /// backtracks chronologically.
    fn analyze(&mut self, confl: Confl) -> Status {
        debug_assert!(self.config.backjump);

        let current = self.current_level;

        // A conflict before the first guess can never be resolved by
        // backjumping, so the search backtracks chronologically.
        if current == 0 {
            return self.backtrack();
        }

        // Bump the analysis stamp. A stamp of zero means "not seen".
        self.analysis_stamp = self.analysis_stamp.wrapping_add(1);
        if self.analysis_stamp == 0 {
            self.seen_stamp.fill(0);
            self.analysis_stamp = 1;
        }

        let mut clause: Vec<*const LifeCell> = Vec::new();
        let mut literals: Vec<*const LifeCell> = Vec::new();

        let mut max_level = 0;
        let mut seen_count = 0;

        // Note a literal of the clause being built. A literal at the current
        // level is marked for resolution, unless it was already marked; a
        // literal below the current level is kept in the learned clause.
        //
        // This accesses raw memory, so it must be called in an [`unsafe`] block.
        macro_rules! note_lit {
            ($lit:expr) => {{
                let lit = $lit;
                if (*lit).state().is_some() {
                    let index = self.cell_index(lit);
                    let level = self.cell_level[index];
                    if level == current {
                        if self.seen_stamp[index] != self.analysis_stamp {
                            self.seen_stamp[index] = self.analysis_stamp;
                            seen_count += 1;
                        }
                    } else if level != 0 && self.seen_stamp[index] != self.analysis_stamp {
                        self.seen_stamp[index] = self.analysis_stamp;
                        clause.push(lit);
                        max_level = max_level.max(level);
                    }
                }
            }};
        }

        // The seed: the literals that directly participate in the conflict.
        match confl {
            Confl::Rule(source) => unsafe {
                self.descriptor_literals(&*source, std::ptr::null(), usize::MAX, &mut literals);
            },
            Confl::Symmetry(cell, symmetry) => {
                literals.push(cell);
                literals.push(symmetry);
            }
            Confl::Global => unreachable!(),
        }
        unsafe {
            for &lit in &literals {
                note_lit!(lit);
            }
        }

        if seen_count == 0 {
            // No literal at the current level is involved: the conflict is
            // independent of the current decision, so just backtrack.
            return self.backtrack();
        }

        // Resolution: walk the trail downwards (read-only), and resolve the
        // marked literals at the current level, until only one remains.
        let mut i = self.stack.len();
        while seen_count > 1 {
            if i == 0 {
                // Defensive: the trail is exhausted without a 1-UIP.
                return self.backtrack();
            }
            i -= 1;

            let lit = self.stack[i].0;
            if self.trail_meta[i].level != current {
                continue;
            }

            let index = unsafe { self.cell_index(lit) };
            if self.seen_stamp[index] != self.analysis_stamp {
                continue;
            }

            // A reasonless literal at the current level can only be the
            // decision carrier of this level (the invariant of the trail),
            // which is the 1-UIP; stop the resolution here.
            if self.trail_meta[i].antecedent.is_none() {
                break;
            }

            // Resolve this literal: unmark it, and replace it by its antecedent.
            self.seen_stamp[index] = 0;
            seen_count -= 1;

            let antecedent = self.trail_meta[i].antecedent.clone();
            let ok = unsafe { self.reason_literals(lit, antecedent, i, &mut literals) };
            if !ok {
                // The reason of the literal is stale (a learned clause whose
                // cells have been set again since), so the resolution cannot
                // be trusted. Just backtrack chronologically.
                return self.backtrack();
            }
            unsafe {
                for &lit in &literals {
                    note_lit!(lit);
                }
            }
        }

        // Find the 1-UIP: the last remaining marked literal at the current level.
        let uip = loop {
            if i == 0 {
                // Defensive: the trail is exhausted without a 1-UIP.
                return self.backtrack();
            }
            i -= 1;
            let lit = self.stack[i].0;
            if self.trail_meta[i].level != current {
                continue;
            }
            let index = unsafe { self.cell_index(lit) };
            if self.seen_stamp[index] == self.analysis_stamp {
                break lit;
            }
        };

        // The state of the 1-UIP cell before it is popped.
        let state = unsafe { (*uip).state() }.unwrap();

        // Truncate the trail down to the highest level of the learned clause.
        // This pops the 1-UIP cell as well, since it is at a higher level.
        //
        // The search chain (the `next` pointers) is in a fixed spatial order
        // that differs from the trail order, so the resumption point is the
        // chain-earliest cell among the popped ones: any popped cell before
        // the chosen resumption point would otherwise never be revisited.
        let mut resume = std::ptr::null();
        let mut resume_rank = u32::MAX;
        // The lowest trail position of a remaining cell whose descriptor has
        // been changed by the pops. That cell and the cells above it must be
        // re-checked, since their incremental checks are stale (for example,
        // a symmetry deduction may not have set the mirrored cells).
        let mut recheck = self.stack.len();
        while self.current_level > max_level {
            let (cell, _) = self.stack.pop().unwrap();
            let rank = unsafe { self.chain_pos[self.cell_index(cell)] };
            if rank < resume_rank {
                resume_rank = rank;
                resume = cell;
            }
            unsafe {
                let popped = &*cell;
                // The popped cell updated the descriptors of its neighbors
                // and its predecessor; re-check from the lowest one.
                let mut affected = |c: *const LifeCell| {
                    if !c.is_null() {
                        let pos = self.cell_pos[self.cell_index(c)] as usize;
                        if pos < recheck {
                            recheck = pos;
                        }
                    }
                };
                affected(cell);
                if let Some(pred) = popped.predecessor.as_ref() {
                    affected(pred as *const LifeCell);
                }
                for i in 0..popped.neighborhood_len {
                    if let Some(neighbor) = popped.neighborhood[i].as_ref() {
                        affected(neighbor as *const LifeCell);
                    }
                }
                self.pop_meta();
                self.unset_cell(&*cell);
            }
        }

        self.stack_index = recheck;
        self.start = resume;

        // Record the learned clause: each literal with its current stack
        // position. The clause is valid while the cells stay at these
        // positions, i.e. until the cells are set again.
        let clause = clause
            .into_iter()
            .map(|cell| unsafe { (cell, self.cell_pos[self.cell_index(cell)]) })
            .collect::<Box<[_]>>();

        // Re-set the 1-UIP cell to the opposite state, justified by the
        // learned clause.
        unsafe {
            let uip = &*uip;
            self.set_cell(
                uip,
                !state,
                Reason::Deduced,
                Some(Antecedent::Clause(clause)),
                false,
            );
        }

        Status::Running
    }

    /// Collect the known cells in the neighborhood descriptor of a cell.
    ///
    /// This recovers the literals that contribute to a rule-based deduction or
    /// a conflict from the source cell of the descriptor. The `exclude`
    /// argument is the cell that was deduced (which is not part of its own
    /// antecedent), or [`std::ptr::null`] for a conflict seed. The `before`
    /// argument filters the cells to those at earlier stack positions: the
    /// exact antecedent of a deduction consists of the descriptor cells that
    /// were already in the stack when the deduction happened, so a cell set
    /// later (e.g. one that would have changed the deduction) is not part of
    /// it. A conflict seed uses [`usize::MAX`] to include all of them.
    ///
    /// # Safety
    ///
    /// The source cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    unsafe fn descriptor_literals(
        &self,
        source: &LifeCell,
        exclude: *const LifeCell,
        before: usize,
        literals: &mut Vec<*const LifeCell>,
    ) {
        literals.clear();
        unsafe {
            for i in 0..source.neighborhood_len {
                // Safety: the neighbors are in the same world as the cell.
                if let Some(neighbor) = source.neighborhood[i].as_ref()
                    && neighbor.state().is_some()
                    && (self.cell_pos[self.cell_index(neighbor as *const LifeCell)] as usize)
                        < before
                {
                    let neighbor = neighbor as *const LifeCell;
                    if neighbor != exclude {
                        literals.push(neighbor);
                    }
                }
            }

            if let Some(successor) = source.successor.as_ref()
                && successor.state().is_some()
                && (self.cell_pos[self.cell_index(successor as *const LifeCell)] as usize) < before
            {
                let successor = successor as *const LifeCell;
                if successor != exclude {
                    literals.push(successor);
                }
            }

            if source.state().is_some()
                && (self.cell_pos[self.cell_index(source as *const LifeCell)] as usize) < before
            {
                let source = source as *const LifeCell;
                if source != exclude {
                    literals.push(source);
                }
            }
        }
    }

    /// Collect the literals of the antecedent of a cell.
    ///
    /// The antecedent of a rule-based deduction is the exact reason of the
    /// deduction: the part of the descriptor of its source cell that was
    /// known when the deduction happened, excluding the deduced cell itself.
    /// A cell with no antecedent (a guess, or a guess flipped by
    /// [`backtrack`](World::backtrack)) has no reason literals.
    ///
    /// The `position` argument is the stack position of the deduced cell, used
    /// to filter the descriptor to the cells that were known at deduction time.
    ///
    /// Return `false` if the antecedent of a learned clause is stale, i.e. one
    /// of its cells is not at the recorded stack position anymore. In that
    /// case the reason is not valid anymore, and the conflict analysis must
    /// fall back to chronological backtracking.
    ///
    /// # Safety
    ///
    /// The cell and the antecedent must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    unsafe fn reason_literals(
        &self,
        cell: *const LifeCell,
        antecedent: Option<Antecedent>,
        position: usize,
        literals: &mut Vec<*const LifeCell>,
    ) -> bool {
        literals.clear();
        match antecedent {
            Some(Antecedent::Descriptor(source)) => unsafe {
                self.descriptor_literals(&*source, cell, position, literals);
                true
            },
            Some(Antecedent::Symmetry(source)) => {
                if source != cell {
                    unsafe {
                        if (*source).state().is_some() {
                            literals.push(source);
                        }
                    }
                }
                true
            }
            Some(Antecedent::Clause(clause)) => {
                for &(cell, pos) in clause.iter() {
                    unsafe {
                        if (*cell).state().is_none() || self.cell_pos[self.cell_index(cell)] != pos
                        {
                            // The clause is stale.
                            return false;
                        }
                        literals.push(cell);
                    }
                }
                true
            }
            None => true,
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
