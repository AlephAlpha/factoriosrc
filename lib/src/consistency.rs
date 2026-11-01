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

/// The maximal number of unknown cells in the union of two descriptor scopes.
///
/// The joint check enumerates `2^n` assignments, so the bound keeps the work
/// of one check predictable. A pair above the bound is skipped: this loses
/// pruning, never correctness.
pub const MAX_PAIR_UNKNOWNS: usize = 8;

/// The maximal number of distinct cells in two descriptor scopes.
const MAX_PAIR_SCOPE: usize = 2 * (MAX_NEIGHBORHOOD_SIZE + 2);

/// The maximal number of roles that one cell can have in two descriptors.
///
/// A cell can be the current or successor cell of either descriptor, or a
/// neighbor in either descriptor: at most six roles.
const MAX_VAR_ROLES: usize = 6;

/// A role of a cell inside one of the two descriptors.
#[derive(Clone, Copy)]
enum Role {
    Current(u8),
    Successor(u8),
    Neighbor(u8, usize),
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
            roles: [Role::Current(0); MAX_VAR_ROLES],
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

    /// The union of the unknown cells exceeded [`MAX_PAIR_UNKNOWNS`].
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

/// Find a variable in the scope list, or add it.
fn variable_index(
    variables: &mut [Variable; MAX_PAIR_SCOPE],
    len: &mut usize,
    cell: *const LifeCell,
    in_first: bool,
    in_second: bool,
) -> usize {
    if let Some(index) = variables[..*len]
        .iter()
        .position(|variable| variable.cell == cell)
    {
        variables[index].in_first |= in_first;
        variables[index].in_second |= in_second;
        return index;
    }

    debug_assert!(*len < MAX_PAIR_SCOPE);
    let index = *len;
    variables[index] = Variable::new(cell, in_first, in_second);
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
    variables: &mut [Variable; MAX_PAIR_SCOPE],
    len: &mut usize,
) {
    let source_ptr = source as *const LifeCell;
    let index = variable_index(variables, len, source_ptr, side == 0, side == 1);
    variables[index].add_role(Role::Current(side));

    if let Some(successor) = unsafe { source.successor.as_ref() } {
        let successor_ptr = successor as *const LifeCell;
        let index = variable_index(variables, len, successor_ptr, side == 0, side == 1);
        variables[index].add_role(Role::Successor(side));
    }

    for i in 0..source.neighborhood_len {
        let Some(neighbor) = (unsafe { source.neighborhood[i].as_ref() }) else {
            continue;
        };
        let neighbor_ptr = neighbor as *const LifeCell;
        let index = variable_index(variables, len, neighbor_ptr, side == 0, side == 1);
        variables[index].add_role(Role::Neighbor(side, i));
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
        match role {
            Role::Current(0) => first.update_current(state),
            Role::Current(_) => second.update_current(state),
            Role::Successor(0) => first.update_successor(state),
            Role::Successor(_) => second.update_successor(state),
            Role::Neighbor(0, i) => rule.assume_neighbor(first, i, state),
            Role::Neighbor(_, i) => rule.assume_neighbor(second, i, state),
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
    let mut variables = [Variable::new(std::ptr::null(), false, false); MAX_PAIR_SCOPE];
    let mut len = 0;
    unsafe {
        add_descriptor_scope(first, 0, &mut variables, &mut len);
        add_descriptor_scope(second, 1, &mut variables, &mut len);
    }

    let mut unknown = [0u8; MAX_PAIR_UNKNOWNS];
    let mut unknown_len = 0;
    let mut shared = 0;
    for (index, variable) in variables[..len].iter().enumerate() {
        let state = unsafe { (*variable.cell).state() };
        if state.is_some() {
            continue;
        }
        if variable.in_first && variable.in_second {
            shared += 1;
        }
        if unknown_len == MAX_PAIR_UNKNOWNS {
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
            let state = if assignment & (1 << bit) == 0 {
                CellState::Dead
            } else {
                CellState::Alive
            };
            *support |= 1 << state.number();
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
    fn fixture(
        known: &[(i32, i32)],
        center_dead: bool,
        extra: &[(i32, i32, CellState)],
    ) -> (Vec<LifeCell>, usize, usize, HashMap<(i32, i32), usize>) {
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

        let result = unsafe { check_pair(&rule, &cells[first], &cells[second]) };
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
                ((-1, -1), CellState::Dead)
                    | ((0, 0), CellState::Alive)
                    | ((2, 0), CellState::Alive)
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

        let result = unsafe { check_pair(&rule, &cells[first], &cells[second]) };
        assert_eq!(result.assignments, 64);
        assert_eq!(result.outcome, PairCheck::Conflict);
    }
}
