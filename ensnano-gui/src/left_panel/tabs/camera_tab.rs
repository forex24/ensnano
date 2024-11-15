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
use ensnano_interactor::graphics::{
    Background3D, RenderingMode, ALL_BACKGROUND3D, ALL_RENDERING_MODE,
};

pub struct CameraTab {
    fog: FogParameters,
    scroll: scrollable::State,
    selection_visibility_btn: button::State,
    compl_visibility_btn: button::State,
    all_visible_btn: button::State,
    pub background3d: Background3D,
    background3d_picklist: pick_list::State<Background3D>,
    pub rendering_mode: RenderingMode,
    rendering_mode_picklist: pick_list::State<RenderingMode>,
    check_xover_picklist: pick_list::State<CheckXoversParameter>,
    h_bonds_picklist: pick_list::State<HBondDisplay>,
}

impl CameraTab {
    pub fn new() -> Self {
        Self {
            fog: Default::default(),
            scroll: Default::default(),
            selection_visibility_btn: Default::default(),
            compl_visibility_btn: Default::default(),
            all_visible_btn: Default::default(),
            background3d: Default::default(),
            background3d_picklist: Default::default(),
            rendering_mode: Default::default(),
            rendering_mode_picklist: Default::default(),
            check_xover_picklist: Default::default(),
            h_bonds_picklist: Default::default(),
        }
    }

    pub fn view<'a, S: AppState>(
        &'a mut self,
        ui_size: UiSize,
        app_state: &S,
    ) -> Element<'a, Message<S>> {
        let mut ret = Column::new().spacing(5);
        section!(ret, ui_size, "Camera");
        subsection!(ret, ui_size, "Visibility");
        ret = ret.push(
            text_btn(
                &mut self.selection_visibility_btn,
                "Toggle Selected Visibility",
                ui_size.clone(),
            )
            .on_press(Message::ToggleVisibility(false)),
        );
        ret = ret.push(
            text_btn(
                &mut self.compl_visibility_btn,
                "Toggle NonSelected Visibility",
                ui_size.clone(),
            )
            .on_press(Message::ToggleVisibility(true)),
        );
        ret = ret.push(
            text_btn(
                &mut self.all_visible_btn,
                "Everything visible",
                ui_size.clone(),
            )
            .on_press(Message::AllVisible),
        );
        ret = ret.push(self.fog.view(&ui_size));

        let h_bond_column = Column::new()
            .push(Text::new("Show H-Bonds").size(ui_size.intermediate_text()))
            .push(PickList::new(
                &mut self.h_bonds_picklist,
                [
                    HBondDisplay::No,
                    HBondDisplay::Stick,
                    HBondDisplay::Ellipsoid,
                ]
                .as_slice(),
                Some(app_state.get_h_bonds_display()),
                Message::ShowHBonds,
            ));

        ret = ret.push(h_bond_column);

        ret = ret.push(right_checkbox(
            app_state.show_stereographic_camera(),
            "Show stereographic camera",
            Message::ShowStereographicCamera,
            ui_size,
        ));

        ret = ret.push(right_checkbox(
            app_state.follow_stereographic_camera(),
            "Follow stereographic camera",
            Message::FollowStereographicCamera,
            ui_size,
        ));

        subsection!(ret, ui_size, "Highlight Xovers");
        ret = ret.push(PickList::new(
            &mut self.check_xover_picklist,
            CheckXoversParameter::ALL,
            Some(app_state.get_checked_xovers_parameters()),
            Message::CheckXoversParameter,
        ));

        subsection!(ret, ui_size, "Rendering");
        ret = ret.push(Text::new("Style"));
        ret = ret.push(PickList::new(
            &mut self.rendering_mode_picklist,
            &ALL_RENDERING_MODE[..],
            Some(self.rendering_mode),
            Message::RenderingMode,
        ));
        ret = ret.push(Text::new("Background"));
        ret = ret.push(PickList::new(
            &mut self.background3d_picklist,
            &ALL_BACKGROUND3D[..],
            Some(self.background3d),
            Message::Background3D,
        ));
        ret = ret.push(Checkbox::new(
            app_state.expand_insertions(),
            "Expand insertions",
            Message::SetExpandInsertions,
        ));

        Scrollable::new(&mut self.scroll).push(ret).into()
    }

    pub fn fog_visible(&mut self, visible: bool) {
        self.fog.visible = visible
    }

    pub fn fog_dark(&mut self, dark: bool) {
        self.fog.dark = dark
    }

    pub fn fog_reversed(&mut self, reversed: bool) {
        self.fog.reversed = reversed
    }

    pub fn fog_length(&mut self, length: f32) {
        self.fog.length = length
    }

    pub fn fog_radius(&mut self, radius: f32) {
        self.fog.radius = radius
    }

    pub fn fog_camera(&mut self, from_camera: bool) {
        self.fog.from_camera = from_camera;
    }

    pub fn get_fog_request(&self) -> Fog {
        self.fog.request()
    }
}

struct FogParameters {
    visible: bool,
    from_camera: bool,
    dark: bool,
    radius: f32,
    radius_slider: slider::State,
    length: f32,
    length_slider: slider::State,
    picklist: pick_list::State<FogChoice>,
    reversed: bool,
}

impl FogParameters {
    fn view<S: AppState>(&mut self, ui_size: &UiSize) -> Column<Message<S>> {
        let mut column = Column::new()
            .push(Text::new("Fog").size(ui_size.intermediate_text()))
            .push(PickList::new(
                &mut self.picklist,
                &ALL_FOG_CHOICE[..],
                Some(FogChoice::from_param(
                    self.visible,
                    self.from_camera,
                    self.dark,
                    self.reversed,
                )),
                Message::FogChoice,
            ));

        let radius_text = if self.visible {
            Text::new("Radius")
        } else {
            Text::new("Radius").color([0.6, 0.6, 0.6])
        };

        let gradient_text = if self.visible {
            Text::new("Softness")
        } else {
            Text::new("Softness").color([0.6, 0.6, 0.6])
        };

        let length_slider = if self.visible {
            Slider::new(
                &mut self.length_slider,
                0f32..=100f32,
                self.length,
                Message::FogLength,
            )
        } else {
            Slider::new(&mut self.length_slider, 0f32..=100f32, self.length, |_| {
                Message::Nothing
            })
            .style(DesactivatedSlider)
        };

        let softness_slider = if self.visible {
            Slider::new(
                &mut self.radius_slider,
                0f32..=100f32,
                self.radius,
                Message::FogRadius,
            )
        } else {
            Slider::new(&mut self.radius_slider, 0f32..=100f32, self.radius, |_| {
                Message::Nothing
            })
            .style(DesactivatedSlider)
        };

        column = column
            .push(Row::new().spacing(5).push(radius_text).push(length_slider))
            .push(
                Row::new()
                    .spacing(5)
                    .push(gradient_text)
                    .push(softness_slider),
            );
        column
    }

    fn request(&self) -> Fog {
        Fog {
            radius: self.radius,
            fog_kind: FogChoice::from_param(
                self.visible,
                self.from_camera,
                self.dark,
                self.reversed,
            )
            .fog_kind(),
            length: self.length,
            from_camera: self.from_camera,
            alt_fog_center: None,
        }
    }
}

impl Default for FogParameters {
    fn default() -> Self {
        Self {
            visible: false,
            dark: false,
            length: 10.,
            radius: 10.,
            length_slider: Default::default(),
            radius_slider: Default::default(),
            from_camera: true,
            picklist: Default::default(),
            reversed: false,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Copy)]
pub enum FogChoice {
    None,
    FromCamera,
    FromPivot,
    DarkFromCamera,
    DarkFromPivot,
    ReversedFromPivot,
}

impl Default for FogChoice {
    fn default() -> Self {
        Self::None
    }
}

const ALL_FOG_CHOICE: &'static [FogChoice] = &[
    FogChoice::None,
    FogChoice::FromCamera,
    FogChoice::FromPivot,
    FogChoice::DarkFromCamera,
    FogChoice::DarkFromPivot,
    FogChoice::ReversedFromPivot,
];

impl std::fmt::Display for FogChoice {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let ret = match self {
            Self::None => "None",
            Self::FromCamera => "From Camera",
            Self::FromPivot => "From Pivot",
            Self::DarkFromCamera => "Dark from Camera",
            Self::DarkFromPivot => "Dark from Pivot",
            Self::ReversedFromPivot => "Reversed from Pivot",
        };
        write!(f, "{}", ret)
    }
}

impl FogChoice {
    fn from_param(visible: bool, from_camera: bool, dark: bool, reversed: bool) -> Self {
        Self::None
            .visible(visible)
            .dark(dark)
            .from_camera(from_camera)
            .reversed(reversed)
    }

    pub fn to_param(&self) -> (bool, bool, bool, bool) {
        (
            self.is_visible(),
            self.is_from_camera(),
            self.is_dark(),
            self.is_reversed(),
        )
    }

    fn visible(self, visible: bool) -> Self {
        if visible {
            if let Self::None = self {
                Self::FromPivot
            } else {
                self
            }
        } else {
            Self::None
        }
    }

    fn from_camera(self, from_camera: bool) -> Self {
        if from_camera {
            match self {
                Self::FromPivot => Self::FromCamera,
                Self::DarkFromPivot => Self::DarkFromCamera,
                _ => self,
            }
        } else {
            match self {
                Self::FromCamera => Self::FromPivot,
                Self::DarkFromCamera => Self::DarkFromPivot,
                _ => self,
            }
        }
    }

    fn reversed(self, reversed: bool) -> Self {
        match (self, reversed) {
            (Self::FromPivot, true) => Self::ReversedFromPivot,
            (Self::ReversedFromPivot, false) => Self::FromPivot,
            _ => self,
        }
    }

    fn dark(self, dark: bool) -> Self {
        if dark {
            match self {
                Self::FromCamera => Self::DarkFromCamera,
                Self::FromPivot => Self::DarkFromPivot,
                _ => self,
            }
        } else {
            match self {
                Self::DarkFromCamera => Self::FromCamera,
                Self::DarkFromPivot => Self::FromPivot,
                _ => self,
            }
        }
    }

    fn is_visible(&self) -> bool {
        !matches!(self, Self::None)
    }

    fn is_from_camera(&self) -> bool {
        matches!(self, Self::FromCamera | Self::DarkFromCamera)
    }

    fn is_dark(&self) -> bool {
        matches!(self, Self::DarkFromCamera | Self::DarkFromPivot)
    }

    fn is_reversed(&self) -> bool {
        matches!(self, Self::ReversedFromPivot)
    }

    fn fog_kind(&self) -> u32 {
        use ensnano_interactor::graphics::fog_kind;
        match self {
            Self::None => fog_kind::NO_FOG,
            Self::FromCamera | Self::FromPivot => fog_kind::TRANSPARENT_FOG,
            Self::DarkFromPivot | Self::DarkFromCamera => fog_kind::DARK_FOG,
            Self::ReversedFromPivot => fog_kind::REVERSED_FOG,
        }
    }
}
