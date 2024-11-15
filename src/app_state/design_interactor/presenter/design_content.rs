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

use super::*;
use crate::scene::GridInstance;
use ahash::RandomState;
use cadnano_format::color;
use ensnano_design::drawing_style::{ColorType, DrawingAttribute, DrawingStyle};
use ensnano_design::elements::{DesignElement, DesignElementKey};
use ensnano_design::grid::{GridId, GridObject, GridPosition, HelixGridPosition};
use ensnano_design::*;
use ensnano_interactor::consts::{
    BOND_RADIUS, CLONE_OPACITY, HELIX_CYLINDER_COLOR, HELIX_CYLINDER_RADIUS, SPHERE_RADIUS,
};
use ensnano_interactor::{
    graphics::{LoopoutBond, LoopoutNucl},
    ObjectType,
};
use ensnano_utils::clic_counter::ClicCounter;
use futures::stream::LocalBoxStream;
use iced::slider::draw;
use iced::Element;
use serde::Serialize;
use std::borrow::Cow;
use std::clone;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::f32::consts::PI;
use std::str::FromStr;
use std::sync::Arc;
use ultraviolet::{Rotor3, Vec3};

use ensnano_design::grid::GridData;

mod xover_suggestions;
use xover_suggestions::XoverSuggestions;

use ensnano_design::isometry3_descriptor::{
    Isometry3Descriptor, Isometry3DescriptorItem, Isometry3MissingMethods,
};
use ensnano_utils::colors;
use ensnano_utils::instance::Instance;

#[derive(Default, Clone)]
pub struct NuclCollection {
    identifier: BTreeMap<Nucl, u32>, //HashMap<Nucl, u32, RandomState>,
    virtual_nucl_map: HashMap<VirtualNucl, Nucl, RandomState>,
}

impl super::NuclCollection for NuclCollection {
    fn iter_nucls_ids<'a>(&'a self) -> Box<dyn Iterator<Item = (&'a Nucl, &'a u32)> + 'a> {
        Box::new(self.identifier.iter())
    }

    fn contains_nucl(&self, nucl: &Nucl) -> bool {
        self.identifier.contains_key(nucl)
    }

    fn iter_nucls<'a>(&'a self) -> Box<dyn Iterator<Item = &'a Nucl> + 'a> {
        Box::new(self.identifier.keys())
    }

    fn virtual_to_real(&self, virtual_nucl: &VirtualNucl) -> Option<&Nucl> {
        self.virtual_nucl_map.get(virtual_nucl)
    }
}

impl NuclCollection {
    pub fn get_identifier(&self, nucl: &Nucl) -> Option<&u32> {
        self.identifier.get(nucl)
    }

    pub fn contains_nucl(&self, nucl: &Nucl) -> bool {
        self.identifier.contains_key(nucl)
    }

    pub fn nb_nucls(&self) -> usize {
        self.identifier.len()
    }

    fn insert(&mut self, key: Nucl, id: u32) -> Option<u32> {
        self.identifier.insert(key, id)
    }

    fn insert_virtual(&mut self, virtual_nucl: VirtualNucl, nucl: Nucl) -> Option<Nucl> {
        self.virtual_nucl_map.insert(virtual_nucl, nucl)
    }
}

#[derive(Default, Clone)]
pub(super) struct DesignContent {
    /// Maps identifer of elements to their object type
    pub object_type: HashMap<u32, ObjectType, RandomState>,
    /// Maps identifier of nucleotide to Nucleotide objects
    pub nucleotide: HashMap<u32, Nucl, RandomState>,
    /// Maps identifier of bonds to the pair of nucleotides involved in the bond
    pub nucleotides_involved: HashMap<u32, (Nucl, Nucl), RandomState>,
    /// Maps identifier of element to their position in the Model's coordinates
    pub space_position: HashMap<u32, [f32; 3], RandomState>,
    /// Maps identifier of nucl element to their axis position in the Model's coordinates
    pub axis_space_position: HashMap<u32, [f32; 3], RandomState>,
    /// Maps identifier of nucl element to whether the element is on axis or not in the Model's coordinates
    pub on_axis: HashMap<u32, bool, RandomState>,
    /// Maps a Nucl object to its identifier
    pub nucl_collection: Arc<NuclCollection>,
    /// Maps a pair of nucleotide forming a bond to the identifier of the bond
    pub identifier_bond: HashMap<(Nucl, Nucl), u32, RandomState>,
    /// Maps the identifier of a element to the identifier of the strands to which it belongs
    pub strand_map: HashMap<u32, usize, RandomState>,
    /// Maps the identifier of a element to the identifier of the helix to which it belongs
    pub helix_map: HashMap<u32, usize, RandomState>,
    /// Maps the identifier of an element to its color
    pub color_map: HashMap<u32, u32, RandomState>,
    /// Maps the identifier of an element to its radius
    pub radius_map: HashMap<u32, f32, RandomState>,
    /// Maps the identifier of a bond element to its helix radius (if ≠ None)
    pub helix_radius_map: HashMap<u32, f32, RandomState>,
    pub helix_color_map: HashMap<u32, u32, RandomState>,
    pub letter_map: Arc<HashMap<Nucl, char, RandomState>>,
    pub prime3_set: Vec<Prime3End>,
    pub elements: Vec<DesignElement>,
    pub suggestions: Vec<(Nucl, Nucl)>,
    pub(super) grid_manager: GridData,
    pub loopout_nucls: Vec<LoopoutNucl>,
    pub loopout_bonds: Vec<LoopoutBond>,
    /// Maps bonds identifier to the length of the corresponding insertion.
    pub insertion_length: HashMap<u32, usize, RandomState>,
    /// Maps identifiers to drawingstyles
    pub drawing_styles: HashMap<DesignElementKey, DrawingStyle, RandomState>,
    /// Maps identifiers to drawingstyles
    pub xover_coloring_map: HashMap<u32, bool, RandomState>,
    pub clone_transformations: Vec<Isometry3>,
    pub with_cones_map: HashMap<u32, bool, RandomState>,
    // min value, max value and rainow function(t, min, max)->color
    pub scalebar: Option<(f32, f32, fn(f32, f32, f32) -> u32)>,
}

impl DesignContent {
    pub(super) fn get_grid_instances(&self) -> BTreeMap<GridId, GridInstance> {
        self.grid_manager.grid_instances(0)
    }

    pub(super) fn get_helices_on_grid(&self, g_id: GridId) -> Option<HashSet<usize>> {
        self.grid_manager.get_helices_on_grid(g_id)
    }
    /// Return the position of an element.
    /// If the element is a nucleotide, return the center of the nucleotide.
    /// If the element is a bond, return the middle of the segment between the two nucleotides
    /// involved in the bond.
    pub(super) fn get_element_position(&self, id: u32) -> Option<Vec3> {
        if let Some(object_type) = self.object_type.get(&id) {
            match object_type {
                ObjectType::Nucleotide(id) => self.space_position.get(&id).map(|x| x.into()),
                ObjectType::Bond(e1, e2) | ObjectType::SlicedBond(_, e1, e2, _) => {
                    let a = self.space_position.get(e1)?;
                    let b = self.space_position.get(e2)?;
                    Some((Vec3::from(*a) + Vec3::from(*b)) / 2.)
                }
                ObjectType::HelixCylinder(e1, e2) | ObjectType::ColoredHelixCylinder(e1, e2, _) => {
                    let a = self.axis_space_position.get(e1)?;
                    let b = self.axis_space_position.get(e2)?;
                    Some((Vec3::from(*a) + Vec3::from(*b)) / 2.)
                }
            }
        } else {
            None
        }
    }

    pub(super) fn get_element_graphic_position(&self, id: u32) -> Option<Vec3> {
        if let Some(true) = self.on_axis.get(&id) {
            // println!("{id} on axis");
            return self.get_element_axis_position(id);
        }
        return self.get_element_position(id);
    }

    pub(super) fn get_element_axis_position(&self, id: u32) -> Option<Vec3> {
        if let Some(object_type) = self.object_type.get(&id) {
            match object_type {
                ObjectType::Nucleotide(id) => self.axis_space_position.get(&id).map(|x| x.into()),
                ObjectType::Bond(e1, e2)
                | ObjectType::HelixCylinder(e1, e2)
                | ObjectType::ColoredHelixCylinder(e1, e2, _)
                | ObjectType::SlicedBond(_, e1, e2, _) => {
                    let a = self.axis_space_position.get(e1)?;
                    let b = self.axis_space_position.get(e2)?;
                    Some((Vec3::from(*a) + Vec3::from(*b)) / 2.)
                }
            }
        } else {
            None
        }
    }

    pub(super) fn get_helix_grid_position(&self, h_id: usize) -> Option<HelixGridPosition> {
        self.grid_manager.get_helix_grid_position(h_id)
    }

    pub(super) fn get_grid_latice_position(&self, position: GridPosition) -> Option<Vec3> {
        let grid = self.grid_manager.grids.get(&position.grid)?;
        Some(grid.position_helix(position.x, position.y))
    }

    /// Return a list of pairs ((x, y), h_id) of all the used helices on the grid g_id
    pub(super) fn get_helices_grid_key_coord(&self, g_id: GridId) -> Vec<((isize, isize), usize)> {
        self.grid_manager.get_helices_grid_key_coord(g_id)
    }

    pub(super) fn get_used_coordinates_on_grid(&self, g_id: GridId) -> Vec<(isize, isize)> {
        self.grid_manager.get_used_coordinates_on_grid(g_id)
    }

    pub(super) fn get_helix_id_at_grid_coord(&self, position: GridPosition) -> Option<usize> {
        self.grid_manager
            .pos_to_object(position)
            .map(|obj| obj.helix())
    }

    pub(super) fn get_persistent_phantom_helices_id(&self) -> HashSet<u32> {
        self.grid_manager.get_persistent_phantom_helices_id()
    }

    pub(super) fn grid_has_small_spheres(&self, g_id: GridId) -> bool {
        self.grid_manager.small_spheres.contains(&g_id)
    }

    pub(super) fn grid_has_persistent_phantom(&self, g_id: GridId) -> bool {
        !self.grid_manager.no_phantoms.contains(&g_id)
    }

    pub(super) fn get_grid_nb_turn(&self, g_id: GridId) -> Option<f32> {
        self.grid_manager
            .grids
            .get(&g_id)
            .and_then(|g| g.grid_type.get_nb_turn().map(|x| x as f32))
    }

    pub(super) fn get_grid_shift(&self, g_id: GridId) -> Option<f32> {
        self.grid_manager
            .grids
            .get(&g_id)
            .and_then(|g| g.grid_type.get_shift())
    }

    pub(super) fn get_staple_mismatch(&self, design: &Design) -> Option<Nucl> {
        let basis_map = self.letter_map.as_ref();
        for strand in design.strands.values() {
            for domain in &strand.domains {
                if let Domain::HelixDomain(dom) = domain {
                    for position in dom.iter() {
                        let nucl = Nucl {
                            position,
                            forward: dom.forward,
                            helix: dom.helix,
                        };
                        if !basis_map.contains_key(&nucl) {
                            return Some(nucl);
                        }
                    }
                }
            }
        }
        None
    }

    pub(super) fn get_grid_object(&self, position: GridPosition) -> Option<GridObject> {
        self.grid_manager.pos_to_object(position)
    }

    pub(super) fn get_staples(&self, design: &Design, presenter: &Presenter) -> Vec<Staple> {
        let mut ret = Vec::new();
        let mut sequences: BTreeMap<(Vec<String>, usize, isize, usize, isize), StapleInfo> =
            Default::default();
        let basis_map = self.letter_map.as_ref();
        for (s_id, strand) in design.strands.iter() {
            if strand.length() == 0 || design.scaffold_id == Some(*s_id) {
                // skip zero length staples and scaffold
                continue;
            }
            let mut sequence = String::new();
            let mut first = true;
            let mut previous_char_is_basis = None;
            let mut intervals = StapleIntervals {
                staple_id: *s_id,
                intervals: Vec::new(),
            };
            for domain in &strand.domains {
                let mut staple_domain = None;
                let scaffold = design.scaffold_id.and_then(|id| design.strands.get(&id));
                if !first {
                    sequence.push(' ');
                }
                let helices = &design.helices;
                first = false;
                if let Domain::HelixDomain(dom) = domain {
                    for position in dom.iter() {
                        let nucl = Nucl {
                            position,
                            forward: dom.forward,
                            helix: dom.helix,
                        };

                        let next_basis = basis_map.get(&nucl);
                        if let Some(basis) = next_basis {
                            if previous_char_is_basis == Some(false) {
                                sequence.push(' ');
                            }
                            sequence.push(*basis);
                            previous_char_is_basis = Some(true);
                        } else {
                            if previous_char_is_basis == Some(true) {
                                sequence.push(' ');
                            }
                            sequence.push('?');
                            previous_char_is_basis = Some(false);
                        }
                        if let Some(virtual_nucl) = Nucl::map_to_virtual_nucl(nucl, helices) {
                            if let Some(scaffold) = scaffold {
                                let result = scaffold
                                    .locate_virtual_nucl(&virtual_nucl.compl(), helices)
                                    .map(|v| ScaffoldPosition {
                                        domain_id: v.domain_id,
                                        scaffold_position: (v.pos_on_strand + scaffold.length()
                                            - design.scaffold_shift.unwrap_or(0))
                                            % scaffold.length(),
                                    });
                                if staple_domain.is_none() {
                                    staple_domain = Some(StapleDomain::init(result));
                                }
                                let d = staple_domain.take().unwrap();
                                match d.read_position(result) {
                                    ReadResult::Continue(d) => staple_domain = Some(d),
                                    ReadResult::Stop {
                                        interval,
                                        new_reader,
                                    } => {
                                        intervals.intervals.push(interval);
                                        staple_domain = Some(new_reader);
                                    }
                                }
                            }
                        } else {
                            log::error!("Could not map to virtual nucl");
                        }
                    }
                } else if let Domain::Insertion { nb_nucl, .. } = domain {
                    // Number of nucleotides inserted added
                    sequence.push_str(&format!("**INSERTION {nb_nucl}**"))
                }
                if let Some(d) = staple_domain {
                    intervals.intervals.push(d.finish())
                }
            }
            let group_names = presenter.get_name_of_group_having_strand(*s_id);
            let key = if let Some((prim5, prim3)) = strand.get_5prime().zip(strand.get_3prime()) {
                (
                    group_names,
                    prim5.helix,
                    prim5.position,
                    prim3.helix,
                    prim3.position,
                )
            } else {
                log::warn!("WARNING, STAPLE WITH NO KEY !!!");
                (vec![], 0, 0, 0, 0)
            };
            sequences.insert(
                key,
                StapleInfo {
                    s_id: *s_id,
                    sequence,
                    strand_name: strand.name.clone(),
                    domain_decomposition: presenter.decompose_length(*s_id),
                    length: strand.length(),
                    color: strand.color & 0xFFFFFF,
                    group_names: presenter.get_name_of_group_having_strand(*s_id),
                    intervals,
                },
            );
        }
        for (n, ((_, h5, nt5, h3, nt3), staple_info)) in sequences.iter().enumerate() {
            let plate = n / 96 + 1;
            let row = (n % 96) / 8 + 1;
            let column = match (n % 96) % 8 {
                0 => 'A',
                1 => 'B',
                2 => 'C',
                3 => 'D',
                4 => 'E',
                5 => 'F',
                6 => 'G',
                7 => 'H',
                _ => unreachable!(),
            };
            ret.push(Staple {
                plate,
                well: format!("{}{}", column, row.to_string()),
                sequence: staple_info.sequence.clone(),
                name: (if let Some(name) = &staple_info.strand_name {
                    format!("{name} #{}", staple_info.s_id).into()
                } else {
                    format!(
                        "#{:04}; 5':h{}:nt{}>3':h{}:nt{}",
                        staple_info.s_id, *h5, *nt5, *h3, *nt3
                    )
                    .into()
                }),
                color_str: format!("{:#08X}", staple_info.color)
                    .trim_start_matches("0x")
                    .to_string(),
                group_names: staple_info.group_names.clone(),
                group_names_string: staple_info.group_names.join(" ; "),
                length_str: staple_info.length.to_string(),
                domain_decomposition: staple_info
                    .domain_decomposition
                    .split_once("=")
                    .map(|split| split.1.to_string())
                    .unwrap_or(staple_info.domain_decomposition.clone()),
                intervals: staple_info.intervals.clone(),
            });
        }
        ret
    }

    pub fn get_all_visible_nucl_ids(
        &self,
        design: &Design,
        invisible_nucls: &HashSet<Nucl>,
    ) -> Vec<u32> {
        let check_visiblity = |&(_, v): &(&u32, &Nucl)| {
            !invisible_nucls.contains(v)
                && design
                    .helices
                    .get(&v.helix)
                    .map(|h| h.visible)
                    .unwrap_or_default()
        };
        self.nucleotide
            .iter()
            .filter(check_visiblity)
            .map(|t| *t.0)
            .collect()
    }

    pub fn get_all_visible_bonds(
        &self,
        design: &Design,
        invisible_nucls: &HashSet<Nucl>,
    ) -> Vec<u32> {
        let check_visibility = |&(id, bond): &(&u32, &(Nucl, Nucl))| {
            if self.object_type.get(id).unwrap().is_helix_cylinder() {
                return true;
                // !(invisible_nucls.contains(&bond.0)
                // && invisible_nucls.contains(&bond.0.compl())
                // && invisible_nucls.contains(&bond.1)
                // && invisible_nucls.contains(&bond.1.compl()))
                // let visible = design.helices.get(&bond.0.helix).unwrap().visible;
                // visible // Apparently this return always true has the field .visible of an helix is never used...
            } else {
                !(invisible_nucls.contains(&bond.0) && invisible_nucls.contains(&bond.1))
                    && (design
                        .helices
                        .get(&bond.0.helix)
                        .map(|h| h.visible)
                        .unwrap_or_default()
                        || design
                            .helices
                            .get(&bond.1.helix)
                            .map(|h| h.visible)
                            .unwrap_or_default())
            }
        };
        self.nucleotides_involved
            .iter()
            .filter(check_visibility)
            .map(|t| *t.0)
            .collect()
    }

    /// NOT IMPLEMENTED YET
    pub fn get_drawing_style(&self, id: u32) -> DrawingStyle {
        println!("get_drawing_style not implemented yet!");
        let style = DrawingStyle::default();
        return style;
    }
}

#[derive(Debug)]
pub struct Staple {
    pub well: String,
    pub name: Cow<'static, str>,
    pub sequence: String,
    pub plate: usize,
    pub color_str: String,
    pub group_names: Vec<String>,
    pub group_names_string: String,
    pub domain_decomposition: String,
    pub length_str: String,
    pub intervals: StapleIntervals,
}

#[derive(Debug, Serialize, Clone)]
pub struct StapleIntervals {
    pub staple_id: usize,
    pub intervals: Vec<(isize, isize)>,
}

struct StapleInfo {
    s_id: usize,
    sequence: String,
    strand_name: Option<Cow<'static, str>>,
    color: u32,
    group_names: Vec<String>,
    domain_decomposition: String,
    length: usize,
    intervals: StapleIntervals,
}

#[derive(Clone)]
pub struct Prime3End {
    pub nucl: Nucl,
    pub color: u32,
}

impl DesignContent {
    /// Update all the hash maps - called after every edit operation
    pub(super) fn make_hash_maps(
        mut design: Design,
        xover_ids: &JunctionsIds,
        suggestion_parameters: &SuggestionParameters,
    ) -> (Self, Design, JunctionsIds) {
        let groups = design.groups.clone();
        let mut object_type = HashMap::default();
        let mut space_position = HashMap::default();
        let mut axis_space_position = HashMap::default();
        let mut on_axis = HashMap::default();
        let mut nucl_collection = NuclCollection::default();
        let mut identifier_bond = HashMap::default();
        let mut nucleotides_involved = HashMap::default();
        let mut nucleotide = HashMap::default();
        let mut strand_map = HashMap::default();
        let mut color_map = HashMap::default();
        let mut radius_map = HashMap::default();
        let mut helix_radius_map = HashMap::default();
        let mut helix_color_map = HashMap::default();
        let mut helix_map = HashMap::default();
        let mut letter_map = HashMap::default();
        let mut with_cones_map = HashMap::default();
        let mut loopout_bonds = Vec::new();
        let mut loopout_nucls = Vec::new();
        let mut id_TMP = 0u32;
        let mut id_clic_counter = ClicCounter::new();
        let mut nucl_id;
        let mut prev_nucl: Option<Nucl> = None;
        let mut prev_nucl_id: Option<u32> = None;
        let mut elements = Vec::new();
        let mut prime3_set = Vec::new();
        let mut new_junctions: JunctionsIds = Default::default();
        let mut suggestion_maker = XoverSuggestions::default();
        let mut insertion_length = HashMap::default();
        let mut drawing_styles = HashMap::default();
        let mut xover_coloring_map = HashMap::default();
        let mut clone_transformations = Vec::new();
        let mut clone_variables: HashMap<String, f32> = HashMap::new();
        let mut scalebar: Option<(f32, f32, fn(f32, f32, f32) -> u32)> = None;

        xover_ids.copy_next_id_to(&mut new_junctions);
        let rainbow_strand = design.scaffold_id.filter(|_| design.rainbow_scaffold);
        let grid_manager = design.get_updated_grid_data().clone();

        // Build drawing style map from organizer tree
        if let Some(ref t) = design.organizer_tree {
            // Read drawing style
            let prefix = "style:"; // PREFIX SHOULD BELONG TO CONST.RS
            let h = t.get_hashmap_to_all_groupnames_with_prefix(prefix);
            for (e, names) in h {
                let drawing_attributes = names
                    .iter()
                    .map(|x| {
                        x.split(&[' ', ':'])
                            .map(|x| DrawingAttribute::from_str(x))
                            .filter(|x| x.is_ok())
                            .map(|x| x.unwrap())
                            .collect::<Vec<DrawingAttribute>>()
                    })
                    .flatten()
                    .collect::<Vec<DrawingAttribute>>();
                let mut style = DrawingStyle::from(drawing_attributes);
                drawing_styles.insert(e, style);
            }

            // collect all the variables defined in the organizer tree - these variables can only be used in the cloning transformations
            let all_group_names = t.get_names_of_all_groups_without_id();
            let clone_variables_declaration = &all_group_names
                .iter()
                .filter(|g| g.starts_with("vars:"))
                .map(|x| x[5..].split(&[' ', ',']).filter(|y| *y != ""))
                .flatten()
                .collect::<Vec<&str>>();
            println!("{:?}", clone_variables_declaration);
            for x in clone_variables_declaration {
                let _s = x.split('=').filter(|y| *y != "").collect::<Vec<&str>>();
                if _s.len() == 2 {
                    if let Ok(value) = f32::from_str(_s[1]) {
                        clone_variables.insert(_s[0].to_string(), value);
                    }
                }
            }

            // collect cloning operations from the organizer tree - these are globally applied regardless of the content of the groups
            clone_transformations = all_group_names
                .iter()
                .filter(|g| g.starts_with("clone:"))
                .map(|s| Isometry3::from_str_with_variables(&s[6..], Some(&clone_variables)))
                .collect::<Vec<Isometry3>>();
        }

        // Scanning strands
        for (s_id, strand) in design.strands.iter_mut() {
            elements.push(elements::DesignElement::StrandElement {
                id: *s_id, // the key in design.strands btreemap
                length: strand.length(),
                domain_lengths: strand.domain_lengths(),
            });
            let parameters = design.helix_parameters.unwrap_or_default();
            strand.update_insertions(&design.helices, &parameters);
            let mut strand_position = 0;
            let strand_seq = strand.sequence.as_ref().filter(|s| s.is_ascii());
            let strand_color = strand.color;

            // Compute strand drawing style
            let strand_style = drawing_styles
                .get(&DesignElementKey::Strand(*s_id))
                .unwrap_or(&DrawingStyle::default())
                .clone()
                .complete_with_attributes(vec![
                    DrawingAttribute::SphereColor(strand_color), // strand color gets after color in strand style
                    DrawingAttribute::BondColor(strand_color), // strand color gets after color in strand style
                ]);

            // Compute the length for rainbow coloring
            let rainbow_len =
                if Some(*s_id) == rainbow_strand || Some(true) == strand_style.rainbow_strand {
                    strand.length()
                } else {
                    0
                };
            // If the strand is not the rainbow strand, the rainbow iterator will be empty and the
            // real strand color will be used.
            let mut rainbow_iterator = (0..rainbow_len).map(|i| {
                let hsv = color_space::Hsv::new(i as f64 * 360. / rainbow_len as f64, 1., 1.);
                let hsv = color_space::Hsv::new(i as f64 * 360. / rainbow_len as f64, 1., 1.);
                let rgb = color_space::Rgb::from(hsv);
                (0xFF << 24) | ((rgb.r as u32) << 16) | ((rgb.g as u32) << 8) | (rgb.b as u32)
                // colors::purple_to_blue_gradient_color(i as f32 / rainbow_len as f32)
            });

            // the sequence of bond ids
            let mut bond_ids_sequence = Vec::new();

            // Iter on the domains of the strand
            let mut last_xover_junction: Option<&mut DomainJunction> = None;
            let mut prev_loopout_pos = None;
            let mut prev_style = strand_style; // style of the previous domain, only used for cyclic strand outside the domain loop
            let bond_coloring = strand_style.xover_coloring.unwrap_or(true);

            let strand_on_axis = strand_style.on_axis.unwrap_or(false);
            for (i, domain) in strand.domains.iter().enumerate() {
                // Update junctions if xover or not
                if let Some((prime5, prime3)) = prev_nucl.clone().zip(domain.prime5_end()) {
                    Self::update_junction(
                        &mut new_junctions,
                        *last_xover_junction
                            .as_mut()
                            .expect("Broke Invariant [LastXoverJunction]"),
                        (prime5, prime3),
                    );
                    if let Some(id) = xover_ids.get_id(&(prime5, prime3)) {
                        // THIS IS A XOVER INTERNAL -> take action
                        elements.push(DesignElement::CrossOverElement {
                            xover_id: id,
                            helix5prime: prime5.helix,
                            position5prime: prime5.position,
                            forward5prime: prime5.forward,
                            helix3prime: prime3.helix,
                            position3prime: prime3.position,
                            forward3prime: prime3.forward,
                        });
                    }
                }

                // Real domain or Insertion
                if let Domain::HelixDomain(domain) = domain {
                    // Real helix domain
                    // Compute domain style
                    let mut domain_style = strand_style;
                    // - domain style completed with helix style
                    if let Some(helix_style) =
                        drawing_styles.get(&DesignElementKey::Helix(domain.helix))
                    {
                        domain_style = domain_style.complete_with(helix_style);
                    }
                    // - domain style completed with grid style if there is a grid
                    if let Some(grid_position) = grid_manager.get_helix_grid_position(domain.helix)
                    {
                        if let GridId::FreeGrid(h_id) = grid_position.grid {
                            if let Some(grid_style) =
                                drawing_styles.get(&DesignElementKey::Grid(h_id))
                            {
                                domain_style = domain_style.complete_with(grid_style);
                            }
                        }
                    }
                    // Get the drawing parameters
                    let bond_radius = domain_style.bond_radius.unwrap_or(BOND_RADIUS);
                    let nucl_radius = domain_style.sphere_radius.unwrap_or(SPHERE_RADIUS);
                    prev_style = domain_style;
                    // Get the sequence if any
                    let dom_seq = domain.sequence.as_ref().filter(|s| s.is_ascii());

                    // Iterate along the domain
                    for (dom_position, nucl_position) in domain.iter().enumerate() {
                        let axis_position = {
                            let p = design.helices.get(&domain.helix).unwrap().axis_position(
                                design.helix_parameters.as_ref().unwrap(),
                                nucl_position,
                                domain.forward,
                            );
                            [p.x as f32, p.y as f32, p.z as f32]
                        };
                        let position = design.helices.get(&domain.helix).unwrap().space_pos(
                            design.helix_parameters.as_ref().unwrap(),
                            nucl_position,
                            domain.forward,
                        );
                        let nucl: Nucl = Nucl {
                            position: nucl_position,
                            forward: domain.forward,
                            helix: domain.helix,
                        };
                        let virtual_nucl = Nucl::map_to_virtual_nucl(nucl, &design.helices);
                        if let Some(v_nucl) = virtual_nucl {
                            let previous = nucl_collection.insert_virtual(v_nucl, nucl);
                            if previous.is_some() && previous != Some(nucl) {
                                log::error!("NUCLEOTIDE CONFLICTS: nucls {:?} and {:?} are mapped to the same virtual position {:?}", previous, nucl, v_nucl);
                            }
                        } else {
                            log::error!("Could not get virtual nucl corresponding to {:?}", nucl);
                        }

                        elements.push(DesignElement::NucleotideElement {
                            helix: nucl.helix,
                            position: nucl.position,
                            forward: nucl.forward,
                        });

                        let rainbow_color = rainbow_iterator.next();
                        let bond_color = rainbow_color.unwrap_or(domain_style.bond_color.unwrap());

                        let nucl_color =
                            rainbow_color.unwrap_or(domain_style.sphere_color.unwrap());
                        if let Some(prev_pos) = prev_loopout_pos.take() {
                            loopout_bonds.push(LoopoutBond {
                                position_prime5: prev_pos,
                                position_prime3: position.into(),
                                color: bond_color,
                                repr_bond_identifier: id_TMP,
                            });
                        }
                        nucl_id = if let Some(prev_nucl) = prev_nucl {
                            let bond_id = id_TMP;
                            id_TMP += 1;
                            let bond = (prev_nucl, nucl);
                            object_type
                                .insert(bond_id, ObjectType::Bond(prev_nucl_id.unwrap(), id_TMP)); // To be overwritten by a sliced bond later
                            bond_ids_sequence.push(bond_id);
                            identifier_bond.insert(bond, bond_id);
                            nucleotides_involved.insert(bond_id, bond);
                            color_map.insert(bond_id, bond_color); // color given to the bond
                            radius_map.insert(bond_id, bond_radius); // radius given to the bond
                            strand_map.insert(bond_id, *s_id); // get strand_id from bond_id
                            helix_map.insert(bond_id, nucl.helix); // get helix_id from bond_id
                            xover_coloring_map.insert(bond_id, bond_coloring);
                            if Some(false) == strand_style.with_cones {
                                with_cones_map.insert(bond_id, false);
                            }
                            id_TMP
                        } else {
                            id_TMP
                        };
                        id_TMP += 1;
                        object_type.insert(nucl_id, ObjectType::Nucleotide(nucl_id));
                        nucleotide.insert(nucl_id, nucl);
                        nucl_collection.insert(nucl, nucl_id);
                        strand_map.insert(nucl_id, *s_id); // get the strand_id from the nucl_id
                        color_map.insert(nucl_id, nucl_color);
                        radius_map.insert(nucl_id, nucl_radius); // radius given to the bond
                        helix_map.insert(nucl_id, nucl.helix); // get helix_id from bond_id

                        let letter = dom_seq
                            .as_ref()
                            .and_then(|s| s.as_bytes().get(dom_position))
                            .or_else(|| {
                                strand_seq
                                    .as_ref()
                                    .and_then(|s| s.as_bytes().get(strand_position))
                            });
                        if let Some(letter) = letter {
                            letter_map.insert(nucl, *letter as char);
                        } else {
                            letter_map.remove(&nucl);
                        }
                        strand_position += 1;
                        suggestion_maker.add_nucl(nucl, position, groups.as_ref());
                        let position = [position[0] as f32, position[1] as f32, position[2] as f32];
                        space_position.insert(nucl_id, position);
                        axis_space_position.insert(nucl_id, axis_position);
                        if strand_on_axis {
                            on_axis.insert(nucl_id, true);
                        }
                        prev_nucl = Some(nucl);
                        prev_nucl_id = Some(nucl_id);
                    }
                    if strand.junctions.len() <= i {
                        log::debug!("{:?}", strand.junctions);
                    }
                    last_xover_junction = Some(&mut strand.junctions[i]);
                } else if let Domain::Insertion {
                    nb_nucl,
                    instanciation,
                    sequence: dom_seq,
                    ..
                } = domain
                {
                    if let Some(instanciation) = instanciation.as_ref() {
                        for (dom_position, pos) in instanciation.as_ref().pos().iter().enumerate() {
                            let color = rainbow_iterator.next().unwrap_or(strand_color);
                            let basis = dom_seq
                                .as_ref()
                                .and_then(|s| s.as_bytes().get(dom_position))
                                .or_else(|| {
                                    strand_seq
                                        .as_ref()
                                        .and_then(|s| s.as_bytes().get(strand_position))
                                });
                            loopout_nucls.push(LoopoutNucl {
                                position: *pos,
                                color,
                                repr_bond_identifier: id_TMP,
                                basis: basis.and_then(|b| b.clone().try_into().ok()),
                            });
                            if let Some(prev_pos) =
                                prev_loopout_pos.take().or(prev_nucl_id
                                    .and_then(|id| space_position.get(&id).map(Vec3::from)))
                            {
                                loopout_bonds.push(LoopoutBond {
                                    position_prime5: prev_pos,
                                    position_prime3: *pos,
                                    color,
                                    repr_bond_identifier: id_TMP,
                                });
                            }
                            prev_loopout_pos = Some(*pos);
                            strand_position += 1;
                        }
                    }
                    insertion_length.insert(id_TMP, *nb_nucl);
                    last_xover_junction = Some(&mut strand.junctions[i]);
                }
            }
            if strand.is_cyclic {
                let nucl = strand.get_5prime().unwrap();
                let prime5_id = nucl_collection.get_identifier(&nucl).unwrap();
                let bond_id = id_TMP;
                if let Some((prev_pos, position)) =
                    prev_loopout_pos.take().zip(space_position.get(&prime5_id))
                {
                    loopout_bonds.push(LoopoutBond {
                        position_prime5: prev_pos,
                        position_prime3: position.into(),
                        color: strand_color,
                        repr_bond_identifier: id_TMP,
                    });
                }
                id_TMP += 1;
                let bond = (prev_nucl.unwrap(), nucl);
                object_type.insert(bond_id, ObjectType::Bond(prev_nucl_id.unwrap(), *prime5_id)); // to be overwritten by a sliced bond later
                bond_ids_sequence.push(bond_id);
                identifier_bond.insert(bond, bond_id);
                nucleotides_involved.insert(bond_id, bond);
                color_map.insert(bond_id, strand_color);
                radius_map.insert(bond_id, prev_style.bond_radius.unwrap_or(BOND_RADIUS)); // radius given to the bond
                strand_map.insert(bond_id, *s_id); // match bond_id to strand_id
                helix_map.insert(bond_id, nucl.helix);
                xover_coloring_map.insert(bond_id, bond_coloring);
                if Some(false) == strand_style.with_cones {
                    with_cones_map.insert(bond_id, false);
                }

                log::debug!("adding {:?}, {:?}", bond.0, bond.1);
                Self::update_junction(
                    &mut new_junctions,
                    strand
                        .junctions
                        .last_mut()
                        .expect("Broke Invariant [LastXoverJunction]"),
                    (bond.0, bond.1),
                );
                let (prime5, prime3) = bond;
                if let Some(id) = new_junctions.get_id(&(prime5, prime3)) {
                    // Final XOVER DE strand cyclic
                    elements.push(DesignElement::CrossOverElement {
                        xover_id: id,
                        helix5prime: prime5.helix,
                        position5prime: prime5.position,
                        forward5prime: prime5.forward,
                        helix3prime: prime3.helix,
                        position3prime: prime3.position,
                        forward3prime: prime3.forward,
                    });
                }
            } else {
                if let Some(len) = insertion_length.remove(&id_TMP) {
                    insertion_length.insert(id_TMP - 1, len);
                    for loopout_nucl in loopout_nucls.iter_mut() {
                        if loopout_nucl.repr_bond_identifier == id_TMP {
                            loopout_nucl.repr_bond_identifier = id_TMP - 1;
                        }
                    }
                    for loopout_bond in loopout_bonds.iter_mut() {
                        if loopout_bond.repr_bond_identifier == id_TMP {
                            loopout_bond.repr_bond_identifier = id_TMP - 1;
                        }
                    }
                }
                if let Some(nucl) = prev_nucl {
                    let color = strand.color;
                    prime3_set.push(Prime3End { nucl, color });
                }
            }

            // Set the sliced bonds properly by adding the prev and next nucleotides
            let nucl1_ids = bond_ids_sequence
                .iter()
                .map(|x| {
                    let ObjectType::Bond(id1, _) = object_type.get(x).unwrap() else {
                        panic!("The bond is not a bond");
                    };
                    *id1
                })
                .collect::<Vec<u32>>();
            let nucl2_ids = bond_ids_sequence
                .iter()
                .map(|x| {
                    let ObjectType::Bond(_, id2) = object_type.get(x).unwrap() else {
                        panic!("The bond is not a bond");
                    };
                    *id2
                })
                .collect::<Vec<u32>>();
            if bond_ids_sequence.len() > 0 {
                let n = bond_ids_sequence.len();
                for ((prev_id, bond_id), next_id) in nucl1_ids
                    .iter()
                    .cycle()
                    .skip(n - 1)
                    .zip(bond_ids_sequence.clone())
                    .zip(nucl2_ids.iter().cycle().skip(1))
                {
                    let ObjectType::Bond(id1, id2) = object_type.get(&bond_id).unwrap() else {
                        panic!("The bond is not a bond");
                    };

                    object_type.insert(
                        bond_id,
                        ObjectType::SlicedBond(*prev_id, *id1, *id2, *next_id),
                    );
                }
                if !strand.is_cyclic {
                    // modify the first bond to repeat the first nucl_id
                    let first_id = bond_ids_sequence[0];
                    let ObjectType::SlicedBond(_, id1, id2, next_id) =
                        object_type.get(&first_id).unwrap()
                    else {
                        unreachable!("The sliced bond is not a sliced bond");
                    };
                    object_type
                        .insert(first_id, ObjectType::SlicedBond(*id1, *id1, *id2, *next_id));
                    // modify the last bond to repeat the second nucl_id
                    let last_id = bond_ids_sequence[n - 1];
                    let ObjectType::SlicedBond(prev_id, id1, id2, _) =
                        object_type.get(&last_id).unwrap()
                    else {
                        unreachable!("The sliced bond is not a sliced bond");
                    };
                    object_type.insert(last_id, ObjectType::SlicedBond(*prev_id, *id1, *id2, *id2));
                }
            }
            // next iteration
            prev_nucl = None;
            prev_nucl_id = None;
        } // Scanning strands

        // Scanning grids
        for g_id in grid_manager.grids.keys() {
            if let GridId::FreeGrid(id) = g_id {
                elements.push(DesignElement::GridElement {
                    id: *id,
                    visible: grid_manager.get_visibility(*g_id),
                })
            }
        }

        for (h_id, h) in design.helices.iter() {
            elements.push(DesignElement::HelixElement {
                id: *h_id,
                group: groups.get(h_id).cloned(),
                visible: h.visible,
                locked_for_simulations: h.locked_for_simulations,
            });
        }

        // Make the helices tubes
        if nucl_collection.identifier.len() > 0 {
            let all_nt = nucl_collection
                .identifier
                .keys()
                .map(|x| x.clone())
                .collect::<Vec<Nucl>>();

            let all_forward_nt = all_nt
                .iter()
                .filter(|x| x.forward)
                .map(|x| (x.helix, x.position))
                .collect::<Vec<(usize, isize)>>();

            let all_backward_nt = all_nt
                .iter()
                .filter(|x| !x.forward)
                .map(|x| (x.helix, x.position))
                .rev()
                .collect::<Vec<(usize, isize)>>();

            let mut hash_f = BTreeMap::new();
            for (h, i) in all_forward_nt.into_iter() {
                let mut a = hash_f.get(&h).unwrap_or(&Vec::<isize>::new()).clone();
                a.push(i);
                hash_f.insert(h, a);
            }
            let mut hash_b = BTreeMap::new();
            for (h, i) in all_backward_nt.into_iter() {
                let mut a = hash_b.get(&h).unwrap_or(&Vec::<isize>::new()).clone();
                a.push(i);
                hash_b.insert(h, a);
            }
            let mut hash_intersection = HashMap::<usize, Vec<isize>>::new();
            for (h, f) in hash_f {
                if let Some(b) = hash_b.get(&h) {
                    let mut inter = Vec::new();
                    let mut i_f = f.into_iter();
                    let mut i_b = b.into_iter();
                    let mut last_f = i_f.next();
                    let mut s_f = 0isize;
                    let mut last_b = i_b.next();
                    let mut s_b = 0isize;
                    while !last_b.is_none() && !last_f.is_none() {
                        while let (Some(l_f), Some(l_b)) = (last_f, last_b) {
                            if l_f >= *l_b {
                                break;
                            }
                            last_f = i_f.next();
                        }
                        while let (Some(l_f), Some(l_b)) = (last_f, last_b) {
                            if *l_b >= l_f {
                                break;
                            }
                            last_b = i_b.next();
                        }
                        while let (Some(l_f), Some(l_b)) = (last_f, last_b) {
                            if (l_f != *l_b) {
                                break;
                            }
                            inter.push(*l_b);
                            last_f = i_f.next();
                            last_b = i_b.next();
                        }
                    }
                    if inter.len() > 0 {
                        hash_intersection.insert(h, inter);
                    }
                }
            }

            let mut hash_intervals = HashMap::<usize, Vec<(isize, isize)>>::new();
            for (h, a) in hash_intersection {
                let mut b = Vec::new();
                let mut last_i = None;
                let mut current_start = None;
                for i in a.into_iter() {
                    match last_i {
                        Some(l_i) if l_i + 1 < i => {
                            b.push((current_start.unwrap(), l_i + 1));
                            current_start = Some(i);
                        }
                        None => {
                            current_start = Some(i);
                        }
                        _ => {}
                    }
                    last_i = Some(i);
                }
                if let Some(l_i) = last_i {
                    b.push((current_start.unwrap(), l_i + 1));
                }
                hash_intervals.insert(h, b);
            }

            // DO NOT USE id_TMP beyond this point
            id_clic_counter.set(id_TMP);

            // USE id_clic_counter
            let mut helix_cylinders = Vec::new();
            for (h, a) in hash_intervals {
                let mut helix_style = drawing_styles
                    .get(&DesignElementKey::Helix(h))
                    .unwrap_or(&DrawingStyle::default())
                    .clone();
                if let Some(grid_position) = grid_manager.get_helix_grid_position(h) {
                    if let GridId::FreeGrid(h) = grid_position.grid {
                        if let Some(grid_style) = drawing_styles.get(&DesignElementKey::Grid(h)) {
                            helix_style = helix_style.complete_with(grid_style);
                        }
                    }
                }
                let radius = helix_style
                    .helix_as_cylinder_radius
                    .unwrap_or(HELIX_CYLINDER_RADIUS);
                let color =
                    helix_style
                        .helix_as_cylinder_color
                        .map_or(HELIX_CYLINDER_COLOR, |ct| {
                            if let ColorType::Color(c) = ct {
                                c
                            } else {
                                HELIX_CYLINDER_COLOR
                            }
                        });
                for (i, j) in a {
                    let bond_id = id_clic_counter.next();
                    let n_i = Nucl {
                        helix: h,
                        position: i,
                        forward: true,
                    };
                    let n_i_id = nucl_collection.get_identifier(&n_i).unwrap();
                    let n_j = Nucl {
                        helix: h,
                        position: j - 1,
                        forward: true,
                    };
                    let n_j_id = nucl_collection.get_identifier(&n_j).unwrap();
                    let helix = design.helices.get(&h).unwrap();
                    if helix.curve.is_none() || helix_style.curvature.is_none() {
                        object_type.insert(bond_id, ObjectType::HelixCylinder(*n_i_id, *n_j_id));
                    } else {
                        let (r_min, r_max) = helix_style.curvature.unwrap();
                        scalebar =
                            Some((r_min, r_max, colors::purple_to_blue_gradient_color_in_range));

                        let colors = (i..=j)
                            .map(|n| {
                                let n = if n == j { i } else { n };
                                if let Some(curvature) = helix.curvature_at_pos(n) {
                                    let radius = 1. / curvature;
                                    colors::purple_to_blue_gradient_color_in_range(
                                        radius as f32,
                                        r_min,
                                        r_max,
                                    )
                                } else {
                                    color
                                }
                            })
                            .collect::<Vec<u32>>();
                        object_type.insert(
                            bond_id,
                            ObjectType::ColoredHelixCylinder(*n_i_id, *n_j_id, colors),
                        );
                    }
                    radius_map.insert(bond_id, radius);
                    color_map.insert(bond_id, color);
                    nucleotides_involved.insert(bond_id, (n_i, n_j));
                    helix_map.insert(bond_id, h);
                    helix_cylinders.push(bond_id);
                }
            }

            // Clone - hacked version
            // Get the clone transformations from the file
            if let Some(ref clone_isometries_descriptors) = design.clone_isometries {
                clone_transformations.extend(
                    clone_isometries_descriptors
                        .iter()
                        .map(|d| Isometry3::from_descriptor(d))
                        .collect::<Vec<Isometry3>>(),
                );
            }

            // Cloned Nucleotide
            for isometry3 in &clone_transformations {
                let mut nucleotides_clones = HashMap::new();
                for (nucl, nucl_id) in nucl_collection.identifier.iter() {
                    let clone_nucl_id = id_clic_counter.next();
                    nucleotides_clones.insert(nucl, clone_nucl_id);
                    let nucl_color = color_map.get(nucl_id).unwrap_or(&0);
                    let nucl_radius = radius_map.get(nucl_id).unwrap_or(&0.);
                    let s_id = strand_map.get(nucl_id).unwrap_or(&0);
                    let position = space_position.get(nucl_id).unwrap_or(&[0f32; 3]);
                    let axis_position = axis_space_position.get(nucl_id).unwrap_or(&[0f32; 3]);
                    object_type.insert(clone_nucl_id, ObjectType::Nucleotide(clone_nucl_id));
                    nucleotide.insert(clone_nucl_id, nucl.clone());
                    strand_map.insert(clone_nucl_id, s_id.clone()); // get the strand_id from the nucl_id
                    color_map.insert(
                        clone_nucl_id,
                        Instance::color_au32_with_alpha_scaled_by(*nucl_color, CLONE_OPACITY),
                    );
                    radius_map.insert(clone_nucl_id, *nucl_radius); // radius given to the bond
                    helix_map.insert(clone_nucl_id, nucl.helix); // get helix_id from bond_id

                    let nucl_posi = Vec3::new(position[0], position[1], position[2]);
                    let clone_posi =
                        isometry3.translation.clone() + nucl_posi.rotated_by(isometry3.rotation);
                    space_position
                        .insert(clone_nucl_id, [clone_posi[0], clone_posi[1], clone_posi[2]]);

                    let nucl_axis_posi =
                        Vec3::new(axis_position[0], axis_position[1], axis_position[2]);
                    let clone_axis_posi = isometry3.translation.clone()
                        + nucl_axis_posi.rotated_by(isometry3.rotation);
                    axis_space_position.insert(
                        clone_nucl_id,
                        [clone_axis_posi[0], clone_axis_posi[1], clone_axis_posi[2]],
                    );
                    if let Some(o) = on_axis.get(&nucl_id) {
                        on_axis.insert(clone_nucl_id, *o);
                    }
                }
                // Cloned bonds
                for (bond, bond_id) in &identifier_bond {
                    let clone_bond_id = id_clic_counter.next();
                    let nucl_1_id = nucleotides_clones.get(&bond.0).unwrap();
                    let nucl_1 = nucleotide.get(nucl_1_id).unwrap();
                    let nucl_2_id = nucleotides_clones.get(&bond.1).unwrap();
                    let nucl_2 = nucleotide.get(nucl_2_id).unwrap();
                    let bond_color = color_map.get(&bond_id).unwrap();
                    let clone_bond_color =
                        Instance::color_au32_with_alpha_scaled_by(*bond_color, CLONE_OPACITY);
                    let bond_radius = radius_map.get(&bond_id).unwrap();
                    let strand_id = strand_map.get(&bond_id).unwrap();
                    let helix_id = helix_map.get(&bond_id).unwrap();
                    let xover_coloring = xover_coloring_map.get(&bond_id).unwrap();
                    object_type.insert(clone_bond_id, ObjectType::Bond(*nucl_1_id, *nucl_2_id));
                    nucleotides_involved.insert(clone_bond_id, (*nucl_1, *nucl_2));
                    color_map.insert(clone_bond_id, clone_bond_color);
                    radius_map.insert(clone_bond_id, *bond_radius); // radius given to the bond
                    strand_map.insert(clone_bond_id, *strand_id); // match bond_id to strand_id
                    helix_map.insert(clone_bond_id, *helix_id);
                    xover_coloring_map.insert(clone_bond_id, *xover_coloring);
                    if let Some(o) = on_axis.get(&bond_id) {
                        on_axis.insert(clone_bond_id, *o);
                    }
                    if let Some(wc) = with_cones_map.get(&bond_id) {
                        with_cones_map.insert(clone_bond_id, *wc);
                    }
                }
                // Cloned cylinders
                for bond_id in &helix_cylinders {
                    let clone_bond_id = id_clic_counter.next();
                    let (nucl_1, nucl_2) = nucleotides_involved.get(bond_id).unwrap();
                    let clone_nucl_1_id = nucleotides_clones.get(nucl_1).unwrap();
                    let clone_nucl_2_id = nucleotides_clones.get(nucl_2).unwrap();
                    let bond_color = color_map.get(&bond_id).unwrap();
                    let clone_bond_color =
                        Instance::color_au32_with_alpha_scaled_by(*bond_color, CLONE_OPACITY);
                    let bond_radius = radius_map.get(bond_id).unwrap();
                    let helix_id = helix_map.get(bond_id).unwrap();
                    object_type.insert(
                        clone_bond_id,
                        ObjectType::HelixCylinder(*clone_nucl_1_id, *clone_nucl_2_id), // SHOULD REMEMBER THE CLONING NUMBER -> have a map that maps a nucl to the array of its clones and use it when displaying the helix cylinder for real
                    );
                    nucleotides_involved.insert(clone_bond_id, (*nucl_1, *nucl_2));
                    color_map.insert(clone_bond_id, clone_bond_color);
                    radius_map.insert(clone_bond_id, *bond_radius); // radius given to the bond
                    helix_map.insert(clone_bond_id, *helix_id);
                }
            }
        }

        // Output
        let mut ret = Self {
            object_type,
            nucleotide,
            nucleotides_involved,
            nucl_collection: Arc::new(nucl_collection),
            identifier_bond,
            strand_map,
            space_position,
            axis_space_position,
            on_axis,
            color_map,
            radius_map,
            helix_map,
            helix_radius_map,
            helix_color_map,
            letter_map: Arc::new(letter_map),
            prime3_set,
            elements,
            grid_manager,
            suggestions: vec![],
            loopout_bonds,
            loopout_nucls,
            insertion_length,
            drawing_styles,
            xover_coloring_map,
            clone_transformations,
            with_cones_map,
            scalebar,
        };
        let suggestions = suggestion_maker.get_suggestions(&design, suggestion_parameters);
        ret.suggestions = suggestions;

        drop(groups);

        if log::log_enabled!(log::Level::Warn) {
            if let Some(s) = design
                .scaffold_id
                .as_ref()
                .and_then(|s_id| design.strands.get(s_id))
            {
                for d in s.domains.iter() {
                    if let Domain::HelixDomain(interval) = d {
                        for n in interval.iter() {
                            let nucl = Nucl {
                                helix: interval.helix,
                                position: n,
                                forward: !interval.forward,
                            };
                            if !ret.nucl_collection.contains_nucl(&nucl) {
                                log::warn!("Missing {nucl}");
                            }
                        }
                    }
                }
            }
        }

        #[cfg(test)]
        {
            ret.test_named_junction(&design, &mut new_junctions, "TEST AFTER MAKE HASH MAP");
        }
        (ret, design, new_junctions)
    }

    fn update_junction(
        new_xover_ids: &mut JunctionsIds,
        junction: &mut DomainJunction,
        bond: (Nucl, Nucl),
    ) {
        let is_xover = bond.0.prime3() != bond.1; // true if different from the next in the strand
        match junction {
            DomainJunction::Adjacent if is_xover => {
                let id = new_xover_ids.insert(bond);
                *junction = DomainJunction::IdentifiedXover(id);
            }
            DomainJunction::UnindentifiedXover | DomainJunction::IdentifiedXover(_)
                if !is_xover =>
            {
                *junction = DomainJunction::Adjacent;
            }
            DomainJunction::UnindentifiedXover => {
                let id = new_xover_ids.insert(bond);
                *junction = DomainJunction::IdentifiedXover(id);
            }
            DomainJunction::IdentifiedXover(id) => {
                new_xover_ids.insert_at(bond, *id);
            }
            _ => (),
        }
    }

    #[allow(dead_code)]
    pub fn get_shift(&self, g_id: GridId) -> Option<f32> {
        self.grid_manager
            .grids
            .get(&g_id)
            .and_then(|g| g.grid_type.get_shift())
    }

    pub fn read_simulation_update(&mut self, update: &dyn SimulationUpdate) {
        update.update_positions(self.nucl_collection.as_ref(), &mut self.space_position)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    impl DesignContent {
        pub(super) fn test_named_junction(
            &self,
            design: &Design,
            xover_ids: &JunctionsIds,
            fail_msg: &'static str,
        ) {
            let mut xover_cpy = xover_ids.clone();
            for s in design.strands.values() {
                let mut expected_prime5: Option<Nucl> = None;
                let mut expected_prime5_domain: Option<usize> = None;
                let nb_taken = if s.is_cyclic {
                    2 * s.domains.len()
                } else {
                    s.domains.len()
                };
                for (i, d) in s.domains.iter().enumerate().cycle().take(nb_taken) {
                    if let Some(prime3) = d.prime5_end() {
                        if let Some(prime5) = expected_prime5 {
                            if prime5.prime3() == prime3 {
                                // Expect adjacent
                                if s.junctions[expected_prime5_domain.unwrap()]
                                    != DomainJunction::Adjacent
                                {
                                    panic!(
                                        "In test{} \n
                                        Expected junction {:?}, got {:?}\n
                                        junctions are {:?}",
                                        fail_msg,
                                        DomainJunction::Adjacent,
                                        s.junctions[expected_prime5_domain.unwrap()],
                                        s.junctions,
                                    );
                                }
                            } else {
                                // Expect named xover
                                if let Some(id) = xover_ids.get_id(&(prime5, prime3)) {
                                    xover_cpy.remove(id);
                                    if s.junctions[expected_prime5_domain.unwrap()]
                                        != DomainJunction::IdentifiedXover(id)
                                    {
                                        panic!(
                                            "In test{} \n
                                        Expected junction {:?}, got {:?}\n
                                        junctions are {:?}",
                                            fail_msg,
                                            DomainJunction::IdentifiedXover(id),
                                            s.junctions[expected_prime5_domain.unwrap()],
                                            s.junctions,
                                        );
                                    }
                                } else {
                                    panic!(
                                        "In test{} \n
                                        Could not find xover in xover_ids {:?}
                                        xover_ids: {:?}",
                                        fail_msg,
                                        (prime5, prime3),
                                        xover_ids.get_all_elements(),
                                    );
                                }
                            }
                            if expected_prime5_domain.unwrap() >= i {
                                break;
                            }
                        }
                    }
                    if let Some(nucl) = d.prime3_end() {
                        expected_prime5 = Some(nucl);
                    }
                    expected_prime5_domain = Some(i);
                }
            }
            assert!(
                xover_cpy.is_empty(),
                "In test {}\n
            Remaining xovers {:?}",
                fail_msg,
                xover_cpy.get_all_elements()
            );
        }
    }
}

trait GridInstancesMaker {
    fn grid_instances(&self, design_id: usize) -> BTreeMap<GridId, GridInstance>;
}

impl GridInstancesMaker for GridData {
    fn grid_instances(&self, design_id: usize) -> BTreeMap<GridId, GridInstance> {
        let mut ret = BTreeMap::new();
        for (g_id, g) in self.grids.iter() {
            let grid = GridInstance {
                grid: g.clone(),
                min_x: -2,
                max_x: 2,
                min_y: -2,
                max_y: 2,
                color: 0x00_00_FF,
                design: design_id,
                id: *g_id,
                fake: false,
                visible: !g.invisible,
            };
            ret.insert(*g_id, grid);
        }
        for grid_position in self.get_all_used_grid_positions() {
            if let Some(grid) = ret.get_mut(&grid_position.grid) {
                grid.min_x = grid.min_x.min(grid_position.x as i32 - 2);
                grid.max_x = grid.max_x.max(grid_position.x as i32 + 2);
                grid.min_y = grid.min_y.min(grid_position.y as i32 - 2);
                grid.max_y = grid.max_y.max(grid_position.y as i32 + 2);
            }
        }
        ret
    }
}

enum StapleDomain {
    ScaffoldDomain {
        domain_id: usize,
        first_scaffold_position: usize,
        last_scaffold_position: usize,
    },
    OtherDomain {
        length: usize,
    },
}

#[derive(Clone, Copy)]
struct ScaffoldPosition {
    domain_id: usize,
    scaffold_position: usize,
}

impl StapleDomain {
    fn init(scaffold_position: Option<ScaffoldPosition>) -> Self {
        if let Some(pos) = scaffold_position {
            Self::ScaffoldDomain {
                domain_id: pos.domain_id,
                first_scaffold_position: pos.scaffold_position,
                last_scaffold_position: pos.scaffold_position,
            }
        } else {
            Self::OtherDomain { length: 0 }
        }
    }

    fn reset(scaffold_position: Option<ScaffoldPosition>) -> Self {
        if let Some(pos) = scaffold_position {
            Self::ScaffoldDomain {
                domain_id: pos.domain_id,
                first_scaffold_position: pos.scaffold_position,
                last_scaffold_position: pos.scaffold_position,
            }
        } else {
            Self::OtherDomain { length: 1 }
        }
    }

    fn finish(&self) -> (isize, isize) {
        match self {
            Self::OtherDomain { length } => (-1, -(*length as isize)),
            Self::ScaffoldDomain {
                first_scaffold_position,
                last_scaffold_position,
                ..
            } => (
                *first_scaffold_position as isize,
                *last_scaffold_position as isize,
            ),
        }
    }

    fn read_position(mut self, position: Option<ScaffoldPosition>) -> ReadResult {
        match &mut self {
            Self::OtherDomain { length } => {
                if position.is_none() {
                    *length += 1;
                    ReadResult::Continue(self)
                } else {
                    ReadResult::Stop {
                        interval: self.finish(),
                        new_reader: Self::reset(position),
                    }
                }
            }
            Self::ScaffoldDomain {
                domain_id,
                last_scaffold_position,
                ..
            } => {
                if let Some(pos) = position.filter(|p| p.domain_id == *domain_id) {
                    *last_scaffold_position = pos.scaffold_position;
                    ReadResult::Continue(self)
                } else {
                    ReadResult::Stop {
                        interval: self.finish(),
                        new_reader: Self::reset(position),
                    }
                }
            }
        }
    }
}

enum ReadResult {
    Continue(StapleDomain),
    Stop {
        interval: (isize, isize),
        new_reader: StapleDomain,
    },
}
