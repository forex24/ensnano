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
use ensnano_design::Domain;
use ensnano_exports::oxdna::*;
use std::mem::ManuallyDrop;

impl Presenter {

    pub fn oxdna_export(&self, config_name: &PathBuf) -> std::io::Result<(PathBuf, PathBuf)> {
        let mut topology_name = config_name.clone();
        topology_name.set_extension("top");
        let (config, topo) = self.to_oxdna();
        config.write(config_name.clone())?;
        topo.write(topology_name.clone())?;
        Ok((config_name.to_path_buf(), topology_name))
    }

    pub fn cando_export(
        &self,
        out_path: &PathBuf,
    ) -> Result<(), ensnano_exports::cando::CanDoError> {
        use ensnano_exports::cando;

        let mut exporter = cando::CanDoFormater::new();
        let parameters = self.current_design.parameters.unwrap_or_default();

        for s in self.current_design.strands.values() {
            let mut cando_strand = exporter.add_strand();

            for d in s.domains.iter() {
                if let Domain::HelixDomain(dom) = d {
                    for position in dom.iter() {
                        let ox_nucl = self
                            .current_design
                            .helices
                            .get(&dom.helix)
                            .unwrap()
                            .ox_dna_nucl(position, dom.forward, &parameters);
                        let nucl = Nucl {
                            position,
                            helix: dom.helix,
                            forward: dom.forward,
                        };

                        let base = self.content.basis_map.get(&nucl).cloned();
                        //let base = if dom.forward { 'C' } else { 'G'};
                        let sign = if nucl.forward { 1. } else { -1. };
                        cando_strand.add_nucl(
                            nucl,
                            ox_nucl.position,
                            sign * ox_nucl.normal,
                            base,
                        )?;
                    }
                }
            }
            cando_strand.end(s.cyclic)?;
        }
        exporter
            .write_to(out_path)
            .map_err(|e| cando::CanDoError::IOError(e))
    }

    pub fn pdb_export(&self, out_path: &PathBuf) -> Result<(), ensnano_exports::pdb::PdbError> {
        use ensnano_exports::pdb;
        let parameters = self.current_design.parameters.unwrap_or_default();
        let na_kind = if parameters.name().name.contains("RNA") {
            ensnano_exports::pdb::NucleicAcidKind::Rna
        } else {
            ensnano_exports::pdb::NucleicAcidKind::Dna
        };
        let mut exporter = pdb::PdbFormatter::new(out_path, na_kind)?;
        let mut previous_position = None;

        for s in self.current_design.strands.values() {
            let mut pdb_strand = exporter.start_strand(s.cyclic);

            for d in s.domains.iter() {
                if let Domain::HelixDomain(dom) = d {
                    for position in dom.iter() {
                        let ox_nucl = self
                            .current_design
                            .helices
                            .get(&dom.helix)
                            .unwrap()
                            .ox_dna_nucl(position, dom.forward, &parameters);
                        let nucl = Nucl {
                            position,
                            helix: dom.helix,
                            forward: dom.forward,
                        };
                        previous_position = Some(ox_nucl.position);
                        let base =
                            self.content
                                .basis_map
                                .get(&nucl)
                                .cloned()
                                .unwrap_or(if dom.forward {
                                    'A'
                                } else {
                                    na_kind.compl_to_a()
                                });
                        //let base = if dom.forward { 'C' } else { 'G'};
                        pdb_strand.add_nucl(base, ox_nucl.position * 10., ox_nucl.get_basis())?;
                    }
                } else if let Domain::Insertion {
                    instanciation: Some(instanciation),
                    ..
                } = d
                {
                    for (insertion_idx, position) in instanciation.pos().iter().enumerate() {
                        let ox_nucl = ensnano_exports::oxdna::free_oxdna_nucl(
                            *position,
                            previous_position,
                            insertion_idx,
                            &parameters,
                        );
                        previous_position = Some(*position);
                        pdb_strand.add_nucl(
                            na_kind.compl_to_a(),
                            ox_nucl.position * 10.,
                            ox_nucl.get_basis(),
                        )?;
                    }
                }
            }
            pdb_strand.write()?;
        }

        Ok(())
    }
}
