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

use crate::app_state::AddressPointer;
use ensnano_design::{
    grid::{Edge, GridDescriptor, GridPosition},
    mutate_helix, Design, Domain, DomainJunction, Helix, Nucl, Strand,
};
use ensnano_interactor::operation::Operation;
use ensnano_interactor::{
    DesignOperation, DesignRotation, DesignTranslation, DomainIdentifier, IsometryTarget,
    NeighbourDescriptor, NeighbourDescriptorGiver, Selection, StrandBuilder,
};
use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use self::clipboard::{PastedStrand, StrandClipboard};

use super::grid_data::GridManager;
use ultraviolet::{Isometry2, Rotor3, Vec2, Vec3};

mod clipboard;
use clipboard::Clipboard;
pub use clipboard::CopyOperation;

#[derive(Clone, Default)]
pub(super) struct Controller {
    color_idx: usize,
    state: ControllerState,
    clipboard: AddressPointer<Clipboard>,
}

impl Controller {
    fn new_color(color_idx: &mut usize) -> u32 {
        let color = {
            let hue = (*color_idx as f64 * (1. + 5f64.sqrt()) / 2.).fract() * 360.;
            let saturation = (*color_idx as f64 * 7. * (1. + 5f64.sqrt() / 2.)).fract() * 0.4 + 0.4;
            let value = (*color_idx as f64 * 11. * (1. + 5f64.sqrt() / 2.)).fract() * 0.7 + 0.1;
            let hsv = color_space::Hsv::new(hue, saturation, value);
            let rgb = color_space::Rgb::from(hsv);
            (0xFF << 24) | ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32)
        };
        *color_idx += 1;
        color
    }

    /// Apply an operation to the design. This will either produce a modified copy of the design,
    /// or result in an error that could be shown to the user to explain why the requested
    /// operation could no be applied.
    pub fn apply_operation(
        &self,
        design: &Design,
        operation: DesignOperation,
    ) -> Result<(OkOperation, Self), ErrOperation> {
        if !self.check_compatibilty(&operation) {
            return Err(ErrOperation::IncompatibleState);
        }
        match operation {
            DesignOperation::RecolorStaples => Ok(self.ok_apply(Self::recolor_stapples, design)),
            DesignOperation::SetScaffoldSequence(sequence) => Ok(self.ok_apply(
                |ctrl, design| ctrl.set_scaffold_sequence(design, sequence),
                design,
            )),
            DesignOperation::HelicesToGrid(selection) => {
                self.apply(|c, d| c.turn_selection_into_grid(d, selection), design)
            }
            DesignOperation::AddGrid(descriptor) => {
                Ok(self.ok_apply(|c, d| c.add_grid(d, descriptor), design))
            }
            DesignOperation::ChangeColor { color, strands } => {
                Ok(self.ok_apply(|c, d| c.change_color_strands(d, color, strands), design))
            }
            DesignOperation::SetHelicesPersistance {
                grid_ids,
                persistant,
            } => Ok(self.ok_apply(
                |c, d| c.set_helices_persisance(d, grid_ids, persistant),
                design,
            )),
            DesignOperation::SetSmallSpheres { grid_ids, small } => {
                Ok(self.ok_apply(|c, d| c.set_small_spheres(d, grid_ids, small), design))
            }
            DesignOperation::SnapHelices {
                pivots,
                translation,
            } => Ok(self.ok_apply(|c, d| c.snap_helices(d, pivots, translation), design)),
            DesignOperation::SetIsometry { helix, isometry } => {
                Ok(self.ok_apply(|c, d| c.set_isometry(d, helix, isometry), design))
            }
            DesignOperation::RotateHelices {
                helices,
                center,
                angle,
            } => Ok(self.ok_apply(|c, d| c.rotate_helices(d, helices, center, angle), design)),
            DesignOperation::Translation(translation) => {
                self.apply(|c, d| c.apply_translation(d, translation), design)
            }
            DesignOperation::Rotation(rotation) => {
                self.apply(|c, d| c.apply_rotattion(d, rotation), design)
            }
            DesignOperation::RequestStrandBuilders { nucls } => {
                self.apply(|c, d| c.request_strand_builders(d, nucls), design)
            }
            DesignOperation::MoveBuilders(n) => {
                self.apply(|c, d| c.move_strand_builders(d, n), design)
            }
            DesignOperation::Cut { nucl, .. } => self.apply(|c, d| c.cut(d, nucl), design),
            DesignOperation::AddGridHelix {
                position,
                length,
                start,
            } => self.apply(|c, d| c.add_grid_helix(d, position, start, length), design),
            DesignOperation::CrossCut {
                target_3prime,
                source_id,
                target_id,
                nucl,
            } => self.apply(
                |c, d| c.apply_cross_cut(d, source_id, target_id, nucl, target_3prime),
                design,
            ),
            DesignOperation::Xover {
                prime5_id,
                prime3_id,
            } => self.apply(|c, d| c.apply_merge(d, prime5_id, prime3_id), design),
            DesignOperation::GeneralXover { source, target } => {
                self.apply(|c, d| c.general_cross_over(d, source, target), design)
            }
            _ => Err(ErrOperation::NotImplemented),
        }
    }

    pub fn update_pending_operation(
        &self,
        design: &Design,
        operation: Arc<dyn Operation>,
    ) -> Result<(OkOperation, Self), ErrOperation> {
        let effect = operation.effect();
        let mut ret = self.apply_operation(design, effect)?;
        ret.1.state.update_operation(operation);
        Ok(ret)
    }

    pub fn apply_copy_operation(
        &self,
        design: &Design,
        operation: CopyOperation,
    ) -> Result<(OkOperation, Self), ErrOperation> {
        match operation {
            CopyOperation::CopyStrands(strand_ids) => {
                self.apply_no_op(|c, d| c.set_templates(d, strand_ids), design)
            }
            CopyOperation::PositionPastingPoint(nucl) => {
                if self.get_pasting_point() == Some(nucl) {
                    Ok((OkOperation::NoOp, self.clone()))
                } else {
                    self.apply(
                        |c, d| {
                            c.position_strand_copies(&d, nucl)?;
                            Ok(d)
                        },
                        design,
                    )
                }
            }
            CopyOperation::InitStrandsDuplication(strand_ids) => self.apply_no_op(
                |c, d| {
                    c.set_templates(d, strand_ids)?;
                    let clipboard = c.clipboard.as_ref().get_strand_clipboard()?;
                    c.state = ControllerState::PositioningDuplicationPoint {
                        pasted_strands: vec![],
                        duplication_edge: None,
                        pasting_point: None,
                        clipboard,
                    };
                    Ok(())
                },
                design,
            ),
            CopyOperation::Duplicate => self.apply(|c, d| c.apply_duplication(d), design),
            CopyOperation::Paste => {
                Self::make_undoable(self.apply(|c, d| c.apply_paste(d), design))
            }
            _ => Err(ErrOperation::NotImplemented),
        }
    }

    pub fn can_iterate_duplication(&self) -> bool {
        if let ControllerState::WithPendingDuplication { .. } = self.state {
            true
        } else {
            false
        }
    }

    pub fn size_of_clipboard(&self) -> usize {
        self.clipboard.size()
    }

    pub fn is_pasting(&self) -> bool {
        match self.state {
            ControllerState::PositioningPastingPoint { .. } => true,
            ControllerState::PositioningDuplicationPoint { .. } => true,
            _ => false,
        }
    }

    pub fn notify(&self, notification: InteractorNotification) -> Self {
        let mut new_interactor = self.clone();
        match notification {
            InteractorNotification::FinishOperation => new_interactor.state = self.state.finish(),
        }
        new_interactor
    }

    fn check_compatibilty(&self, operation: &DesignOperation) -> bool {
        match self.state {
            ControllerState::Normal => true,
            ControllerState::WithPendingOp(_) => true,
            ControllerState::WithPendingDuplication { .. } => true,
            ControllerState::ChangingColor => {
                if let DesignOperation::ChangeColor { .. } = operation {
                    true
                } else {
                    false
                }
            }
            ControllerState::ApplyingOperation { .. } => true,
            ControllerState::BuildingStrand { initializing, .. } => {
                if let DesignOperation::MoveBuilders(_) = operation {
                    true
                } else {
                    initializing
                }
            }
            _ => false,
        }
    }

    fn update_state_and_design(&mut self, design: &mut Design) {
        if let ControllerState::ApplyingOperation {
            design: design_ptr, ..
        } = &self.state
        {
            *design = design_ptr.clone_inner();
        } else {
            self.state = ControllerState::ApplyingOperation {
                design: AddressPointer::new(design.clone()),
                operation: None,
            };
        }
    }

    fn return_design(&self, design: Design) -> OkOperation {
        match self.state {
            ControllerState::Normal => OkOperation::Push(design),
            ControllerState::WithPendingOp(_) => OkOperation::Push(design),
            ControllerState::WithPendingDuplication { .. } => OkOperation::Push(design),
            _ => OkOperation::Replace(design),
        }
    }

    /// Apply an opperation that cannot fail on the design
    fn ok_apply<F>(&self, design_op: F, design: &Design) -> (OkOperation, Self)
    where
        F: FnOnce(&mut Self, Design) -> Design,
    {
        let mut new_controller = self.clone();
        let returned_design = design_op(&mut new_controller, design.clone());
        (self.return_design(returned_design), new_controller)
    }

    /// Apply an operation that modifies the interactor and not the design, and that cannot fail.
    fn ok_no_op<F>(&self, interactor_op: F, design: &Design) -> (OkOperation, Self)
    where
        F: FnOnce(&mut Self, &Design),
    {
        let mut new_controller = self.clone();
        interactor_op(&mut new_controller, design);
        (OkOperation::NoOp, new_controller)
    }

    fn apply<F>(&self, design_op: F, design: &Design) -> Result<(OkOperation, Self), ErrOperation>
    where
        F: FnOnce(&mut Self, Design) -> Result<Design, ErrOperation>,
    {
        let mut new_controller = self.clone();
        let returned_design = design_op(&mut new_controller, design.clone())?;
        Ok((self.return_design(returned_design), new_controller))
    }

    fn make_undoable(
        result: Result<(OkOperation, Self), ErrOperation>,
    ) -> Result<(OkOperation, Self), ErrOperation> {
        match result {
            Ok((ok_op, interactor)) => Ok((ok_op.into_undoable(), interactor)),
            Err(e) => Err(e),
        }
    }

    fn apply_no_op<F>(
        &self,
        interactor_op: F,
        design: &Design,
    ) -> Result<(OkOperation, Self), ErrOperation>
    where
        F: FnOnce(&mut Self, &Design) -> Result<(), ErrOperation>,
    {
        let mut new_controller = self.clone();
        interactor_op(&mut new_controller, design)?;
        Ok((OkOperation::NoOp, new_controller))
    }

    fn turn_selection_into_grid(
        &mut self,
        mut design: Design,
        selection: Vec<Selection>,
    ) -> Result<Design, ErrOperation> {
        let mut grid_manager = GridManager::new_from_design(&design);
        let helices =
            ensnano_interactor::list_of_helices(&selection).ok_or(ErrOperation::BadSelection)?;
        grid_manager.make_grid_from_helices(&mut design, &helices.1)?;
        Ok(design)
    }

    fn add_grid(&mut self, mut design: Design, descriptor: GridDescriptor) -> Design {
        let mut new_grids = Vec::clone(design.grids.as_ref());
        new_grids.push(descriptor);
        design.grids = Arc::new(new_grids);
        design
    }

    pub(super) fn is_changing_color(&self) -> bool {
        if let ControllerState::ChangingColor = self.state {
            true
        } else {
            false
        }
    }

    pub(super) fn get_strand_builders(&self) -> &[StrandBuilder] {
        if let ControllerState::BuildingStrand { builders, .. } = &self.state {
            builders.as_slice()
        } else {
            &[]
        }
    }

    fn apply_translation(
        &mut self,
        design: Design,
        translation: DesignTranslation,
    ) -> Result<Design, ErrOperation> {
        match translation.target {
            IsometryTarget::Design => Err(ErrOperation::NotImplemented),
            IsometryTarget::Helices(helices, snap) => {
                Ok(self.translate_helices(design, snap, helices, translation.translation))
            }
            IsometryTarget::Grids(grid_ids) => {
                Ok(self.translate_grids(design, grid_ids, translation.translation))
            }
        }
    }

    fn apply_rotattion(
        &mut self,
        design: Design,
        rotation: DesignRotation,
    ) -> Result<Design, ErrOperation> {
        match rotation.target {
            IsometryTarget::Design => Err(ErrOperation::NotImplemented),
            IsometryTarget::Helices(helices, snap) => Ok(self.rotate_helices_3d(
                design,
                snap,
                helices,
                rotation.rotation,
                rotation.origin,
            )),
            IsometryTarget::Grids(grid_ids) => {
                Ok(self.rotate_grids(design, grid_ids, rotation.rotation, rotation.origin))
            }
        }
    }

    fn translate_helices(
        &mut self,
        mut design: Design,
        snap: bool,
        helices: Vec<usize>,
        translation: Vec3,
    ) -> Design {
        self.update_state_and_design(&mut design);
        let mut new_helices = BTreeMap::clone(design.helices.as_ref());
        for h_id in helices.iter() {
            if let Some(h) = new_helices.get_mut(h_id) {
                mutate_helix(h, |h| h.translate(translation));
            }
        }
        let mut new_design = design.clone();
        new_design.helices = Arc::new(new_helices);
        if snap {
            self.attempt_reattach(design, new_design, &helices)
        } else {
            new_design
        }
    }

    fn rotate_helices_3d(
        &mut self,
        mut design: Design,
        snap: bool,
        helices: Vec<usize>,
        rotation: Rotor3,
        origin: Vec3,
    ) -> Design {
        self.update_state_and_design(&mut design);
        let mut new_helices = BTreeMap::clone(design.helices.as_ref());
        for h_id in helices.iter() {
            if let Some(h) = new_helices.get_mut(h_id) {
                mutate_helix(h, |h| h.rotate_arround(rotation, origin))
            }
        }
        let mut new_design = design.clone();
        new_design.helices = Arc::new(new_helices);
        if snap {
            self.attempt_reattach(design, new_design, &helices)
        } else {
            new_design
        }
    }

    fn attempt_reattach(
        &mut self,
        design: Design,
        mut new_design: Design,
        helices: &[usize],
    ) -> Design {
        let mut grid_manager = GridManager::new_from_design(&new_design);
        let mut successfull_reattach = true;
        for h_id in helices.iter() {
            successfull_reattach &= grid_manager.reattach_helix(*h_id, &mut new_design, true);
        }
        if successfull_reattach {
            new_design
        } else {
            design
        }
    }

    fn translate_grids(
        &mut self,
        mut design: Design,
        grid_ids: Vec<usize>,
        translation: Vec3,
    ) -> Design {
        self.update_state_and_design(&mut design);
        let mut new_grids = Vec::clone(design.grids.as_ref());
        for g_id in grid_ids.into_iter() {
            if let Some(desc) = new_grids.get_mut(g_id) {
                desc.position += translation;
            }
        }
        design.grids = Arc::new(new_grids);
        design
    }

    fn rotate_grids(
        &mut self,
        mut design: Design,
        grid_ids: Vec<usize>,
        rotation: Rotor3,
        origin: Vec3,
    ) -> Design {
        self.update_state_and_design(&mut design);
        let mut new_grids = Vec::clone(design.grids.as_ref());
        for g_id in grid_ids.into_iter() {
            if let Some(desc) = new_grids.get_mut(g_id) {
                desc.position -= origin;
                desc.orientation = rotation * desc.orientation;
                desc.position = rotation * desc.position;
                desc.position += origin;
            }
        }
        design.grids = Arc::new(new_grids);
        design
    }
}

/// An operation has been successfully applied on a design, resulting in a new modified design. The
/// variants of these enums indicate different ways in which the result should be handled
pub enum OkOperation {
    /// Push the current design on the undo stack and replace it by the wrapped value. This variant
    /// is produced when the operation has been peroformed on a non transitory design and can be
    /// undone.
    Push(Design),
    /// Replace the current design by the wrapped value. This variant is produced when the
    /// operation has been peroformed on a transitory design and should not been undone.
    ///
    /// This happens for example for operations that are performed by drag and drop, where each new
    /// mouse mouvement produce a new design. In this case, the successive design should not be
    /// pushed on the undo stack, since an undo is expected to revert back to the state prior to
    /// the whole drag and drop operation.
    Replace(Design),
    NoOp,
}

impl OkOperation {
    fn into_undoable(self) -> Self {
        match self {
            Self::Replace(design) => Self::Push(design),
            Self::Push(design) => Self::Push(design),
            Self::NoOp => Self::NoOp,
        }
    }
}

#[derive(Debug)]
pub enum ErrOperation {
    NotImplemented,
    NotEnoughHelices {
        actual: usize,
        required: usize,
    },
    /// The operation cannot be applied on the current selection
    BadSelection,
    /// The controller is in a state incompatible with applying the operation
    IncompatibleState,
    CannotBuildOn(Nucl),
    CutInexistingStrand,
    GridDoesNotExist(usize),
    GridPositionAlreadyUsed,
    StrandDoesNotExist(usize),
    HelixDoesNotExists(usize),
    HelixHasNoGridPosition(usize),
    CouldNotMakeEdge(GridPosition, GridPosition),
    MergingSameStrand,
    XoverOnSameHelix,
    NuclDoesNotExist(Nucl),
    XoverBetweenTwoPrime5,
    XoverBetweenTwoPrime3,
    CouldNotCreateTemplates,
    CouldNotCreateEdges,
    EmptyOrigin,
    EmptyClipboard,
    CannotPasteHere,
}

impl Controller {
    fn recolor_stapples(&mut self, mut design: Design) -> Design {
        for (s_id, strand) in design.strands.iter_mut() {
            if Some(*s_id) != design.scaffold_id {
                let color = {
                    let hue = (self.color_idx as f64 * (1. + 5f64.sqrt()) / 2.).fract() * 360.;
                    let saturation =
                        (self.color_idx as f64 * 7. * (1. + 5f64.sqrt() / 2.)).fract() * 0.4 + 0.4;
                    let value =
                        (self.color_idx as f64 * 11. * (1. + 5f64.sqrt() / 2.)).fract() * 0.7 + 0.1;
                    let hsv = color_space::Hsv::new(hue, saturation, value);
                    let rgb = color_space::Rgb::from(hsv);
                    (0xFF << 24) | ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32)
                };
                self.color_idx += 1;
                strand.color = color;
            }
        }
        design
    }

    fn set_scaffold_sequence(&mut self, mut design: Design, sequence: String) -> Design {
        design.scaffold_sequence = Some(sequence);
        design
    }

    fn change_color_strands(
        &mut self,
        mut design: Design,
        color: u32,
        strands: Vec<usize>,
    ) -> Design {
        self.state = ControllerState::ChangingColor;
        for s_id in strands.iter() {
            if let Some(strand) = design.strands.get_mut(s_id) {
                strand.color = color;
            }
        }
        design
    }

    fn set_helices_persisance(
        &mut self,
        mut design: Design,
        grid_ids: Vec<usize>,
        persistant: bool,
    ) -> Design {
        for g_id in grid_ids.into_iter() {
            if persistant {
                design.no_phantoms.remove(&g_id);
            } else {
                design.no_phantoms.insert(g_id);
            }
        }
        design
    }

    fn set_small_spheres(
        &mut self,
        mut design: Design,
        grid_ids: Vec<usize>,
        small: bool,
    ) -> Design {
        for g_id in grid_ids.into_iter() {
            if small {
                design.small_spheres.insert(g_id);
            } else {
                design.small_spheres.remove(&g_id);
            }
        }
        design
    }

    fn snap_helices(&mut self, mut design: Design, pivots: Vec<Nucl>, translation: Vec2) -> Design {
        self.update_state_and_design(&mut design);
        let mut new_helices = BTreeMap::clone(design.helices.as_ref());
        for p in pivots.iter() {
            if let Some(h) = new_helices.get_mut(&p.helix) {
                if let Some(old_pos) = nucl_pos_2d(&design, p) {
                    let position = old_pos + translation;
                    let position = Vec2::new(position.x.round(), position.y.round());
                    mutate_helix(h, |h| {
                        if let Some(isometry) = h.isometry2d.as_mut() {
                            isometry.append_translation(position - old_pos)
                        }
                    })
                }
            }
        }
        design.helices = Arc::new(new_helices);
        design
    }

    fn set_isometry(&mut self, mut design: Design, h_id: usize, isometry: Isometry2) -> Design {
        let mut new_helices = BTreeMap::clone(design.helices.as_ref());
        if let Some(h) = new_helices.get_mut(&h_id) {
            mutate_helix(h, |h| h.isometry2d = Some(isometry));
            design.helices = Arc::new(new_helices);
        }
        design
    }

    fn rotate_helices(
        &mut self,
        mut design: Design,
        helices: Vec<usize>,
        center: Vec2,
        angle: f32,
    ) -> Design {
        self.update_state_and_design(&mut design);
        let angle = {
            let k = (angle / std::f32::consts::FRAC_PI_8).round();
            k * std::f32::consts::FRAC_PI_8
        };
        let mut new_helices = BTreeMap::clone(design.helices.as_ref());
        for h_id in helices.iter() {
            if let Some(h) = new_helices.get_mut(h_id) {
                mutate_helix(h, |h| {
                    if let Some(isometry) = h.isometry2d.as_mut() {
                        isometry.append_translation(-center);
                        isometry.append_rotation(ultraviolet::Rotor2::from_angle(angle));
                        isometry.append_translation(center);
                    }
                })
            }
        }
        design.helices = Arc::new(new_helices);
        design
    }

    fn request_strand_builders(
        &mut self,
        mut design: Design,
        nucls: Vec<Nucl>,
    ) -> Result<Design, ErrOperation> {
        let mut builders = Vec::with_capacity(nucls.len());
        for nucl in nucls.into_iter() {
            builders.push(
                self.request_one_builder(&mut design, nucl)
                    .ok_or(ErrOperation::CannotBuildOn(nucl))?,
            );
        }
        self.state = ControllerState::BuildingStrand {
            builders,
            initializing: true,
            // The initial design is indeed the one AFTER adding the new strands
            initial_design: AddressPointer::new(design.clone()),
        };
        Ok(design)
    }

    fn request_one_builder(&mut self, design: &mut Design, nucl: Nucl) -> Option<StrandBuilder> {
        // if there is a strand that passes through the nucleotide
        if design.get_strand_nucl(&nucl).is_some() {
            self.strand_builder_on_exisiting(design, nucl)
        } else {
            self.new_strand_builder(design, nucl)
        }
    }

    fn strand_builder_on_exisiting(
        &mut self,
        design: &Design,
        nucl: Nucl,
    ) -> Option<StrandBuilder> {
        let left = design.get_neighbour_nucl(nucl.left());
        let right = design.get_neighbour_nucl(nucl.right());
        let axis = design
            .helices
            .get(&nucl.helix)
            .map(|h| h.get_axis(&design.parameters.unwrap_or_default()))?;
        let desc = design.get_neighbour_nucl(nucl)?;
        let strand_id = desc.identifier.strand;
        let filter = |d: &NeighbourDescriptor| d.identifier != desc.identifier;
        let neighbour_desc = left.filter(filter).or(right.filter(filter));
        let stick = neighbour_desc.map(|d| d.identifier.strand) == Some(strand_id);
        if left.filter(filter).and(right.filter(filter)).is_some() {
            // TODO maybe we should do something else ?
            return None;
        }
        match design.strands.get(&strand_id).map(|s| s.length()) {
            Some(n) if n > 1 => Some(StrandBuilder::init_existing(
                desc.identifier,
                nucl,
                axis,
                desc.fixed_end,
                neighbour_desc,
                stick,
            )),
            _ => Some(StrandBuilder::init_empty(
                DomainIdentifier {
                    strand: strand_id,
                    domain: 0,
                },
                nucl,
                axis,
                neighbour_desc,
                false,
            )),
        }
    }

    fn new_strand_builder(&mut self, design: &mut Design, nucl: Nucl) -> Option<StrandBuilder> {
        let left = design.get_neighbour_nucl(nucl.left());
        let right = design.get_neighbour_nucl(nucl.right());
        let axis = design
            .helices
            .get(&nucl.helix)
            .map(|h| h.get_axis(&design.parameters.unwrap_or_default()))?;
        if left.is_some() && right.is_some() {
            return None;
        }
        let new_key = self.init_strand(design, nucl);
        Some(StrandBuilder::init_empty(
            DomainIdentifier {
                strand: new_key,
                domain: 0,
            },
            nucl,
            axis,
            left.or(right),
            true,
        ))
    }

    fn init_strand(&mut self, design: &mut Design, nucl: Nucl) -> usize {
        let s_id = design.strands.keys().max().map(|n| n + 1).unwrap_or(0);
        let color = {
            let hue = (self.color_idx as f64 * (1. + 5f64.sqrt()) / 2.).fract() * 360.;
            let saturation =
                (self.color_idx as f64 * 7. * (1. + 5f64.sqrt() / 2.)).fract() * 0.4 + 0.4;
            let value = (self.color_idx as f64 * 11. * (1. + 5f64.sqrt() / 2.)).fract() * 0.7 + 0.1;
            let hsv = color_space::Hsv::new(hue, saturation, value);
            let rgb = color_space::Rgb::from(hsv);
            (0xFF << 24) | ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32)
        };
        self.color_idx += 1;
        design.strands.insert(
            s_id,
            Strand::init(nucl.helix, nucl.position, nucl.forward, color),
        );
        s_id
    }

    fn add_strand(
        &mut self,
        design: &mut Design,
        helix: usize,
        position: isize,
        forward: bool,
    ) -> usize {
        let new_key = if let Some(k) = design.strands.keys().max() {
            *k + 1
        } else {
            0
        };
        let color = {
            let hue = (self.color_idx as f64 * (1. + 5f64.sqrt()) / 2.).fract() * 360.;
            let saturation =
                (self.color_idx as f64 * 7. * (1. + 5f64.sqrt() / 2.)).fract() * 0.4 + 0.4;
            let value = (self.color_idx as f64 * 11. * (1. + 5f64.sqrt() / 2.)).fract() * 0.7 + 0.1;
            let hsv = color_space::Hsv::new(hue, saturation, value);
            let rgb = color_space::Rgb::from(hsv);
            (0xFF << 24) | ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32)
        };
        self.color_idx += 1;

        design
            .strands
            .insert(new_key, Strand::init(helix, position, forward, color));
        new_key
    }

    fn move_strand_builders(&mut self, _: Design, n: isize) -> Result<Design, ErrOperation> {
        if let ControllerState::BuildingStrand {
            initial_design,
            builders,
            initializing,
        } = &mut self.state
        {
            let mut design = initial_design.clone_inner();
            for builder in builders.iter_mut() {
                builder.move_to(n, &mut design)
            }
            *initializing = false;
            Ok(design)
        } else {
            Err(ErrOperation::IncompatibleState)
        }
    }

    fn cut(&mut self, mut design: Design, nucl: Nucl) -> Result<Design, ErrOperation> {
        let _ = Self::split_strand(&mut design, &nucl, None)?;
        Ok(design)
    }

    /// Split a strand at nucl, and return the id of the newly created strand
    ///
    /// The part of the strand that contains nucl is given the original
    /// strand's id, the other part is given a new id.
    ///
    /// If `force_end` is `Some(true)`, nucl will be on the 3 prime half of the split.
    /// If `force_end` is `Some(false)` nucl will be on the 5 prime half of the split.
    /// If `force_end` is `None`, nucl will be on the 5 prime half of the split unless nucl is the 3
    /// prime extremity of a crossover, in which case nucl will be on the 3 prime half of the
    /// split.
    fn split_strand(
        design: &mut Design,
        nucl: &Nucl,
        force_end: Option<bool>,
    ) -> Result<usize, ErrOperation> {
        let id = design
            .get_strand_nucl(nucl)
            .ok_or(ErrOperation::CutInexistingStrand)?;

        let strand = design.strands.remove(&id).expect("strand");
        if strand.cyclic {
            let new_strand = Self::break_cycle(strand.clone(), *nucl, force_end);
            design.strands.insert(id, new_strand);
            //self.clean_domains_one_strand(id);
            //println!("Cutting cyclic strand");
            return Ok(id);
        }
        if strand.length() <= 1 {
            // return without putting the strand back
            return Err(ErrOperation::CutInexistingStrand);
        }
        let mut i = strand.domains.len();
        let mut prim5_domains = Vec::new();
        let mut len_prim5 = 0;
        let mut domains = None;
        let mut on_3prime = force_end.unwrap_or(false);
        let mut prev_helix = None;
        let mut prime5_junctions: Vec<DomainJunction> = Vec::new();
        let mut prime3_junctions: Vec<DomainJunction> = Vec::new();

        println!("Spliting");
        println!("{:?}", strand.domains);
        println!("{:?}", strand.junctions);

        for (d_id, domain) in strand.domains.iter().enumerate() {
            if domain.prime5_end() == Some(*nucl)
                && prev_helix != domain.helix()
                && force_end != Some(false)
            {
                // nucl is the 5' end of the next domain so it is the on the 3' end of a xover.
                // nucl is not required to be on the 5' half of the split, so we put it on the 3'
                // half
                on_3prime = true;
                i = d_id;
                if let Some(j) = prime5_junctions.last_mut() {
                    *j = DomainJunction::Prime3;
                }
                break;
            } else if domain.prime3_end() == Some(*nucl) && force_end != Some(true) {
                // nucl is the 3' end of the current domain so it is the on the 5' end of a xover.
                // nucl is not required to be on the 3' half of the split, so we put it on the 5'
                // half
                i = d_id + 1;
                prim5_domains.push(domain.clone());
                len_prim5 += domain.length();
                prime5_junctions.push(DomainJunction::Prime3);
                break;
            } else if let Some(n) = domain.has_nucl(nucl) {
                let n = if force_end == Some(true) { n - 1 } else { n };
                i = d_id;
                len_prim5 += n;
                domains = domain.split(n);
                prime5_junctions.push(DomainJunction::Prime3);
                prime3_junctions.push(strand.junctions[d_id].clone());
                break;
            } else {
                len_prim5 += domain.length();
                prim5_domains.push(domain.clone());
                prime5_junctions.push(strand.junctions[d_id].clone());
            }
            prev_helix = domain.helix();
        }

        let mut prim3_domains = Vec::new();
        if let Some(ref domains) = domains {
            prim5_domains.push(domains.0.clone());
            prim3_domains.push(domains.1.clone());
            i += 1;
        }

        for n in i..strand.domains.len() {
            let domain = &strand.domains[n];
            prim3_domains.push(domain.clone());
            prime3_junctions.push(strand.junctions[n].clone());
        }

        let seq_prim5;
        let seq_prim3;
        if let Some(seq) = strand.sequence {
            let seq = seq.into_owned();
            let chars = seq.chars();
            seq_prim5 = Some(Cow::Owned(chars.clone().take(len_prim5).collect()));
            seq_prim3 = Some(Cow::Owned(chars.clone().skip(len_prim5).collect()));
        } else {
            seq_prim3 = None;
            seq_prim5 = None;
        }

        println!("prime5 {:?}", prim5_domains);
        println!("prime5 {:?}", prime5_junctions);

        println!("prime3 {:?}", prim3_domains);
        println!("prime3 {:?}", prime3_junctions);
        let strand_5prime = Strand {
            domains: prim5_domains,
            color: strand.color,
            junctions: prime5_junctions,
            cyclic: false,
            sequence: seq_prim5,
        };

        let strand_3prime = Strand {
            domains: prim3_domains,
            color: strand.color,
            cyclic: false,
            junctions: prime3_junctions,
            sequence: seq_prim3,
        };
        let new_id = (*design.strands.keys().max().unwrap_or(&0)).max(id) + 1;
        println!("new id {}, ; id {}", new_id, id);
        let (id_5prime, id_3prime) = if !on_3prime {
            (id, new_id)
        } else {
            (new_id, id)
        };
        if strand_5prime.domains.len() > 0 {
            design.strands.insert(id_5prime, strand_5prime);
        }
        if strand_3prime.domains.len() > 0 {
            design.strands.insert(id_3prime, strand_3prime);
        }
        //self.make_hash_maps();

        /*
        if crate::MUST_TEST {
            self.test_named_junction("TEST AFTER SPLIT STRAND");
        }*/
        Ok(new_id)
    }

    /// Split a cyclic strand at nucl
    ///
    /// If `force_end` is `Some(true)`, nucl will be the new 5' end of the strand.
    /// If `force_end` is `Some(false)` nucl will be the new 3' end of the strand.
    /// If `force_end` is `None`, nucl will be the new 3' end of the strand unless nucl is the 3'
    /// prime extremity of a crossover, in which case nucl will be the new 5' end of the strand
    fn break_cycle(mut strand: Strand, nucl: Nucl, force_end: Option<bool>) -> Strand {
        let mut last_dom = None;
        let mut replace_last_dom = None;
        let mut prev_helix = None;

        let mut junctions: Vec<DomainJunction> = Vec::with_capacity(strand.domains.len());

        for (i, domain) in strand.domains.iter().enumerate() {
            if domain.prime5_end() == Some(nucl)
                && prev_helix != domain.helix()
                && force_end != Some(false)
            {
                last_dom = if i != 0 {
                    Some(i - 1)
                } else {
                    Some(strand.domains.len() - 1)
                };

                break;
            } else if domain.prime3_end() == Some(nucl) && force_end != Some(true) {
                last_dom = Some(i);
                break;
            } else if let Some(n) = domain.has_nucl(&nucl) {
                let n = if force_end == Some(true) { n - 1 } else { n };
                last_dom = Some(i);
                replace_last_dom = domain.split(n);
            }
            prev_helix = domain.helix();
        }
        let last_dom = last_dom.expect("Could not find nucl in strand");
        let mut new_domains = Vec::new();
        if let Some((_, ref d2)) = replace_last_dom {
            new_domains.push(d2.clone());
            junctions.push(strand.junctions[last_dom].clone());
        }
        for (i, d) in strand.domains.iter().enumerate().skip(last_dom + 1) {
            new_domains.push(d.clone());
            junctions.push(strand.junctions[i].clone());
        }
        for (i, d) in strand.domains.iter().enumerate().take(last_dom) {
            new_domains.push(d.clone());
            junctions.push(strand.junctions[i].clone());
        }

        if let Some((ref d1, _)) = replace_last_dom {
            new_domains.push(d1.clone())
        } else {
            new_domains.push(strand.domains[last_dom].clone())
        }
        junctions.push(DomainJunction::Prime3);

        strand.domains = new_domains;
        strand.cyclic = false;
        strand.junctions = junctions;
        strand
    }

    fn add_grid_helix(
        &mut self,
        mut design: Design,
        position: GridPosition,
        start: isize,
        length: usize,
    ) -> Result<Design, ErrOperation> {
        let grid_manager = GridManager::new_from_design(&design);
        if grid_manager
            .pos_to_helix(position.grid, position.x, position.y)
            .is_some()
        {
            return Err(ErrOperation::GridPositionAlreadyUsed);
        }
        let grid = grid_manager
            .grids
            .get(position.grid)
            .ok_or(ErrOperation::GridDoesNotExist(position.grid))?;
        let mut new_helices = BTreeMap::clone(design.helices.as_ref());
        let helix = Helix::new_on_grid(grid, position.x, position.y, position.grid);
        let helix_id = new_helices.keys().last().unwrap_or(&0) + 1;
        new_helices.insert(helix_id, Arc::new(helix));
        if length > 0 {
            for b in [false, true].iter() {
                let new_key = self.add_strand(&mut design, helix_id, start, *b);
                if let Domain::HelixDomain(ref mut dom) =
                    design.strands.get_mut(&new_key).unwrap().domains[0]
                {
                    dom.end = dom.start + length as isize;
                }
            }
        }
        design.helices = Arc::new(new_helices);
        Ok(design)
    }

    /// Merge two strands with identifier prime5 and prime3. The resulting strand will have
    /// identifier prime5.
    fn merge_strands(
        design: &mut Design,
        prime5: usize,
        prime3: usize,
    ) -> Result<(), ErrOperation> {
        // We panic, if we can't find the strand, because this means that the program has a bug
        if prime5 != prime3 {
            let strand5prime = design
                .strands
                .remove(&prime5)
                .ok_or(ErrOperation::StrandDoesNotExist(prime5))?;
            let strand3prime = design
                .strands
                .remove(&prime3)
                .ok_or(ErrOperation::StrandDoesNotExist(prime3))?;
            let len = strand5prime.domains.len() + strand3prime.domains.len();
            let mut domains = Vec::with_capacity(len);
            let mut junctions = Vec::with_capacity(len);
            for (i, domain) in strand5prime.domains.iter().enumerate() {
                domains.push(domain.clone());
                junctions.push(strand5prime.junctions[i].clone());
            }
            let skip;
            let last_helix = domains.last().and_then(|d| d.half_helix());
            let next_helix = strand3prime
                .domains
                .iter()
                .next()
                .and_then(|d| d.half_helix());
            if last_helix == next_helix && last_helix.is_some() {
                skip = 1;
                domains
                    .last_mut()
                    .as_mut()
                    .unwrap()
                    .merge(strand3prime.domains.iter().next().unwrap());
                junctions.pop();
            } else {
                skip = 0;
                if let Some(j) = junctions.iter_mut().last() {
                    *j = DomainJunction::UnindentifiedXover
                }
            }
            for domain in strand3prime.domains.iter().skip(skip) {
                domains.push(domain.clone());
            }
            for junction in strand3prime.junctions.iter() {
                junctions.push(junction.clone());
            }
            let sequence = if let Some((seq5, seq3)) = strand5prime
                .sequence
                .clone()
                .zip(strand3prime.sequence.clone())
            {
                let new_seq = seq5.into_owned() + &seq3.into_owned();
                Some(Cow::Owned(new_seq))
            } else if let Some(ref seq5) = strand5prime.sequence {
                Some(seq5.clone())
            } else if let Some(ref seq3) = strand3prime.sequence {
                Some(seq3.clone())
            } else {
                None
            };
            let new_strand = Strand {
                domains,
                color: strand5prime.color,
                sequence,
                junctions,
                cyclic: false,
            };
            design.strands.insert(prime5, new_strand);
            Ok(())
        } else {
            // To make a cyclic strand use `make_cyclic_strand` instead
            Err(ErrOperation::MergingSameStrand)
        }
    }

    /// Make a strand cyclic by linking the 3' and the 5' end, or undo this operation.
    fn make_cycle(design: &mut Design, strand_id: usize, cyclic: bool) -> Result<(), ErrOperation> {
        design
            .strands
            .get_mut(&strand_id)
            .ok_or(ErrOperation::StrandDoesNotExist(strand_id))?
            .cyclic = cyclic;

        let strand = design
            .strands
            .get_mut(&strand_id)
            .ok_or(ErrOperation::StrandDoesNotExist(strand_id))?;
        if cyclic {
            let first_last_domains = (strand.domains.iter().next(), strand.domains.iter().last());
            let merge_insertions =
                if let (Some(Domain::Insertion(n1)), Some(Domain::Insertion(n2))) =
                    first_last_domains
                {
                    Some(n1 + n2)
                } else {
                    None
                };
            if let Some(n) = merge_insertions {
                // If the strand starts and finishes by an Insertion, merge the insertions.
                // TODO UNITTEST for this specific case
                *strand.domains.last_mut().unwrap() = Domain::Insertion(n);
                // remove the first insertions
                strand.domains.remove(0);
                strand.junctions.remove(0);
            }

            let first_last_domains = (strand.domains.iter().next(), strand.domains.iter().last());
            let skip_last = if let (_, Some(Domain::Insertion(_))) = first_last_domains {
                1
            } else {
                0
            };
            let skip_first = if let (Some(Domain::Insertion(_)), _) = first_last_domains {
                1
            } else {
                0
            };
            let last_first_intervals = (
                strand.domains.iter().rev().skip(skip_last).next(),
                strand.domains.get(skip_first),
            );
            if let (Some(Domain::HelixDomain(i1)), Some(Domain::HelixDomain(i2))) =
                last_first_intervals
            {
                let junction = junction(i1, i2);
                *strand.junctions.last_mut().unwrap() = junction;
            } else {
                panic!("Invariant Violated: SaneDomains")
            }
        } else {
            *strand.junctions.last_mut().unwrap() = DomainJunction::Prime3;
        }
        Ok(())
    }

    fn apply_cross_cut(
        &mut self,
        mut design: Design,
        source_strand: usize,
        target_strand: usize,
        nucl: Nucl,
        target_3prime: bool,
    ) -> Result<Design, ErrOperation> {
        Self::cross_cut(
            &mut design,
            source_strand,
            target_strand,
            nucl,
            target_3prime,
        )?;
        Ok(design)
    }

    fn apply_merge(
        &mut self,
        mut design: Design,
        prime5_id: usize,
        prime3_id: usize,
    ) -> Result<Design, ErrOperation> {
        if prime5_id != prime3_id {
            Self::merge_strands(&mut design, prime5_id, prime3_id)?;
        } else {
            Self::make_cycle(&mut design, prime5_id, true)?;
        }
        Ok(design)
    }

    /// Cut the target strand at nucl and the make a cross over from the source strand to the part
    /// that contains nucl
    fn cross_cut(
        design: &mut Design,
        source_strand: usize,
        target_strand: usize,
        nucl: Nucl,
        target_3prime: bool,
    ) -> Result<(), ErrOperation> {
        let new_id = design.strands.keys().max().map(|n| n + 1).unwrap_or(0);
        let was_cyclic = design
            .strands
            .get(&target_strand)
            .ok_or(ErrOperation::StrandDoesNotExist(target_strand))?
            .cyclic;
        //println!("half1 {}, ; half0 {}", new_id, target_strand);
        Self::split_strand(design, &nucl, Some(target_3prime))?;
        //println!("splitted");

        if !was_cyclic && source_strand != target_strand {
            if target_3prime {
                // swap the position of the two half of the target strands so that the merged part is the
                // new id
                let half0 = design
                    .strands
                    .remove(&target_strand)
                    .ok_or(ErrOperation::StrandDoesNotExist(target_strand))?;
                let half1 = design
                    .strands
                    .remove(&new_id)
                    .ok_or(ErrOperation::StrandDoesNotExist(new_id))?;
                design.strands.insert(new_id, half0);
                design.strands.insert(target_strand, half1);
                Self::merge_strands(design, source_strand, new_id)
            } else {
                // if the target strand is the 5' end of the merge, we give the new id to the source
                // strand because it is the one that is lost in the merge.
                let half0 = design
                    .strands
                    .remove(&source_strand)
                    .ok_or(ErrOperation::StrandDoesNotExist(source_strand))?;
                let half1 = design
                    .strands
                    .remove(&new_id)
                    .ok_or(ErrOperation::StrandDoesNotExist(new_id))?;
                design.strands.insert(new_id, half0);
                design.strands.insert(source_strand, half1);
                Self::merge_strands(design, target_strand, new_id)
            }
        } else if source_strand == target_strand {
            Self::make_cycle(design, source_strand, true)
        } else {
            if target_3prime {
                Self::merge_strands(design, source_strand, target_strand)
            } else {
                Self::merge_strands(design, target_strand, source_strand)
            }
        }
    }

    fn general_cross_over(
        &mut self,
        mut design: Design,
        source_nucl: Nucl,
        target_nucl: Nucl,
    ) -> Result<Design, ErrOperation> {
        if source_nucl.helix == target_nucl.helix {
            return Err(ErrOperation::XoverOnSameHelix);
        }
        println!("cross over between {:?} and {:?}", source_nucl, target_nucl);
        let source_id = design
            .get_strand_nucl(&source_nucl)
            .ok_or(ErrOperation::NuclDoesNotExist(source_nucl))?;
        let target_id = design
            .get_strand_nucl(&target_nucl)
            .ok_or(ErrOperation::NuclDoesNotExist(target_nucl))?;

        let source = design
            .strands
            .get(&source_id)
            .cloned()
            .ok_or(ErrOperation::StrandDoesNotExist(source_id))?;
        let target = design
            .strands
            .get(&target_id)
            .cloned()
            .ok_or(ErrOperation::StrandDoesNotExist(target_id))?;

        let source_strand_end = design.is_strand_end(&source_nucl);
        let target_strand_end = design.is_strand_end(&target_nucl);
        println!(
            "source strand {:?}, target strand {:?}",
            source_id, target_id
        );
        println!(
            "source end {:?}, target end {:?}",
            source_strand_end.to_opt(),
            target_strand_end.to_opt()
        );
        match (source_strand_end.to_opt(), target_strand_end.to_opt()) {
            (Some(true), Some(true)) => return Err(ErrOperation::XoverBetweenTwoPrime3),
            (Some(false), Some(false)) => return Err(ErrOperation::XoverBetweenTwoPrime5),
            (Some(true), Some(false)) => {
                // We can xover directly
                if source_id == target_id {
                    Self::make_cycle(&mut design, source_id, true)?
                } else {
                    Self::merge_strands(&mut design, source_id, target_id)?
                }
            }
            (Some(false), Some(true)) => {
                // We can xover directly but we must reverse the xover
                if source_id == target_id {
                    Self::make_cycle(&mut design, target_id, true)?
                } else {
                    Self::merge_strands(&mut design, target_id, source_id)?
                }
            }
            (Some(b), None) => {
                // We can cut cross directly, but only if the target and source's helices are
                // different
                let target_3prime = b;
                if source_nucl.helix != target_nucl.helix {
                    Self::cross_cut(
                        &mut design,
                        source_id,
                        target_id,
                        target_nucl,
                        target_3prime,
                    )?
                }
            }
            (None, Some(b)) => {
                // We can cut cross directly but we need to reverse the xover
                let target_3prime = b;
                if source_nucl.helix != target_nucl.helix {
                    Self::cross_cut(
                        &mut design,
                        target_id,
                        source_id,
                        source_nucl,
                        target_3prime,
                    )?
                }
            }
            (None, None) => {
                if source_nucl.helix != target_nucl.helix {
                    if source_id != target_id {
                        Self::split_strand(&mut design, &source_nucl, None)?;
                        Self::cross_cut(&mut design, source_id, target_id, target_nucl, true)?;
                    } else if source.cyclic {
                        Self::split_strand(&mut design, &source_nucl, Some(false))?;
                        Self::cross_cut(&mut design, source_id, target_id, target_nucl, true)?;
                    } else {
                        // if the two nucleotides are on the same strand care must be taken
                        // because one of them might be on the newly crated strand after the
                        // split
                        let pos1 = source
                            .find_nucl(&source_nucl)
                            .ok_or(ErrOperation::NuclDoesNotExist(source_nucl))?;
                        let pos2 = source
                            .find_nucl(&target_nucl)
                            .ok_or(ErrOperation::NuclDoesNotExist(target_nucl))?;
                        if pos1 > pos2 {
                            // the source nucl will be on the 5' end of the split and the
                            // target nucl as well
                            Self::split_strand(&mut design, &source_nucl, Some(false))?;
                            Self::cross_cut(&mut design, source_id, target_id, target_nucl, true)?;
                        } else {
                            let new_id =
                                Self::split_strand(&mut design, &source_nucl, Some(false))?;
                            Self::cross_cut(&mut design, source_id, new_id, target_nucl, true)?;
                        }
                    }
                }
            }
        }
        Ok(design)
    }
}

fn nucl_pos_2d(design: &Design, nucl: &Nucl) -> Option<Vec2> {
    let local_position = nucl.position as f32 * Vec2::unit_x()
        + if nucl.forward {
            Vec2::zero()
        } else {
            Vec2::unit_y()
        };
    let isometry = design.helices.get(&nucl.helix).and_then(|h| h.isometry2d);

    isometry.map(|i| i.into_homogeneous_matrix().transform_point2(local_position))
}

#[derive(Clone)]
enum ControllerState {
    Normal,
    MakingHyperboloid,
    BuildingStrand {
        builders: Vec<StrandBuilder>,
        initial_design: AddressPointer<Design>,
        initializing: bool,
    },
    ChangingColor,
    WithPendingOp(Arc<dyn Operation>),
    ApplyingOperation {
        design: AddressPointer<Design>,
        operation: Option<Arc<dyn Operation>>,
    },
    PositioningPastingPoint {
        pasting_point: Option<Nucl>,
        pasted_strands: Vec<PastedStrand>,
    },
    PositioningDuplicationPoint {
        pasting_point: Option<Nucl>,
        pasted_strands: Vec<PastedStrand>,
        duplication_edge: Option<(Edge, isize)>,
        clipboard: StrandClipboard,
    },
    WithPendingDuplication {
        last_pasting_point: Nucl,
        duplication_edge: (Edge, isize),
        clipboard: StrandClipboard,
    },
}

impl Default for ControllerState {
    fn default() -> Self {
        Self::Normal
    }
}

impl ControllerState {
    fn update_pasting_position(
        &mut self,
        point: Option<Nucl>,
        strands: Vec<PastedStrand>,
        duplication_edge: Option<(Edge, isize)>,
    ) -> Result<(), ErrOperation> {
        match self {
            Self::PositioningPastingPoint { .. } | Self::Normal | Self::WithPendingOp(_) => {
                *self = Self::PositioningPastingPoint {
                    pasting_point: point,
                    pasted_strands: strands,
                };
                Ok(())
            }
            Self::PositioningDuplicationPoint { clipboard, .. } => {
                *self = Self::PositioningDuplicationPoint {
                    pasting_point: point,
                    pasted_strands: strands,
                    duplication_edge,
                    clipboard: clipboard.clone(),
                };
                Ok(())
            }
            _ => Err(ErrOperation::IncompatibleState),
        }
    }
    fn update_operation(&mut self, op: Arc<dyn Operation>) {
        match self {
            Self::ApplyingOperation { operation, .. } => *operation = Some(op),
            Self::WithPendingOp(old_op) => *old_op = op,
            _ => (),
        }
    }

    fn get_operation(&self) -> Option<Arc<dyn Operation>> {
        match self {
            Self::ApplyingOperation { operation, .. } => operation.clone(),
            Self::WithPendingOp(op) => Some(op.clone()),
            _ => None,
        }
    }

    fn finish(&self) -> Self {
        if let Some(op) = self.get_operation() {
            Self::WithPendingOp(op)
        } else {
            Self::Normal
        }
    }
}

pub enum InteractorNotification {
    FinishOperation,
}

use ensnano_design::HelixInterval;
/// Return the appropriate junction between two HelixInterval
pub(super) fn junction(prime5: &HelixInterval, prime3: &HelixInterval) -> DomainJunction {
    let prime5_nucl = prime5.prime3();
    let prime3_nucl = prime3.prime5();

    if prime3_nucl == prime5_nucl.prime3() {
        DomainJunction::Adjacent
    } else {
        DomainJunction::UnindentifiedXover
    }
}
