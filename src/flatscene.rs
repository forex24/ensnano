//! This module handles the 2D view

use crate::design::Design;
use crate::mediator;
use crate::{DrawArea, PhySize, WindowEvent};
use iced_wgpu::wgpu;
use iced_winit::winit;
use mediator::{ActionMode, Application, Notification};
use std::cell::RefCell;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use wgpu::{Device, Queue};
use winit::dpi::PhysicalPosition;

mod camera;
mod controller;
mod data;
mod view;
use camera::{Camera, Globals};
use controller::Controller;
use data::{Data, Helix, Nucl, Strand};
use view::View;

type ViewPtr = Rc<RefCell<View>>;
type DataPtr = Rc<RefCell<Data>>;
type CameraPtr = Rc<RefCell<Camera>>;

pub struct FlatScene {
    view: Vec<ViewPtr>,
    data: Vec<DataPtr>,
    controller: Vec<Controller>,
    area: DrawArea,
    window_size: PhySize,
    selected_design: usize,
    device: Rc<Device>,
    queue: Rc<Queue>,
    action_mode: ActionMode,
}

impl FlatScene {
    pub fn new(device: Rc<Device>, queue: Rc<Queue>, window_size: PhySize, area: DrawArea) -> Self {
        Self {
            view: Vec::new(),
            data: Vec::new(),
            controller: Vec::new(),
            area,
            window_size,
            selected_design: 0,
            device,
            queue,
            action_mode: ActionMode::Normal,
        }
    }

    pub fn add_design(&mut self, design: Arc<Mutex<Design>>) {
        let globals = Globals {
            resolution: [self.area.size.width as f32, self.area.size.height as f32],
            scroll_offset: [0., 0.],
            zoom: 100.,
            _padding: 0.,
        };
        let camera = Rc::new(RefCell::new(Camera::new(globals)));
        let view = Rc::new(RefCell::new(View::new(
            self.device.clone(),
            self.queue.clone(),
            self.window_size,
            camera.clone(),
        )));
        let data = Rc::new(RefCell::new(Data::new(view.clone(), design)));
        let controller = Controller::new(
            view.clone(),
            data.clone(),
            self.window_size,
            self.area.size,
            camera,
        );
        self.view.push(view);
        self.data.push(data);
        self.controller.push(controller);
    }

    pub fn draw_view(&mut self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if let Some(view) = self.view.get(self.selected_design) {
            self.data[self.selected_design]
                .borrow_mut()
                .perform_update();
            view.borrow_mut().draw(encoder, target, self.area);
        }
    }

    pub fn resize(&mut self, window_size: PhySize, area: DrawArea) {
        self.window_size = window_size;
        self.area = area;
        for view in self.view.iter() {
            view.borrow_mut().resize(window_size);
        }
        for controller in self.controller.iter_mut() {
            controller.resize(window_size, area.size);
        }
    }

    pub fn change_action_mode(&mut self, action_mode: ActionMode) {
        self.action_mode = action_mode
    }

    pub fn input(&mut self, event: &WindowEvent, cursor_position: PhysicalPosition<f64>) {
        if let Some(controller) = self.controller.get_mut(self.selected_design) {
            let consequence = controller.input(event, cursor_position);
            use controller::Consequence::*;
            match consequence {
                Clicked(x, y) => match self.action_mode {
                    ActionMode::Rotate => {
                        let nucl = self.data[self.selected_design].borrow().get_click(x, y);
                        self.data[self.selected_design]
                            .borrow_mut()
                            .set_selected_helix(nucl.map(|n| n.helix));
                        if let Some(nucl) = nucl {
                            let pivot = self.data[self.selected_design]
                                .borrow()
                                .get_pivot_position(nucl.helix, nucl.position);
                            self.controller[self.selected_design].set_pivot(pivot.unwrap())
                        } else {
                            self.controller[self.selected_design].notify_unselect()
                        }
                    }
                    ActionMode::Translate => {
                        let nucl = self.data[self.selected_design].borrow().get_click(x, y);
                        self.data[self.selected_design]
                            .borrow_mut()
                            .set_selected_helix(nucl.map(|n| n.helix));
                        if nucl.is_some() {
                            self.controller[self.selected_design].notify_select()
                        } else {
                            self.controller[self.selected_design].notify_unselect()
                        }
                    }
                    _ => (),
                },
                Translated(x, y) => self.data[self.selected_design]
                    .borrow_mut()
                    .translate_helix(ultraviolet::Vec2::new(x, y)),
                Rotated(pivot, angle) => self.data[self.selected_design]
                    .borrow_mut()
                    .rotate_helix(pivot, angle),
                MovementEnded => self.data[self.selected_design].borrow_mut().end_movement(),
                _ => (),
            }
        }
    }

    pub fn needs_redraw(&self) -> bool {
        if let Some(view) = self.view.get(self.selected_design) {
            self.data[self.selected_design]
                .borrow_mut()
                .perform_update();
            view.borrow().needs_redraw()
        } else {
            false
        }
    }
}

impl Application for FlatScene {
    fn on_notify(&mut self, notification: Notification) {
        match notification {
            Notification::NewDesign(design) => self.add_design(design),
            _ => (),
        }
    }
}