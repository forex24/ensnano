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

use crate::{Parameters, parameters};

use super::Curved;
use std::f64::consts::{PI, TAU};
use ultraviolet::{DRotor3, DVec3};

#[derive(Clone, Serialize, Deserialize, Debug)]
pub struct SphereConcentricCircleDescriptor {
    pub radius: f64,
    pub theta_0: f64,
    pub helix_index: i32, // 0 is the equator, negative for below the equator, positive above
}

fn default_number_of_helices() -> usize {
    3
}

impl SphereConcentricCircleDescriptor {
    pub(super) fn with_parameters(self, parameters: Parameters) -> SphereConcentricCircle {
        let φ = PI / 2.0 
                - self.helix_index as f64 * Parameters::INTER_CENTER_GAP as f64/ self.radius;
        let z_radius = self.radius * φ.sin();
        let z = self.radius * φ.cos(); 
        let perimeter = TAU * z_radius;

        SphereConcentricCircle {
            _parameters: parameters,
            radius: self.radius,
            theta_0: self.theta_0,
            helix_index: self.helix_index,
            perimeter,
            φ,
            z_radius,
            z,
            t_min: 0.,
            t_max: 1.,
        }
    }
}

pub(super) struct SphereConcentricCircle {
    pub _parameters: Parameters,
    pub radius: f64,
    pub theta_0: f64,
    pub helix_index: i32,
    pub perimeter: f64,
    pub φ: f64,
    pub z_radius: f64,
    pub z: f64,
    pub t_min: f64,
    pub t_max: f64,
}

impl SphereConcentricCircle {

    fn theta(&self, t: f64) -> f64 {
        t * TAU + self.theta_0
    }

    pub(super) fn last_theta(&self) -> f64 {
        self.theta(1.)
    }

    pub(super) fn t_min(&self) -> f64 {
        0.
    }

    pub(super) fn t_max(&self) -> f64 {
        1.
    }
}

impl Curved for SphereConcentricCircle {
    fn position(&self, t: f64) -> DVec3 {
        let theta = self.theta(t);
        DVec3 {
            x: self.z_radius * theta.cos(),
            y: self.z_radius * theta.sin(),
            z: self.z,
        }
    }

    fn speed(&self, t: f64) -> DVec3 {
        let theta = self.theta(t);

        let x = -self.z_radius * TAU * theta.sin();

        let y = self.z_radius * TAU * theta.cos();

        let z = 0.0;

        DVec3 { x, y, z }
    }

    fn acceleration(&self, t: f64) -> DVec3 {
        let theta = self.theta(t);

        let x = -self.z_radius * TAU * TAU * theta.cos();

        let y = -self.z_radius * TAU * TAU * theta.sin();

        let z = 0.;

        DVec3 { x, y, z }
    }

    fn curvilinear_abscissa(&self, _t: f64) -> Option<f64> {
        Some(self.z_radius * TAU * _t)
    }

    fn inverse_curvilinear_abscissa(&self, _x: f64) -> Option<f64> {
        Some(_x / TAU / self.z_radius)
    }

    fn bounds(&self) -> super::CurveBounds {
        super::CurveBounds::Finite
    }

    // fn subdivision_for_t(&self, t: f64) -> Option<usize> {
    //     None
    // }

    // fn is_time_maps_singleton(&self) -> bool {
    //     true
    // }

    fn first_theta(&self) -> Option<f64> {
        Some(self.theta_0)
    }

    fn last_theta(&self) -> Option<f64> {
        Some(self.last_theta())
    }

    fn full_turn_at_t(&self) -> Option<f64> {
        Some(self.t_max())
    }

    fn t_max(&self) -> f64 {
        self.t_max
    }

    fn t_min(&self) -> f64 {
        self.t_min
    }
}
