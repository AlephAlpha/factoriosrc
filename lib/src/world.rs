use crate::{
    cell::LifeCell,
    config::{Config, SearchOrder},
    error::ConfigError,
    rule::{CellState, RuleTable},
    symmetry::Symmetry,
};
use rand::{rngs::StdRng, SeedableRng};
use std::fmt::Write;

/// Coordinates of a cell in the world.
///
/// The first two coordinates are the x and y coordinates, respectively.
/// The third coordinate is the generation of the cell.
pub type Coord = (i32, i32, i32);

/// The reason why a cell is set to a state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Reason {
    /// The state is known from the configuration before the search.
    Known,

    /// The state is deduced from some other cells.
    Deduced,

    /// The state is chosen as a guess.
    Guessed,
}

/// Status of the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    /// The search has not started yet.
    NotStarted,
    /// The search is still running.
    Running,
    /// The search has found a solution.
    Solved,
    /// The search has shown that there is no solution.
    NoSolution,
}

/// The main struct of the search algorithm.
///
/// # Example
///
/// ```
/// use factoriosrc_lib::{Config, Status, World};
///
/// // Create a configuration that searches for a 3x3 oscillator with period 2 in Conway's Life.
/// let config = Config::new("B3/S23", 3, 3, 2);
/// // Create a world from the configuration.
/// let mut world = World::new(config).unwrap();
/// // Search for a solution.
/// world.search(None);
/// assert_eq!(world.status(), Status::Solved);
/// // Print the solution in RLE format.
/// println!("{}", world.rle(0));
/// ```
#[derive(Debug)]
pub struct World {
    /// The configuration of the world.
    pub(crate) config: Config,

    /// The rule table.
    pub(crate) rule: RuleTable,

    /// A pointer to the list of cells.
    pub(crate) cells_ptr: *mut [LifeCell],

    /// The length of the list of cells.
    pub(crate) size: usize,

    /// A random number generator for guessing the state of an unknown cell.
    pub(crate) rng: StdRng,

    /// The number of living cells on each generation.
    pub(crate) population: Vec<usize>,

    /// The upper bound of the population.
    pub(crate) max_population: Option<usize>,

    /// The number of unknown or living cells on the front, i.e. the first row or column,
    /// depending on the search order.
    ///
    /// This is used to ensure that the front is always non-empty.
    ///
    /// If we find a pattern where the front is always empty, we can move the whole pattern
    /// one cell towards the front, and the pattern will still be valid.
    /// So we can assume in the first place that the front is always non-empty.
    /// This will reduce the search space.
    ///
    /// However, some symmetries may disallow such a move.
    /// In that case, we will view the whole pattern at the first generation as the front,
    /// so that we won't find an empty pattern.
    pub(crate) front_count: usize,

    /// A stack for backtracking.
    ///
    /// It records the cells that have been set to a state,
    /// and the reason why they are set to that state.
    pub(crate) stack: Vec<(*const LifeCell, Reason)>,

    /// The index of the next cell to be checked in the stack.
    ///
    /// The part of the stack starting from this index can be seen as a queue.
    pub(crate) stack_index: usize,

    /// The starting point to look for an unknown cell according to the search order.
    pub(crate) start: *const LifeCell,

    /// Search status.
    pub(crate) status: Status,
}

impl Drop for World {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.cells_ptr);
        }
    }
}

impl World {
    /// Create a new world from a configuration.
    pub fn new(config: Config) -> Result<Self, ConfigError> {
        let config = config.check()?;

        let rule = RuleTable::new(config.parse_rule()?)?;
        let max_population = config.max_population;

        let (w, h, p) = (
            config.width as i32,
            config.height as i32,
            config.period as i32,
        );
        let r = rule.radius as i32;

        // Number of cells in the world.
        let size = ((w + 2 * r) * (h + 2 * r) * p) as usize;

        let cells = (0..size)
            .map(|i| LifeCell::new(i as i32 % p))
            .collect::<Box<[_]>>();

        let cells_ptr = Box::into_raw(cells);

        let rng = config
            .seed
            .map_or_else(StdRng::from_entropy, StdRng::seed_from_u64);

        let mut world = Self {
            config,
            rule,
            cells_ptr,
            size,
            rng,
            population: vec![0; p as usize],
            max_population,
            front_count: 0,
            stack: Vec::with_capacity(size),
            stack_index: 0,
            start: std::ptr::null(),
            status: Status::NotStarted,
        };
        world.init();

        Ok(world)
    }

    /// Initialize the world.
    fn init(&mut self) {
        self.init_front();
        self.init_neighborhood();
        self.init_predecessor_successor();
        self.init_symmetry();
        self.init_next();
        self.init_known();
    }

    /// For each cell, check if it is on the front.
    fn init_front(&mut self) {
        let mut use_front = false;

        match self.config.search_order.unwrap() {
            // If the search order is row-first, the front is the first row.
            SearchOrder::RowFirst => {
                if self.config.symmetry.is_subgroup_of(Symmetry::D2H)
                    && self.config.diagonal_width.is_none()
                {
                    use_front = true;

                    // If `dx` is zero, a pattern is still valid if we reflect it horizontally.
                    // So we only need to consider the left half of the first row.

                    let w = if self.config.dx == 0 {
                        (self.config.width + 1) / 2
                    } else {
                        self.config.width
                    };

                    // If both `dx` and `dy` are zero, a pattern is still valid if we rotate the
                    // generations, i.e. the first generation becomes the last, the second becomes
                    // the first, and so on. So we only need to consider the first generation.

                    // If `dx` is zero, `dy` is positive, a similar argument still applies.
                    // But the front becomes the `dy-1`-th row of the first generation.

                    if self.config.dx == 0 && self.config.dy >= 0 {
                        let y = self.config.dy.max(1) - 1;
                        for x in 0..w as i32 {
                            self.get_cell_by_coord_mut((x, y, 0)).unwrap().is_front = true;
                            self.front_count += 1;
                        }
                    } else {
                        for x in 0..w as i32 {
                            for t in 0..self.config.period as i32 {
                                self.get_cell_by_coord_mut((x, 0, t)).unwrap().is_front = true;
                                self.front_count += 1;
                            }
                        }
                    }
                }
            }

            // If the search order is column-first, the front is the first column.
            SearchOrder::ColumnFirst => {
                if self.config.symmetry.is_subgroup_of(Symmetry::D2V)
                    && self.config.diagonal_width.is_none()
                {
                    use_front = true;

                    // If `dy` is zero, a pattern is still valid if we reflect it vertically.
                    // So we only need to consider the top half of the first column.

                    let h = if self.config.dy == 0 {
                        (self.config.height + 1) / 2
                    } else {
                        self.config.height
                    };

                    // If both `dx` and `dy` are zero, a pattern is still valid if we rotate the
                    // generations, i.e. the first generation becomes the last, the second becomes
                    // the first, and so on. So we only need to consider the first generation.

                    // If `dy` is zero, `dx` is positive, a similar argument still applies.
                    // But the front becomes the `dx-1`-th column of the first generation.

                    if self.config.dx >= 0 && self.config.dy == 0 {
                        let x = self.config.dx.max(1) - 1;
                        for y in 0..h as i32 {
                            self.get_cell_by_coord_mut((x, y, 0)).unwrap().is_front = true;
                            self.front_count += 1;
                        }
                    } else {
                        for y in 0..h as i32 {
                            for t in 0..self.config.period as i32 {
                                self.get_cell_by_coord_mut((0, y, t)).unwrap().is_front = true;
                                self.front_count += 1;
                            }
                        }
                    }
                }
            }

            // If the search order is diagonal, the front is both the first row and the first column.
            SearchOrder::Diagonal => {
                if self.config.symmetry.is_subgroup_of(Symmetry::D2D) {
                    use_front = true;

                    let d = self.config.diagonal_width.unwrap_or(self.config.width);

                    // If `dx` equals `dy`, a pattern is still valid if we reflect it diagonally.
                    // So we only need to consider the first row, not the first column.

                    // If both `dx` and `dy` are zero, a pattern is still valid if we rotate the
                    // generations, i.e. the first generation becomes the last, the second becomes
                    // the first, and so on. So we only need to consider the first generation.

                    // If `dx` equals `dy` and is positive, a similar argument still applies.
                    // But the front becomes the `dy-1`-th row of the first generation.

                    if self.config.dx == self.config.dy && self.config.dx >= 0 {
                        let y = self.config.dy.max(1) - 1;
                        for x in 0..d as i32 {
                            self.get_cell_by_coord_mut((x, y, 0)).unwrap().is_front = true;
                            self.front_count += 1;
                        }
                    } else {
                        for x in 0..d as i32 {
                            for t in 0..self.config.period as i32 {
                                self.get_cell_by_coord_mut((x, 0, t)).unwrap().is_front = true;
                                self.front_count += 1;
                            }
                        }

                        if self.config.dx != self.config.dy {
                            for y in 1..d as i32 {
                                for t in 0..self.config.period as i32 {
                                    self.get_cell_by_coord_mut((0, y, t)).unwrap().is_front = true;
                                    self.front_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // If `use_front` is false, the front is the whole pattern at the first generation.
        if !use_front {
            for x in 0..self.config.width as i32 {
                for y in 0..self.config.height as i32 {
                    self.get_cell_by_coord_mut((x, y, 0)).unwrap().is_front = true;
                    self.front_count += 1;
                }
            }
        }
    }

    /// Set the neighborhood of each cell.
    ///
    /// Some cells may have a neighbor that is outside the world.
    /// In this case, the neighbor is set to [`None`].
    fn init_neighborhood(&mut self) {
        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );
        let r = self.rule.radius as i32;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    for i in 0..self.rule.neighborhood_size {
                        let (ox, oy) = self.rule.offsets[i];
                        let neighbor_coord = (x + ox, y + oy, t);
                        let neighbor = self.get_cell_by_coord_ptr(neighbor_coord);

                        let cell = self.get_cell_by_coord_mut((x, y, t)).unwrap();

                        cell.neighborhood[i] = neighbor;

                        // If some neighbor is outside the world, the state of that neighbor is assumed to be dead.
                        // So we update the neighborhood descriptor of the cell here.
                        if neighbor.is_null() {
                            cell.increment_dead();
                        }
                    }
                }
            }
        }
    }

    /// Set the predecessor and successor of each cell.
    fn init_predecessor_successor(&mut self) {
        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );
        let r = self.rule.radius as i32;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    let predecessor_coord = if t == 0 {
                        (x - self.config.dx, y - self.config.dy, p - 1)
                    } else {
                        (x, y, t - 1)
                    };

                    let successor_coord = if t == p - 1 {
                        (x + self.config.dx, y + self.config.dy, 0)
                    } else {
                        (x, y, t + 1)
                    };

                    let predecessor = self.get_cell_by_coord_ptr(predecessor_coord);
                    let successor = self.get_cell_by_coord_ptr(successor_coord);

                    let cell = self.get_cell_by_coord_mut((x, y, t)).unwrap();

                    // If the successor is outside the world, the state of the successor is assumed to be dead.
                    // So we update the neighborhood descriptor of the cell here.
                    if successor.is_null() {
                        cell.update_successor(CellState::Dead);
                    }

                    cell.predecessor = predecessor;
                    cell.successor = successor;
                }
            }
        }
    }

    // Set the symmetry cells of each cell.
    fn init_symmetry(&mut self) {
        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );
        let r = self.rule.radius as i32;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    let symmetry = self.config.symmetry;

                    let mut symmetry_coords = Vec::with_capacity(7);

                    if Symmetry::D2H.is_subgroup_of(symmetry) {
                        symmetry_coords.push((w - x - 1, y, t));
                    }

                    if Symmetry::D2V.is_subgroup_of(symmetry) {
                        symmetry_coords.push((x, h - y - 1, t));
                    }

                    if Symmetry::D2D.is_subgroup_of(symmetry) {
                        symmetry_coords.push((y, x, t));
                    }

                    if Symmetry::D2A.is_subgroup_of(symmetry) {
                        symmetry_coords.push((h - y - 1, w - x - 1, t));
                    }

                    if Symmetry::C4.is_subgroup_of(symmetry) {
                        symmetry_coords.push((y, w - x - 1, t));
                        symmetry_coords.push((h - y - 1, x, t));
                    }

                    if Symmetry::C2.is_subgroup_of(symmetry) {
                        symmetry_coords.push((w - x - 1, h - y - 1, t));
                    }

                    symmetry_coords.sort_unstable();
                    symmetry_coords.dedup();

                    let symmetry_cells = symmetry_coords
                        .into_iter()
                        .map(|coord| self.get_cell_by_coord_ptr(coord) as *const LifeCell)
                        .filter(|&cell| !cell.is_null())
                        .collect();

                    self.get_cell_by_coord_mut((x, y, t)).unwrap().symmetry = symmetry_cells;
                }
            }
        }
    }

    /// For each cell, find the next cell to be searched according to the search order.
    fn init_next(&mut self) {
        match self.config.search_order.unwrap() {
            SearchOrder::RowFirst => {
                for y in (0..self.config.height as i32).rev() {
                    for x in (0..self.config.width as i32).rev() {
                        for t in (0..self.config.period as i32).rev() {
                            let cell = self.get_cell_by_coord_ptr((x, y, t));

                            unsafe {
                                if (*cell).state().is_none() {
                                    let next = self.start;
                                    self.start = cell;
                                    self.get_cell_by_coord_mut((x, y, t)).unwrap().next = next;
                                }
                            }
                        }
                    }
                }
            }

            SearchOrder::ColumnFirst => {
                for x in (0..self.config.width as i32).rev() {
                    for y in (0..self.config.height as i32).rev() {
                        for t in (0..self.config.period as i32).rev() {
                            let cell = self.get_cell_by_coord_ptr((x, y, t));

                            unsafe {
                                if (*cell).state().is_none() {
                                    let next = self.start;
                                    self.start = cell;
                                    self.get_cell_by_coord_mut((x, y, t)).unwrap().next = next;
                                }
                            }
                        }
                    }
                }
            }

            SearchOrder::Diagonal => {
                let w = self.config.width as i32;

                for a in (0..2 * w - 1).rev() {
                    for x in (0..w).rev() {
                        let y = a - x;

                        if (0..w).contains(&y)
                            && !self
                                .config
                                .diagonal_width
                                .is_some_and(|d| (x - y).abs() >= d as i32)
                        {
                            for t in (0..self.config.period as i32).rev() {
                                let cell = self.get_cell_by_coord_ptr((x, y, t));

                                unsafe {
                                    if (*cell).state().is_none() {
                                        let next = self.start;
                                        self.start = cell;
                                        self.get_cell_by_coord_mut((x, y, t)).unwrap().next = next;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Set the state of known cells.
    ///
    /// The cells outside the bounding box are known to be dead.
    ///
    /// If the predecessor of a cell is outside the world, that cell is also known to be dead.
    ///
    /// In the future, user may be able to specify some cells to be known.
    fn init_known(&mut self) {
        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );
        let r = self.rule.radius as i32;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    let cell = self.get_cell_by_coord_ptr((x, y, t));

                    unsafe {
                        if !(0..w).contains(&x)
                            || !(0..h).contains(&y)
                            || self
                                .config
                                .diagonal_width
                                .is_some_and(|d| (x - y).abs() >= d as i32)
                            || (*cell).predecessor.is_null()
                        {
                            self.set_cell(&*cell, CellState::Dead, Reason::Known);
                        }
                    }
                }
            }
        }
    }

    /// Get a raw pointer to a cell by its coordinates.
    ///
    /// Return a null pointer if the cell is outside the world.
    fn get_cell_by_coord_ptr(&self, coord: Coord) -> *mut LifeCell {
        let (x, y, t) = coord;
        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );
        let r = self.rule.radius as i32;

        if (-r..w + r).contains(&x) && (-r..h + r).contains(&y) && (0..p).contains(&t) {
            let index = t + (x + r) * p + (y + r) * p * (w + 2 * r);
            debug_assert!(index >= 0 && index < self.size as i32);
            unsafe { (self.cells_ptr as *mut LifeCell).offset(index as isize) }
        } else {
            std::ptr::null_mut()
        }
    }

    /// Get a cell by its coordinates.
    ///
    /// Return [`None`] if the cell is outside the world.
    fn get_cell_by_coord(&self, coord: Coord) -> Option<&LifeCell> {
        unsafe { self.get_cell_by_coord_ptr(coord).as_ref() }
    }

    /// Get a mutable reference to a cell by its coordinates.
    ///
    /// Return [`None`] if the cell is outside the world.
    fn get_cell_by_coord_mut(&mut self, coord: Coord) -> Option<&mut LifeCell> {
        unsafe { self.get_cell_by_coord_ptr(coord).as_mut() }
    }

    /// Set the state of a cell. The cell should be unknown.
    pub(crate) fn set_cell(&mut self, cell: &LifeCell, state: CellState, reason: Reason) {
        debug_assert!(cell.state().is_none());
        cell.state.set(Some(state));

        // Update the neighborhood descriptor of the cell, its neighbors and predecessor.
        cell.update_current(state);

        for i in 0..self.rule.neighborhood_size {
            if let Some(neighbor) = unsafe { cell.neighborhood[i].as_ref() } {
                match state {
                    CellState::Dead => neighbor.increment_dead(),
                    CellState::Alive => neighbor.increment_alive(),
                }
            }
        }

        if let Some(predecessor) = unsafe { cell.predecessor.as_ref() } {
            predecessor.update_successor(state);
        }

        // If the cell is on the front, update the front count.
        if cell.is_front && state == CellState::Dead {
            self.front_count -= 1;
        }

        // If the cell is alive, update the population.
        if state == CellState::Alive {
            self.population[cell.generation as usize] += 1;
        }

        // Push the cell to the stack.
        self.stack.push((cell, reason));
    }

    /// Unset the state of a cell. The cell should be known.
    pub(crate) fn unset_cell(&mut self, cell: &LifeCell) {
        debug_assert!(cell.state().is_some());
        let state = cell.state().unwrap();
        cell.state.set(None);

        // Update the neighborhood descriptor of the cell, its neighbors and predecessor.
        cell.update_current(state);

        for i in 0..self.rule.neighborhood_size {
            if let Some(neighbor) = unsafe { cell.neighborhood[i].as_ref() } {
                match state {
                    CellState::Dead => neighbor.decrement_dead(),
                    CellState::Alive => neighbor.decrement_alive(),
                }
            }
        }

        if let Some(predecessor) = unsafe { cell.predecessor.as_ref() } {
            predecessor.update_successor(state);
        }

        // If the cell is on the front, update the front count.
        if cell.is_front && state == CellState::Dead {
            self.front_count += 1;
        }

        // If the cell was alive, update the population.
        if state == CellState::Alive {
            self.population[cell.generation as usize] -= 1;
        }
    }

    /// Get the state of a cell by its coordinates.
    ///
    /// If the cell is outside the world, it is considered to be dead.
    ///
    /// If the cell is unknown, return [`None`].
    #[inline]
    pub fn get_cell_state(&self, coord: Coord) -> Option<CellState> {
        self.get_cell_by_coord(coord)
            .map_or(Some(CellState::Dead), |cell| cell.state())
    }

    /// Get the search status.
    #[inline]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// Get the configuration.
    #[inline]
    pub const fn config(&self) -> &Config {
        &self.config
    }

    /// Get the number of living cells on a generation.
    #[inline]
    pub fn population(&self, t: i32) -> usize {
        let t = t.rem_euclid(self.config.period as i32);
        self.population[t as usize]
    }

    /// Output a generation of the world in RLE format.
    ///
    /// - Dead cells are represented by `.`.
    /// - Alive cells are represented by `o`.
    /// - Unknown cells are represented by `?`.
    /// - Each row is terminated by `$`.
    /// - The whole pattern is terminated by `!`.
    ///
    /// If the generation is out of the range `0..period`, we will take the modulo.
    pub fn rle(&self, t: i32) -> String {
        let mut s = String::new();

        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );

        let t = t.rem_euclid(p);

        writeln!(s, "x = {}, y = {}, rule = {}", w, h, self.config.rule_str).unwrap();

        for y in 0..h {
            for x in 0..w {
                let c = match self.get_cell_state((x, y, t)) {
                    Some(CellState::Dead) => '.',
                    Some(CellState::Alive) => 'o',
                    None => '?',
                };

                s.push(c);
            }

            if y < h - 1 {
                s.push('$');
            } else {
                s.push('!');
            }
            s.push('\n');
        }

        s
    }
}

#[cfg(test)]
mod test {
    use super::*;

    /// Test with Miri to see if there is any undefined behavior.
    #[test]
    fn test_miri() {
        let config = Config::new("B3/S23", 3, 3, 2);
        let mut world = World::new(config).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }
}
