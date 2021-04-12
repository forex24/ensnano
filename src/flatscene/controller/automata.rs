use super::super::data::ClickResult;
use super::super::view::CircleInstance;
use super::super::{FlatHelix, FlatNucl};
use super::*;
use crate::design::StrandBuilder;

const WHEEL_RADIUS: f32 = 1.5;
use crate::consts::*;

pub struct Transition {
    pub new_state: Option<Box<dyn ControllerState>>,
    pub consequences: Consequence,
}

impl Transition {
    pub fn nothing() -> Self {
        Self {
            new_state: None,
            consequences: Consequence::Nothing,
        }
    }

    pub fn consequence(consequences: Consequence) -> Self {
        Self {
            new_state: None,
            consequences,
        }
    }
}

pub trait ControllerState {
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition;

    #[allow(dead_code)]
    fn display(&self) -> String;

    fn transition_from(&self, controller: &Controller) -> ();

    fn transition_to(&self, controller: &Controller) -> ();
}

pub struct NormalState {
    pub mouse_position: PhysicalPosition<f64>,
}

impl ControllerState for NormalState {
    fn display(&self) -> String {
        String::from("Normal state")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } if controller.action_mode != ActionMode::Cut && controller.modifiers.shift() => {
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::Nothing => Transition {
                        new_state: Some(Box::new(DraggingSelection {
                            mouse_position: self.mouse_position,
                            fixed_corner: self.mouse_position,
                            adding: true,
                        })),
                        consequences: Consequence::Nothing,
                    },
                    click_result => Transition {
                        new_state: Some(Box::new(AddClick {
                            mouse_position: self.mouse_position,
                            click_result,
                        })),
                        consequences: Consequence::Nothing,
                    },
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Pressed,
                    "Released mouse button in normal mode"
                );*/
                if *state == ElementState::Released {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::CircleWidget { .. } | ClickResult::Nothing
                        if controller.pasting =>
                    {
                        Transition {
                            new_state: Some(Box::new(Pasting {
                                nucl: None,
                                mouse_position: self.mouse_position,
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::Nucl(nucl) if controller.pasting => Transition {
                        new_state: Some(Box::new(Pasting {
                            nucl: Some(nucl),
                            mouse_position: self.mouse_position,
                        })),
                        consequences: Consequence::Nothing,
                    },
                    ClickResult::HelixHandle { h_id, handle } => Transition {
                        new_state: Some(Box::new(TranslatingHandle { h_id, handle })),
                        consequences: Consequence::Nothing,
                    },
                    ClickResult::Nucl(nucl)
                        if controller.data.borrow().is_suggested(&nucl)
                            && controller.modifiers.alt() =>
                    {
                        Transition {
                            new_state: Some(Box::new(FollowingSuggestion {
                                nucl,
                                mouse_position: self.mouse_position,
                                double: controller.modifiers.shift(),
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::Nucl(nucl) => {
                        if controller.action_mode == ActionMode::Cut {
                            Transition {
                                new_state: Some(Box::new(Cutting {
                                    nucl,
                                    mouse_position: self.mouse_position,
                                    whole_strand: controller.modifiers.shift(),
                                })),
                                consequences: Consequence::Nothing,
                            }
                        } else {
                            let stick = if let ActionMode::Build(b) = controller.action_mode {
                                b
                            } else {
                                false
                            };
                            if let Some(builder) = controller.data.borrow().get_builder(nucl, stick)
                            {
                                if builder.created_de_novo() {
                                    Transition {
                                        new_state: Some(Box::new(Building {
                                            mouse_position: self.mouse_position,
                                            nucl,
                                            builder,
                                            can_attach: false,
                                        })),
                                        consequences: Consequence::Nothing,
                                    }
                                } else {
                                    Transition {
                                        new_state: Some(Box::new(InitBuilding {
                                            mouse_position: self.mouse_position,
                                            nucl,
                                            builder,
                                            end: controller.data.borrow().is_strand_end(nucl),
                                        })),
                                        consequences: Consequence::Nothing,
                                    }
                                }
                            } else if let Some(attachement) =
                                controller.data.borrow().attachable_neighbour(nucl)
                            {
                                Transition {
                                    new_state: Some(Box::new(InitAttachement {
                                        mouse_position: self.mouse_position,
                                        from: nucl,
                                        to: attachement,
                                    })),
                                    consequences: Consequence::Nothing,
                                }
                            } else if controller.data.borrow().has_nucl(nucl)
                                && controller.data.borrow().is_xover_end(&nucl).is_none()
                            {
                                Transition {
                                    new_state: Some(Box::new(InitCutting {
                                        mouse_position: self.mouse_position,
                                        nucl,
                                    })),
                                    consequences: Consequence::Nothing,
                                }
                            } else {
                                Transition {
                                    new_state: Some(Box::new(DraggingSelection {
                                        mouse_position: self.mouse_position,
                                        fixed_corner: self.mouse_position,
                                        adding: false,
                                    })),
                                    consequences: Consequence::Nothing,
                                }
                            }
                        }
                    }
                    ClickResult::CircleWidget { translation_pivot }
                        if ctrl(&controller.modifiers) =>
                    {
                        Transition {
                            new_state: Some(Box::new(FlipVisibility {
                                mouse_position: self.mouse_position,
                                helix: translation_pivot.helix,
                                apply_to_other: controller.modifiers.alt(),
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::CircleWidget { translation_pivot }
                        if controller.modifiers.alt() =>
                    {
                        Transition {
                            new_state: Some(Box::new(FlipGroup {
                                mouse_position: self.mouse_position,
                                helix: translation_pivot.helix,
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::CircleWidget { translation_pivot } => {
                        if controller.action_mode == ActionMode::Cut {
                            Transition {
                                new_state: Some(Box::new(RmHelix {
                                    mouse_position: self.mouse_position,
                                    helix: translation_pivot.helix,
                                })),
                                consequences: Consequence::Nothing,
                            }
                        } else {
                            let clicked =
                                controller.get_camera(position.y).borrow().screen_to_world(
                                    self.mouse_position.x as f32,
                                    self.mouse_position.y as f32,
                                );
                            if controller.modifiers.shift() {
                                controller.data.borrow_mut().add_helix_selection(
                                    click_result,
                                    &controller.get_camera(position.y),
                                );
                            } else {
                                controller.data.borrow_mut().set_helix_selection(
                                    click_result,
                                    &controller.get_camera(position.y),
                                );
                            }
                            Transition {
                                new_state: Some(Box::new(Translating {
                                    mouse_position: self.mouse_position,
                                    world_clicked: clicked.into(),
                                    translation_pivots: vec![translation_pivot],
                                })),
                                consequences: Consequence::SelectionChanged,
                            }
                        }
                    }
                    ClickResult::Nothing => Transition {
                        new_state: Some(Box::new(DraggingSelection {
                            mouse_position: self.mouse_position,
                            fixed_corner: self.mouse_position,
                            adding: false,
                        })),
                        consequences: Consequence::Nothing,
                    },
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Middle,
                state: ElementState::Pressed,
                ..
            } => Transition {
                new_state: Some(Box::new(MovingCamera {
                    mouse_position: self.mouse_position,
                    clicked_position_screen: self.mouse_position,
                    translation_pivots: vec![],
                    rotation_pivots: vec![],
                })),
                consequences: Consequence::Nothing,
            },
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Pressed,
                    "Released mouse button in normal mode"
                );*/
                if *state == ElementState::Released {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::Nucl(nucl) if controller.data.borrow().is_suggested(&nucl) => {
                        Transition {
                            new_state: Some(Box::new(CenteringSuggestion {
                                nucl,
                                mouse_position: self.mouse_position,
                                bottom: controller.is_bottom(position.y),
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::Nucl(nucl) => Transition::consequence(Consequence::Select(nucl)),
                    _ => Transition::nothing(),
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let pivot_opt = if let ClickResult::Nucl(nucl) =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y))
                {
                    Some(nucl)
                } else {
                    None
                };
                Transition::consequence(Consequence::NewCandidate(pivot_opt))
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }

    fn transition_to(&self, controller: &Controller) {
        controller.data.borrow_mut().set_selected_helices(vec![]);
        controller.data.borrow_mut().set_free_end(None);
    }

    fn transition_from(&self, _controller: &Controller) {
        ()
    }
}

pub struct Translating {
    mouse_position: PhysicalPosition<f64>,
    world_clicked: Vec2,
    translation_pivots: Vec<FlatNucl>,
}

impl ControllerState for Translating {
    fn display(&self) -> String {
        String::from("Translating state")
    }
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in translating mode"
                );
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                controller.data.borrow_mut().end_movement();
                let mut translation_pivots = vec![];
                let mut rotation_pivots = vec![];
                for pivot in self.translation_pivots.iter() {
                    if let Some(rotation_pivot) = controller
                        .data
                        .borrow()
                        .get_rotation_pivot(pivot.helix.flat, &controller.get_camera(position.y))
                    {
                        translation_pivots.push(pivot.clone());
                        rotation_pivots.push(rotation_pivot);
                    }
                }

                if rotation_pivots.len() > 0 {
                    Transition {
                        new_state: Some(Box::new(ReleasedPivot {
                            mouse_position: self.mouse_position,
                            translation_pivots,
                            rotation_pivots,
                        })),
                        consequences: Consequence::Nothing,
                    }
                } else {
                    Transition {
                        new_state: Some(Box::new(NormalState {
                            mouse_position: self.mouse_position,
                        })),
                        consequences: Consequence::Nothing,
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(position.x as f32, position.y as f32);
                /*controller
                .data
                .borrow_mut()
                .translate_helix(Vec2::new(mouse_dx, mouse_dy));*/
                for pivot in self.translation_pivots.iter() {
                    controller
                        .data
                        .borrow_mut()
                        .snap_helix(*pivot, Vec2::new(x, y) - self.world_clicked);
                }
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }

    fn transition_from(&self, controller: &Controller) {
        controller.data.borrow_mut().end_movement()
    }

    fn transition_to(&self, controller: &Controller) {
        let helices = self.translation_pivots.iter().map(|p| p.helix).collect();
        controller.data.borrow_mut().set_selected_helices(helices)
    }
}

pub struct MovingCamera {
    mouse_position: PhysicalPosition<f64>,
    clicked_position_screen: PhysicalPosition<f64>,
    translation_pivots: Vec<FlatNucl>,
    rotation_pivots: Vec<Vec2>,
}

impl ControllerState for MovingCamera {
    fn display(&self) -> String {
        String::from("Moving camera")
    }
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Middle,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in translating mode"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                if self.rotation_pivots.len() > 0 {
                    Transition {
                        new_state: Some(Box::new(ReleasedPivot {
                            mouse_position: self.mouse_position,
                            translation_pivots: self.translation_pivots.clone(),
                            rotation_pivots: self.rotation_pivots.clone(),
                        })),
                        consequences: Consequence::Nothing,
                    }
                } else {
                    Transition {
                        new_state: Some(Box::new(NormalState {
                            mouse_position: self.mouse_position,
                        })),
                        consequences: Consequence::Nothing,
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let mouse_dx = (position.x as f32 - self.clicked_position_screen.x as f32)
                    / controller.area_size.width as f32;
                let mouse_dy = (position.y as f32 - self.clicked_position_screen.y as f32)
                    / controller.get_height() as f32;
                controller
                    .get_camera(self.clicked_position_screen.y)
                    .borrow_mut()
                    .process_mouse(mouse_dx, mouse_dy);
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }

    fn transition_from(&self, controller: &Controller) {
        controller.end_movement();
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }
}

pub struct ReleasedPivot {
    pub mouse_position: PhysicalPosition<f64>,
    pub translation_pivots: Vec<FlatNucl>,
    pub rotation_pivots: Vec<Vec2>,
}

impl ControllerState for ReleasedPivot {
    fn transition_to(&self, controller: &Controller) {
        let helices = self.translation_pivots.iter().map(|p| p.helix).collect();
        controller.data.borrow_mut().set_selected_helices(helices);

        let wheels = self
            .rotation_pivots
            .iter()
            .map(|p| CircleInstance::new(*p, WHEEL_RADIUS, -1, CIRCLE2D_GREY))
            .collect();
        controller.view.borrow_mut().set_wheels(wheels);
    }

    fn transition_from(&self, controller: &Controller) {
        controller.view.borrow_mut().set_wheels(vec![]);
    }

    fn display(&self) -> String {
        String::from("Released Pivot")
    }
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Pressed,
                ..
            } if controller.modifiers.shift() => {
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::CircleWidget { .. } => {
                        // Clicked on an other circle
                        Transition {
                            new_state: Some(Box::new(AddClickPivots {
                                translation_pivots: self.translation_pivots.clone(),
                                rotation_pivots: self.rotation_pivots.clone(),
                                mouse_position: self.mouse_position,
                                click_result,
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    _ => Transition {
                        new_state: Some(Box::new(LeavingPivot {
                            clicked_position_screen: self.mouse_position,
                            mouse_position: self.mouse_position,
                            translation_pivots: self.translation_pivots.clone(),
                            rotation_pivots: self.rotation_pivots.clone(),
                            shift: controller.modifiers.shift(),
                        })),
                        consequences: Consequence::Nothing,
                    },
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Pressed,
                    "Released mouse button in ReleasedPivot state"
                );*/
                if *state == ElementState::Released {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::CircleWidget { translation_pivot }
                        if ctrl(&controller.modifiers) =>
                    {
                        Transition {
                            new_state: Some(Box::new(FlipVisibility {
                                mouse_position: self.mouse_position,
                                helix: translation_pivot.helix,
                                apply_to_other: controller.modifiers.alt(),
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::HelixHandle { h_id, handle } => Transition {
                        new_state: Some(Box::new(TranslatingHandle { h_id, handle })),
                        consequences: Consequence::Nothing,
                    },
                    ClickResult::Nucl(nucl) => {
                        if controller.action_mode == ActionMode::Cut {
                            Transition {
                                new_state: Some(Box::new(Cutting {
                                    nucl,
                                    mouse_position: self.mouse_position,
                                    whole_strand: controller.modifiers.shift(),
                                })),
                                consequences: Consequence::Nothing,
                            }
                        } else {
                            let stick = if let ActionMode::Build(b) = controller.action_mode {
                                b
                            } else {
                                false
                            };
                            if let Some(builder) = controller.data.borrow().get_builder(nucl, stick)
                            {
                                Transition {
                                    new_state: Some(Box::new(InitBuilding {
                                        mouse_position: self.mouse_position,
                                        nucl,
                                        builder,
                                        end: controller.data.borrow().is_strand_end(nucl),
                                    })),
                                    consequences: Consequence::Nothing,
                                }
                            } else {
                                Transition {
                                    new_state: Some(Box::new(DraggingSelection {
                                        mouse_position: self.mouse_position,
                                        fixed_corner: self.mouse_position,
                                        adding: controller.modifiers.shift(),
                                    })),
                                    consequences: Consequence::Nothing,
                                }
                            }
                        }
                    }
                    ClickResult::CircleWidget { translation_pivot }
                        if self.translation_pivots.contains(&translation_pivot) =>
                    {
                        let clicked = controller.get_camera(position.y).borrow().screen_to_world(
                            self.mouse_position.x as f32,
                            self.mouse_position.y as f32,
                        );
                        Transition {
                            new_state: Some(Box::new(Translating {
                                translation_pivots: self.translation_pivots.clone(),
                                world_clicked: clicked.into(),
                                mouse_position: self.mouse_position,
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::CircleWidget { translation_pivot } => {
                        // Clicked on an other circle
                        let clicked = controller.get_camera(position.y).borrow().screen_to_world(
                            self.mouse_position.x as f32,
                            self.mouse_position.y as f32,
                        );
                        controller
                            .data
                            .borrow_mut()
                            .set_helix_selection(click_result, &controller.get_camera(position.y));
                        Transition {
                            new_state: Some(Box::new(Translating {
                                translation_pivots: vec![translation_pivot],
                                world_clicked: clicked.into(),
                                mouse_position: self.mouse_position,
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                    ClickResult::Nothing => Transition {
                        new_state: Some(Box::new(LeavingPivot {
                            clicked_position_screen: self.mouse_position,
                            mouse_position: self.mouse_position,
                            translation_pivots: self.translation_pivots.clone(),
                            rotation_pivots: self.rotation_pivots.clone(),
                            shift: controller.modifiers.shift(),
                        })),
                        consequences: Consequence::Nothing,
                    },
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Pressed,
                    "Released right mouse button in ReleasedPivot state"
                );*/
                if *state == ElementState::Released {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(Rotating::new(
                        self.translation_pivots.clone(),
                        self.rotation_pivots.clone(),
                        self.mouse_position,
                        self.mouse_position,
                    ))),
                    consequences: Consequence::Nothing,
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Middle,
                state,
                ..
            } => {
                if *state == ElementState::Released {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(MovingCamera {
                        mouse_position: self.mouse_position,
                        clicked_position_screen: self.mouse_position,
                        translation_pivots: self.translation_pivots.clone(),
                        rotation_pivots: self.rotation_pivots.clone(),
                    })),
                    consequences: Consequence::Nothing,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let pivot_opt = if let ClickResult::Nucl(nucl) =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y))
                {
                    Some(nucl)
                } else {
                    None
                };
                Transition::consequence(Consequence::NewCandidate(pivot_opt))
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

/// This state in entered when use user has clicked after realising a pivot. If the user moves
/// their mouse, go in moving camera mode without unselecting the helix. If the user release their
/// click without moving their mouse, clear selection
pub struct LeavingPivot {
    translation_pivots: Vec<FlatNucl>,
    rotation_pivots: Vec<Vec2>,
    clicked_position_screen: PhysicalPosition<f64>,
    mouse_position: PhysicalPosition<f64>,
    shift: bool,
}

impl ControllerState for LeavingPivot {
    fn transition_to(&self, controller: &Controller) {
        let wheels = self
            .rotation_pivots
            .iter()
            .map(|p| CircleInstance::new(*p, WHEEL_RADIUS, -1, CIRCLE2D_GREY))
            .collect();
        controller.view.borrow_mut().set_wheels(wheels);
    }

    fn transition_from(&self, controller: &Controller) {
        controller.view.borrow_mut().set_wheels(vec![]);
    }

    fn display(&self) -> String {
        String::from("Leaving Pivot")
    }
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*
                assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in LeavingPivot state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences: Consequence::ClearSelection,
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Pressed,
                    "Released right mouse button in ReleasedPivot state"
                );*/
                if *state == ElementState::Released {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(Rotating::new(
                        self.translation_pivots.clone(),
                        self.rotation_pivots.clone(),
                        self.mouse_position,
                        self.mouse_position,
                    ))),
                    consequences: Consequence::Nothing,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                if position_difference(self.clicked_position_screen, self.mouse_position) > 5. {
                    Transition {
                        new_state: Some(Box::new(DraggingSelection {
                            mouse_position: self.mouse_position,
                            fixed_corner: self.clicked_position_screen,
                            adding: self.shift,
                        })),
                        consequences: Consequence::Nothing,
                    }
                } else {
                    Transition::nothing()
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

pub struct Rotating {
    translation_pivots: Vec<FlatNucl>,
    rotation_pivots: Vec<Vec2>,
    clicked_position_screen: PhysicalPosition<f64>,
    mouse_position: PhysicalPosition<f64>,
    pivot_center: Vec2,
}

impl Rotating {
    pub fn new(
        translation_pivots: Vec<FlatNucl>,
        rotation_pivots: Vec<Vec2>,
        clicked_position_screen: PhysicalPosition<f64>,
        mouse_position: PhysicalPosition<f64>,
    ) -> Self {
        let mut min_x = rotation_pivots[0].x;
        let mut max_x = rotation_pivots[0].x;
        let mut min_y = rotation_pivots[0].y;
        let mut max_y = rotation_pivots[0].y;
        for p in rotation_pivots.iter() {
            min_x = min_x.min(p.x);
            max_x = max_x.max(p.x);
            min_y = min_y.min(p.y);
            max_y = max_y.max(p.y);
        }
        Self {
            translation_pivots,
            rotation_pivots,
            clicked_position_screen,
            mouse_position,
            pivot_center: Vec2::new((min_x + max_x) / 2., (min_y + max_y) / 2.),
        }
    }
}

impl ControllerState for Rotating {
    fn transition_to(&self, controller: &Controller) {
        let helices = self.translation_pivots.iter().map(|p| p.helix).collect();
        controller.data.borrow_mut().set_selected_helices(helices);

        let wheels = self
            .rotation_pivots
            .iter()
            .map(|p| CircleInstance::new(*p, WHEEL_RADIUS, -1, CIRCLE2D_GREY))
            .collect();
        controller.view.borrow_mut().set_wheels(wheels);
    }

    fn transition_from(&self, controller: &Controller) {
        controller.data.borrow_mut().end_movement();
        controller.view.borrow_mut().set_wheels(vec![]);
    }

    fn display(&self) -> String {
        String::from("Rotating")
    }
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Rotating state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(ReleasedPivot {
                        translation_pivots: self.translation_pivots.clone(),
                        rotation_pivots: self.rotation_pivots.clone(),
                        mouse_position: self.mouse_position,
                    })),
                    consequences: Consequence::Nothing,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let angle = {
                    let (x, y) = controller
                        .get_camera(self.clicked_position_screen.y)
                        .borrow()
                        .screen_to_world(position.x as f32, position.y as f32);
                    let (old_x, old_y) = controller
                        .get_camera(self.clicked_position_screen.y)
                        .borrow()
                        .screen_to_world(
                            self.clicked_position_screen.x as f32,
                            self.clicked_position_screen.y as f32,
                        );
                    (y - self.pivot_center.y).atan2(x - self.pivot_center.x)
                        - (old_y - self.pivot_center.y).atan2(old_x - self.pivot_center.x)
                };
                for i in 0..self.rotation_pivots.len() {
                    controller.data.borrow_mut().rotate_helix(
                        self.translation_pivots[i].helix,
                        self.pivot_center,
                        angle,
                    );
                }
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct InitCutting {
    mouse_position: PhysicalPosition<f64>,
    nucl: FlatNucl,
}

impl ControllerState for InitCutting {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Init Cutting")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Init Building state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences: Consequence::Cut(self.nucl),
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::Nucl(nucl2) if nucl2 == self.nucl => Transition::nothing(),
                    _ => {
                        let strand_id = controller.data.borrow().get_strand_id(self.nucl).unwrap();
                        Transition {
                            new_state: Some(Box::new(MovingFreeEnd {
                                mouse_position: self.mouse_position,
                                from: self.nucl,
                                prime3: true,
                                strand_id,
                            })),
                            consequences: Consequence::CutFreeEnd(
                                self.nucl,
                                Some(FreeEnd {
                                    strand_id,
                                    point: Vec2::new(x, y),
                                    prime3: true,
                                }),
                            ),
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct InitAttachement {
    mouse_position: PhysicalPosition<f64>,
    from: FlatNucl,
    to: FlatNucl,
}

impl ControllerState for InitAttachement {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Init Attachement")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Init Building state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                println!("from {:?} to {:?}", self.from, self.to);
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences: Consequence::Xover(self.from, self.to),
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::Nucl(nucl2) if nucl2 == self.from => Transition::nothing(),
                    _ => {
                        let strand_id = controller.data.borrow().get_strand_id(self.from).unwrap();
                        Transition {
                            new_state: Some(Box::new(MovingFreeEnd {
                                mouse_position: self.mouse_position,
                                from: self.from,
                                prime3: controller.data.borrow().is_strand_end(self.from).unwrap(),
                                strand_id,
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct InitBuilding {
    mouse_position: PhysicalPosition<f64>,
    builder: StrandBuilder,
    nucl: FlatNucl,
    end: Option<bool>,
}

impl ControllerState for InitBuilding {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Init Building")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Init Building state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                if let Some(attachement) = controller.data.borrow().attachable_neighbour(self.nucl)
                {
                    Transition {
                        new_state: Some(Box::new(NormalState {
                            mouse_position: self.mouse_position,
                        })),
                        consequences: Consequence::Xover(self.nucl, attachement),
                    }
                } else {
                    Transition {
                        new_state: Some(Box::new(NormalState {
                            mouse_position: self.mouse_position,
                        })),
                        consequences: Consequence::Cut(self.nucl),
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::Nucl(FlatNucl {
                        helix,
                        position,
                        forward,
                    }) if helix == self.nucl.helix && forward == self.nucl.forward => {
                        if position != self.nucl.position {
                            self.builder.move_to(position);
                            controller.data.borrow_mut().notify_update();
                            Transition {
                                new_state: Some(Box::new(Building {
                                    mouse_position: self.mouse_position,
                                    builder: self.builder.clone(),
                                    nucl: self.nucl,
                                    can_attach: true,
                                })),
                                consequences: Consequence::Nothing,
                            }
                        } else {
                            Transition::nothing()
                        }
                    }
                    ClickResult::Nucl(nucl)
                        if controller.data.borrow().can_cross_to(self.nucl, nucl) =>
                    {
                        self.builder.reset();
                        controller.data.borrow_mut().notify_update();
                        Transition {
                            new_state: Some(Box::new(Crossing {
                                mouse_position: self.mouse_position,
                                from: self.nucl,
                                to: nucl,
                                strand_id: self.builder.get_strand_id(),
                                from3prime: self.end.expect("from3prime"),
                                cut: false,
                            })),
                            consequences: Consequence::FreeEnd(self.end.map(|b| FreeEnd {
                                strand_id: self.builder.get_strand_id(),
                                point: Vec2::new(x, y),
                                prime3: b,
                            })),
                        }
                    }
                    _ => {
                        if let Some(prime3) = self.end {
                            Transition {
                                new_state: Some(Box::new(MovingFreeEnd {
                                    mouse_position: self.mouse_position,
                                    from: self.nucl,
                                    prime3,
                                    strand_id: self.builder.get_strand_id(),
                                })),
                                consequences: Consequence::FreeEnd(Some(FreeEnd {
                                    strand_id: self.builder.get_strand_id(),
                                    point: Vec2::new(x, y),
                                    prime3,
                                })),
                            }
                        } else {
                            let prime3 = controller
                                .data
                                .borrow()
                                .is_xover_end(&self.nucl)
                                .unwrap_or(true);
                            Transition {
                                new_state: Some(Box::new(MovingFreeEnd {
                                    mouse_position: self.mouse_position,
                                    from: self.nucl,
                                    prime3,
                                    strand_id: self.builder.get_strand_id(),
                                })),
                                consequences: Consequence::CutFreeEnd(
                                    self.nucl,
                                    Some(FreeEnd {
                                        strand_id: self.builder.get_strand_id(),
                                        point: Vec2::new(x, y),
                                        prime3,
                                    }),
                                ),
                            }
                        }
                    }
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct MovingFreeEnd {
    mouse_position: PhysicalPosition<f64>,
    from: FlatNucl,
    strand_id: usize,
    prime3: bool,
}

impl ControllerState for MovingFreeEnd {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Moving Free End")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Moving Free End state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences: Consequence::FreeEnd(None),
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click_result =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                match click_result {
                    ClickResult::Nucl(nucl) if nucl == self.from => Transition::nothing(),
                    ClickResult::Nucl(nucl)
                        if controller.data.borrow().can_cross_to(self.from, nucl) =>
                    {
                        controller.data.borrow_mut().notify_update();
                        Transition {
                            new_state: Some(Box::new(Crossing {
                                mouse_position: self.mouse_position,
                                from: self.from,
                                to: nucl,
                                from3prime: self.prime3,
                                strand_id: self.strand_id,
                                cut: false,
                            })),
                            consequences: Consequence::FreeEnd(Some(FreeEnd {
                                strand_id: self.strand_id,
                                point: Vec2::new(x, y),
                                prime3: self.prime3,
                            })),
                        }
                    }
                    ClickResult::Nucl(nucl)
                        if controller.data.borrow().can_cut_cross_to(self.from, nucl) =>
                    {
                        controller.data.borrow_mut().notify_update();
                        Transition {
                            new_state: Some(Box::new(Crossing {
                                mouse_position: self.mouse_position,
                                from: self.from,
                                to: nucl,
                                from3prime: self.prime3,
                                strand_id: self.strand_id,
                                cut: true,
                            })),
                            consequences: Consequence::FreeEnd(Some(FreeEnd {
                                strand_id: self.strand_id,
                                point: Vec2::new(x, y),
                                prime3: self.prime3,
                            })),
                        }
                    }
                    _ => Transition::consequence(Consequence::FreeEnd(Some(FreeEnd {
                        strand_id: self.strand_id,
                        point: Vec2::new(x, y),
                        prime3: self.prime3,
                    }))),
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct Building {
    mouse_position: PhysicalPosition<f64>,
    builder: StrandBuilder,
    nucl: FlatNucl,
    can_attach: bool,
}

impl ControllerState for Building {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Building")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Building state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                if self.can_attach {
                    if let Some(attachement) =
                        controller.data.borrow().attachable_neighbour(self.nucl)
                    {
                        return Transition {
                            new_state: Some(Box::new(NormalState {
                                mouse_position: self.mouse_position,
                            })),
                            consequences: Consequence::Xover(self.nucl, attachement),
                        };
                    }
                }
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences: Consequence::Built(Box::new(self.builder.clone())),
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click_unbounded_helix(x, y, self.nucl.helix);
                if nucl != self.nucl {
                    self.can_attach = false;
                }
                match nucl {
                    FlatNucl {
                        helix, position, ..
                    } if helix == self.nucl.helix => {
                        self.builder.move_to(position);
                        controller.data.borrow_mut().notify_update();
                        Transition::consequence(Consequence::FreeEnd(None))
                    }
                    _ => Transition::nothing(),
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

pub struct Crossing {
    mouse_position: PhysicalPosition<f64>,
    from: FlatNucl,
    to: FlatNucl,
    from3prime: bool,
    strand_id: usize,
    cut: bool,
}

impl ControllerState for Crossing {
    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Crossing")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Crossing state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences: if self.cut {
                        Consequence::CutCross(self.from, self.to)
                    } else {
                        Consequence::Xover(self.from, self.to)
                    },
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                if nucl != ClickResult::Nucl(self.to) {
                    Transition {
                        new_state: Some(Box::new(MovingFreeEnd {
                            mouse_position: self.mouse_position,
                            from: self.from,
                            prime3: self.from3prime,
                            strand_id: self.strand_id,
                        })),
                        consequences: Consequence::Nothing,
                    }
                } else {
                    Transition::consequence(Consequence::FreeEnd(Some(FreeEnd {
                        strand_id: self.strand_id,
                        point: Vec2::new(x, y),
                        prime3: self.from3prime,
                    })))
                }
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct Cutting {
    mouse_position: PhysicalPosition<f64>,
    nucl: FlatNucl,
    whole_strand: bool,
}

impl ControllerState for Cutting {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Cutting")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                let consequences = if nucl == ClickResult::Nucl(self.nucl) {
                    if self.whole_strand {
                        Consequence::RmStrand(self.nucl)
                    } else {
                        Consequence::Cut(self.nucl)
                    }
                } else {
                    Consequence::Nothing
                };
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct RmHelix {
    mouse_position: PhysicalPosition<f64>,
    helix: FlatHelix,
}

impl ControllerState for RmHelix {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("RmHelix")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                let consequences = if let ClickResult::CircleWidget { translation_pivot } = nucl {
                    if translation_pivot.helix == self.helix {
                        Consequence::RmHelix(self.helix)
                    } else {
                        Consequence::Nothing
                    }
                } else {
                    Consequence::Nothing
                };
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct FlipGroup {
    mouse_position: PhysicalPosition<f64>,
    helix: FlatHelix,
}

impl ControllerState for FlipGroup {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("FlipGroup")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                let consequences = if let ClickResult::CircleWidget { translation_pivot } = nucl {
                    if translation_pivot.helix == self.helix {
                        Consequence::FlipGroup(self.helix)
                    } else {
                        Consequence::Nothing
                    }
                } else {
                    Consequence::Nothing
                };
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct FlipVisibility {
    mouse_position: PhysicalPosition<f64>,
    helix: FlatHelix,
    apply_to_other: bool,
}

impl ControllerState for FlipVisibility {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("RmHelix")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                let consequences = if let ClickResult::CircleWidget { translation_pivot } = nucl {
                    if translation_pivot.helix == self.helix {
                        Consequence::FlipVisibility(self.helix, self.apply_to_other)
                    } else {
                        Consequence::Nothing
                    }
                } else {
                    Consequence::Nothing
                };
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct FollowingSuggestion {
    mouse_position: PhysicalPosition<f64>,
    nucl: FlatNucl,
    double: bool,
}

impl ControllerState for FollowingSuggestion {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Following Suggestion")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                let consequences = if let ClickResult::Nucl(nucl) = nucl {
                    if nucl == self.nucl {
                        Consequence::FollowingSuggestion(self.nucl, self.double)
                    } else {
                        Consequence::Nothing
                    }
                } else {
                    Consequence::Nothing
                };
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct CenteringSuggestion {
    mouse_position: PhysicalPosition<f64>,
    nucl: FlatNucl,
    bottom: bool,
}

impl ControllerState for CenteringSuggestion {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("CenteringSuggestion")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Right,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                let consequences = if let ClickResult::Nucl(nucl) = nucl {
                    if nucl == self.nucl {
                        let nucl = controller.data.borrow().get_best_suggestion(self.nucl);
                        Consequence::Centering(nucl.unwrap_or(self.nucl), !self.bottom)
                    } else {
                        Consequence::Nothing
                    }
                } else {
                    Consequence::Nothing
                };
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct Pasting {
    mouse_position: PhysicalPosition<f64>,
    nucl: Option<FlatNucl>,
}

impl ControllerState for Pasting {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("Pasting")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let nucl =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                let consequences = if self.nucl.is_none() {
                    Consequence::PasteRequest(self.nucl)
                } else if nucl == ClickResult::Nucl(self.nucl.unwrap()) {
                    Consequence::PasteRequest(self.nucl)
                } else {
                    Consequence::Nothing
                };
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: self.mouse_position,
                    })),
                    consequences,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

/// The user is drawing a selection
struct DraggingSelection {
    pub mouse_position: PhysicalPosition<f64>,
    pub fixed_corner: PhysicalPosition<f64>,
    pub adding: bool,
}

impl ControllerState for DraggingSelection {
    fn display(&self) -> String {
        format!("Dragging Selection {}", self.adding)
    }
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state: ElementState::Released,
                ..
            } => {
                let valid_rectangle = controller.is_bottom(self.fixed_corner.y)
                    == controller.is_bottom(self.mouse_position.y);
                let corner1_world = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.fixed_corner.x as f32, self.fixed_corner.y as f32);
                let corner2_world = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let (translation_pivots, rotation_pivots) = if valid_rectangle {
                    controller.data.borrow_mut().select_rectangle(
                        corner1_world.into(),
                        corner2_world.into(),
                        &controller.get_camera(position.y),
                        self.adding,
                    )
                } else {
                    (vec![], vec![])
                };
                if translation_pivots.len() > 0 {
                    Transition {
                        new_state: Some(Box::new(ReleasedPivot {
                            translation_pivots,
                            rotation_pivots,
                            mouse_position: self.mouse_position,
                        })),
                        consequences: Consequence::ReleasedSelection(
                            corner1_world.into(),
                            corner2_world.into(),
                        ),
                    }
                } else {
                    Transition {
                        new_state: Some(Box::new(NormalState {
                            mouse_position: self.mouse_position,
                        })),
                        consequences: Consequence::ReleasedSelection(
                            corner1_world.into(),
                            corner2_world.into(),
                        ),
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                if position.x < controller.area_size.width as f64
                    && position.x >= 0.
                    && position.y <= controller.area_size.height as f64
                    && position.y >= 0.
                {
                    self.mouse_position = position;
                }
                Transition::consequence(Consequence::DrawingSelection(
                    self.fixed_corner,
                    self.mouse_position,
                ))
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }

    fn transition_from(&self, controller: &Controller) {
        controller.end_movement();
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }
}

struct AddClick {
    mouse_position: PhysicalPosition<f64>,
    click_result: ClickResult,
}

impl ControllerState for AddClick {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("AddClick")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                if let ClickResult::CircleWidget { .. } = click {
                    if let Some((translation_pivots, rotation_pivots)) = controller
                        .data
                        .borrow_mut()
                        .add_helix_selection(click.clone(), &controller.get_camera(position.y))
                        .filter(|_| click == self.click_result)
                    {
                        Transition {
                            new_state: Some(Box::new(ReleasedPivot {
                                mouse_position: position,
                                translation_pivots,
                                rotation_pivots,
                            })),
                            consequences: Consequence::SelectionChanged,
                        }
                    } else {
                        Transition {
                            new_state: Some(Box::new(NormalState {
                                mouse_position: self.mouse_position,
                            })),
                            consequences: Consequence::Nothing,
                        }
                    }
                } else {
                    let consequences = if click == self.click_result {
                        Consequence::AddClick(click)
                    } else {
                        Consequence::Nothing
                    };
                    Transition {
                        new_state: Some(Box::new(NormalState {
                            mouse_position: self.mouse_position,
                        })),
                        consequences,
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct AddClickPivots {
    mouse_position: PhysicalPosition<f64>,
    translation_pivots: Vec<FlatNucl>,
    rotation_pivots: Vec<Vec2>,
    click_result: ClickResult,
}

impl ControllerState for AddClickPivots {
    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }

    fn display(&self) -> String {
        String::from("AddClick")
    }

    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                /*assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in Cutting state"
                );*/
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(self.mouse_position.x as f32, self.mouse_position.y as f32);
                let click =
                    controller
                        .data
                        .borrow()
                        .get_click(x, y, &controller.get_camera(position.y));
                if click == self.click_result {
                    if let Some((translation_pivots, rotation_pivots)) = controller
                        .data
                        .borrow_mut()
                        .add_helix_selection(click, &controller.get_camera(position.y))
                    {
                        Transition {
                            new_state: Some(Box::new(ReleasedPivot {
                                mouse_position: self.mouse_position,
                                translation_pivots,
                                rotation_pivots,
                            })),
                            consequences: Consequence::SelectionChanged,
                        }
                    } else {
                        Transition {
                            new_state: Some(Box::new(ReleasedPivot {
                                mouse_position: self.mouse_position,
                                translation_pivots: self.translation_pivots.clone(),
                                rotation_pivots: self.rotation_pivots.clone(),
                            })),
                            consequences: Consequence::SelectionChanged,
                        }
                    }
                } else {
                    Transition {
                        new_state: Some(Box::new(ReleasedPivot {
                            mouse_position: self.mouse_position,
                            translation_pivots: self.translation_pivots.clone(),
                            rotation_pivots: self.rotation_pivots.clone(),
                        })),
                        consequences: Consequence::Nothing,
                    }
                }
            }
            WindowEvent::CursorMoved { .. } => {
                self.mouse_position = position;
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, self.mouse_position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }
}

struct TranslatingHandle {
    h_id: FlatHelix,
    handle: super::super::data::HelixHandle,
}

impl ControllerState for TranslatingHandle {
    fn display(&self) -> String {
        String::from("Translating state")
    }
    fn input(
        &mut self,
        event: &WindowEvent,
        position: PhysicalPosition<f64>,
        controller: &Controller,
    ) -> Transition {
        match event {
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => {
                assert!(
                    *state == ElementState::Released,
                    "Pressed mouse button in translating mode"
                );
                if *state == ElementState::Pressed {
                    return Transition::nothing();
                }
                Transition {
                    new_state: Some(Box::new(NormalState {
                        mouse_position: position,
                    })),
                    consequences: Consequence::Nothing,
                }
            }
            WindowEvent::CursorMoved { .. } => {
                let (x, y) = controller
                    .get_camera(position.y)
                    .borrow()
                    .screen_to_world(position.x as f32, position.y as f32);
                /*controller
                .data
                .borrow_mut()
                .translate_helix(Vec2::new(mouse_dx, mouse_dy));*/
                controller
                    .data
                    .borrow_mut()
                    .move_handle(self.h_id, self.handle, Vec2::new(x, y));
                Transition::nothing()
            }
            WindowEvent::KeyboardInput { .. } => {
                controller.process_keyboard(event);
                Transition::nothing()
            }
            WindowEvent::MouseWheel { delta, .. } => {
                controller
                    .get_camera(position.y)
                    .borrow_mut()
                    .process_scroll(delta, position);
                Transition::nothing()
            }
            _ => Transition::nothing(),
        }
    }

    fn transition_from(&self, _controller: &Controller) {
        ()
    }

    fn transition_to(&self, _controller: &Controller) {
        ()
    }
}

fn position_difference(a: PhysicalPosition<f64>, b: PhysicalPosition<f64>) -> f64 {
    (a.x - b.x).abs().max((a.y - b.y).abs())
}

fn ctrl(modifiers: &ModifiersState) -> bool {
    if cfg!(target_os = "macos") {
        modifiers.logo()
    } else {
        modifiers.ctrl()
    }
}
