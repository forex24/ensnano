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
//! DNA geometric parmeters.

use super::codenano;
use std::f32::consts::{PI, SQRT_2, TAU};

/// DNA geometric parameters.
#[derive(Copy, Clone, Debug, Serialize, Deserialize)]
pub struct HelixParameters {
    /// Distance between two consecutive bases along the axis of a
    /// helix, in nanometers.
    #[serde(alias = "z_step")]
    pub rise: f32,
    /// Radius of a helix, in nanometers.
    pub helix_radius: f32,
    /// Number of bases per turn.
    pub bases_per_turn: f32,
    /// Minor groove angle. DNA helices have a "minor groove" and a
    /// "major groove", meaning that two paired nucleotides are not at
    /// opposite positions around a double helix (i.e. at an angle of
    /// 180°), but instead have a different angle.
    ///
    /// Strands are directed. The "normal" direction is called "5' to
    /// 3'" (named after parts of the nucleotides). This parameter is
    /// the small angle, which is clockwise from the normal strand to
    /// the reverse strand.
    pub groove_angle: f32,

    /// Gap between two neighbouring helices.
    pub inter_helix_gap: f32,

    /// The inclination of paired phosphates relative to the helical axis
    #[serde(default)]
    pub inclination: f32,
}

macro_rules! parameters_from_p_stick_model {
    ($p_stick_model: expr) => {
        HelixParameters {
            groove_angle: ($p_stick_model).groove_angle
                - 2.0 * std::f32::consts::PI / ($p_stick_model).bases_per_turn,
            inclination: ($p_stick_model).inclination - ($p_stick_model).rise,
            ..($p_stick_model)
        }
    };
}

macro_rules! parameters_from_p_stick_model_plus_or_minus {
    ($p_stick_model: expr) => {
        if ($p_stick_model).inclination >= 0.0 {
            HelixParameters {
                groove_angle: ($p_stick_model).groove_angle
                    - 2.0 * std::f32::consts::PI / ($p_stick_model).bases_per_turn,
                inclination: ($p_stick_model).inclination - ($p_stick_model).rise,
                ..($p_stick_model)
            }
        } else {
            HelixParameters {
                groove_angle: ($p_stick_model).groove_angle
                    + 2.0 * std::f32::consts::PI / ($p_stick_model).bases_per_turn,
                inclination: ($p_stick_model).inclination + ($p_stick_model).rise,
                ..($p_stick_model)
            }
        }
    };
}

impl HelixParameters {
    pub const INTER_CENTER_GAP: f32 = HelixParameters::ENSNANO_2021.helix_radius * 2.
        + HelixParameters::ENSNANO_2021.inter_helix_gap;

    /// Value used for versions >= 0.4.1.
    /// Taken from "Design Principles for Single-Stranded RNA Origami Structures, Geary & Andersen
    /// 2014
    pub const GEARY_2014_DNA_P_STICK: HelixParameters = {
        let helix_radius = 0.93;
        HelixParameters {
            rise: 0.332,
            helix_radius,
            bases_per_turn: 10.44,
            groove_angle: 170.4 / 180.0 * std::f32::consts::PI,
            inclination: 0.375,
            // From Paul's paper.
            inter_helix_gap: Self::INTER_CENTER_GAP - 2. * helix_radius,
        }
    };

    pub fn inter_helix_axis_gap(&self) -> f32 {
        return 2.0 * self.helix_radius + self.inter_helix_gap;
    }

    pub const GEARY_2014_DNA: HelixParameters =
        parameters_from_p_stick_model!(Self::GEARY_2014_DNA_P_STICK);

    /// Value used for RNA designs
    /// Taken from "Design Principles for Single-Stranded RNA Origami Structures, Geary & Andersen
    /// 2014
    pub const GEARY_2014_RNA_P_STICK: HelixParameters = {
        let helix_radius = 0.87;
        HelixParameters {
            helix_radius,
            rise: 0.281,
            inclination: -0.745,
            groove_angle: 139.9 / 180.0 * std::f32::consts::PI,
            bases_per_turn: 11.0,
            inter_helix_gap: Self::INTER_CENTER_GAP - 2. * helix_radius,
        }
    };

    pub const GEARY_2014_RNA: HelixParameters =
        parameters_from_p_stick_model!(Self::GEARY_2014_RNA_P_STICK);

    pub const GEARY_2014_RNA2: HelixParameters =
        parameters_from_p_stick_model_plus_or_minus!(Self::GEARY_2014_RNA_P_STICK);

    pub const DEFAULT: Self = Self::GEARY_2014_DNA;

    /// Values used in version perior to 0.4.1, taken from the litterature (Wikipedia, Cargo
    /// sorting paper, Woo 2011).
    pub const ENSNANO_2021: HelixParameters = HelixParameters {
        // z-step and helix radius from: Wikipedia
        rise: 0.332,
        helix_radius: 1.,
        // bases per turn from Woo Rothemund (Nature Chemistry).
        bases_per_turn: 10.44,
        // minor groove 12 Å, major groove 22 Å total 34 Å
        groove_angle: 2. * PI * 12. / 34.,
        // From Paul's paper.
        inter_helix_gap: 0.65,
        // Previous version of ENSnano did not have an inclination parameter
        inclination: 0.0,
    };

    pub const TRIPLEX_DNA_TWO_HELICES: HelixParameters = {
        let helix_diameter = 2.75; // nm
        let helix_radius = helix_diameter / 2.0; // nm
        HelixParameters {
            rise: 0.34,                                         // nm
            helix_radius,                                       // nm
            bases_per_turn: 11.9,                               // bp per turn
            groove_angle: 120.0 / 180.0 * std::f32::consts::PI, // rad
            inclination: 0.0,                                   // nm
            // From Paul's paper.
            inter_helix_gap: 0.70, // nm
        }
    };

    pub fn from_codenano(codenano_param: &codenano::Parameters) -> Self {
        Self {
            rise: codenano_param.rise as f32,
            helix_radius: codenano_param.helix_radius as f32,
            bases_per_turn: codenano_param.bases_per_turn as f32,
            groove_angle: codenano_param.groove_angle as f32,
            inter_helix_gap: codenano_param.inter_helix_gap as f32,
            inclination: 0.0,
        }
    }

    pub fn formated_string(&self) -> String {
        use std::fmt::Write;
        let mut ret = String::new();
        writeln!(&mut ret, "  Radius: {:.3} nm", self.helix_radius).unwrap_or_default();
        writeln!(&mut ret, "  Rise: {:.3} nm", self.rise).unwrap_or_default();
        writeln!(&mut ret, "  Inclination {:.3} nm", self.inclination).unwrap_or_default();
        writeln!(&mut ret, "  Helicity: {:.2} bp", self.bases_per_turn).unwrap_or_default();
        writeln!(
            &mut ret,
            "  Minor groove: {:.1}°",
            self.groove_angle.to_degrees()
        )
        .unwrap_or_default();
        writeln!(
            &mut ret,
            "  Inter helix gap: {:.2} nm",
            self.inter_helix_gap
        )
        .unwrap_or_default();
        writeln!(&mut ret, " Expected xover length: {:.2} nm", self.dist_ac()).unwrap_or_default();
        ret
    }
}

impl std::default::Default for HelixParameters {
    fn default() -> Self {
        Self::DEFAULT
    }
}

impl HelixParameters {
    /// The angle AOC_2 where
    ///
    /// * A is a base on the helix
    /// * B is the base paired to A
    /// * O is the projection of A on the axis of the helix
    /// * C is the 3' neighbour of A
    /// * C_2 is the projection of C in the AOB plane
    fn angle_aoc2(&self) -> f32 {
        TAU / self.bases_per_turn
    }

    /// The distance |AC| where
    ///
    /// * A is a base on the helix
    /// * C is the 3' neighbour of A
    pub fn dist_ac(&self) -> f32 {
        (self.dist_ac2() * self.dist_ac2() + self.rise * self.rise).sqrt()
    }

    /// The distance |AC_2| where
    ///
    /// * A is a base on the helix
    /// * B is the base paired to A
    /// * O is the projection of A on the axis of the helix
    /// * C is the 3' neighbour of A
    /// * C_2 is the projection of C in the AOB plane
    pub fn dist_ac2(&self) -> f32 {
        SQRT_2 * (1. - self.angle_aoc2().cos()).sqrt() * self.helix_radius
    }

    pub fn name(&self) -> &'static NamedParameter {
        let mut best_name = &NAMED_DNA_PARAMETERS[0];
        let mut best_delta = f32::INFINITY;
        for p in NAMED_DNA_PARAMETERS.iter() {
            let delta = self.delta_model(&p.value);
            if delta < best_delta {
                best_name = p;
                best_delta = delta;
            }
        }
        best_name
    }

    fn delta_model(&self, other: &Self) -> f32 {
        (self.inclination - other.inclination).abs()
            + (self.helix_radius - other.helix_radius).abs()
            + (self.inter_helix_gap - other.inter_helix_gap).abs()
            + (self.groove_angle - other.groove_angle).abs()
            + (self.rise - other.rise).abs()
            + (self.bases_per_turn - other.bases_per_turn).abs()
    }
}

#[derive(Clone, Debug)]
pub struct NamedParameter {
    pub name: &'static str,
    pub value: HelixParameters,
}

impl ToString for NamedParameter {
    fn to_string(&self) -> String {
        self.name.to_string()
    }
}

pub const NAMED_DNA_PARAMETERS: [NamedParameter; 7] = [
    NamedParameter {
        name: "Geary et al 2014 B-DNA",
        value: HelixParameters::GEARY_2014_DNA,
    },
    NamedParameter {
        name: "Geary et al 2014 A-RNA",
        value: HelixParameters::GEARY_2014_RNA,
    },
    NamedParameter {
        name: "Geary et al 2014 A-RNA(2)",
        value: HelixParameters::GEARY_2014_RNA2,
    },
    NamedParameter {
        name: "ENSnano 2021",
        value: HelixParameters::ENSNANO_2021,
    },
    NamedParameter {
        name: "Geary et al 2014 B-DNA Phosphate balls",
        value: HelixParameters::GEARY_2014_DNA_P_STICK,
    },
    NamedParameter {
        name: "Geary et al 2014 A-RNA Phosphate balls",
        value: HelixParameters::GEARY_2014_RNA_P_STICK,
    },
    NamedParameter {
        name: "Triplex DNA Helix with 2 helices",
        value: HelixParameters::TRIPLEX_DNA_TWO_HELICES,
    },
];

impl PartialEq for NamedParameter {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
    }
}

impl Eq for NamedParameter {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Helix;
    use ultraviolet::{Rotor3, Vec3};

    #[test]
    fn dist_ac_is_correct() {
        let p = HelixParameters::DEFAULT;

        let h = Helix::new(Vec3::zero(), Rotor3::identity());
        let n1 = h.space_pos(&p, 0, true);
        let n2 = h.space_pos(&p, 1, true);

        let measured_dist = (n1 - n2).mag();

        assert!((measured_dist - p.dist_ac()).abs() < 1e-4);
    }
}
