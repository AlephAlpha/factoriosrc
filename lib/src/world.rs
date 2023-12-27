use crate::{
    cell::LifeCell,
    config::{Config, SearchOrder, Symmetry},
    error::ConfigError,
    rule::{CellState, RuleTable},
};
use std::fmt::Write;

/// Coordinates of a cell in the world.
pub type Coord = (isize, isize, isize);

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

/// A helper struct to allocate cells for the world.
///
/// # Why do we need this?
///
/// **TLDR: to make Rust's borrow checker happy.**
///
/// We need to allocate a vector of cells for the world. Each cell contains some references to
/// other cells. All the cells and the world itself have the same lifetime `'a`.
///
/// However, Rust does not allow self-referential struct in safe code. So we cannot put the
/// vector in the [`World`] directly. To work around this, we put the vector in a separate struct,
/// and use a reference to the vector in the [`World`].
///
/// The user should first create a [`WorldAllocator`], and then create a [`World`] from a mutable
/// reference to the [`WorldAllocator`].
///
/// Since the [`WorldAllocator`] and the [`World`] have the same lifetime `'a`, you cannot create
/// multiple worlds from the same allocator.
///
/// # Example
///
/// ```
/// use factoriosrc_lib::{Config, Rule, WorldAllocator};
///
/// // Create a world allocator.
/// let mut allocator = WorldAllocator::new();
/// // Create a configuration that searches for a 3x3 oscillator with period 2 in Conway's Life.
/// let config = Config::new(Rule::Life, 3, 3, 2);
/// // Create a world from the configuration and the allocator.
/// let mut world = allocator.new_world(config).unwrap();
/// // The world is now ready to be searched.
/// ```
#[derive(Debug, Clone, Default)]
pub struct WorldAllocator<'a> {
    /// A vector of cells.
    cells: Vec<LifeCell<'a>>,
}

impl<'a> WorldAllocator<'a> {
    /// Create a new `WorldAllocator`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Create a new `World` from a configuration and the allocator.
    pub fn new_world(&'a mut self, config: Config) -> Result<World<'a>, ConfigError> {
        World::new(config, self)
    }
}

/// The main struct of the search algorithm.
///
/// Due to the limitation of Rust's borrow checker, we need to create the world from a mutable
/// reference to a [`WorldAllocator`]. Please see the documentation of [`WorldAllocator`] for more
/// details.
///
/// # Example
///
/// ```
/// use factoriosrc_lib::{Config, Rule, Status, WorldAllocator};
///
/// // Create a world allocator.
/// let mut allocator = WorldAllocator::new();
/// // Create a configuration that searches for a 3x3 oscillator with period 2 in Conway's Life.
/// let config = Config::new(Rule::Life, 3, 3, 2);
/// // Create a world from the configuration and the allocator.
/// let mut world = allocator.new_world(config).unwrap();
/// // Search for a solution.
/// world.search(None);
/// assert_eq!(world.status(), Status::Solved);
/// // Print the solution in RLE format.
/// println!("{}", world.rle(0));
/// ```
#[derive(Debug)]
pub struct World<'a> {
    /// The configuration of the world.
    pub(crate) config: Config,

    /// The rule table.
    pub(crate) rule: RuleTable,

    /// The world itself. A list of cells.
    pub(crate) cells: &'a [LifeCell<'a>],

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
    pub(crate) stack: Vec<(&'a LifeCell<'a>, Reason)>,

    /// The index of the next cell to be checked in the stack.
    ///
    /// The part of the stack starting from this index can be seen as a queue.
    pub(crate) stack_index: usize,

    /// The starting point to look for an unknown cell according to the search order.
    pub(crate) start: Option<&'a LifeCell<'a>>,

    /// Search status.
    pub(crate) status: Status,
}

impl<'a> World<'a> {
    /// Create a new world from a configuration and a world allocator.
    pub fn new(config: Config, allocator: &'a mut WorldAllocator<'a>) -> Result<Self, ConfigError> {
        let config = config.check()?;

        let rule = config.rule.table();

        // Number of cells in the world.
        let size =
            (config.width + 2 * rule.radius) * (config.height + 2 * rule.radius) * config.period;

        allocator.cells.clear();
        allocator
            .cells
            .extend((0..size).map(|_| LifeCell::default()));

        let cells = allocator.cells.as_slice();

        let mut world = Self {
            config,
            rule,
            cells,
            front_count: 0,
            stack: Vec::with_capacity(size),
            stack_index: 0,
            start: None,
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
        self.init_known();
        self.init_next();
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

                    // If dx is zero, a pattern is still valid if we reflect it horizontally.
                    // So we only need to consider the left half of the first row.

                    let w = if self.config.dx == 0 {
                        (self.config.width + 1) / 2
                    } else {
                        self.config.width
                    };

                    for x in 0..w as isize {
                        for t in 0..self.config.period as isize {
                            let cell = self.get_cell_by_coord((x, 0, t)).unwrap();
                            cell.is_front.set(true);
                            self.front_count += 1;
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

                    // If dy is zero, a pattern is still valid if we reflect it vertically.
                    // So we only need to consider the top half of the first column.

                    let h = if self.config.dy == 0 {
                        (self.config.height + 1) / 2
                    } else {
                        self.config.height
                    };

                    for y in 0..h as isize {
                        for t in 0..self.config.period as isize {
                            let cell = self.get_cell_by_coord((0, y, t)).unwrap();
                            cell.is_front.set(true);
                            self.front_count += 1;
                        }
                    }
                }
            }

            // If the search order is diagonal, the front is both the first row and the first column.
            SearchOrder::Diagonal => {
                if self.config.symmetry.is_subgroup_of(Symmetry::D2D) {
                    use_front = true;

                    let d = self.config.diagonal_width.unwrap_or(self.config.width);

                    for x in 0..d as isize {
                        for t in 0..self.config.period as isize {
                            let cell = self.get_cell_by_coord((x, 0, t)).unwrap();
                            cell.is_front.set(true);
                            self.front_count += 1;
                        }
                    }

                    // If dx equals dy, a pattern is still valid if we reflect it diagonally.
                    // So we only need to consider the first row, not the first column.

                    if self.config.dx != self.config.dy {
                        for y in 1..d as isize {
                            for t in 0..self.config.period as isize {
                                let cell = self.get_cell_by_coord((0, y, t)).unwrap();
                                cell.is_front.set(true);
                                self.front_count += 1;
                            }
                        }
                    }
                }
            }
        }

        // If `use_front` is false, the front is the whole pattern at the first generation.
        if !use_front {
            for x in 0..self.config.width as isize {
                for y in 0..self.config.height as isize {
                    let cell = self.get_cell_by_coord((x, y, 0)).unwrap();
                    cell.is_front.set(true);
                    self.front_count += 1;
                }
            }
        }
    }

    /// Set the neighborhood of each cell.
    ///
    /// Some cells may have a neighbor that is outside the world.
    /// In this case, the neighbor is set to `None`.
    fn init_neighborhood(&mut self) {
        let (w, h, p) = (
            self.config.width as isize,
            self.config.height as isize,
            self.config.period as isize,
        );
        let r = self.rule.radius as isize;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    let cell = self.get_cell_by_coord((x, y, t)).unwrap();

                    for i in 0..self.rule.neighborhood_size {
                        let (ox, oy) = self.rule.offsets[i];
                        let neighbor_coord = (x + ox, y + oy, t);
                        let neighbor = self.get_cell_by_coord(neighbor_coord);

                        cell.neighborhood[i].set(neighbor);

                        // If some neighbor is outside the world, the state of that neighbor is assumed to be dead.
                        // So we update the neighborhood descriptor of the cell here.
                        if neighbor.is_none() {
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
            self.config.width as isize,
            self.config.height as isize,
            self.config.period as isize,
        );
        let r = self.rule.radius as isize;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    let cell = self.get_cell_by_coord((x, y, t)).unwrap();

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

                    let predecessor = self.get_cell_by_coord(predecessor_coord);
                    let successor = self.get_cell_by_coord(successor_coord);

                    // If the successor is outside the world, the state of the successor is assumed to be dead.
                    // So we update the neighborhood descriptor of the cell here.
                    if successor.is_none() {
                        cell.update_successor(CellState::Dead);
                    }

                    cell.predecessor.set(predecessor);
                    cell.successor.set(successor);
                }
            }
        }
    }

    // Set the symmetry cells of each cell.
    fn init_symmetry(&mut self) {
        let (w, h, p) = (
            self.config.width as isize,
            self.config.height as isize,
            self.config.period as isize,
        );
        let r = self.rule.radius as isize;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    let cell = self.get_cell_by_coord((x, y, t)).unwrap();

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

                    symmetry_coords.sort();
                    symmetry_coords.dedup();

                    let symmetry_cells = symmetry_coords
                        .into_iter()
                        .filter_map(|coord| self.get_cell_by_coord(coord));

                    cell.symmetry.borrow_mut().extend(symmetry_cells);
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
            self.config.width as isize,
            self.config.height as isize,
            self.config.period as isize,
        );
        let r = self.rule.radius as isize;

        for x in -r..w + r {
            for y in -r..h + r {
                for t in 0..p {
                    let cell = self.get_cell_by_coord((x, y, t)).unwrap();

                    if !(0..w).contains(&x)
                        || !(0..h).contains(&y)
                        || self
                            .config
                            .diagonal_width
                            .is_some_and(|d| (x - y).abs() >= d as isize)
                        || cell.predecessor.get().is_none()
                    {
                        self.set_cell(cell, CellState::Dead, Reason::Known);
                    }
                }
            }
        }
    }

    /// For each cell, find the next cell to be searched according to the search order.
    fn init_next(&mut self) {
        match self.config.search_order.unwrap() {
            SearchOrder::RowFirst => {
                for y in (0..self.config.height as isize).rev() {
                    for x in (0..self.config.width as isize).rev() {
                        for t in (0..self.config.period as isize).rev() {
                            let cell = self.get_cell_by_coord((x, y, t)).unwrap();

                            if cell.state().is_none() {
                                cell.next.set(self.start);
                                self.start = Some(cell);
                            }
                        }
                    }
                }
            }

            SearchOrder::ColumnFirst => {
                for x in (0..self.config.width as isize).rev() {
                    for y in (0..self.config.height as isize).rev() {
                        for t in (0..self.config.period as isize).rev() {
                            let cell = self.get_cell_by_coord((x, y, t)).unwrap();

                            if cell.state().is_none() {
                                cell.next.set(self.start);
                                self.start = Some(cell);
                            }
                        }
                    }
                }
            }

            SearchOrder::Diagonal => {
                let w = self.config.width as isize;

                for a in (0..2 * w - 1).rev() {
                    for x in (0..w).rev() {
                        let y = a - x;

                        if (0..w).contains(&y)
                            && !self
                                .config
                                .diagonal_width
                                .is_some_and(|d| (x - y).abs() >= d as isize)
                        {
                            for t in (0..self.config.period as isize).rev() {
                                let cell = self.get_cell_by_coord((x, y, t)).unwrap();

                                if cell.state().is_none() {
                                    cell.next.set(self.start);
                                    self.start = Some(cell);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Get a cell by its coordinates.
    ///
    /// Return `None` if the cell is outside the world.
    fn get_cell_by_coord(&self, coord: Coord) -> Option<&'a LifeCell<'a>> {
        let (x, y, t) = coord;
        let (w, h, p) = (
            self.config.width as isize,
            self.config.height as isize,
            self.config.period as isize,
        );
        let r = self.rule.radius as isize;

        if (-r..w + r).contains(&x) && (-r..h + r).contains(&y) && (0..p).contains(&t) {
            let index = t + (x + r) * p + (y + r) * p * (w + 2 * r);
            Some(&self.cells[index as usize])
        } else {
            None
        }
    }

    /// Set the state of a cell. The cell should be unknown.
    pub(crate) fn set_cell(&mut self, cell: &'a LifeCell<'a>, state: CellState, reason: Reason) {
        debug_assert!(cell.state().is_none());
        cell.state.set(Some(state));

        // Update the neighborhood descriptor of the cell, its neighbors and predecessor.
        cell.update_current(state);

        for i in 0..self.rule.neighborhood_size {
            if let Some(neighbor) = cell.neighborhood[i].get() {
                match state {
                    CellState::Dead => neighbor.increment_dead(),
                    CellState::Alive => neighbor.increment_alive(),
                }
            }
        }

        if let Some(predecessor) = cell.predecessor.get() {
            predecessor.update_successor(state);
        }

        // If the cell is on the front, update the front count.
        if cell.is_front() && state == CellState::Dead {
            self.front_count -= 1;
        }

        // Push the cell to the stack.
        self.stack.push((cell, reason));
    }

    /// Unset the state of a cell. The cell should be known.
    pub(crate) fn unset_cell(&mut self, cell: &'a LifeCell<'a>) {
        debug_assert!(cell.state().is_some());
        let state = cell.state().unwrap();
        cell.state.set(None);

        // Update the neighborhood descriptor of the cell, its neighbors and predecessor.
        cell.update_current(state);

        for i in 0..self.rule.neighborhood_size {
            if let Some(neighbor) = cell.neighborhood[i].get() {
                match state {
                    CellState::Dead => neighbor.decrement_dead(),
                    CellState::Alive => neighbor.decrement_alive(),
                }
            }
        }

        if let Some(predecessor) = cell.predecessor.get() {
            predecessor.update_successor(state);
        }

        // If the cell is on the front, update the front count.
        if cell.is_front() && state == CellState::Dead {
            self.front_count += 1;
        }
    }

    /// Get the state of a cell by its coordinates.
    ///
    /// If the cell is outside the world, it is considered to be dead.
    ///
    /// If the cell is unknown, return `None`.
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

    /// Output a generation of the world in RLE format.
    ///
    /// - Dead cells are represented by `.`.
    /// - Alive cells are represented by `o`.
    /// - Unknown cells are represented by `?`.
    /// - Each row is terminated by `$`.
    /// - The whole pattern is terminated by `!`.
    ///
    /// If the generation is out of the range `0..period`, we will take the modulo.
    pub fn rle(&self, t: isize) -> String {
        let mut s = String::new();

        let (w, h, p) = (
            self.config.width as isize,
            self.config.height as isize,
            self.config.period as isize,
        );

        let t = t.rem_euclid(p);

        writeln!(
            s,
            "x = {}, y = {}, rule = {}",
            w,
            h,
            self.config.rule.name()
        )
        .unwrap();

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
