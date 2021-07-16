/*
ENSnano, a 3d graphical application for DNA nanostructures.
    Copyright (C) 2021  Nicolas Levy <nicolaspierrelevy@gmail.com> and Nicolas Schabanel <nicolas.schabanel@ens-lyon.fr>

    This program is free software: you can redistribute it and/or modify
    it under the terms of the GNU General Public License as published by
    the Free Software Foundation, either version 3 of the License, or
    (at your option) any later version.

    This program is distributed in the hope that it will be useful,
    but WITHOUT ANY WARRANTY; without even the implied warranty of
    MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
    GNU General Public License for more details.

    You should have received a copy of the GNU General Public License
    along with this program.  If not, see <https://www.gnu.org/licenses/>.
*/

use super::AddressPointer;
use ensnano_design::{Design, Parameters};
use ensnano_interactor::{operation::Operation, DesignOperation, SimulationState, StrandBuilder};

mod presenter;
use presenter::{update_presenter, Presenter};
pub(super) mod controller;
use controller::Controller;
pub use controller::{
    CopyOperation, InteractorNotification, PastingStatus, ShiftOptimizationResult,
    ShiftOptimizerReader,
};

pub(super) use controller::ErrOperation;
use controller::OkOperation;

mod file_parsing;
pub use file_parsing::ParseDesignError;

mod grid_data;

/// The `DesignInteractor` handles all read/write operations on the design. It is a stateful struct
/// so it is meant to be unexpansive to clone.
#[derive(Clone, Default)]
pub struct DesignInteractor {
    /// The current design
    design: AddressPointer<Design>,
    /// The structure that handles "read" operations. The graphic components of EnsNano access the
    /// presenter via a trait that defines each components needs.
    presenter: AddressPointer<Presenter>,
    /// The structure that handles "write" operations.
    controller: AddressPointer<Controller>,
}

impl DesignInteractor {
    pub(super) fn get_design_reader(&self) -> DesignReader {
        DesignReader {
            presenter: self.presenter.clone(),
            controller: self.controller.clone(),
        }
    }
    pub(super) fn optimize_shift(
        &self,
        reader: &mut dyn ShiftOptimizerReader,
    ) -> Result<InteractorResult, ErrOperation> {
        let nucl_map = self.presenter.get_nucl_map().clone();
        let result = self
            .controller
            .optimize_shift(reader, Arc::new(nucl_map), &self.design);
        self.handle_operation_result(result)
    }

    pub(super) fn apply_operation(
        &self,
        operation: DesignOperation,
    ) -> Result<InteractorResult, ErrOperation> {
        let result = self
            .controller
            .apply_operation(self.design.as_ref(), operation);
        self.handle_operation_result(result)
    }

    pub(super) fn apply_copy_operation(
        &self,
        operation: CopyOperation,
    ) -> Result<InteractorResult, ErrOperation> {
        let result = self
            .controller
            .apply_copy_operation(self.design.as_ref(), operation);
        self.handle_operation_result(result)
    }

    pub(super) fn update_pending_operation(
        &self,
        operation: Arc<dyn Operation>,
    ) -> Result<InteractorResult, ErrOperation> {
        let result = self
            .controller
            .update_pending_operation(self.design.as_ref(), operation);
        self.handle_operation_result(result)
    }

    fn handle_operation_result(
        &self,
        result: Result<(OkOperation, Controller), ErrOperation>,
    ) -> Result<InteractorResult, ErrOperation> {
        match result {
            Ok((OkOperation::Replace(design), controller)) => {
                let mut ret = self.clone();
                ret.controller = AddressPointer::new(controller);
                ret.design = AddressPointer::new(design);
                Ok(InteractorResult::Replace(ret))
            }
            Ok((OkOperation::Push(design), controller)) => {
                let mut ret = self.clone();
                ret.controller = AddressPointer::new(controller);
                ret.design = AddressPointer::new(design);
                Ok(InteractorResult::Push(ret))
            }
            Ok((OkOperation::NoOp, controller)) => {
                let mut ret = self.clone();
                ret.controller = AddressPointer::new(controller);
                Ok(InteractorResult::Replace(ret))
            }
            Err(e) => Err(e),
        }
    }

    pub(super) fn notify(&self, notification: InteractorNotification) -> Self {
        let mut ret = self.clone();
        ret.controller = AddressPointer::new(ret.controller.notify(notification));
        ret
    }

    pub(super) fn with_updated_design_reader(mut self) -> Self {
        if cfg!(test) {
            print!("Old design: ");
            self.design.show_address();
        }
        let (new_presenter, new_design) = update_presenter(&self.presenter, self.design.clone());
        self.presenter = new_presenter;
        if cfg!(test) {
            print!("New design: ");
            new_design.show_address();
        }
        self.design = new_design;
        self
    }

    pub(super) fn with_updated_design(&self, design: Design) -> Self {
        let mut new_interactor = self.clone();
        new_interactor.design = AddressPointer::new(design);
        new_interactor
    }

    pub(super) fn has_different_design_than(&self, other: &Self) -> bool {
        self.design != other.design
    }

    pub(super) fn has_different_model_matrix_than(&self, other: &Self) -> bool {
        self.presenter
            .has_different_model_matrix_than(other.presenter.as_ref())
    }

    pub(super) fn get_simulation_state(&self) -> SimulationState {
        //TODO
        SimulationState::None
    }

    pub(super) fn get_dna_parameters(&self) -> Parameters {
        self.presenter.current_design.parameters.unwrap_or_default()
    }

    pub(super) fn is_changing_color(&self) -> bool {
        self.controller.is_changing_color()
    }

    pub(super) fn get_strand_builders(&self) -> &[StrandBuilder] {
        self.controller.get_strand_builders()
    }

    pub(super) fn is_pasting(&self) -> PastingStatus {
        self.controller.is_pasting()
    }

    pub(super) fn can_iterate_duplication(&self) -> bool {
        self.controller.can_iterate_duplication()
    }
}

/// An opperation has been successfully applied to the design, resulting in a new modifed
/// interactor. The variants of these enum indicate different ways in which the result should be
/// handled
pub(super) enum InteractorResult {
    Push(DesignInteractor),
    Replace(DesignInteractor),
}

/// A reference to a Presenter that is guaranted to always have up to date internal data
/// structures.
pub struct DesignReader {
    presenter: AddressPointer<Presenter>,
    controller: AddressPointer<Controller>,
}

use crate::controller::SaveDesignError;
use std::{path::PathBuf, sync::Arc};
impl DesignReader {
    pub fn save_design(&self, path: &PathBuf) -> Result<(), SaveDesignError> {
        use std::io::Write;
        let json_content = serde_json::to_string_pretty(&self.presenter.current_design.as_ref())?;
        let mut f = std::fs::File::create(path)?;
        f.write_all(json_content.as_bytes())?;
        Ok(())
    }

    pub fn oxdna_export(&self, target_dir: &PathBuf) -> std::io::Result<(PathBuf, PathBuf)> {
        self.presenter.oxdna_export(target_dir)
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use super::controller::CopyOperation;
    use super::*;
    use crate::scene::DesignReader as Reader3d;
    use ensnano_design::grid::GridPosition;
    use ensnano_design::{grid::GridDescriptor, DomainJunction, Nucl, Strand};
    use ensnano_interactor::operation::GridHelixCreation;
    use std::path::PathBuf;
    use ultraviolet::{Rotor3, Vec3};

    fn test_path(design_name: &'static str) -> PathBuf {
        let mut ret = PathBuf::from(std::env!("CARGO_MANIFEST_DIR"));
        ret.push("tests");
        ret.push(design_name);
        ret
    }

    fn one_helix_path() -> PathBuf {
        test_path("one_helix.json")
    }

    fn assert_good_strand<S: std::ops::Deref<Target = str>>(strand: &Strand, objective: S) {
        use regex::Regex;
        let re = Regex::new(r#"\[[^\]]*\]"#).unwrap();
        let formated_strand = strand.formated_domains();
        let left = re.find_iter(&formated_strand);
        let right = re.find_iter(&objective);
        for (a, b) in left.zip(right) {
            assert_eq!(a.as_str(), b.as_str());
        }
    }

    /// A design with one strand h1: 0 -> 5 ; h2: 0 <- 5
    fn one_xover() -> AppState {
        let path = test_path("one_xover.json");
        AppState::import_design(&path).ok().unwrap()
    }

    /// A design with one strand h1: -1 -> 7 ; h2: -1 <- 7 ; h3: 0 -> 9 that can be pasted on
    /// helices 4, 5 and 6
    fn pastable_design() -> AppState {
        let path = test_path("pastable.json");
        AppState::import_design(&path).ok().unwrap()
    }

    fn fake_design_update(state: &mut AppState) {
        let design = state.0.design.design.clone_inner();
        let mut new_state = std::mem::take(state);
        *state = new_state.with_updated_design(design);
    }

    #[test]
    fn read_one_helix() {
        let path = one_helix_path();
        let interactor = DesignInteractor::new_with_path(&path).ok().unwrap();
        let interactor = interactor.with_updated_design_reader();
        let reader = interactor.get_design_reader();
        assert_eq!(reader.get_all_visible_nucl_ids().len(), 24)
    }

    #[test]
    fn first_update_has_effect() {
        use crate::scene::AppState as App3d;
        let path = one_helix_path();
        let mut app_state = AppState::import_design(&path).ok().unwrap();
        let old_app_state = app_state.clone();
        fake_design_update(&mut app_state);
        let app_state = app_state.updated();
        assert!(old_app_state.design_was_modified(&app_state));
    }

    #[test]
    fn second_update_has_no_effect() {
        use crate::scene::AppState as App3d;
        let path = one_helix_path();
        let mut app_state = AppState::import_design(&path).ok().unwrap();
        fake_design_update(&mut app_state);
        app_state = app_state.updated();
        let old_app_state = app_state.clone();
        let app_state = app_state.updated();
        assert!(!old_app_state.design_was_modified(&app_state));
    }

    #[test]
    fn strand_builder_on_xover_end() {
        let mut app_state = one_xover();
        app_state
            .apply_design_op(DesignOperation::RequestStrandBuilders {
                nucls: vec![
                    Nucl {
                        helix: 1,
                        position: 5,
                        forward: true,
                    },
                    Nucl {
                        helix: 2,
                        position: 5,
                        forward: false,
                    },
                ],
            })
            .unwrap();
        app_state.update();
        assert_eq!(app_state.0.design.get_strand_builders().len(), 2);
    }

    #[test]
    fn moving_one_strand_builder() {
        let mut app_state = one_xover();
        app_state
            .apply_design_op(DesignOperation::RequestStrandBuilders {
                nucls: vec![Nucl {
                    helix: 1,
                    position: 5,
                    forward: true,
                }],
            })
            .unwrap();
        app_state.update();
        app_state
            .apply_design_op(DesignOperation::MoveBuilders(7))
            .unwrap();
        app_state.update();
        let strand = app_state
            .0
            .design
            .presenter
            .current_design
            .strands
            .get(&0)
            .expect("No strand 0");
        assert_good_strand(strand, "[H1: 0 -> 7] [H2: 0 <- 5]");
        assert_eq!(
            strand.junctions,
            vec![DomainJunction::IdentifiedXover(0), DomainJunction::Prime3]
        )
    }

    #[test]
    fn moving_xover_preserve_ids() {
        let mut app_state = one_xover();
        app_state
            .apply_design_op(DesignOperation::RequestStrandBuilders {
                nucls: vec![Nucl {
                    helix: 1,
                    position: 5,
                    forward: true,
                }],
            })
            .unwrap();
        app_state.update();
        app_state
            .apply_design_op(DesignOperation::MoveBuilders(7))
            .unwrap();
        app_state.update();

        let n1 = Nucl {
            helix: 1,
            position: 7,
            forward: true,
        };
        let n2 = Nucl {
            helix: 2,
            position: 5,
            forward: false,
        };
        let xover_id = app_state
            .0
            .design
            .presenter
            .junctions_ids
            .get_all_elements();
        assert_eq!(xover_id, vec![(0, (n1, n2))]);
    }

    #[test]
    fn add_grid() {
        let mut app_state = AppState::default();
        app_state
            .apply_design_op(DesignOperation::AddGrid(GridDescriptor {
                position: Vec3::zero(),
                orientation: Rotor3::identity(),
                grid_type: ensnano_design::grid::GridTypeDescr::Square,
            }))
            .unwrap();
        app_state.update();
        assert_eq!(app_state.0.design.presenter.current_design.grids.len(), 1)
    }

    #[test]
    fn add_grid_helix_via_op() {
        let mut app_state = AppState::default();
        app_state
            .apply_design_op(DesignOperation::AddGrid(GridDescriptor {
                position: Vec3::zero(),
                orientation: Rotor3::identity(),
                grid_type: ensnano_design::grid::GridTypeDescr::Square,
            }))
            .unwrap();
        app_state.update();
        app_state
            .update_pending_operation(Arc::new(GridHelixCreation {
                design_id: 0,
                grid_id: 0,
                x: 0,
                y: 0,
                position: 0,
                length: 0,
            }))
            .unwrap();
        app_state.update();
        assert_eq!(app_state.0.design.presenter.current_design.helices.len(), 1)
    }

    #[test]
    fn add_grid_helix_directly() {
        let mut app_state = AppState::default();
        app_state
            .apply_design_op(DesignOperation::AddGrid(GridDescriptor {
                position: Vec3::zero(),
                orientation: Rotor3::identity(),
                grid_type: ensnano_design::grid::GridTypeDescr::Square,
            }))
            .unwrap();
        app_state.update();
        app_state
            .apply_design_op(DesignOperation::AddGridHelix {
                position: GridPosition::from_grid_id_x_y(0, 0, 0),
                start: 0,
                length: 0,
            })
            .unwrap();
        app_state.update();
        assert_eq!(app_state.0.design.presenter.current_design.helices.len(), 1)
    }

    #[test]
    fn copy_creates_clipboard() {
        let mut app_state = pastable_design();
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        assert_eq!(app_state.0.design.controller.size_of_clipboard(), 1)
    }

    #[test]
    fn coping_one_strand() {
        let mut app_state = pastable_design();
        assert_eq!(app_state.0.design.design.strands.len(), 1);
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 4,
                position: 5,
                forward: true,
            })))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::Paste)
            .unwrap();
        app_state.update();
        assert_eq!(app_state.0.design.design.strands.len(), 2);
    }

    #[test]
    fn pasting_is_undoable() {
        let mut app_state = pastable_design();
        assert_eq!(app_state.0.design.design.strands.len(), 1);
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 4,
                position: 5,
                forward: true,
            })))
            .unwrap();
        assert!(app_state
            .apply_copy_operation(CopyOperation::Paste)
            .unwrap()
            .is_some()); // apply_copy_operation returns Some(self) when the action is
                         // undoable and nothing otherwise
    }

    #[test]
    fn can_paste_on_same_helix_if_not_intersecting() {
        let mut app_state = pastable_design();
        assert_eq!(app_state.0.design.design.strands.len(), 1);
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 10,
                forward: true,
            })))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::Paste)
            .unwrap();
        app_state.update();
        assert_eq!(app_state.0.design.design.strands.len(), 2);
    }

    #[test]
    fn copy_cannot_intersect_existing_strand() {
        let mut app_state = pastable_design();
        assert_eq!(app_state.0.design.design.strands.len(), 1);
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 5,
                forward: true,
            })))
            .unwrap();
        match app_state.apply_copy_operation(CopyOperation::Paste) {
            Err(ErrOperation::CannotPasteHere) => (),
            x => panic!("expected CannotPasteHere, got {:?}", x),
        }
    }

    #[test]
    fn not_pasting_after_copy() {
        let mut app_state = pastable_design();
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::None)
    }

    #[test]
    fn pasting_after_copy_and_request_paste() {
        let mut app_state = pastable_design();
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(None))
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::Copy)
    }

    #[test]
    fn not_pasting_after_paste() {
        let mut app_state = pastable_design();
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 4,
                position: 5,
                forward: true,
            })))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::Paste)
            .unwrap();
        app_state.update();
        assert_eq!(app_state.is_pasting(), PastingStatus::None)
    }

    #[test]
    fn pasting_after_duplicate() {
        let mut app_state = pastable_design();
        app_state
            .apply_copy_operation(CopyOperation::InitStrandsDuplication(vec![0]))
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::Duplication)
    }

    #[test]
    fn duplication_of_one_strand() {
        let mut app_state = pastable_design();
        app_state
            .apply_copy_operation(CopyOperation::InitStrandsDuplication(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 10,
                forward: true,
            })))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::Duplicate)
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 2);
        app_state
            .apply_copy_operation(CopyOperation::Duplicate)
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 3);
    }

    #[ignore]
    #[test]
    fn correct_simulation_state() {
        assert!(false)
    }

    #[test]
    fn pasting_candidate_position_are_accessible() {
        let mut app_state = pastable_design();
        assert_eq!(app_state.0.design.controller.get_pasted_position().len(), 0);
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 5,
                forward: true,
            })))
            .unwrap();
        assert!(app_state.0.design.controller.get_pasted_position().len() > 0);
    }

    #[test]
    fn setting_a_candidate_triggers_update() {
        use crate::scene::AppState as App3d;
        let mut app_state = pastable_design();
        let old_app_state = app_state.clone();
        assert!(!old_app_state.design_was_modified(&app_state));
        assert_eq!(app_state.0.design.controller.get_pasted_position().len(), 0);
        app_state
            .apply_copy_operation(CopyOperation::CopyStrands(vec![0]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 5,
                forward: true,
            })))
            .unwrap();
        app_state.update();
        assert!(old_app_state.design_was_modified(&app_state));
    }

    #[test]
    fn positioning_xovers_paste() {
        let mut app_state = pastable_design();
        let (n1, n2) = app_state.get_design_reader().get_xover_with_id(0).unwrap();
        app_state
            .apply_copy_operation(CopyOperation::CopyXovers(vec![(n1, n2)]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(None))
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 1);
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 5,
                forward: true,
            })))
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 2);
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 3,
                forward: true,
            })))
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 2);
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(None))
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 1);
    }

    #[test]
    fn pasting_when_positioning_xovers() {
        let mut app_state = pastable_design();
        let (n1, n2) = app_state.get_design_reader().get_xover_with_id(0).unwrap();
        app_state
            .apply_copy_operation(CopyOperation::CopyXovers(vec![(n1, n2)]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(None))
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::Copy);
    }

    #[test]
    fn duplicating_xovers() {
        let mut app_state = pastable_design();
        let (n1, n2) = app_state.get_design_reader().get_xover_with_id(0).unwrap();
        app_state
            .apply_copy_operation(CopyOperation::InitXoverDuplication(vec![(n1, n2)]))
            .unwrap();
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(None))
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 1);
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 5,
                forward: true,
            })))
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 2);
        app_state
            .apply_copy_operation(CopyOperation::Duplicate)
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 2);
        app_state
            .apply_copy_operation(CopyOperation::Duplicate)
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 3);
        app_state
            .apply_copy_operation(CopyOperation::Duplicate)
            .unwrap();
        assert_eq!(app_state.0.design.design.strands.len(), 4);
    }

    #[test]
    fn duplicating_xovers_pasting_status() {
        let mut app_state = pastable_design();
        let (n1, n2) = app_state.get_design_reader().get_xover_with_id(0).unwrap();
        app_state
            .apply_copy_operation(CopyOperation::InitXoverDuplication(vec![(n1, n2)]))
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::Duplication);
        app_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(Some(Nucl {
                helix: 1,
                position: 5,
                forward: true,
            })))
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::Duplication);
        app_state
            .apply_copy_operation(CopyOperation::Duplicate)
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::None);
        app_state
            .apply_copy_operation(CopyOperation::Duplicate)
            .unwrap();
        assert_eq!(app_state.is_pasting(), PastingStatus::None);
    }
}
