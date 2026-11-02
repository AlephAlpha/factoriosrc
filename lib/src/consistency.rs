//! Bounded consistency checks between overlapping neighborhood descriptors.
//!
//! A single descriptor is an n-ary transition constraint. The ordinary
//! propagation reaches a fixed point of the implications of each descriptor
//! separately, but it cannot combine the supports of two constraints when
//! neither one forces a value by itself. This module implements an exact,
//! bounded joint check for one pair of descriptors.
//!
//! The check is deliberately local and non-mutating: it enumerates the
//! assignments to the unknown cells in the union of the two scopes and
//! projects the joint solutions back onto those cells. It never changes the
//! real [`World`](crate::World), so a lookahead-style rollback is not needed.

use crate::{
    cell::LifeCell,
    rule::{CellState, Descriptor, MAX_NEIGHBORHOOD_SIZE, RuleTable},
};
use std::mem::MaybeUninit;

/// The maximal number of unknown cells in the union of two descriptor scopes.
///
/// The joint check enumerates `2^n` assignments, so the bound keeps the work
/// of one check predictable. A pair above the bound is skipped: this loses
/// pruning, never correctness. The scratch buffers keep room for the wider
/// bound used by the unit tests, but the search uses [`PAIR_UNKNOWN_LIMIT`].
pub const MAX_PAIR_UNKNOWNS: usize = 8;

/// The unknown-union limit used by the search.
pub const PAIR_UNKNOWN_LIMIT: usize = 4;

/// The maximal number of distinct cells in two descriptor scopes.
const MAX_PAIR_SCOPE: usize = 2 * (MAX_NEIGHBORHOOD_SIZE + 2);

/// The maximal number of roles that one cell can have in two descriptors.
///
/// A cell can be the current or successor cell of either descriptor, or a
/// neighbor in either descriptor: at most six roles.
const MAX_VAR_ROLES: usize = 6;

/// A role of a cell inside one of the two descriptors.
#[derive(Clone, Copy)]
struct Role {
    side: u8,
    kind: u8,
    index: u8,
}

impl Role {
    const CURRENT: u8 = 0;
    const SUCCESSOR: u8 = 1;
    const NEIGHBOR: u8 = 2;

    const fn new(side: u8, kind: u8, index: usize) -> Self {
        Self {
            side,
            kind,
            index: index as u8,
        }
    }
}

/// A cell that occurs in the union of two descriptor scopes.
#[derive(Clone, Copy)]
struct Variable {
    cell: *const LifeCell,
    roles: [Role; MAX_VAR_ROLES],
    role_len: usize,
    in_first: bool,
    in_second: bool,
}

impl Variable {
    const fn new(cell: *const LifeCell, in_first: bool, in_second: bool) -> Self {
        Self {
            cell,
            roles: [Role::new(0, Role::CURRENT, 0); MAX_VAR_ROLES],
            role_len: 0,
            in_first,
            in_second,
        }
    }

    fn add_role(&mut self, role: Role) {
        debug_assert!(self.role_len < MAX_VAR_ROLES);
        self.roles[self.role_len] = role;
        self.role_len += 1;
    }
}

/// The result of one bounded pair check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairCheck {
    /// The pair shared fewer than two unknown cells, so a fixed point of the
    /// individual descriptors cannot be strengthened by joining them.
    TooFewShared,

    /// The union of the unknown cells exceeded the configured limit.
    TooManyUnknowns,

    /// Every unknown cell still has a support for both states.
    NoNewFact,

    /// A cell has only one supported state in every joint solution.
    Deduced {
        /// The cell forced by the joint check.
        target: *const LifeCell,
        /// The only supported state.
        state: CellState,
    },

    /// The two descriptors have no joint solution.
    Conflict,
}

/// The result of a pair check together with its enumeration work.
pub struct PairCheckResult {
    /// The semantic outcome.
    pub outcome: PairCheck,

    /// The number of complete local assignments enumerated.
    pub assignments: u64,
}

/// The prefix of initialized variables used by one pair check.
///
/// Only the first `len` entries are initialized. The array is not initialized
/// up front because its full size is much larger than the two scopes that are
/// actually collected, and the check runs on a hot path.
type Variables = [MaybeUninit<Variable>; MAX_PAIR_SCOPE];

/// Find a variable in the scope list, or add it.
fn variable_index(
    variables: &mut Variables,
    len: &mut usize,
    cell: *const LifeCell,
    in_first: bool,
    in_second: bool,
) -> usize {
    for (index, slot) in variables.iter_mut().enumerate().take(*len) {
        // Safety: the prefix `0..len` is initialized.
        let variable = unsafe { slot.assume_init_mut() };
        if variable.cell == cell {
            variable.in_first |= in_first;
            variable.in_second |= in_second;
            return index;
        }
    }

    debug_assert!(*len < MAX_PAIR_SCOPE);
    let index = *len;
    variables[index].write(Variable::new(cell, in_first, in_second));
    *len += 1;
    index
}

/// Add the scope and roles of one descriptor source to the variable list.
///
/// # Safety
///
/// All cells reachable from `source` must belong to the same world and remain
/// alive for the duration of the check.
unsafe fn add_descriptor_scope(
    source: &LifeCell,
    side: u8,
    variables: &mut Variables,
    len: &mut usize,
) {
    let source_ptr = source as *const LifeCell;
    let index = variable_index(variables, len, source_ptr, side == 0, side == 1);
    // Safety: `variable_index` returned an initialized entry.
    unsafe { variables[index].assume_init_mut() }.add_role(Role::new(side, Role::CURRENT, 0));

    if let Some(successor) = unsafe { source.successor.as_ref() } {
        let successor_ptr = successor as *const LifeCell;
        let index = variable_index(variables, len, successor_ptr, side == 0, side == 1);
        unsafe { variables[index].assume_init_mut() }.add_role(Role::new(side, Role::SUCCESSOR, 0));
    }

    for i in 0..source.neighborhood_len {
        let Some(neighbor) = (unsafe { source.neighborhood[i].as_ref() }) else {
            continue;
        };
        let neighbor_ptr = neighbor as *const LifeCell;
        let index = variable_index(variables, len, neighbor_ptr, side == 0, side == 1);
        unsafe { variables[index].assume_init_mut() }.add_role(Role::new(side, Role::NEIGHBOR, i));
    }
}

/// Whether a complete descriptor is inconsistent with its transition rule.
#[inline]
fn descriptor_conflicts(rule: &RuleTable, descriptor: Descriptor) -> bool {
    use crate::rule::Implication;

    rule.implies(descriptor)
        .flags()
        .contains(Implication::Conflict)
}

/// Apply one hypothetical assignment to all roles of a variable.
#[inline]
fn apply_assignment(
    rule: &RuleTable,
    variable: Variable,
    state: CellState,
    descriptors: [&mut Descriptor; 2],
) {
    let [first, second] = descriptors;
    for &role in &variable.roles[..variable.role_len] {
        let descriptor = if role.side == 0 {
            &mut *first
        } else {
            &mut *second
        };
        match role.kind {
            Role::CURRENT => descriptor.update_current(state),
            Role::SUCCESSOR => descriptor.update_successor(state),
            Role::NEIGHBOR => rule.assume_neighbor(descriptor, role.index as usize, state),
            _ => unreachable!("invalid descriptor role"),
        }
    }
}

/// Perform an exact joint support check for two descriptors.
///
/// The known cells are already encoded in the descriptors. The function
/// enumerates all assignments of the unknown cells in the union of the two
/// scopes and returns a conflict, one forced assignment, or no new fact. A
/// pair that is too large is skipped without drawing any conclusion.
///
/// # Safety
///
/// The two source cells must belong to the same [`World`](crate::World), and
/// all pointers reachable from them must be valid for the duration of the
/// call.
pub unsafe fn check_pair(rule: &RuleTable, first: &LifeCell, second: &LifeCell) -> PairCheckResult {
    unsafe { check_pair_with_limit(rule, first, second, PAIR_UNKNOWN_LIMIT) }
}

/// The implementation of [`check_pair`] with an explicit unknown-cell limit.
///
/// The unit tests use the wider [`MAX_PAIR_UNKNOWNS`] limit to exercise joint
/// consequences that need more than four unknown cells.
///
/// # Safety
///
/// The same requirements as [`check_pair`] apply. `limit` must not exceed
/// [`MAX_PAIR_UNKNOWNS`].
unsafe fn check_pair_with_limit(
    rule: &RuleTable,
    first: &LifeCell,
    second: &LifeCell,
    limit: usize,
) -> PairCheckResult {
    debug_assert!(limit <= MAX_PAIR_UNKNOWNS);
    let mut variables: Variables = [const { MaybeUninit::uninit() }; MAX_PAIR_SCOPE];
    let mut len = 0;
    unsafe {
        add_descriptor_scope(first, 0, &mut variables, &mut len);
        add_descriptor_scope(second, 1, &mut variables, &mut len);
    }
    // Safety: the prefix `0..len` has been initialized by the scope walk.
    let variables: &[Variable] =
        unsafe { std::slice::from_raw_parts(variables.as_ptr().cast(), len) };

    let mut unknown = [0u8; MAX_PAIR_UNKNOWNS];
    let mut unknown_len = 0;
    let mut shared = 0;
    for (index, variable) in variables.iter().enumerate() {
        let state = unsafe { (*variable.cell).state() };
        if state.is_some() {
            continue;
        }
        if variable.in_first && variable.in_second {
            shared += 1;
        }
        if unknown_len == limit {
            return PairCheckResult {
                outcome: PairCheck::TooManyUnknowns,
                assignments: 0,
            };
        }
        unknown[unknown_len] = index as u8;
        unknown_len += 1;
    }

    if shared < 2 {
        return PairCheckResult {
            outcome: PairCheck::TooFewShared,
            assignments: 0,
        };
    }

    let base_first = first.descriptor();
    let base_second = second.descriptor();
    let assignments = 1u64 << unknown_len;
    let mut supports = [0u8; MAX_PAIR_UNKNOWNS];
    let mut any_solution = false;
    let mut fully_supported = 0;

    for assignment in 0..assignments {
        let mut first_descriptor = base_first;
        let mut second_descriptor = base_second;

        for (bit, &variable_index) in unknown[..unknown_len].iter().enumerate() {
            let state = if assignment & (1 << bit) == 0 {
                CellState::Dead
            } else {
                CellState::Alive
            };
            apply_assignment(
                rule,
                variables[variable_index as usize],
                state,
                [&mut first_descriptor, &mut second_descriptor],
            );
        }

        if descriptor_conflicts(rule, first_descriptor)
            || descriptor_conflicts(rule, second_descriptor)
        {
            continue;
        }

        any_solution = true;
        for (bit, support) in supports.iter_mut().enumerate().take(unknown_len) {
            if *support == 0b11 {
                continue;
            }
            let state = if assignment & (1 << bit) == 0 {
                CellState::Dead
            } else {
                CellState::Alive
            };
            *support |= 1 << state.number();
            if *support == 0b11 {
                fully_supported += 1;
            }
        }

        // Every variable already has both values in some joint solution.
        // Continuing the enumeration cannot produce a deduction or a conflict,
        // because at least one joint solution already exists.
        if fully_supported == unknown_len {
            return PairCheckResult {
                outcome: PairCheck::NoNewFact,
                assignments: assignment + 1,
            };
        }
    }

    let outcome = if !any_solution {
        PairCheck::Conflict
    } else if let Some(bit) = (0..unknown_len).find(|&bit| supports[bit].count_ones() == 1) {
        let variable = variables[unknown[bit] as usize];
        let state = if supports[bit] == 1 {
            CellState::Dead
        } else {
            CellState::Alive
        };
        PairCheck::Deduced {
            target: variable.cell,
            state,
        }
    } else {
        PairCheck::NoNewFact
    };

    PairCheckResult {
        outcome,
        assignments,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ca_rules2::parse_rule;
    use std::collections::HashMap;

    const WIDTH: i32 = 4;
    const HEIGHT: i32 = 3;

    fn index(x: i32, y: i32) -> usize {
        ((x + 1) + (y + 1) * WIDTH) as usize
    }

    /// Build the two horizontally adjacent centers of the pair fixture.
    ///
    /// The returned vector contains the 3x4 spatial block plus the two known
    /// successor cells at the end.
    type Fixture = (Vec<LifeCell>, usize, usize, HashMap<(i32, i32), usize>);

    fn fixture(
        known: &[(i32, i32)],
        center_dead: bool,
        extra: &[(i32, i32, CellState)],
    ) -> Fixture {
        let rule = RuleTable::new(&parse_rule("B3/S23").unwrap()).unwrap();
        let spatial = (WIDTH * HEIGHT) as usize;
        let mut cells = (0..spatial + 2)
            .map(|_| LifeCell::new(0))
            .collect::<Vec<_>>();
        let cells_ptr = cells.as_ptr();

        let mut coords = HashMap::new();
        for y in -1..2 {
            for x in -1..3 {
                coords.insert((x, y), index(x, y));
            }
        }

        for &(x, y) in known {
            cells[index(x, y)].state.set(Some(CellState::Alive));
        }
        if center_dead {
            cells[index(0, 0)].state.set(Some(CellState::Dead));
        }
        for &(x, y, state) in extra {
            cells[index(x, y)].state.set(Some(state));
        }
        cells[spatial].state.set(Some(CellState::Alive));
        cells[spatial + 1].state.set(Some(CellState::Alive));

        let centers = [index(0, 0), index(1, 0)];
        for (which, &center_index) in centers.iter().enumerate() {
            let successor = spatial + which;
            let current_state = cells[center_index].state();
            cells[center_index].successor = unsafe { cells_ptr.add(successor) };
            cells[center_index]
                .successor_state
                .set(Some(CellState::Alive));
            cells[center_index].update_successor(CellState::Alive);
            if let Some(state) = current_state {
                cells[center_index].update_current(state);
            }

            for (neighbor_index, &(ox, oy)) in rule.offsets.iter().enumerate() {
                let cx = if which == 0 { 0 } else { 1 };
                let coord = (cx + ox, oy);
                if let Some(&cell_index) = coords.get(&coord) {
                    let ptr = unsafe { cells_ptr.add(cell_index) };
                    cells[center_index].neighborhood[neighbor_index] = ptr;
                    if let Some(state) = cells[cell_index].state() {
                        let mut descriptor = cells[center_index].descriptor.get();
                        rule.assume_neighbor(&mut descriptor, neighbor_index, state);
                        cells[center_index].descriptor.set(descriptor);
                    }
                }
            }
            cells[center_index].neighborhood_len = rule.neighborhood_size;
        }

        (cells, centers[0], centers[1], coords)
    }

    fn no_single_implication(rule: &RuleTable, cell: &LifeCell) {
        let result = rule.implies(cell.descriptor());
        assert!(!result.flags().contains(crate::rule::Implication::Conflict));
        assert_eq!(result.forced(), 0);
        assert!(!result.flags().intersects(
            crate::rule::Implication::SuccessorAlive
                | crate::rule::Implication::SuccessorDead
                | crate::rule::Implication::CurrentAlive
                | crate::rule::Implication::CurrentDead
                | crate::rule::Implication::NeighborhoodAlive
                | crate::rule::Implication::NeighborhoodDead
        ));
    }

    #[test]
    fn test_pair_check_deduces_a_joint_consequence() {
        let rule = RuleTable::new(&parse_rule("B3/S23").unwrap()).unwrap();
        let known = [(-1, 0), (-1, 1)];
        let extra = [(2, -1, CellState::Dead), (2, 1, CellState::Dead)];
        let (cells, first, second, coords) = fixture(&known, false, &extra);

        no_single_implication(&rule, &cells[first]);
        no_single_implication(&rule, &cells[second]);

        let result = unsafe {
            check_pair_with_limit(&rule, &cells[first], &cells[second], MAX_PAIR_UNKNOWNS)
        };
        assert_eq!(result.assignments, 256);
        let PairCheck::Deduced { target, state } = result.outcome else {
            panic!("expected a joint deduction, got {:?}", result.outcome);
        };
        let target = coords
            .iter()
            .find_map(|(&coord, &index)| (std::ptr::eq(target, &cells[index])).then_some(coord))
            .expect("the target must be in the fixture scope");
        assert!(
            matches!(
                (target, state),
                ((-1, -1), CellState::Dead) | ((0 | 2, 0), CellState::Alive)
            ),
            "unexpected joint deduction: {target:?} = {state:?}"
        );
    }

    #[test]
    fn test_pair_check_detects_a_joint_conflict() {
        let rule = RuleTable::new(&parse_rule("B3/S23").unwrap()).unwrap();
        let known = [(-1, -1), (-1, 1)];
        let extra = [
            (2, -1, CellState::Dead),
            (2, 0, CellState::Dead),
            (-1, 0, CellState::Dead),
        ];
        let (cells, first, second, _) = fixture(&known, true, &extra);

        no_single_implication(&rule, &cells[first]);
        no_single_implication(&rule, &cells[second]);

        let result = unsafe {
            check_pair_with_limit(&rule, &cells[first], &cells[second], MAX_PAIR_UNKNOWNS)
        };
        assert_eq!(result.assignments, 64);
        assert_eq!(result.outcome, PairCheck::Conflict);
    }
}
