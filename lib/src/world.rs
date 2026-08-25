#[cfg(feature = "serde")]
use crate::error::SerdeError;
use crate::{
    cell::{Antecedent, LifeCell, Reason},
    config::{Config, KnownCell, SearchOrder},
    error::ConfigError,
    nogood::NogoodDb,
    rule::{CellState, RuleTable},
};
use ca_symmetry::{Symmetry, Transformation};
#[cfg(feature = "documented")]
use documented::{Documented, DocumentedFields};
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;
#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize, Serializer};
use strum::Display;

/// Coordinates of a cell in the world.
///
/// The first two coordinates are the x and y coordinates, respectively.
/// The third coordinate is the generation of the cell.
pub type Coord = (i32, i32, i32);

/// Status of the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Display)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
#[cfg_attr(feature = "documented", derive(Documented, DocumentedFields))]
pub enum Status {
    /// Not started yet.
    NotStarted,
    /// Searching...
    Running,
    /// A solution was found.
    Solved,
    /// No more solutions.
    NoSolution,
}

/// A piece of metadata of a stack entry, used for conflict analysis when
/// backjumping is enabled.
///
/// When [`Config::backjump`](crate::Config::backjump) is `false` (the default),
/// no metadata is recorded, and the search behaves exactly like before.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrailMeta {
    /// The decision level of the entry: the number of decision carriers in
    /// the part of the stack up to and including this entry.
    ///
    /// A decision carrier is a guessed cell, or a guess flipped by
    /// [`backtrack`](World::backtrack) which is a new value of the same
    /// decision and therefore occupies the same level.
    pub(crate) level: u32,

    /// Whether this entry is a decision carrier.
    ///
    /// This is `true` for guessed cells and for the re-tried states of guessed
    /// cells in `backtrack`; the level counter counts these entries only, so
    /// that every level has exactly one decision carrier. This is what the
    /// conflict analysis relies on.
    pub(crate) decision: bool,

    /// The antecedent of the deduction that set the cell.
    ///
    /// This is [`None`] for [`Known`](Reason::Known),
    /// [`Guessed`](Reason::Guessed), and re-tried guessed cells.
    pub(crate) antecedent: Option<Antecedent>,
}

/// Why a conflict occurred while checking a cell.
///
/// When backjumping is enabled, a [`Rule`](Confl::Rule) or
/// [`Symmetry`](Confl::Symmetry) conflict is analyzed to backjump; a
/// [`Global`](Confl::Global) conflict is a global constraint failure and is
/// handled by chronological backtracking.
#[derive(Debug, Clone, Copy)]
pub enum Confl {
    /// The rule lookup on the neighborhood descriptor of a cell found a conflict.
    ///
    /// The cell must be in the same world as `self`.
    Rule(*const LifeCell),

    /// Two symmetry-equivalent cells have different states.
    ///
    /// The cells must be in the same world as `self`.
    Symmetry(*const LifeCell, *const LifeCell),

    /// A learned nogood is fully matched by the current partial assignment.
    ///
    /// The field is the id of the entry in the nogood database. All the cells
    /// of the nogood currently hold their recorded states, which no solution
    /// can extend. Like a rule or symmetry conflict, this is analyzed to
    /// backjump when backjumping is enabled (which it always is when the
    /// nogood database is enabled).
    Nogood(u32),

    /// A global constraint failed: the front is empty or the population is too large.
    Global,
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
/// // Print the solution of the first generation in RLE format.
/// println!("{}", world.rle(0, true));
/// ```
#[derive(Debug)]
#[cfg_attr(feature = "serde", derive(Deserialize), serde(try_from = "WorldSerde"))]
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
    pub(crate) rng: Xoshiro256PlusPlus,

    /// The number of living cells on each generation.
    pub(crate) population: Vec<usize>,

    /// The upper bound of the population.
    pub(crate) max_population: Option<usize>,

    /// The number of generations whose population is at most [`max_population`](World::max_population).
    ///
    /// This is used to ensure that the population is never too large: the search
    /// fails when no generation is at or below the upper bound, i.e. when the
    /// minimum population over all generations exceeds the bound.
    ///
    /// This is maintained incrementally in [`set_cell`](World::set_cell) and
    /// [`unset_cell`](World::unset_cell), so that the check in
    /// [`check_affected`](World::check_affected) is O(1).
    pub(crate) below_max: usize,

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

    /// The metadata of the entries in [`stack`](World::stack).
    ///
    /// This is only used when [`Config::backjump`](crate::Config::backjump) is
    /// enabled; the entries are pushed and popped in lockstep with the stack.
    /// When it is disabled, this vector is always empty.
    pub(crate) trail_meta: Vec<TrailMeta>,

    /// The current decision level: the number of decision carriers in the
    /// stack.
    ///
    /// A decision carrier is a guessed cell, or a guess flipped by
    /// [`backtrack`](World::backtrack) which is a new value of the same
    /// decision. Every level has exactly one decision carrier.
    ///
    /// This is only maintained when [`Config::backjump`](crate::Config::backjump)
    /// is enabled.
    pub(crate) current_level: u32,

    /// The decision level of each cell, indexed by the cell index in the world.
    ///
    /// This is only used when [`Config::backjump`](crate::Config::backjump) is
    /// enabled. When it is disabled, this vector is always empty.
    pub(crate) cell_level: Vec<u32>,

    /// The position of each cell in the stack, indexed by the cell index in
    /// the world; meaningless for cells that are not in the stack.
    ///
    /// This is only used when [`Config::backjump`](crate::Config::backjump) is
    /// enabled. It is used by the conflict analysis to recover the exact
    /// antecedent of a deduction: a deduction is based on the cells that were
    /// already in the stack when the deduction happened, so the antecedent is
    /// the known part of the descriptor with stack positions before the
    /// deduced cell. When it is disabled, this vector is always empty.
    pub(crate) cell_pos: Vec<u32>,

    /// The rank of each cell in the search-order chain, indexed by the cell
    /// index in the world; meaningless for cells not in the chain.
    ///
    /// This is only used when [`Config::backjump`](crate::Config::backjump) is
    /// enabled. The conflict analysis resumes the search in the chain after a
    /// backjump: since the chain order and the trail order differ, the
    /// resumption point is the smallest chain rank among the popped cells,
    /// not the deepest trail position.
    pub(crate) chain_pos: Vec<u32>,

    /// The timestamp of the last conflict analysis, used to mark the cells
    /// that have been seen by the analysis.
    ///
    /// This is only used when [`Config::backjump`](crate::Config::backjump) is
    /// enabled. When it is disabled, this vector is always empty.
    pub(crate) seen_stamp: Vec<u32>,

    /// The current [`analysis_stamp`](World::analysis_stamp).
    ///
    /// A cell is marked as seen when its stamp equals this number. The stamp
    /// is increased for each analysis, so the marks do not need to be cleared
    /// between analyses.
    pub(crate) analysis_stamp: u32,

    /// The index of the next cell to be checked in the stack.
    ///
    /// The part of the stack starting from this index can be seen as a queue.
    pub(crate) stack_index: usize,

    /// The starting point to look for an unknown cell according to the search order.
    pub(crate) start: *const LifeCell,

    /// Whether the search is currently running a lookahead probe.
    ///
    /// A probe temporarily sets the states of some cells and rolls them back
    /// immediately. This flag prevents the probe from updating the phases of
    /// the cells for phase saving, since the assignments of a probe are not
    /// real.
    pub(crate) in_probe: bool,

    /// The database of learned nogoods.
    ///
    /// This is only used when [`Config::nogood`](crate::Config::nogood) is
    /// enabled; otherwise it is a disabled database that never learns or
    /// blocks anything.
    ///
    /// The nogoods are stored by absolute cell indices, so they are only
    /// valid within this world. The database is empty in a freshly built
    /// world, which includes the worlds rebuilt by save/load (the
    /// serialization does not store the learned nogoods) and by
    /// [`increase_world_size`](World::increase_world_size).
    pub(crate) nogood_db: NogoodDb,

    /// The ids of the database entries whose firing condition must be
    /// re-evaluated, maintained by [`set_cell`](World::set_cell) and drained
    /// by the firing loop in `nogood_after_set`.
    ///
    /// This is a scratch buffer that is always empty between calls to
    /// [`set_cell`](World::set_cell).
    pub(crate) nogood_scratch: Vec<u32>,

    /// A fully matched nogood that has not been reported as a conflict yet.
    ///
    /// This is set by [`set_cell`](World::set_cell) when every literal of a
    /// stored nogood holds, and consumed at the next call of
    /// `check_affected`, which reports it as [`Confl::Nogood`].
    pub(crate) pending_nogood_confl: Option<u32>,

    /// The search status.
    pub(crate) status: Status,
}

impl Drop for World {
    fn drop(&mut self) {
        unsafe {
            let _ = Box::from_raw(self.cells_ptr);
        }
    }
}

#[cfg(feature = "serde")]
impl Serialize for World {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.to_serde().serialize(serializer)
    }
}

impl World {
    /// Create a new world from a configuration.
    pub fn new(config: Config) -> Result<Self, ConfigError> {
        let mut config = config;
        config.check()?;

        let parsed_rule = config.parse_rule()?;
        let rule_symmetry = parsed_rule.symmetry_elements();
        let rule = RuleTable::new(&parsed_rule)?;
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

        let rng = config.seed.map_or_else(
            || Xoshiro256PlusPlus::from_rng(&mut rand::rng()),
            Xoshiro256PlusPlus::seed_from_u64,
        );

        let backjump = config.backjump;
        let nogood = config.nogood;

        let mut world = Self {
            config,
            rule,
            cells_ptr,
            size,
            rng,
            population: vec![0; p as usize],
            max_population,
            // The populations of all generations are initially 0, so they are
            // all at or below the maximum.
            below_max: p as usize,
            front_count: 0,
            stack: Vec::with_capacity(size),
            trail_meta: Vec::new(),
            current_level: 0,
            cell_level: if backjump { vec![0; size] } else { Vec::new() },
            cell_pos: if backjump { vec![0; size] } else { Vec::new() },
            chain_pos: if backjump {
                vec![u32::MAX; size]
            } else {
                Vec::new()
            },
            seen_stamp: if backjump { vec![0; size] } else { Vec::new() },
            analysis_stamp: 0,
            stack_index: 0,
            start: std::ptr::null(),
            in_probe: false,
            nogood_db: if nogood {
                NogoodDb::with_default_capacity()
            } else {
                NogoodDb::new(0)
            },
            nogood_scratch: Vec::new(),
            pending_nogood_confl: None,
            status: Status::NotStarted,
        };
        world.init(&rule_symmetry)?;

        Ok(world)
    }

    /// Initialize the world.
    fn init(&mut self, rule_symmetry: &[Transformation]) -> Result<(), ConfigError> {
        self.init_front(rule_symmetry);
        self.init_neighborhood();
        self.init_predecessor_successor();
        self.init_symmetry();
        self.init_known()?;
        self.init_next();
        Ok(())
    }

    /// For each cell, check if it is on the front.
    ///
    /// See [this GitHub issue](https://github.com/AlephAlpha/rlifesrc/issues/81) for the detailed reasoning.
    ///
    /// The `rule_symmetry` argument is the symmetry group of the rule. The front
    /// optimization is only sound if the rule is invariant under the reflections
    /// used by the arguments below. See `docs/front.md` for more information.
    fn init_front(&mut self, rule_symmetry: &[Transformation]) {
        let mut use_front = false;

        // The number of generations of the front when the front is restricted
        // to the first few generations by the generation-rotation argument.
        //
        // For a rule without `B0`, an empty front on the first generation
        // implies that the whole pattern is empty, so only the first
        // generation is needed.
        //
        // For a B0 rule, the pattern is a deviation from a periodic background,
        // and rotating the pattern in time changes the phase of the background.
        // So the first `background_period` generations are needed. For a rule
        // with both `B0` and `S-max`, the background is constant, so only the
        // first generation is needed.
        let max_t = if self.rule.has_b0() {
            self.rule.background_period() as i32
        } else {
            1
        };

        if self.config.known_cells.is_empty() {
            match self.config.search_order.unwrap() {
                // If the search order is row-first, the front is the first row.
                SearchOrder::RowFirst => {
                    if self.config.symmetry.is_subgroup_of(Symmetry::D2H)
                        && self.config.transformation.is_element_of(Symmetry::D2H)
                        && self.config.diagonal_width.is_none()
                        // If `dx` is zero, the front is halved, which relies on
                        // the rule being invariant under the horizontal
                        // reflection `S2`.
                        && (self.config.dx != 0
                            || rule_symmetry.contains(&Transformation::S2))
                    {
                        use_front = true;

                        // If `dx` is zero, a pattern is still valid if we reflect it horizontally.
                        // So we only need to consider the left half of the first row.

                        let w = if self.config.dx == 0 {
                            self.config.width.div_ceil(2)
                        } else {
                            self.config.width
                        };

                        // If both `dx` and `dy` are zero, a pattern is still valid if we rotate the
                        // generations, i.e. the first generation becomes the last, the second becomes
                        // the first, and so on. So we only need to consider the first generation.

                        // If `dx` is zero, `dy` is positive, a similar argument still applies.
                        // But the front becomes the `dy-1`-th row of the first few generations.

                        if self.config.dx == 0 && self.config.dy >= 0 {
                            let y = self.config.dy.max(1) - 1;
                            for x in 0..w as i32 {
                                for t in 0..max_t {
                                    self.get_cell_by_coord_mut((x, y, t)).unwrap().is_front = true;
                                    self.front_count += 1;
                                }
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
                        && self.config.transformation.is_element_of(Symmetry::D2V)
                        && self.config.diagonal_width.is_none()
                        // If `dy` is zero, the front is halved, which relies on
                        // the rule being invariant under the vertical
                        // reflection `S0`.
                        && (self.config.dy != 0
                            || rule_symmetry.contains(&Transformation::S0))
                    {
                        use_front = true;

                        // If `dy` is zero, a pattern is still valid if we reflect it vertically.
                        // So we only need to consider the top half of the first column.

                        let h = if self.config.dy == 0 {
                            self.config.height.div_ceil(2)
                        } else {
                            self.config.height
                        };

                        // If both `dx` and `dy` are zero, a pattern is still valid if we rotate the
                        // generations, i.e. the first generation becomes the last, the second becomes
                        // the first, and so on. So we only need to consider the first generation.

                        // If `dy` is zero, `dx` is positive, a similar argument still applies.
                        // But the front becomes the `dx-1`-th column of the first few generations.

                        if self.config.dx >= 0 && self.config.dy == 0 {
                            let x = self.config.dx.max(1) - 1;
                            for y in 0..h as i32 {
                                for t in 0..max_t {
                                    self.get_cell_by_coord_mut((x, y, t)).unwrap().is_front = true;
                                    self.front_count += 1;
                                }
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
                    if self.config.symmetry.is_subgroup_of(Symmetry::D2D)
                        && self.config.transformation.is_element_of(Symmetry::D2D)
                        // If `dx` equals `dy`, the front is only the first row,
                        // which relies on the rule being invariant under the
                        // diagonal reflection `S1`.
                        && (self.config.dx != self.config.dy
                            || self.config.dx < 0
                            || rule_symmetry.contains(&Transformation::S1))
                    {
                        use_front = true;

                        let d = self.config.diagonal_width.unwrap_or(self.config.width);

                        // If `dx` equals `dy`, a pattern is still valid if we reflect it diagonally.
                        // So we only need to consider the first row, not the first column.

                        // If both `dx` and `dy` are zero, a pattern is still valid if we rotate the
                        // generations, i.e. the first generation becomes the last, the second becomes
                        // the first, and so on. So we only need to consider the first generation.

                        // If `dx` equals `dy` and is positive, a similar argument still applies.
                        // But the front becomes the `dy-1`-th row of the first few generations.

                        if self.config.dx == self.config.dy && self.config.dx >= 0 {
                            let y = self.config.dy.max(1) - 1;
                            for x in 0..d as i32 {
                                for t in 0..max_t {
                                    self.get_cell_by_coord_mut((x, y, t)).unwrap().is_front = true;
                                    self.front_count += 1;
                                }
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
                                        self.get_cell_by_coord_mut((0, y, t)).unwrap().is_front =
                                            true;
                                        self.front_count += 1;
                                    }
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

                        let cell = self.get_cell_by_coord_ptr((x, y, t));

                        unsafe {
                            (*cell).neighborhood[i] = neighbor;

                            // If some neighbor is outside the world, the state of that neighbor is assumed to be in the
                            // background state. So we update the neighborhood descriptor of the cell here.
                            if neighbor.is_null() {
                                self.rule.set_outside_neighbor(&*cell, i);
                            }
                        }
                    }

                    // For a totalistic rule, the state update of a neighbor does
                    // not depend on the position of the neighbor in the array,
                    // so the non-null neighbors can be packed to the front,
                    // and the update loops can avoid the null checks.
                    let is_totalistic = self.rule.is_totalistic();
                    let neighborhood_size = self.rule.neighborhood_size;

                    let cell = self.get_cell_by_coord_mut((x, y, t)).unwrap();
                    if is_totalistic {
                        let mut len = 0;
                        for i in 0..neighborhood_size {
                            let neighbor = cell.neighborhood[i];
                            if !neighbor.is_null() {
                                cell.neighborhood[len] = neighbor;
                                len += 1;
                            }
                        }
                        cell.neighborhood_len = len;
                    } else {
                        cell.neighborhood_len = neighborhood_size;
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
                    let predecessor_coord = self.canonicalize_coord((x, y, t - 1));

                    let successor_coord = self.canonicalize_coord((x, y, t + 1));

                    let predecessor = self.get_cell_by_coord_ptr(predecessor_coord);
                    let successor = self.get_cell_by_coord_ptr(successor_coord);

                    // If the successor is outside the world, the state of the successor is assumed to be the
                    // background state. So we update the neighborhood descriptor of the cell here.
                    let successor_background = self.rule.background(successor_coord.2);

                    let cell = self.get_cell_by_coord_mut((x, y, t)).unwrap();

                    if successor.is_null() {
                        cell.update_successor(successor_background);
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

                    for transformation in symmetry.transformations() {
                        let (x1, y1) = transformation.apply_with_size(x, y, w, h);
                        symmetry_coords.push((x1, y1, t));
                    }

                    symmetry_coords.sort_unstable();
                    symmetry_coords.dedup();

                    let symmetry_cells = symmetry_coords
                        .into_iter()
                        .map(|coord| self.get_cell_by_coord_ptr(coord).cast_const())
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
                // If the pattern is symmetric under the reflection `S2`, the
                // cells in the left half of the world are determined by the
                // cells in the right half, so only the right half is searched.
                let x_start = if Transformation::S2.is_element_of(self.config.symmetry) {
                    self.config.width as i32 / 2
                } else {
                    0
                };
                for y in (0..self.config.height as i32).rev() {
                    for x in (x_start..self.config.width as i32).rev() {
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
                // If the pattern is symmetric under the reflection `S0`, the
                // cells in the upper half of the world are determined by the
                // cells in the lower half, so only the lower half is searched.
                let y_start = if Transformation::S0.is_element_of(self.config.symmetry) {
                    self.config.height as i32 / 2
                } else {
                    0
                };
                for x in (0..self.config.width as i32).rev() {
                    for y in (y_start..self.config.height as i32).rev() {
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
                            && self
                                .config
                                .diagonal_width
                                .is_none_or(|d| (x - y).abs() < d as i32)
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

        // The ranks of the cells in the chain, in chain order. The chain is
        // built by pushing cells to the front, so the order of the chain is
        // the reverse of the order of the build loops.
        if self.config.backjump {
            let mut rank = 0u32;
            let mut cell = self.start;
            while !cell.is_null() {
                unsafe {
                    let index = self.cell_index(cell);
                    self.chain_pos[index] = rank;
                    cell = (*cell).next;
                }
                rank += 1;
            }
        }
    }

    /// Set the state of known cells.
    ///
    /// The cells outside the bounding box are known to be in the background state.
    ///
    /// If the predecessor of a cell is outside the world, that cell is also known to be
    /// in the background state.
    ///
    /// User-specified known cells are applied after the implicit background cells.
    fn init_known(&mut self) -> Result<(), ConfigError> {
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
                            self.set_known_cell(&*cell, self.rule.background(t))?;
                        }
                    }
                }
            }
        }

        for known_cell in self.config.known_cells.clone() {
            let cell = self.get_cell_by_coord_ptr((
                known_cell.x as i32,
                known_cell.y as i32,
                known_cell.t as i32,
            ));

            debug_assert!(!cell.is_null());

            unsafe {
                self.set_known_cell(&*cell, known_cell.state)?;
            }
        }

        Ok(())
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
            unsafe { (self.cells_ptr.cast::<LifeCell>()).offset(index as isize) }
        } else {
            std::ptr::null_mut()
        }
    }

    /// The index of a cell in the world.
    ///
    /// # Safety
    ///
    /// The pointer must be valid and point to a cell in the world.
    /// Otherwise the behavior is undefined.
    pub(crate) const unsafe fn cell_index(&self, cell: *const LifeCell) -> usize {
        unsafe {
            let offset = cell.offset_from(self.cells_ptr as *const LifeCell);
            offset as usize
        }
    }

    /// Get a raw pointer to a cell by its index in the world.
    ///
    /// This is the inverse of [`cell_index`](World::cell_index).
    ///
    /// # Safety
    ///
    /// The index must be in the range `0..size`.
    /// Otherwise the behavior is undefined.
    #[inline]
    pub(crate) const unsafe fn cell_by_index(&self, index: u32) -> *const LifeCell {
        unsafe { (self.cells_ptr as *const LifeCell).add(index as usize) }
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
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    unsafe fn set_known_cell(
        &mut self,
        cell: &LifeCell,
        state: CellState,
    ) -> Result<(), ConfigError> {
        match cell.state() {
            None => {
                unsafe {
                    self.set_cell(cell, state, Reason::Known, None, false);
                }
                Ok(())
            }
            Some(existing_state) if existing_state == state => Ok(()),
            Some(_) => Err(ConfigError::ConflictingKnownCells),
        }
    }

    /// Set the state of a cell. The cell should be unknown.
    ///
    /// The `antecedent` argument records the cells that caused the set: the
    /// source cell of a rule-based deduction or the source cell of a symmetry
    /// deduction, or none for a guess or a known cell. It is only used when
    /// [`Config::backjump`](crate::Config::backjump) is enabled, in which case
    /// it is stored on the stack for the conflict analysis.
    ///
    /// The `decision` argument is whether the set starts a new decision level
    /// (in the sense of the conflict analysis): this is `true` for a guessed
    /// cell and for the re-tried value of a guessed cell in
    /// [`backtrack`](World::backtrack), which is the same decision with the
    /// opposite state. It is only used when backjumping is enabled.
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    pub(crate) unsafe fn set_cell(
        &mut self,
        cell: &LifeCell,
        state: CellState,
        reason: Reason,
        antecedent: Option<Antecedent>,
        decision: bool,
    ) {
        debug_assert!(cell.state().is_none());
        cell.state.set(Some(state));

        // Update the neighborhood descriptor of the cell, its neighbors and predecessor.
        cell.update_current(state);

        self.rule.set_neighbors(cell, state);

        if let Some(predecessor) = unsafe { cell.predecessor.as_ref() } {
            predecessor.update_successor(state);
        }

        // If the cell is on the front, update the front count.
        // A front cell is empty when its state equals the background state.
        if cell.is_front && state == self.rule.background(cell.generation) {
            self.front_count -= 1;
        }

        // If the cell is part of the pattern, update the population.
        //
        // For a B0S8 rule, the background is alive, so the pattern consists of
        // the dead cells. For other rules, it consists of the alive cells.
        let pattern_state = if self.rule.has_b0() && self.rule.has_s_max() {
            CellState::Dead
        } else {
            CellState::Alive
        };
        if state == pattern_state {
            let t = cell.generation as usize;
            self.population[t] += 1;
            // If the population of this generation just exceeded the maximum,
            // one fewer generation is at or below the maximum.
            if let Some(max_population) = self.max_population
                && self.population[t] == max_population + 1
            {
                debug_assert!(self.below_max > 0);
                self.below_max -= 1;
            }
        }

        // Track the reason on the cell itself.
        cell.reason.set(Some(reason));

        // If phase saving is enabled, remember the state of the cell.
        // The remembered state is not cleared when the cell is unset.
        // The assignments of a lookahead probe are not real, so they do not
        // update the phase.
        if self.config.phase_saving && !self.in_probe {
            cell.phase.set(Some(state));
        }

        // Push the cell to the stack.
        self.stack.push((cell, reason));

        // Record the metadata for the conflict analysis.
        //
        // A decision carrier (a guess or the re-tried value of a guess)
        // starts a new decision level; a deduced cell inherits the current
        // one.
        if self.config.backjump {
            if decision {
                self.current_level += 1;
            }
            self.trail_meta.push(TrailMeta {
                level: self.current_level,
                decision,
                antecedent,
            });
            let index = unsafe { self.cell_index(cell) };
            self.cell_level[index] = self.current_level;
            self.cell_pos[index] = (self.stack.len() - 1) as u32;
            debug_assert_eq!(self.stack.len(), self.trail_meta.len());
        } else {
            debug_assert!(self.trail_meta.is_empty());
        }

        // Update the matched-literal counters of the nogood database, and
        // fire the nogoods that became one literal short of a full match.
        //
        // The assignments of a lookahead probe are temporary, so the probes
        // do not touch the counters; this keeps them in sync with the real
        // trail.
        if self.config.nogood && !self.in_probe {
            let index = unsafe { self.cell_index(cell) } as u32;
            unsafe { self.nogood_after_set(index, state) };
        }
    }

    /// Update the backjump metadata when a cell is popped from the stack.
    ///
    /// This must be called for every cell popped (and then unset) from the
    /// stack. It keeps the metadata in lockstep with the stack, and adjusts
    /// [`current_level`](World::current_level) when the popped entry is a
    /// decision carrier.
    ///
    /// If backjumping is disabled, this is a no-op.
    pub(crate) fn pop_meta(&mut self) {
        if self.config.backjump {
            let meta = self.trail_meta.pop().unwrap();
            debug_assert_eq!(meta.level, self.current_level);
            if meta.decision {
                self.current_level -= 1;
            }
            debug_assert_eq!(self.stack.len(), self.trail_meta.len());
        }
    }

    /// Unset the state of a cell. The cell should be known.
    ///
    /// # Safety
    ///
    /// The cell must be in the same world as `self`.
    /// Otherwise the behavior is undefined.
    pub(crate) unsafe fn unset_cell(&mut self, cell: &LifeCell) {
        debug_assert!(cell.state().is_some());
        let state = cell.state().unwrap();
        cell.state.set(None);

        // Update the neighborhood descriptor of the cell, its neighbors and predecessor.
        cell.update_current(state);

        self.rule.unset_neighbors(cell, state);

        if let Some(predecessor) = unsafe { cell.predecessor.as_ref() } {
            predecessor.update_successor(state);
        }

        // If the cell is on the front, update the front count.
        // A front cell is empty when its state equals the background state.
        if cell.is_front && state == self.rule.background(cell.generation) {
            self.front_count += 1;
        }

        // If the cell is part of the pattern, update the population.
        //
        // For a B0S8 rule, the background is alive, so the pattern consists of
        // the dead cells. For other rules, it consists of the alive cells.
        let pattern_state = if self.rule.has_b0() && self.rule.has_s_max() {
            CellState::Dead
        } else {
            CellState::Alive
        };
        if state == pattern_state {
            let t = cell.generation as usize;
            // If the population of this generation just fell back to the maximum,
            // one more generation is at or below the maximum.
            if let Some(max_population) = self.max_population
                && self.population[t] == max_population + 1
            {
                self.below_max += 1;
            }
            self.population[t] -= 1;
        }

        // Clear the reason on the cell.
        cell.reason.set(None);

        // Update the matched-literal counters of the nogood database. As in
        // [`set_cell`](World::set_cell), lookahead probes do not touch them.
        if self.config.nogood && !self.in_probe {
            let index = unsafe { self.cell_index(cell) } as u32;
            self.nogood_db.on_unset(index, state);
        }
    }

    /// Canonicalize the coordinates of a cell.
    ///
    /// If its generation is out of the range `0..period`, we will move it to
    /// the range by taking the modulo of the generation, and apply the translation
    /// and transformation to the x and y coordinates.
    #[inline]
    pub const fn canonicalize_coord(&self, coord: Coord) -> Coord {
        let (mut x, mut y, mut t) = coord;
        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );
        let transformation = self.config.transformation;
        let dx = self.config.dx;
        let dy = self.config.dy;

        while t < 0 {
            t += p;
            (x, y) = transformation.inverse().apply_with_size(x, y, w, h);
            x -= dx;
            y -= dy;
        }

        while t >= p {
            t -= p;
            x += dx;
            y += dy;
            (x, y) = transformation.apply_with_size(x, y, w, h);
        }

        (x, y, t)
    }

    /// Get the state of a cell by its coordinates.
    ///
    /// The coordinates are [canonicalized](World::canonicalize_coord) before getting the state.
    ///
    /// If the cell is outside the world after canonicalization, it is considered to be in the
    /// background state.
    ///
    /// If the cell is unknown, return [`None`].
    #[inline]
    pub fn get_cell_state(&self, coord: Coord) -> Option<CellState> {
        let coord = self.canonicalize_coord(coord);
        self.get_cell_by_coord(coord)
            .map_or_else(|| Some(self.rule.background(coord.2)), LifeCell::state)
    }

    /// Get the reason why a cell is set to its current state.
    ///
    /// The coordinates are [canonicalized](World::canonicalize_coord) before getting the reason.
    ///
    /// If the cell is outside the world after canonicalization, returns [`Some(Reason::Known)`]
    /// (cells outside the world are implicitly in the background state from the configuration).
    ///
    /// If the cell is unknown, return [`None`].
    #[inline]
    pub fn get_cell_reason(&self, coord: Coord) -> Option<Reason> {
        self.get_cell_by_coord(self.canonicalize_coord(coord))
            .map_or(Some(Reason::Known), |cell| cell.reason.get())
    }

    /// Get the known cells from the configuration.
    #[inline]
    pub fn known_cells(&self) -> &[KnownCell] {
        &self.config.known_cells
    }

    /// Get the number of cells that have been set (checked) during the search.
    ///
    /// This is the size of the internal stack, reflecting how many state assignments
    /// have been made. It is a proxy for search progress.
    #[inline]
    pub const fn cells_checked(&self) -> usize {
        self.stack.len()
    }

    /// Get the statistics of the nogood database.
    ///
    /// Return [`None`] if [`Config::nogood`](Config::nogood) is disabled.
    #[inline]
    pub fn nogood_stats(&self) -> Option<&crate::nogood::NogoodStats> {
        self.config.nogood.then(|| self.nogood_db.stats())
    }

    /// Get the search status.
    #[inline]
    pub const fn status(&self) -> Status {
        self.status
    }

    /// Whether the rule is a Generations rule.
    ///
    /// A Generations rule has at least 3 states. A cell in a dying state
    /// transitions to the next state in each generation, regardless of the rule.
    #[inline]
    pub const fn is_generations_rule(&self) -> bool {
        self.rule.is_generations()
    }

    /// Get the number of states of the rule.
    #[inline]
    pub const fn num_states(&self) -> u8 {
        self.rule.num_states()
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
    /// - Dead cells are represented by `b` if `compact` is `true`, or `.` if `compact` is `false`.
    /// - Alive cells are represented by `o`, or `A` for a Generations rule.
    /// - Dying cells are represented by `B`, `C`, ..., in the order of their state numbers.
    /// - Unknown cells are represented by `?`.
    /// - Each row is terminated by `$`.
    /// - The whole pattern is terminated by `!`.
    ///
    /// If `compact` is `true`, the output will be run-length encoded. In fact, this is
    /// what RLE stands for. For example, the [glider](https://www.conwaylife.com/wiki/Glider)
    /// in Conway's Life is represented as:
    ///
    /// ```plaintext
    /// x = 3, y = 3, rule = B3/S23
    /// bo$2bo$3o!
    /// ```
    ///
    /// If `compact` is `false`, the output will be in a more human-readable format. For example,
    /// the same glider is represented as:
    ///
    /// ```plaintext
    /// x = 3, y = 3, rule = B3/S23
    /// .o.$
    /// ..o$
    /// ooo!
    /// ```
    ///
    /// If the generation is out of the range `0..period`, we will take the modulo.
    pub fn rle(&self, t: i32, compact: bool) -> String {
        let (w, h, p) = (
            self.config.width as i32,
            self.config.height as i32,
            self.config.period as i32,
        );

        let t = t.rem_euclid(p);

        let header = format!("x = {w}, y = {h}, rule = {}\n", self.config.rule_str);

        let mut body = String::new();

        let dead_char = if compact { 'b' } else { '.' };

        for y in 0..h {
            for x in 0..w {
                let c = match self.get_cell_state((x, y, t)) {
                    Some(CellState::Dead) => dead_char,
                    Some(CellState::Alive) => {
                        if self.rule.is_generations() {
                            'A'
                        } else {
                            'o'
                        }
                    }
                    Some(CellState::Dying(i)) => {
                        char::from_u32(b'A' as u32 + i as u32 - 1).unwrap()
                    }
                    None => '?',
                };

                body.push(c);
            }

            // Trim the trailing dead cells if `compact` is true.
            if compact {
                let trim_len = body.trim_end_matches(dead_char).len();
                body.truncate(trim_len);
            }

            if y < h - 1 {
                // Ignore the leading `$` if `compact` is true.
                if !compact || !body.is_empty() {
                    body.push('$');
                }
            } else {
                // Trim the trailing `$` if `compact` is true.
                if compact {
                    let trim_len = body.trim_end_matches('$').len();
                    body.truncate(trim_len);
                }

                body.push('!');
            }
            if !compact {
                body.push('\n');
            }
        }

        if compact {
            // Run-length encode the body.

            let mut result = header;
            let mut line = String::new();
            let mut count = 0;
            let mut chars = body.chars().peekable();

            while let Some(c) = chars.next() {
                count += 1;

                if chars.peek() != Some(&c) {
                    let mut run = if count > 1 {
                        count.to_string()
                    } else {
                        String::new()
                    };
                    run.push(c);

                    // A line in the output should not be longer than 70 characters.
                    if line.len() + run.len() > 70 {
                        result.push_str(&line);
                        result.push('\n');
                        line = run;
                    } else {
                        line.push_str(&run);
                    }

                    count = 0;
                }
            }

            result.push_str(&line);

            result
        } else {
            header + &body
        }
    }

    /// Increment the world size.
    ///
    /// If the diagonal width exists and is smaller than the width, it will be increased by 1.
    /// Otherwise, if the height is greater than the width, the width will increased by 1.
    /// Otherwise, the height will increased by 1.
    ///
    /// If the configuration requires a square world, both the width and the height will be
    /// increased by 1.
    ///
    /// The world will be replaced by a new world with the new size. The current search status
    /// will be lost.
    ///
    /// The learned nogoods are lost as well: they are stored by absolute cell
    /// indices and may rely on facts specific to the old size (e.g. the
    /// background state forced on the cells outside the search range).
    pub fn increase_world_size(&mut self) {
        let mut config = self.config.clone();
        let w = config.width;
        let h = config.height;
        let d = config.diagonal_width;
        if d.is_some_and(|d| d < w) {
            config.diagonal_width = Some(d.unwrap() + 1);
        } else if config.requires_square() {
            config.width = w + 1;
            config.height = h + 1;
        } else if h > w {
            config.width = w + 1;
        } else {
            config.height = h + 1;
        }

        *self = Self::new(config).unwrap();
    }
}

/// A serializable and deserializable version of a [`World`].
#[cfg(feature = "serde")]
#[derive(Debug, Clone, Serialize, Deserialize)]
struct WorldSerde {
    /// The configuration of the world.
    config: Config,

    /// A random number generator for guessing the state of an unknown cell.
    rng: Xoshiro256PlusPlus,

    /// The number of living cells on each generation.
    population: Vec<usize>,

    /// The upper bound of the population.
    max_population: Option<usize>,

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
    front_count: usize,

    /// A stack for backtracking.
    ///
    /// It records the cells that have been set to a state, the state,
    /// and the reason why they are set to that state.
    ///
    /// The cells are represented by their indices in the world.
    stack: Vec<(usize, CellState, Reason)>,

    /// The index of the next cell to be checked in the stack.
    ///
    /// The part of the stack starting from this index can be seen as a queue.
    stack_index: usize,

    /// The starting point to look for an unknown cell according to the search order.
    start: Option<usize>,

    /// The search status.
    status: Status,
}

#[cfg(feature = "serde")]
impl From<World> for WorldSerde {
    fn from(world: World) -> Self {
        world.to_serde()
    }
}

#[cfg(feature = "serde")]
impl TryFrom<WorldSerde> for World {
    type Error = SerdeError;

    fn try_from(serde: WorldSerde) -> Result<Self, Self::Error> {
        Self::try_from_serde(serde)
    }
}

#[cfg(feature = "serde")]
impl World {
    /// Convert a raw pointer to a [`LifeCell`] to an index in the world.
    ///
    /// # Safety
    ///
    /// The raw pointer must be valid and point to a cell in the world.
    /// Otherwise the behavior is undefined.
    const unsafe fn cell_to_index(&self, cell: *const LifeCell) -> usize {
        unsafe {
            let offset = cell.offset_from(self.cells_ptr as *const LifeCell);
            offset as usize
        }
    }

    /// Convert an index in the world to a raw pointer to a [`LifeCell`].
    ///
    /// # Safety
    ///
    /// The index must be in the range `0..size`.
    /// Otherwise the behavior is undefined.
    const unsafe fn index_to_cell(&self, index: usize) -> *const LifeCell {
        unsafe { (self.cells_ptr as *const LifeCell).add(index) }
    }

    /// Convert a [`World`] to a [`WorldSerde`].
    fn to_serde(&self) -> WorldSerde {
        let stack = self
            .stack
            .iter()
            .map(|&(cell, reason)| unsafe {
                let index = self.cell_to_index(cell);
                let state = (*cell).state().unwrap();
                (index, state, reason)
            })
            .collect();

        let start = if self.start.is_null() {
            None
        } else {
            unsafe { Some(self.cell_to_index(self.start)) }
        };

        WorldSerde {
            config: self.config.clone(),
            rng: self.rng.clone(),
            population: self.population.clone(),
            max_population: self.max_population,
            front_count: self.front_count,
            stack,
            stack_index: self.stack_index,
            start,
            status: self.status,
        }
    }

    /// Convert a [`WorldSerde`] to a [`World`].
    ///
    /// Some basic checks are performed, but it is still possible that the world is invalid.
    fn try_from_serde(serde: WorldSerde) -> Result<Self, SerdeError> {
        let mut world = Self::new(serde.config)?;

        // Set the state of the cells according to the stack.
        unsafe {
            let mut all_known = true;

            for (index, state, reason) in serde.stack {
                if index >= world.size {
                    return Err(SerdeError::OutOfBounds);
                }

                // All `Known` reasons should be at the beginning of the stack.
                if reason == Reason::Known {
                    if !all_known {
                        return Err(SerdeError::InvalidStack);
                    }
                } else {
                    all_known = false;
                }

                let cell = world.index_to_cell(index);

                // Skip the cell if it already has a state.
                if (*cell).state().is_none() {
                    world.set_cell(&*cell, state, reason, None, false);
                }
            }
        }

        if let Some(start) = serde.start {
            if start >= world.size {
                return Err(SerdeError::OutOfBounds);
            }
            unsafe {
                world.start = world.index_to_cell(start);
            }
        } else {
            world.start = std::ptr::null();
        }

        world.rng = serde.rng;
        world.population = serde.population;
        world.max_population = serde.max_population;
        world.below_max =
            world
                .max_population
                .map_or(world.config.period as usize, |max_population| {
                    world
                        .population
                        .iter()
                        .filter(|&&pop| pop <= max_population)
                        .count()
                });
        world.front_count = serde.front_count;
        world.stack_index = serde.stack_index;
        world.status = serde.status;

        Ok(world)
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{KnownCell, NewState, Transformation};

    fn front_coords(world: &World) -> Vec<Coord> {
        let mut coords = Vec::new();

        for t in 0..world.config().period as i32 {
            for y in 0..world.config().height as i32 {
                for x in 0..world.config().width as i32 {
                    if world.get_cell_by_coord((x, y, t)).unwrap().is_front {
                        coords.push((x, y, t));
                    }
                }
            }
        }

        coords
    }

    /// Read the states of a generation from the world.
    fn read_generation(world: &World, t: i32) -> Vec<Vec<CellState>> {
        let (w, h) = (
            world.config().width as usize,
            world.config().height as usize,
        );
        (0..h)
            .map(|y| {
                (0..w)
                    .map(|x| world.get_cell_state((x as i32, y as i32, t)).unwrap())
                    .collect()
            })
            .collect()
    }

    /// The background state of the cells outside the search range on the given generation.
    ///
    /// For a rule without `B0`, it is always dead. For a rule with `B0` but
    /// not `S-max`, it cycles through all the states of the rule. For a rule
    /// with both `B0` and `S-max`, it is always alive.
    fn background(rule: &ca_rules2::Rule, t: i32) -> CellState {
        let has_b0 = rule.contains_b0();
        let has_s_max = rule.survival.contains(&rule.neighborhood.max_condition());
        if !has_b0 {
            CellState::Dead
        } else if has_s_max {
            CellState::Alive
        } else {
            CellState::from_number((t.rem_euclid(rule.states as i32)) as u8)
        }
    }

    /// Simulate one generation of a rule.
    ///
    /// This is an independent implementation used to verify the search results.
    /// Cells outside the given grid are assumed to be in the given background state.
    ///
    /// For a totalistic rule, the state of a cell is determined by the number
    /// of living neighbors. For a non-totalistic rule, it is determined by the
    /// arrangement of the living neighbors.
    ///
    /// For a Generations rule, a dying cell transitions to the next state,
    /// regardless of the rule.
    fn simulate(
        rule: &ca_rules2::Rule,
        states: &[Vec<CellState>],
        background: CellState,
    ) -> Vec<Vec<CellState>> {
        let (w, h) = (states[0].len() as i32, states.len() as i32);
        let coords = rule.neighbor_coords();
        let mut result = vec![vec![CellState::Dead; w as usize]; h as usize];

        for y in 0..h {
            for x in 0..w {
                let center = states[y as usize][x as usize];

                // A dying cell always transitions to the next state.
                if let CellState::Dying(i) = center {
                    result[y as usize][x as usize] =
                        CellState::from_number(((i as u16 + 1) % rule.states as u16) as u8);
                    continue;
                }

                let mut mask = 0u64;
                for (i, (ox, oy)) in coords.iter().enumerate() {
                    let (nx, ny) = (x + ox, y + oy);
                    let alive = if (0..w).contains(&nx) && (0..h).contains(&ny) {
                        states[ny as usize][nx as usize] == CellState::Alive
                    } else {
                        // The cell is outside the grid, so it is in the background state.
                        // Dying background cells count as dead for the underlying rule.
                        background == CellState::Alive
                    };
                    if alive {
                        mask |= 1 << i;
                    }
                }
                let conditions = match center {
                    CellState::Dead => &rule.birth,
                    CellState::Alive => &rule.survival,
                    CellState::Dying(_) => unreachable!(),
                };
                let alive = if rule.is_totalistic() {
                    conditions.contains(&(mask.count_ones() as u64))
                } else {
                    conditions.contains(&mask)
                };
                result[y as usize][x as usize] = match (center, alive) {
                    // A dead cell becomes alive if it is born, and stays dead otherwise.
                    (CellState::Dead, alive) => {
                        if alive {
                            CellState::Alive
                        } else {
                            CellState::Dead
                        }
                    }
                    // An alive cell stays alive if it survives. Otherwise it enters the
                    // first dying state, or dies if the rule has only 2 states.
                    (CellState::Alive, alive) => {
                        if alive {
                            CellState::Alive
                        } else if rule.states > 2 {
                            CellState::from_number(2)
                        } else {
                            CellState::Dead
                        }
                    }
                    (CellState::Dying(_), _) => unreachable!(),
                };
            }
        }

        result
    }

    /// Search for a pattern and verify that it is a solution by simulating the
    /// rule independently for a whole period.
    fn search_and_verify(rule_str: &str, width: u32, height: u32, period: u32) {
        search_and_verify_with_translations(rule_str, width, height, period, 0, 0);
    }

    /// Like [`search_and_verify`], but with translations.
    fn search_and_verify_with_translations(
        rule_str: &str,
        width: u32,
        height: u32,
        period: u32,
        dx: i32,
        dy: i32,
    ) {
        let mut world =
            World::new(Config::new(rule_str, width, height, period).with_translations(dx, dy))
                .unwrap();
        world.search(None);
        assert_eq!(
            world.status(),
            Status::Solved,
            "no solution found for {rule_str}"
        );

        let rule = ca_rules2::parse_rule(rule_str).unwrap();
        let p = world.config().period as i32;
        for t in 0..p {
            let generation = read_generation(&world, t);
            let next = read_generation(&world, t + 1);
            let background = background(&rule, t);
            assert_eq!(
                next,
                simulate(&rule, &generation, background),
                "generation {t} of {rule_str}"
            );
        }
    }

    #[test]
    fn test_search_int_rule() {
        // A diagonal pair is a still life in the rule B2a/S12 on the Moore neighborhood.
        search_and_verify("B2a/S12", 3, 3, 1);
    }

    #[test]
    fn test_search_int_hex_rule() {
        // There is a period-2 oscillator in the rule B2o/S23oH on the hexagonal neighborhood.
        search_and_verify("B2o/S23oH", 3, 3, 2);
    }

    #[test]
    fn test_search_hex_totalistic_rule() {
        // There is a period-2 oscillator in the rule B2/S34H on the hexagonal neighborhood.
        search_and_verify("B2/S34H", 3, 2, 2);
    }

    #[test]
    fn test_search_hex_totalistic_still_life() {
        // There are still lifes in the rule B2/S1234H on the hexagonal neighborhood.
        search_and_verify("B2/S1234H", 3, 3, 1);
    }

    #[test]
    fn test_search_generations_rule() {
        // A diamond of four cells is a still life in the Generations rule B3/S23/4.
        search_and_verify("B3/S23/4", 4, 4, 1);
    }

    #[test]
    fn test_search_generations_non_totalistic_rule() {
        // A diagonal pair is a still life in the Generations rule B2a/S12/3.
        search_and_verify("B2a/S12/3", 3, 3, 1);
    }

    #[test]
    fn test_search_generations_with_dying_cells() {
        // The solution of this search contains dying cells.
        search_and_verify("3457/357/5", 8, 5, 5);
    }

    #[test]
    fn test_search_generations_spaceship() {
        // A glider-like spaceship in the Generations rule B3/S23/4.
        // The solution contains dying cells as well.
        search_and_verify_with_translations("B3/S23/4", 9, 9, 4, 1, 1);
    }

    #[test]
    fn test_search_b0_rule() {
        // A single cell is a period-2 oscillator on the alternating background
        // of the B0 rule B026/S1.
        search_and_verify("B026/S1", 4, 4, 2);
    }

    #[test]
    fn test_search_b0_generations_rule() {
        // There are period-3 oscillators on the period-3 background of the
        // Generations B0 rule B0/S23/3.
        search_and_verify("B0/S23/3", 7, 7, 3);
    }

    #[test]
    fn test_search_b0_s8_rule() {
        // In a B0S8 rule, the background is alive, and the pattern consists of
        // the dead cells. A 2x2 dead block is a still life in Day & Night.
        search_and_verify("B3678/S34678", 4, 4, 1);
    }

    #[test]
    fn test_search_b0_s8_generations_rule() {
        // A 2x2 dead block is also a still life in the Generations version of
        // Day & Night.
        search_and_verify("B3678/S34678/3", 4, 4, 1);
    }

    #[test]
    fn test_generations_rle_characters() {
        let mut world = World::new(Config::new("B3/S23/3", 4, 4, 1)).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);

        // The alive cell should be represented by `A` in the RLE.
        let rle = world.rle(0, false);
        assert!(rle.contains('A'), "RLE should contain 'A': {rle}");
        assert!(!rle.contains('o'), "RLE should not contain 'o': {rle}");
    }

    #[test]
    fn test_miri_generations() {
        let config = Config::new("B2a/S12/3", 3, 3, 1);
        let mut world = World::new(config).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    fn test_miri_int() {
        let config = Config::new("B2a/S12", 3, 3, 1);
        let mut world = World::new(config).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_miri_serde_int() {
        let config = Config::new("B2a/S12", 3, 3, 1);
        let mut world = World::new(config).unwrap();

        let serde = world.to_serde();
        let mut world2 = World::try_from(serde).unwrap();

        world.search(None);
        world2.search(None);
        assert_eq!(world.status(), world2.status());
        assert_eq!(world.rle(0, true), world2.rle(0, true));
    }

    #[test]
    fn test_world_new_assigns_automatic_search_order() {
        let world = World::new(Config::new("B3/S23", 2, 5, 1)).unwrap();
        assert_eq!(world.config().search_order, Some(SearchOrder::RowFirst));
    }

    #[test]
    fn test_init_front_uses_half_column_for_vertical_reflection() {
        let world = World::new(
            Config::new("B3/S23", 4, 5, 1)
                .with_symmetry(Symmetry::D2V)
                .with_search_order(SearchOrder::ColumnFirst),
        )
        .unwrap();

        assert_eq!(front_coords(&world), vec![(0, 0, 0), (0, 1, 0), (0, 2, 0)]);
        assert_eq!(world.front_count, 3);
    }

    #[test]
    fn test_init_front_can_shift_row_front_into_generation_zero() {
        let world = World::new(
            Config::new("B3/S23", 4, 5, 2)
                .with_translations(0, 2)
                .with_search_order(SearchOrder::RowFirst),
        )
        .unwrap();

        assert_eq!(front_coords(&world), vec![(0, 1, 0), (1, 1, 0)]);
    }

    #[test]
    fn test_init_front_covers_background_period_for_b0() {
        // For a B0 rule, the front covers the first `background_period`
        // generations instead of just the first generation, because rotating
        // the pattern in time changes the phase of the background.
        let world = World::new(
            Config::new("B026/S1", 4, 5, 4)
                .with_translations(0, 1)
                .with_search_order(SearchOrder::RowFirst),
        )
        .unwrap();

        assert_eq!(
            front_coords(&world),
            vec![(0, 0, 0), (1, 0, 0), (0, 0, 1), (1, 0, 1),]
        );
        assert_eq!(world.front_count, 4);

        // A B0S8 rule has a constant background, so the front covers only the
        // first generation.
        let world = World::new(
            Config::new("B3678/S34678", 4, 5, 4)
                .with_translations(0, 1)
                .with_search_order(SearchOrder::RowFirst),
        )
        .unwrap();

        assert_eq!(front_coords(&world), vec![(0, 0, 0), (1, 0, 0)]);
    }

    #[test]
    fn test_init_front_uses_first_row_for_diagonal_search() {
        let world =
            World::new(Config::new("B3/S23", 4, 4, 1).with_search_order(SearchOrder::Diagonal))
                .unwrap();

        assert_eq!(
            front_coords(&world),
            vec![(0, 0, 0), (1, 0, 0), (2, 0, 0), (3, 0, 0)]
        );
        assert_eq!(world.front_count, 4);
    }

    #[test]
    fn test_init_front_falls_back_to_whole_first_generation() {
        let world = World::new(
            Config::new("B3/S23", 3, 3, 1)
                .with_transformation(Transformation::R1)
                .with_search_order(SearchOrder::RowFirst),
        )
        .unwrap();

        assert_eq!(
            front_coords(&world),
            vec![
                (0, 0, 0),
                (1, 0, 0),
                (2, 0, 0),
                (0, 1, 0),
                (1, 1, 0),
                (2, 1, 0),
                (0, 2, 0),
                (1, 2, 0),
                (2, 2, 0),
            ]
        );
        assert_eq!(world.front_count, 9);
    }

    #[test]
    fn test_init_front_falls_back_to_whole_first_generation_for_hex() {
        // A hexagonal rule is not invariant under the horizontal reflection `S2`,
        // so the halved row-first front cannot be used. The front falls back to
        // the whole first generation.
        let world =
            World::new(Config::new("B2/S34H", 4, 5, 1).with_search_order(SearchOrder::RowFirst))
                .unwrap();

        assert_eq!(front_coords(&world).len(), 20);
        assert!(front_coords(&world).iter().all(|&(_, _, t)| t == 0));
        assert_eq!(world.front_count, 20);
    }

    #[test]
    fn test_hex_rule_symmetry() {
        // A hexagonal rule is invariant only under `R0`, `R2`, `S1`, and `S3`.
        assert!(World::new(Config::new("B2/S34H", 4, 4, 1).with_symmetry(Symmetry::D2D),).is_ok());
        assert!(matches!(
            World::new(Config::new("B2/S34H", 4, 4, 1).with_symmetry(Symmetry::D2H)),
            Err(ConfigError::SymmetryIncompatibleWithRule)
        ));
        assert!(matches!(
            World::new(Config::new("B2/S34H", 4, 4, 1).with_transformation(Transformation::S0),),
            Err(ConfigError::TransformationIncompatibleWithRule)
        ));
    }

    #[test]
    fn test_init_front_falls_back_to_whole_first_generation_when_known_cells_are_present() {
        let world = World::new(
            Config::new("B3/S23", 4, 5, 2)
                .with_symmetry(Symmetry::D2V)
                .with_search_order(SearchOrder::ColumnFirst)
                .with_known_cell(KnownCell::new(1, 1, 1, CellState::Alive)),
        )
        .unwrap();

        let front = front_coords(&world);

        assert_eq!(front.len(), 20);
        assert!(front.iter().all(|&(_, _, t)| t == 0));
        assert!(front.contains(&(3, 4, 0)));
        assert_eq!(world.front_count, 20);
    }

    #[test]
    fn test_front_count_tracks_unknown_or_alive_front_cells() {
        let mut world = World::new(
            Config::new("B3/S23", 4, 5, 1)
                .with_symmetry(Symmetry::D2V)
                .with_search_order(SearchOrder::ColumnFirst),
        )
        .unwrap();

        let initial = world.front_count;
        let alive_front_cell = world.get_cell_by_coord_ptr((0, 0, 0));
        let dead_front_cell = world.get_cell_by_coord_ptr((0, 1, 0));

        unsafe {
            world.set_cell(
                &*alive_front_cell,
                CellState::Alive,
                Reason::Guessed,
                None,
                false,
            );
        }
        assert_eq!(world.front_count, initial);

        unsafe {
            world.set_cell(
                &*dead_front_cell,
                CellState::Dead,
                Reason::Guessed,
                None,
                false,
            );
        }
        assert_eq!(world.front_count, initial - 1);

        unsafe {
            world.unset_cell(&*dead_front_cell);
        }
        assert_eq!(world.front_count, initial);
    }

    #[test]
    fn test_known_cells_are_excluded_from_search_order() {
        let world = World::new(
            Config::new("B3/S23", 1, 1, 1).with_known_cell(KnownCell::new(
                0,
                0,
                0,
                CellState::Alive,
            )),
        )
        .unwrap();

        assert!(world.start.is_null());
        assert_eq!(world.get_cell_state((0, 0, 0)), Some(CellState::Alive));
    }

    #[test]
    fn test_world_new_rejects_known_cells_conflicting_with_implicit_dead() {
        assert!(matches!(
            World::new(
                Config::new("B3/S23", 2, 2, 1)
                    .with_translations(2, 0)
                    .with_known_cell(KnownCell::new(0, 0, 0, CellState::Alive)),
            ),
            Err(ConfigError::ConflictingKnownCells)
        ));
    }

    #[test]
    fn test_below_max_invariant_b0() {
        // The `below_max` invariant should also hold for B0 rules. The
        // population of a B0 rule counts the alive cells, which on the
        // alternating background is the number of pattern cells on the even
        // generations.
        let mut world = World::new(Config::new("B026/S1", 6, 6, 2).with_max_population(3)).unwrap();

        for _ in 0..100 {
            world.search(Some(1000));

            let below_max =
                world
                    .max_population
                    .map_or(world.config.period as usize, |max_population| {
                        world
                            .population
                            .iter()
                            .filter(|&&pop| pop <= max_population)
                            .count()
                    });
            assert_eq!(world.below_max, below_max);

            if world.status() != Status::Running {
                break;
            }
        }

        // A single cell is a period-2 oscillator with population 1, so a
        // solution should be found even with the bound of 3.
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    fn test_below_max_invariant_b0_s8() {
        // In a B0S8 rule, the background is alive, so the population counts
        // the dead cells instead.
        let mut world =
            World::new(Config::new("B3678/S34678", 4, 4, 1).with_max_population(12)).unwrap();

        for _ in 0..100 {
            world.search(Some(1000));

            let below_max =
                world
                    .max_population
                    .map_or(world.config.period as usize, |max_population| {
                        world
                            .population
                            .iter()
                            .filter(|&&pop| pop <= max_population)
                            .count()
                    });
            assert_eq!(world.below_max, below_max);

            if world.status() != Status::Running {
                break;
            }
        }

        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    fn test_search_rejects_known_cells_conflicting_with_symmetry() {
        let mut world = World::new(
            Config::new("B3/S23", 2, 1, 1)
                .with_symmetry(Symmetry::D2H)
                .with_search_order(SearchOrder::RowFirst)
                .with_known_cells([
                    KnownCell::new(0, 0, 0, CellState::Alive),
                    KnownCell::new(1, 0, 0, CellState::Dead),
                ]),
        )
        .unwrap();

        world.search(None);
        assert_eq!(world.status(), Status::NoSolution);
    }

    /// Test with Miri to see if there is any undefined behavior.
    #[test]
    fn test_miri() {
        let config = Config::new("B3/S23", 3, 3, 2);
        let mut world = World::new(config).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    fn test_miri_b0() {
        let config = Config::new("B026/S1", 3, 3, 2);
        let mut world = World::new(config).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    fn test_miri_b0_generations() {
        let config = Config::new("B0/S23/3", 5, 5, 3);
        let mut world = World::new(config).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    fn test_miri_nogood() {
        // Exercise the nogood database under Miri: learning from conflicts
        // and blocking guesses.
        let config = Config::new("B3/S23", 3, 3, 2).with_nogood();
        let mut world = World::new(config).unwrap();
        world.search(Some(2000));
        assert_eq!(world.status(), Status::Solved);

        // A contradictory configuration triggers many conflict analyses.
        let config = Config::new("B3/S23", 4, 4, 2)
            .with_max_population(1)
            .with_nogood();
        let mut world = World::new(config).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::NoSolution);
        assert!(world.nogood_db.stats().learned > 0);
    }

    #[test]
    fn test_below_max_invariant() {
        // The `below_max` counter should always be the number of generations
        // whose population is at most `max_population`.
        let mut world = World::new(Config::new("B3/S23", 6, 6, 2).with_max_population(8)).unwrap();

        for _ in 0..100 {
            world.search(Some(1000));

            let below_max =
                world
                    .max_population
                    .map_or(world.config.period as usize, |max_population| {
                        world
                            .population
                            .iter()
                            .filter(|&&pop| pop <= max_population)
                            .count()
                    });
            assert_eq!(world.below_max, below_max);

            if world.status() != Status::Running {
                break;
            }
        }
    }

    #[test]
    fn test_below_max_invariant_with_reduce() {
        // When reducing the maximum population after each solution, the
        // invariant should hold between the searches, and the search should
        // eventually finish with no solution.
        let mut world =
            World::new(Config::new("B3/S23", 5, 5, 1).with_reduce_max_population()).unwrap();

        while world.search(None) == Status::Solved {
            let below_max =
                world
                    .max_population
                    .map_or(world.config.period as usize, |max_population| {
                        world
                            .population
                            .iter()
                            .filter(|&&pop| pop <= max_population)
                            .count()
                    });
            assert_eq!(world.below_max, below_max);
        }
        assert_eq!(world.status(), Status::NoSolution);
    }

    #[test]
    fn test_max_population_prunes_the_search() {
        // In B3/S23, a single cell dies out, so there is no oscillator with a
        // population of at most 1.
        let mut world = World::new(Config::new("B3/S23", 4, 4, 2).with_max_population(1)).unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::NoSolution);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_miri_serde() {
        let config = Config::new("B3/S23", 3, 3, 2);
        let mut world = World::new(config).unwrap();

        let serde = world.to_serde();
        let mut world2 = World::try_from(serde).unwrap();

        world.search(None);
        world2.search(None);
        assert_eq!(world.status(), world2.status());
        assert_eq!(world.rle(0, true), world2.rle(0, true));
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_known_cells_round_trip_through_serde() {
        let world = World::new(Config::new("B3/S23", 2, 1, 1).with_known_cells([
            KnownCell::new(0, 0, 0, CellState::Alive),
            KnownCell::new(1, 0, 0, CellState::Dead),
        ]))
        .unwrap();

        let world2 = World::try_from(world.to_serde()).unwrap();

        assert_eq!(world.config().known_cells, world2.config().known_cells);
        assert_eq!(
            world.get_cell_state((0, 0, 0)),
            world2.get_cell_state((0, 0, 0))
        );
        assert_eq!(
            world.get_cell_state((1, 0, 0)),
            world2.get_cell_state((1, 0, 0))
        );
    }

    /// Count the number of solutions of a configuration.
    fn count_solutions(config: &Config) -> usize {
        let mut world = World::new(config.clone()).unwrap();
        let mut count = 0;
        while world.search(None) == Status::Solved {
            count += 1;
        }
        count
    }

    #[test]
    fn test_phase_saving_finds_solution() {
        // With phase saving enabled, the search should still find solutions
        // for 2-state and Generations rules.
        for config in [
            Config::new("B3/S23", 3, 3, 2).with_phase_saving(),
            Config::new("B2a/S12", 3, 3, 1).with_phase_saving(),
            Config::new("B3/S23/4", 4, 4, 1).with_phase_saving(),
            Config::new("B2o/S23oH", 3, 3, 2).with_phase_saving(),
        ] {
            let mut world = World::new(config).unwrap();
            world.search(None);
            assert_eq!(world.status(), Status::Solved);
        }
    }

    #[test]
    fn test_phase_saving_enumerates_same_solutions() {
        // Phase saving changes the search order, but not the set of solutions.
        for config in [
            Config::new("B3/S23", 3, 3, 2),
            Config::new("B3/S23/4", 3, 3, 1),
            Config::new("B3/S23", 2, 2, 1),
        ] {
            assert_eq!(
                count_solutions(&config),
                count_solutions(&config.clone().with_phase_saving()),
                "phase saving changes the solution count for {config:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_phase_saving_round_trip_through_serde() {
        // The phase saving option should survive a save/load round trip.
        // The phases of the cells in the stack are restored when the stack is
        // replayed, but the phases of cells that were unset are lost. This
        // only affects the heuristic, not the correctness of the search.
        let mut world = World::new(Config::new("B3/S23", 3, 3, 2).with_phase_saving()).unwrap();
        world.search(Some(1000));

        let mut world2 = World::try_from(world.to_serde()).unwrap();
        assert!(world2.config().phase_saving);

        world.search(None);
        world2.search(None);
        assert_eq!(world.status(), world2.status());
    }

    #[test]
    fn test_lookahead_finds_solution() {
        // With lookahead enabled, the search should still find solutions for
        // 2-state rules. Generations rules are rejected by `Config::check`
        // when lookahead is enabled.
        for config in [
            Config::new("B3/S23", 3, 3, 2).with_lookahead(),
            Config::new("B2a/S12", 3, 3, 1).with_lookahead(),
            Config::new("B2o/S23oH", 3, 3, 2).with_lookahead(),
        ] {
            let mut world = World::new(config).unwrap();
            world.search(None);
            assert_eq!(world.status(), Status::Solved);
        }
    }

    #[test]
    fn test_lookahead_enumerates_same_solutions() {
        // Lookahead changes the states that are guessed first, but not the
        // set of solutions. Generations rules are rejected by
        // `Config::check` when lookahead is enabled.
        for config in [
            Config::new("B3/S23", 3, 3, 2),
            Config::new("B3/S23", 2, 2, 1),
            Config::new("B2o/S23oH", 3, 3, 2),
        ] {
            assert_eq!(
                count_solutions(&config),
                count_solutions(&config.clone().with_lookahead()),
                "lookahead changes the solution count for {config:?}"
            );
        }
    }

    #[test]
    fn test_lookahead_with_max_population() {
        // The population check must still work with lookahead enabled.
        let mut world = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_max_population(1)
                .with_lookahead(),
        )
        .unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::NoSolution);

        let mut world = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_max_population(8)
                .with_lookahead(),
        )
        .unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_lookahead_round_trip_through_serde() {
        // The lookahead option should survive a save/load round trip.
        let mut world = World::new(Config::new("B3/S23", 3, 3, 2).with_lookahead()).unwrap();
        world.search(Some(1000));

        let mut world2 = World::try_from(world.to_serde()).unwrap();
        assert!(world2.config().lookahead);

        world.search(None);
        world2.search(None);
        assert_eq!(world.status(), world2.status());
    }

    #[test]
    fn test_backjump_rejects_generations() {
        // Backjumping is restricted to rules with 2 states, because the
        // implication graph of a Generations deduction is asymmetric.
        assert!(matches!(
            World::new(Config::new("3457/357/5", 3, 3, 1).with_backjump()),
            Err(ConfigError::BackjumpUnsupported)
        ));
        assert!(World::new(Config::new("B3/S23", 3, 3, 2).with_backjump()).is_ok());
    }

    #[test]
    fn test_lookahead_rejects_generations() {
        // Lookahead is restricted to rules with 2 states, because it probes
        // the two possible states of a cell, which has no analogue for the
        // dying states of a Generations rule.
        assert!(matches!(
            World::new(Config::new("3457/357/5", 3, 3, 1).with_lookahead()),
            Err(ConfigError::LookaheadUnsupported)
        ));
        assert!(World::new(Config::new("B3/S23", 3, 3, 2).with_lookahead()).is_ok());
    }

    #[test]
    fn test_backjump_finds_solution() {
        // With backjumping enabled, the search should still find solutions
        // for 2-state rules.
        for config in [
            Config::new("B3/S23", 3, 3, 2).with_backjump(),
            Config::new("B2a/S12", 3, 3, 1).with_backjump(),
            Config::new("B2o/S23oH", 3, 3, 2).with_backjump(),
            Config::new("B026/S1", 4, 4, 2).with_backjump(),
        ] {
            let mut world = World::new(config).unwrap();
            world.search(None);
            assert_eq!(world.status(), Status::Solved);
        }
    }

    #[test]
    fn test_backjump_enumerates_same_solutions() {
        // Backjumping rewrites the backtracking structure, but not the set of
        // solutions. The search may find the same solution multiple times
        // (even without backjumping, a pattern and its generation rotation are
        // both reported), so the sets of unique solutions are compared.
        for config in [
            Config::new("B3/S23", 3, 3, 2),
            Config::new("B3/S23", 2, 2, 1),
            Config::new("B3/S23", 4, 4, 2),
            Config::new("B2o/S23oH", 3, 3, 2),
            Config::new("R3,C2,S2,B3,N+", 3, 3, 1)
                .with_symmetry(Symmetry::D2H)
                .with_transformation(Transformation::S0),
        ] {
            assert_eq!(
                solution_set(&config),
                solution_set(&config.clone().with_backjump()),
                "backjump changes the solution set for {config:?}"
            );
        }
    }

    #[test]
    fn test_backjump_with_max_population() {
        // The population check must still work with backjumping enabled.
        let mut world = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_max_population(1)
                .with_backjump(),
        )
        .unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::NoSolution);

        let mut world = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_max_population(8)
                .with_backjump(),
        )
        .unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_backjump_round_trip_through_serde() {
        // The backjump option should survive a save/load round trip.
        let mut world = World::new(Config::new("B3/S23", 3, 3, 2).with_backjump()).unwrap();
        world.search(Some(1000));

        let mut world2 = World::try_from(world.to_serde()).unwrap();
        assert!(world2.config().backjump);

        world.search(None);
        world2.search(None);
        assert_eq!(world.status(), world2.status());
    }

    #[test]
    fn test_backjump_metadata_is_recorded() {
        // The metadata must be recorded in lockstep with the stack: the level
        // of each entry is the number of decision carriers at or before it
        // (a guess, or the re-tried value of a guess), and deduced cells
        // remember their antecedents.
        let mut world = World::new(Config::new("B3/S23", 3, 3, 2).with_backjump()).unwrap();
        world.search(Some(1000));

        assert_eq!(world.trail_meta.len(), world.stack.len());

        let mut decisions = 0;
        let mut deduced_with_antecedent = 0;
        for (i, meta) in world.trail_meta.iter().enumerate() {
            let expected_level = world.trail_meta[..=i]
                .iter()
                .filter(|meta| meta.decision)
                .count() as u32;
            assert_eq!(meta.level, expected_level);
            if meta.decision {
                decisions += 1;
            }
            if meta.antecedent.is_some() {
                deduced_with_antecedent += 1;
            }
        }

        assert_eq!(world.current_level as usize, decisions);

        // Every level above the root has exactly one decision carrier. This
        // invariant is what the conflict analysis relies on.
        for level in 1..=world.current_level {
            assert_eq!(
                world
                    .trail_meta
                    .iter()
                    .filter(|meta| meta.decision && meta.level == level)
                    .count(),
                1,
                "level {level} does not have exactly one decision carrier"
            );
        }

        assert!(
            deduced_with_antecedent > 0,
            "no deduction was recorded with an antecedent"
        );
    }

    /// The set of unique solutions of a configuration.
    ///
    /// The search itself may find the same solution multiple times (e.g. a
    /// solution and its generation rotation), so the sets of unique solutions
    /// are compared instead of the raw solution counts.
    fn solution_set(config: &Config) -> std::collections::BTreeSet<String> {
        let mut world = World::new(config.clone()).unwrap();
        let mut set = std::collections::BTreeSet::new();
        while world.search(None) == Status::Solved {
            set.insert(world.rle(0, true));
        }
        set
    }

    #[test]
    fn test_backjump_deeper_enumerates_same_solutions() {
        // Deepen the search a bit so that the conflict analysis is exercised
        // more heavily.
        for config in [
            Config::new("B3/S23", 4, 4, 2),
            Config::new("B3/S23", 5, 5, 2),
            Config::new("B2o/S23oH", 4, 4, 2),
        ] {
            assert_eq!(
                solution_set(&config),
                solution_set(&config.clone().with_backjump()),
                "backjump changes the solution set for {config:?}"
            );
        }
    }

    #[test]
    fn test_backjump_with_reduce_max_population() {
        // The reduced population searches must still work with backjumping
        // enabled. The improving solution sequence may contain duplicates
        // (the search can re-find a solution), so the sets of solutions found
        // under decreasing population bounds are compared.
        let mut reference =
            World::new(Config::new("B3/S23", 4, 4, 2).with_reduce_max_population()).unwrap();
        let mut backjump = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_reduce_max_population()
                .with_backjump(),
        )
        .unwrap();

        let mut reference_solutions = std::collections::BTreeSet::new();
        while reference.search(None) == Status::Solved {
            reference_solutions.insert(reference.rle(0, true));
        }
        let mut backjump_solutions = std::collections::BTreeSet::new();
        while backjump.search(None) == Status::Solved {
            backjump_solutions.insert(backjump.rle(0, true));
        }

        assert_eq!(reference_solutions, backjump_solutions);
    }

    #[test]
    fn test_backjump_with_lookahead() {
        // The two experimental features must be able to be enabled together.
        for config in [
            Config::new("B3/S23", 3, 3, 2)
                .with_lookahead()
                .with_backjump(),
            Config::new("B2a/S12", 3, 3, 1)
                .with_lookahead()
                .with_backjump(),
        ] {
            let mut world = World::new(config).unwrap();
            world.search(None);
            assert_eq!(world.status(), Status::Solved);
        }
    }

    #[test]
    fn test_nogood_rejects_generations() {
        // The nogood database builds on backjumping, so it is restricted to
        // rules with 2 states as well.
        assert!(matches!(
            World::new(Config::new("3457/357/5", 3, 3, 1).with_nogood()),
            Err(ConfigError::NogoodUnsupported)
        ));
        assert!(World::new(Config::new("B3/S23", 3, 3, 2).with_nogood()).is_ok());
    }

    #[test]
    fn test_nogood_implies_backjump() {
        // Enabling the nogood database enables the conflict analysis.
        let mut config = Config::new("B3/S23", 3, 3, 2).with_nogood();
        config.check().unwrap();
        assert!(config.backjump);

        let world = World::new(config.clone()).unwrap();
        assert!(world.config().backjump);
    }

    #[test]
    fn test_nogood_finds_solution() {
        // With the nogood database enabled, the search should still find
        // solutions for 2-state rules, including B0 rules.
        for config in [
            Config::new("B3/S23", 3, 3, 2).with_nogood(),
            Config::new("B2a/S12", 3, 3, 1).with_nogood(),
            Config::new("B2o/S23oH", 3, 3, 2).with_nogood(),
            Config::new("B026/S1", 4, 4, 2).with_nogood(),
            Config::new("B3678/S34678", 4, 4, 1).with_nogood(),
            Config::new("R3,C2,S2,B3,N+", 3, 3, 1).with_nogood(),
        ] {
            let mut world = World::new(config).unwrap();
            world.search(None);
            assert_eq!(world.status(), Status::Solved);
        }
    }

    #[test]
    fn test_nogood_learns_and_blocks() {
        // A conflict-free path can find the first solution without learning
        // anything, so the database is exercised by enumerating all of the
        // solutions: the backtracking phases produce conflicts to learn
        // from, and the re-entered subtrees get blocked by the learned
        // nogoods.
        let mut world = World::new(Config::new("B3/S23", 4, 4, 2).with_nogood()).unwrap();
        while world.search(None) == Status::Solved {}

        let stats = world.nogood_stats().expect("nogood is enabled");
        assert!(stats.learned > 0, "no nogoods were learned");
        assert!(stats.fired > 0, "no nogood ever fired during propagation");
    }

    #[test]
    fn test_nogood_enumerates_same_solutions() {
        // Learning changes the traversal order, but not the set of
        // solutions.
        for config in [
            Config::new("B3/S23", 3, 3, 2),
            Config::new("B3/S23", 2, 2, 1),
            Config::new("B3/S23", 4, 4, 2),
            Config::new("B3/S23", 5, 5, 2),
            Config::new("B2a/S12", 3, 3, 1),
            Config::new("B2o/S23oH", 4, 4, 2),
            Config::new("B026/S1", 3, 3, 2),
            Config::new("B3678/S34678", 4, 4, 1),
            Config::new("R3,C2,S2,B3,N+", 3, 3, 1)
                .with_symmetry(Symmetry::D2H)
                .with_transformation(Transformation::S0),
            Config::new("B3/S23", 3, 3, 1).with_transformation(Transformation::R1),
            Config::new("B3/S23", 3, 3, 2).with_symmetry(Symmetry::D2V),
        ] {
            assert_eq!(
                solution_set(&config),
                solution_set(&config.clone().with_nogood()),
                "nogoods change the solution set for {config:?}"
            );
        }
    }

    #[test]
    fn test_nogood_with_max_population() {
        // The population check must still work with the nogood database.
        let mut world = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_max_population(1)
                .with_nogood(),
        )
        .unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::NoSolution);

        let mut world = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_max_population(8)
                .with_nogood(),
        )
        .unwrap();
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }

    #[test]
    fn test_nogood_with_reduce_max_population() {
        // The reduced population searches must still work with the nogood
        // database. The nogoods are derived from rule constraints only, so
        // they stay valid under decreasing population bounds.
        let mut reference =
            World::new(Config::new("B3/S23", 4, 4, 2).with_reduce_max_population()).unwrap();
        let mut nogood = World::new(
            Config::new("B3/S23", 4, 4, 2)
                .with_reduce_max_population()
                .with_nogood(),
        )
        .unwrap();

        let mut reference_solutions = std::collections::BTreeSet::new();
        while reference.search(None) == Status::Solved {
            reference_solutions.insert(reference.rle(0, true));
        }
        let mut nogood_solutions = std::collections::BTreeSet::new();
        while nogood.search(None) == Status::Solved {
            nogood_solutions.insert(nogood.rle(0, true));
        }

        assert_eq!(reference_solutions, nogood_solutions);
    }

    #[test]
    fn test_nogood_with_lookahead() {
        // The experimental features must be able to be enabled together.
        // Each combination must enumerate the same solutions as its own
        // baseline configuration.
        for config in [
            Config::new("B3/S23", 3, 3, 2),
            Config::new("B2a/S12", 3, 3, 1),
            Config::new("B3/S23", 4, 4, 2),
        ] {
            let combined = config
                .clone()
                .with_lookahead()
                .with_phase_saving()
                .with_nogood();
            assert_eq!(
                solution_set(&config),
                solution_set(&combined),
                "the combination changes the solution set for {config:?}"
            );
        }
    }

    #[test]
    fn test_nogood_with_backjump_and_all_features() {
        // Everything enabled together must still enumerate the same
        // solutions as the default search.
        for config in [
            Config::new("B3/S23", 4, 4, 2).with_backjump().with_nogood(),
            Config::new("B3/S23", 4, 4, 2)
                .with_backjump()
                .with_nogood()
                .with_lookahead(),
            Config::new("B3/S23", 4, 4, 2)
                .with_backjump()
                .with_nogood()
                .with_phase_saving()
                .with_new_state(NewState::Alive),
        ] {
            assert_eq!(
                solution_set(&Config::new("B3/S23", 4, 4, 2)),
                solution_set(&config),
                "the combination changes the solution set for {config:?}"
            );
        }
    }

    #[test]
    #[cfg(feature = "serde")]
    fn test_nogood_round_trip_through_serde() {
        // The nogood option should survive a save/load round trip. The
        // learned nogoods themselves are not serialized: the loaded world
        // starts with an empty database, which only affects performance.
        let mut world = World::new(Config::new("B3/S23", 3, 3, 2).with_nogood()).unwrap();
        world.search(Some(1000));
        // Make sure something has been learned before the round trip.
        while world.search(None) == Status::Solved {}
        assert!(!world.nogood_db.is_empty(), "nothing was learned");

        let mut world2 = World::try_from(world.to_serde()).unwrap();
        assert!(world2.config().nogood);
        assert!(world2.config().backjump);
        assert!(world2.nogood_db.is_empty());

        world.search(None);
        world2.search(None);
        assert_eq!(world.status(), world2.status());
    }

    #[test]
    fn test_nogood_increase_world_size_drops_database() {
        // The nogoods are stored by absolute cell indices and are only valid
        // within one world, so growing the world drops them.
        let mut world = World::new(Config::new("B3/S23", 3, 3, 2).with_nogood()).unwrap();
        world.search(Some(1000));
        // Make sure something has been learned before growing the world.
        while world.search(None) == Status::Solved {}
        assert!(!world.nogood_db.is_empty());

        world.increase_world_size();
        assert!(world.nogood_db.is_empty());
        assert_eq!(
            (world.config().width, world.config().height),
            (3, 4),
            "a square world grows in height"
        );

        // The grown search must still be correct.
        world.search(None);
        assert_eq!(world.status(), Status::Solved);
    }
}
