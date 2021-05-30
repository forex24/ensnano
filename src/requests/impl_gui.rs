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

use crate::gui::{Requests as GuiRequests, RigidBodyParametersRequest};
use crate::mediator::RigidBodyConstants;
use std::collections::BTreeSet;

use super::*;

impl GuiRequests for Requests {
    fn ask_use_default_scaffold(&mut self) {
        self.keep_proceed
            .push_back(KeepProceed::AskUseDefaultScafSequence)
    }

    fn close_overlay(&mut self, overlay_type: OverlayType) {
        self.overlay_closed = Some(overlay_type);
    }

    fn open_overlay(&mut self, overlay_type: OverlayType) {
        self.overlay_opened = Some(overlay_type);
    }

    fn change_strand_color(&mut self, color: u32) {
        self.strand_color_change = Some(color);
    }

    fn change_3d_background(&mut self, bg: Background3D) {
        self.background3d = Some(bg);
    }

    fn change_3d_rendering_mode(&mut self, mode: RenderingMode) {
        self.rendering_mode = Some(mode);
    }

    fn set_scaffold_from_selection(&mut self) {
        self.select_scaffold = Some(())
    }

    fn cancel_hyperboloid(&mut self) {
        self.cancel_hyperboloid = Some(())
    }

    fn invert_scroll(&mut self, inverted: bool) {
        self.invert_scroll = Some(inverted);
    }

    fn resize_2d_helices(&mut self, all: bool) {
        self.redim_2d_helices = Some(all);
    }

    fn make_all_elements_visible(&mut self) {
        self.all_visible = Some(());
    }

    fn toggle_visibility(&mut self, visible: bool) {
        self.toggle_visibility = Some(visible);
    }

    fn remove_empty_domains(&mut self) {
        self.clean_requests = Some(());
    }

    fn change_action_mode(&mut self, action_mode: ActionMode) {
        self.action_mode = Some(action_mode);
    }

    fn change_selection_mode(&mut self, selection_mode: SelectionMode) {
        self.selection_mode = Some(selection_mode);
    }

    fn toggle_widget_basis(&mut self) {
        self.toggle_widget = Some(());
    }

    fn set_dna_sequences_visibility(&mut self, visible: bool) {
        self.toggle_text = Some(visible);
    }

    fn download_stapples(&mut self) {
        self.keep_proceed
            .push_back(KeepProceed::DownloadStaplesRequest)
    }

    fn set_selected_strand_sequence(&mut self, sequence: String) {
        self.sequence_change = Some(sequence);
    }

    fn set_scaffold_sequence(&mut self, sequence: String) {
        self.keep_proceed
            .push_back(KeepProceed::SetScaffoldSequence(sequence));
    }

    fn set_scaffold_shift(&mut self, shift: usize) {
        self.scaffold_shift = Some(shift);
    }

    fn set_ui_size(&mut self, size: UiSize) {
        self.new_ui_size = Some(size);
    }

    fn finalize_hyperboloid(&mut self) {
        self.finalize_hyperboloid = Some(())
    }

    fn stop_roll_simulation(&mut self) {
        self.stop_roll = Some(())
    }

    fn start_roll_simulation(&mut self, request: SimulationRequest) {
        self.roll_request = Some(request);
    }

    fn make_grid_from_selection(&mut self) {
        self.make_grids = Some(());
    }

    fn update_rigid_helices_simulation(&mut self, parameters: RigidBodyParametersRequest) {
        let rigid_body_paramters = rigid_parameters(parameters);
        self.rigid_body_parameters = Some(rigid_body_paramters);
    }

    fn update_rigid_grids_simulation(&mut self, parameters: RigidBodyParametersRequest) {
        let rigid_body_parameters = rigid_parameters(parameters);
        self.rigid_body_parameters = Some(rigid_body_parameters);
    }

    fn update_rigid_body_simulation_parameters(&mut self, parameters: RigidBodyParametersRequest) {
        let rigid_body_parameters = rigid_parameters(parameters);
        self.rigid_body_parameters = Some(rigid_body_parameters);
    }

    fn create_new_hyperboloid(&mut self, parameters: HyperboloidRequest) {
        self.new_hyperboloid = Some(parameters);
    }

    fn update_current_hyperboloid(&mut self, parameters: HyperboloidRequest) {
        self.hyperboloid_update = Some(parameters);
    }

    fn update_roll_of_selected_helices(&mut self, roll: f32) {
        self.helix_roll = Some(roll);
    }

    fn update_scroll_sensitivity(&mut self, sensitivity: f32) {
        self.scroll_sensitivity = Some(sensitivity);
    }

    fn set_fog_parameters(&mut self, parameters: FogParameters) {
        self.fog = Some(parameters);
    }

    fn set_torsion_visibility(&mut self, visible: bool) {
        self.show_torsion_request = Some(visible);
    }

    fn set_camera_dir_up_vec(&mut self, direction: Vec3, up: Vec3) {
        self.camera_target = Some((direction, up));
    }

    fn perform_camera_rotation(&mut self, xz: f32, yz: f32, xy: f32) {
        self.camera_rotation = Some((xz, yz, xy));
    }

    fn create_grid(&mut self, grid_type_descriptor: GridTypeDescr) {
        self.new_grid = Some(grid_type_descriptor);
    }

    fn set_candidates_keys(&mut self, candidates: Vec<DnaElementKey>) {
        self.organizer_candidates = Some(candidates);
    }

    fn set_selected_keys(&mut self, selection: Vec<DnaElementKey>) {
        self.organizer_selection = Some(selection);
    }

    fn update_organizer_tree(&mut self, tree: OrganizerTree<DnaElementKey>) {
        self.new_tree = Some(tree);
    }

    fn update_attribute_of_elements(
        &mut self,
        attribute: DnaAttribute,
        keys: BTreeSet<DnaElementKey>,
    ) {
        self.new_attribute = Some((attribute, keys.iter().cloned().collect()));
    }

    fn change_split_mode(&mut self, split_mode: SplitMode) {
        self.toggle_scene = Some(split_mode);
    }

    fn export_to_oxdna(&mut self) {
        self.oxdna = Some(());
    }

    fn toggle_2d_view_split(&mut self) {
        self.split2d = Some(());
    }

    fn undo(&mut self) {
        self.undo = Some(());
    }

    fn redo(&mut self) {
        self.redo = Some(());
    }

    fn force_help(&mut self) {
        self.force_help = Some(());
    }

    fn show_tutorial(&mut self) {
        self.show_tutorial = Some(());
    }

    fn new_design(&mut self) {
        self.keep_proceed.push_back(KeepProceed::NewDesignRequested)
    }

    fn save_as(&mut self) {
        self.keep_proceed.push_back(KeepProceed::SaveAs);
    }

    fn open_file(&mut self) {
        self.keep_proceed.push_back(KeepProceed::SaveBeforeOpen);
    }

    fn fit_design_in_scenes(&mut self) {
        self.fitting = Some(());
    }

    fn update_current_operation(&mut self, operation: Arc<dyn Operation>) {
        self.operation_update = Some(operation);
    }

    fn update_hyperboloid_shift(&mut self, shift: f32) {
        self.new_shift_hyperboloid = Some(shift);
    }

    fn display_error_msg(&mut self, msg: String) {
        self.keep_proceed.push_back(KeepProceed::ErrorMsg(msg))
    }

    fn set_scaffold_id(&mut self, s_id: Option<usize>) {
        self.set_scaffold_id = Some(s_id);
    }

    fn toggle_helices_persistance_of_grid(&mut self, persistant: bool) {
        self.toggle_persistent_helices = Some(persistant);
    }

    fn set_small_sphere(&mut self, small: bool) {
        self.small_spheres = Some(small);
    }
}

fn rigid_parameters(parameters: RigidBodyParametersRequest) -> RigidBodyConstants {
    let ret = RigidBodyConstants {
        k_spring: 10f32.powf(parameters.k_springs),
        k_friction: 10f32.powf(parameters.k_friction),
        mass: 10f32.powf(parameters.mass_factor),
        volume_exclusion: parameters.volume_exclusion,
        brownian_motion: parameters.brownian_motion,
        brownian_rate: 10f32.powf(parameters.brownian_rate),
        brownian_amplitude: parameters.brownian_amplitude,
    };
    println!("{:?}", ret);
    ret
}
