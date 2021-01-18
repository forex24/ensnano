//! This modules handles the separation of the window into different regions.
//!
//! The layout manager split the window into different regions and attribute each region to an
//! an application or a gui component.
//!
//! In addition, the multiplexer holds a Vec of overlays which are floating regions.
//!
//! When an event is recieved by the window, the multiplexer is in charge of forwarding it to the
//! appropriate application, gui component or overlay. The multiplexer also handles some events
//! like resizing events of keyboard input that should be handled independently of the foccussed
//! region.
//!
//!
//!
//! The multiplexer is also in charge of drawing to the frame.
use crate::gui::Requests;
use crate::mediator::{ActionMode, SelectionMode};
use crate::utils::texture::SampledTexture;
use crate::PhySize;
use iced_wgpu::wgpu;
use iced_winit::winit;
use iced_winit::winit::event::*;
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use wgpu::Device;
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{ElementState, MouseButton, WindowEvent},
};

mod layout_manager;
use layout_manager::LayoutTree;

/// A structure that represents an area on which an element can be drawn
#[derive(Clone, Copy, Debug)]
pub struct DrawArea {
    /// The top left corner of the element
    pub position: PhysicalPosition<u32>,
    /// The *physical* size of the element
    pub size: PhySize,
}

/// The different elements represented on the scene. Each element is instanciated once.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Hash)]
pub enum ElementType {
    /// The top menu bar
    TopBar,
    /// The 3D scene
    Scene,
    /// The flat Scene
    FlatScene,
    /// The Left Panel
    LeftPanel,
    /// The status bar
    StatusBar,
    GridPanel,
    /// An overlay area
    Overlay(usize),
    /// An area that has not been attributed to an element
    Unattributed,
}

impl ElementType {
    pub fn is_gui(&self) -> bool {
        match self {
            ElementType::TopBar | ElementType::LeftPanel | ElementType::StatusBar => true,
            _ => false,
        }
    }

    pub fn is_scene(&self) -> bool {
        match self {
            ElementType::Scene | ElementType::FlatScene => true,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SplitMode {
    Flat,
    Scene3D,
    Both,
}

/// A structure that handles the division of the window into different `DrawArea`
pub struct Multiplexer {
    /// The *physical* size of the window
    pub window_size: PhySize,
    /// The scale factor of the window
    pub scale_factor: f64,
    /// The object mapping pixels to drawing areas
    layout_manager: LayoutTree,
    /// The Element on which the mouse cursor is currently on.
    focus: Option<ElementType>,
    /// `true` if the left button of the mouse was pressed on the window, not released since and
    /// the cursor has not left the window since
    mouse_clicked: bool,
    /// The *physical* position of the cursor on the focus area
    cursor_position: PhysicalPosition<f64>,
    /// The area that are drawn on top of the application
    overlays: Vec<Overlay>,
    /// The texture on which the scene is rendered
    scene_texture: Option<SampledTexture>,
    /// The texture on which the top bar gui is rendered
    top_bar_texture: Option<SampledTexture>,
    /// The texture on which the left pannel is rendered
    left_pannel_texture: Option<SampledTexture>,
    /// The textures on which the overlays are rendered
    overlays_textures: Vec<SampledTexture>,
    /// The texture on wich the grid is rendered
    grid_panel_texture: Option<SampledTexture>,
    /// The texutre on which the flat scene is rendered,
    status_bar_texture: Option<SampledTexture>,
    flat_scene_texture: Option<SampledTexture>,
    /// The pointer the node that separate the left pannel from the scene
    left_pannel_split: usize,
    device: Rc<Device>,
    pipeline: Option<wgpu::RenderPipeline>,
    split_mode: SplitMode,
    requests: Arc<Mutex<Requests>>,
}

impl Multiplexer {
    /// Create a new multiplexer for a window with size `window_size`.
    pub fn new(
        window_size: PhySize,
        scale_factor: f64,
        device: Rc<Device>,
        requests: Arc<Mutex<Requests>>,
    ) -> Self {
        let mut layout_manager = LayoutTree::new();
        let (top_bar, scene) = layout_manager.hsplit(0, 0.05);
        let left_pannel_split = scene;
        let left_pannel_prop = proportion(0.2, 200., window_size.width as f64);
        let (left_pannel, scene) = layout_manager.vsplit(scene, left_pannel_prop);
        let (scene, status_bar) = layout_manager.hsplit(scene, 0.90);
        //let (scene, grid_panel) = layout_manager.hsplit(scene, 0.8);
        layout_manager.attribute_element(top_bar, ElementType::TopBar);
        layout_manager.attribute_element(scene, ElementType::Scene);
        layout_manager.attribute_element(status_bar, ElementType::StatusBar);
        layout_manager.attribute_element(left_pannel, ElementType::LeftPanel);
        //layout_manager.attribute_element(grid_panel, ElementType::GridPanel);
        let mut ret = Self {
            window_size,
            scale_factor,
            layout_manager,
            focus: None,
            mouse_clicked: false,
            cursor_position: PhysicalPosition::new(-1., -1.),
            scene_texture: None,
            flat_scene_texture: None,
            top_bar_texture: None,
            left_pannel_texture: None,
            grid_panel_texture: None,
            status_bar_texture: None,
            overlays: Vec::new(),
            overlays_textures: Vec::new(),
            device,
            pipeline: None,
            split_mode: SplitMode::Scene3D,
            requests,
            left_pannel_split,
        };
        ret.generate_textures();
        ret
    }

    /// Return a view of the texture on which the element must be rendered
    pub fn get_texture_view(&self, element_type: ElementType) -> Option<&wgpu::TextureView> {
        match element_type {
            ElementType::Scene => self.scene_texture.as_ref().map(|t| &t.view),
            ElementType::LeftPanel => self.left_pannel_texture.as_ref().map(|t| &t.view),
            ElementType::TopBar => self.top_bar_texture.as_ref().map(|t| &t.view),
            ElementType::Overlay(n) => Some(&self.overlays_textures[n].view),
            ElementType::GridPanel => self.grid_panel_texture.as_ref().map(|t| &t.view),
            ElementType::FlatScene => self.flat_scene_texture.as_ref().map(|t| &t.view),
            ElementType::StatusBar => self.status_bar_texture.as_ref().map(|t| &t.view),
            ElementType::Unattributed => unreachable!(),
        }
    }

    pub fn draw(&mut self, encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView) {
        if self.pipeline.is_none() {
            let bg_layout = &self.top_bar_texture.as_ref().unwrap().bg_layout;
            self.pipeline = Some(create_pipeline(self.device.as_ref(), bg_layout));
        }
        let clear_color = wgpu::Color {
            r: 0.,
            g: 0.,
            b: 0.,
            a: 1.,
        };

        let msaa_texture = None;

        let attachment = if msaa_texture.is_some() {
            msaa_texture.as_ref().unwrap()
        } else {
            target
        };

        let resolve_target = if msaa_texture.is_some() {
            Some(target)
        } else {
            None
        };

        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[wgpu::RenderPassColorAttachmentDescriptor {
                attachment,
                resolve_target,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(clear_color),
                    store: true,
                },
            }],
            depth_stencil_attachment: None,
        });
        if self.window_size.width > 0 && self.window_size.height > 0 {
            for element in [
                ElementType::TopBar,
                ElementType::LeftPanel,
                ElementType::GridPanel,
                ElementType::Scene,
                ElementType::FlatScene,
                ElementType::StatusBar,
            ]
            .iter()
            {
                if let Some(area) = self.get_draw_area(*element) {
                    render_pass.set_bind_group(0, self.get_bind_group(element), &[]);

                    render_pass.set_viewport(
                        area.position.x as f32,
                        area.position.y as f32,
                        area.size.width as f32,
                        area.size.height as f32,
                        0.0,
                        1.0,
                    );
                    render_pass.set_scissor_rect(
                        area.position.x,
                        area.position.y,
                        area.size.width,
                        area.size.height,
                    );
                    render_pass.set_pipeline(self.pipeline.as_ref().unwrap());
                    render_pass.draw(0..4, 0..1);
                }
            }
        }
    }

    fn get_bind_group(&self, element_type: &ElementType) -> &wgpu::BindGroup {
        match element_type {
            ElementType::TopBar => &self.top_bar_texture.as_ref().unwrap().bind_group,
            ElementType::LeftPanel => &self.left_pannel_texture.as_ref().unwrap().bind_group,
            ElementType::Scene => &self.scene_texture.as_ref().unwrap().bind_group,
            ElementType::FlatScene => &self.flat_scene_texture.as_ref().unwrap().bind_group,
            ElementType::GridPanel => &self.grid_panel_texture.as_ref().unwrap().bind_group,
            ElementType::Overlay(n) => &self.overlays_textures[*n].bind_group,
            ElementType::StatusBar => &self.status_bar_texture.as_ref().unwrap().bind_group,
            ElementType::Unattributed => unreachable!(),
        }
    }

    /// Return the drawing area attributed to an element.
    pub fn get_draw_area(&self, element_type: ElementType) -> Option<DrawArea> {
        use ElementType::Overlay;
        let (position, size) = if let Overlay(n) = element_type {
            (self.overlays[n].position, self.overlays[n].size)
        } else {
            let (left, top, right, bottom) = self.layout_manager.get_area(element_type)?;
            let top = top * self.window_size.height as f64;
            let left = left * self.window_size.width as f64;
            let bottom = bottom * self.window_size.height as f64;
            let right = right * self.window_size.width as f64;

            (
                PhysicalPosition::new(left, top).cast::<u32>(),
                PhysicalSize::new(right - left, bottom - top).cast::<u32>(),
            )
        };
        Some(DrawArea { position, size })
    }

    /// Forwards event to the elment on which they happen.
    pub fn event(
        &mut self,
        event: WindowEvent<'static>,
    ) -> Option<(WindowEvent<'static>, ElementType)> {
        let mut focus_changed = false;
        let mut device_id_msg = None;
        let mut captured = false;
        match &event {
            WindowEvent::CursorMoved {
                position,
                device_id,
                ..
            } => {
                let &PhysicalPosition { x, y } = position;
                if x > 0.0 || y > 0.0 {
                    let element = self.pixel_to_element(*position);
                    let area = self
                        .get_draw_area(element)
                        .expect(&format!("Element does not exsist {:?}", element));

                    if !self.mouse_clicked {
                        self.focus = Some(element);
                    } else if self.focus != Some(element) {
                        focus_changed = true;
                        device_id_msg = Some(*device_id);
                    }
                    self.cursor_position.x = position.x - area.position.cast::<f64>().x;
                    self.cursor_position.y = position.y - area.position.cast::<f64>().y;
                }
            }
            WindowEvent::Resized(new_size) => {
                self.window_size = *new_size;
                self.resize(*new_size);
                if self.window_size.width > 0 && self.window_size.height > 0 {
                    self.generate_textures();
                }
            }
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor = *scale_factor;
                if self.window_size.width > 0 && self.window_size.height > 0 {
                    self.generate_textures();
                }
            }
            WindowEvent::MouseInput {
                button: MouseButton::Left,
                state,
                ..
            } => match state {
                ElementState::Pressed => self.mouse_clicked = true,
                ElementState::Released => self.mouse_clicked = false,
            },
            WindowEvent::KeyboardInput {
                input:
                    KeyboardInput {
                        virtual_keycode: Some(key),
                        state: ElementState::Pressed,
                        modifiers,
                        ..
                    },
                ..
            } => {
                captured = true;
                match *key {
                    VirtualKeyCode::Escape => {
                        self.requests.lock().unwrap().action_mode = Some(ActionMode::Normal)
                    }
                    VirtualKeyCode::A => {
                        self.requests.lock().unwrap().action_mode = Some(ActionMode::Build(false))
                    }
                    VirtualKeyCode::R if !modifiers.ctrl() => {
                        self.requests.lock().unwrap().action_mode = Some(ActionMode::Rotate)
                    }
                    VirtualKeyCode::T => {
                        self.requests.lock().unwrap().action_mode = Some(ActionMode::Translate)
                    }
                    VirtualKeyCode::X => {
                        self.requests.lock().unwrap().action_mode = Some(ActionMode::Cut)
                    }
                    VirtualKeyCode::N => {
                        self.requests.lock().unwrap().selection_mode =
                            Some(SelectionMode::Nucleotide)
                    }
                    VirtualKeyCode::H => {
                        self.requests.lock().unwrap().selection_mode = Some(SelectionMode::Helix)
                    }
                    VirtualKeyCode::S => {
                        self.requests.lock().unwrap().selection_mode = Some(SelectionMode::Strand)
                    }
                    VirtualKeyCode::G => {
                        self.requests.lock().unwrap().selection_mode = Some(SelectionMode::Grid)
                    }
                    VirtualKeyCode::K => {
                        self.requests.lock().unwrap().recolor_stapples = true;
                    }
                    _ => captured = false,
                }
            }
            _ => {}
        }

        if let Some(focus) = self.focus.filter(|_| !captured) {
            if focus_changed {
                Some((
                    WindowEvent::CursorLeft {
                        device_id: device_id_msg.unwrap(),
                    },
                    focus,
                ))
            } else {
                Some((event, focus))
            }
        } else {
            None
        }
    }

    pub fn change_split(&mut self, split_mode: SplitMode) {
        if split_mode != self.split_mode {
            match self.split_mode {
                SplitMode::Both => {
                    let new_type = match split_mode {
                        SplitMode::Scene3D => ElementType::Scene,
                        SplitMode::Flat => ElementType::FlatScene,
                        SplitMode::Both => unreachable!(),
                    };
                    self.layout_manager.merge(ElementType::Scene, new_type);
                }
                SplitMode::Scene3D | SplitMode::Flat => {
                    let id = self
                        .layout_manager
                        .get_area_id(ElementType::Scene)
                        .or(self.layout_manager.get_area_id(ElementType::FlatScene))
                        .unwrap();
                    match split_mode {
                        SplitMode::Both => {
                            let (scene, flat_scene) = self.layout_manager.vsplit(id, 0.5);
                            self.layout_manager
                                .attribute_element(scene, ElementType::Scene);
                            self.layout_manager
                                .attribute_element(flat_scene, ElementType::FlatScene);
                        }
                        SplitMode::Scene3D => self
                            .layout_manager
                            .attribute_element(id, ElementType::Scene),
                        SplitMode::Flat => self
                            .layout_manager
                            .attribute_element(id, ElementType::FlatScene),
                    }
                }
            }
        }
        self.split_mode = split_mode;
        self.generate_textures();
    }

    fn resize(&mut self, window_size: PhySize) {
        let left_pannel_prop = proportion(0.2, 200., window_size.width as f64);
        self.layout_manager
            .resize(self.left_pannel_split, left_pannel_prop);
    }

    fn texture(&mut self, element_type: ElementType) -> Option<SampledTexture> {
        self.get_draw_area(element_type)
            .map(|a| SampledTexture::create_target_texture(self.device.as_ref(), &a.size))
    }

    fn generate_textures(&mut self) {
        self.scene_texture = self.texture(ElementType::Scene);
        self.top_bar_texture = self.texture(ElementType::TopBar);
        self.left_pannel_texture = self.texture(ElementType::LeftPanel);
        self.grid_panel_texture = self.texture(ElementType::GridPanel);
        self.flat_scene_texture = self.texture(ElementType::FlatScene);
        self.status_bar_texture = self.texture(ElementType::StatusBar);

        self.overlays_textures.clear();
        for overlay in self.overlays.iter() {
            let size = overlay.size;
            self.overlays_textures
                .push(SampledTexture::create_target_texture(
                    self.device.as_ref(),
                    &size,
                ));
        }
    }

    /// Maps *physical* pixels to an element
    fn pixel_to_element(&self, pixel: PhysicalPosition<f64>) -> ElementType {
        let pixel_u32 = pixel.cast::<u32>();
        for (n, overlay) in self.overlays.iter().enumerate() {
            if overlay.contains_pixel(pixel_u32) {
                return ElementType::Overlay(n);
            }
        }
        self.layout_manager.get_area_pixel(
            pixel.x / self.window_size.width as f64,
            pixel.y / self.window_size.height as f64,
        )
    }

    /// Get the drawing area attributed to an element.
    pub fn get_element_area(&self, element: ElementType) -> Option<DrawArea> {
        self.get_draw_area(element)
    }

    /// Return the *physical* position of the cursor, in the foccused element coordinates
    pub fn get_cursor_position(&self) -> PhysicalPosition<f64> {
        self.cursor_position
    }

    /// Return the foccused element
    pub fn foccused_element(&self) -> Option<ElementType> {
        self.focus
    }

    pub fn set_overlays(&mut self, overlays: Vec<Overlay>) {
        self.overlays = overlays;
        self.overlays_textures.clear();
        for overlay in self.overlays.iter_mut() {
            let size = overlay.size;
            self.overlays_textures
                .push(SampledTexture::create_target_texture(
                    self.device.as_ref(),
                    &size,
                ));
        }
    }
}

#[derive(Clone)]
pub struct Overlay {
    pub position: PhysicalPosition<u32>,
    pub size: PhysicalSize<u32>,
}

impl Overlay {
    pub fn contains_pixel(&self, pixel: PhysicalPosition<u32>) -> bool {
        pixel.x >= self.position.x
            && pixel.y >= self.position.y
            && pixel.x < self.position.x + self.size.width
            && pixel.y < self.position.y + self.size.height
    }
}

fn create_pipeline(device: &Device, bg_layout: &wgpu::BindGroupLayout) -> wgpu::RenderPipeline {
    let vs_module = &device.create_shader_module(wgpu::include_spirv!("multiplexer/draw.vert.spv"));
    let fs_module = &device.create_shader_module(wgpu::include_spirv!("multiplexer/draw.frag.spv"));
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        bind_group_layouts: &[bg_layout],
        push_constant_ranges: &[],
        label: None,
    });

    let desc = wgpu::RenderPipelineDescriptor {
        layout: Some(&pipeline_layout),
        vertex_stage: wgpu::ProgrammableStageDescriptor {
            module: &vs_module,
            entry_point: "main",
        },
        fragment_stage: Some(wgpu::ProgrammableStageDescriptor {
            module: &fs_module,
            entry_point: "main",
        }),
        rasterization_state: Some(wgpu::RasterizationStateDescriptor {
            front_face: wgpu::FrontFace::Ccw,
            cull_mode: wgpu::CullMode::None,
            ..Default::default()
        }),
        primitive_topology: wgpu::PrimitiveTopology::TriangleStrip,
        color_states: &[wgpu::ColorStateDescriptor {
            format: wgpu::TextureFormat::Bgra8UnormSrgb,
            color_blend: wgpu::BlendDescriptor::REPLACE,
            alpha_blend: wgpu::BlendDescriptor::REPLACE,
            write_mask: wgpu::ColorWrite::ALL,
        }],
        depth_stencil_state: None,
        vertex_state: wgpu::VertexStateDescriptor {
            index_format: wgpu::IndexFormat::Uint16,
            vertex_buffers: &[],
        },
        sample_count: 1,
        sample_mask: !0,
        alpha_to_coverage_enabled: false,
        label: None,
    };

    device.create_render_pipeline(&desc)
}

fn proportion(min_prop: f64, max_size: f64, length: f64) -> f64 {
    let max_prop = max_size / length;
    max_prop.min(min_prop)
}
