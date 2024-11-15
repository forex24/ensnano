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

//! `ensnano` is a software for designing 3D DNA nanostructures.
//!
//! # Organization of the software
//!
//!
//! The [main] function owns the event loop and the framebuffer. It recieves window events
//! and handles the framebuffer.
//!
//! ## Drawing process
//!
//! On each redraw request, the [main] function generates a new frame, and asks the
//! [Multiplexer](multiplexer) to draw on a view of that texture.
//!
//! The [Multiplexer](multiplexer) knows how the window is devided into several regions. For each
//! of these region it knows what application or gui component should draw on it. For each region
//! the [Multiplexer](multiplexer) holds a texture, and at each draw request, it will request the
//! corresponding app or gui element to possibly update the texture.
//!
//!
//! ## Handling of events
//!
//! The Global state of the program is encoded in an automaton defined in the
//! [controller] module. This global state determines wether inputs are handled
//! normally or if the program should wait for the user to interact with dialog windows.
//!
//! When the Global automaton is in NormalState, events are forwarded to the
//! [Multiplexer](multiplexer) which decides what application should handle the event. This is
//! usually the application displayed in the active region (the region under the cursor). Special
//! events like resizing of the window are handled by the multiplexer.
//!
//! When GUIs handle an event. They recieve a reference to the state of the main program. This
//! state is encoded in the [AppState] data structure. Each GUI component
//! needs to be able to recieve some specific information about the state of the program to handle
//! events and to draw their views. Theese needs are encoded in traits. GUI component typically
//! defines their own `AppState` trait that must be implemented by the concrete `AppState` type.
//!
//! GUI components may interpret event as a request from the user to modify the design or the state
//! of the main application (for example by changing the selection). These requests are stored in
//! the [Requests] data structure. Each application defines a `Requests` trait
//! that must be implemented by the concrete `Requests` type.
//!
//! On each itteration of the main event loop, if the Global controller is in Normal State,
//! requests are polled and transmitted to the main `AppState` by the main controller. The
//! processing of these requests may have three different kind of consequences:
//!
//!  * An undoable action is performed on the main `AppState`, modifiying it. In that case the
//!  current `AppState` is copied on the undo stack and the replaced by the modified one.
//!
//!  * A non-undoable action is perfomed on the main `AppState`, modyfing it. In that case, the
//!  current `AppState` is replaced by the modified one, but not stored on the undo stack.
//!  This typically happens when the `AppState` is in a transient state for example while the user
//!  is performing a drag and drop action. Transient states are not stored on the undo stack
//!  because they are not meant to be restored by undos.
//!   
//!  * An error is returned. In the case the `AppState` is not modified and the user is notified of
//!  the error. Error typically occur when user attempt to make actions on the design that are not
//!  permitted by the current state of the program. For example an error is returned if the user
//!  try to modify the design durring a simulation.
//!
use std::collections::{HashMap, VecDeque};
use std::env;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use controller::{ChannelReader, ChannelReaderUpdate, SimulationRequest};
use ensnano_design::{grid::GridId, Camera};
use ensnano_exports::{ExportResult, ExportType};
use ensnano_interactor::{
    application::{Application, Notification},
    RevolutionSurfaceSystemDescriptor, UnrootedRevolutionSurfaceDescriptor,
};
use ensnano_interactor::{
    CenterOfSelection, CursorIcon, DesignOperation, DesignReader, RigidBodyConstants,
    SuggestionParameters,
};
use iced_native::Event as IcedEvent;
use iced_wgpu::{wgpu, Settings, Viewport};
use iced_winit::winit::event::VirtualKeyCode;
use iced_winit::{conversion, futures, program, winit, Debug, Size};

use app_state::AppStateParameters;
use futures::task::SpawnExt;
use rand::random;
use ultraviolet::{Rotor3, Vec3};
use winit::{
    dpi::{PhysicalPosition, PhysicalSize},
    event::{Event, ModifiersState, WindowEvent},
    event_loop::{ControlFlow, EventLoop},
    window::Window,
};

#[allow(unused_imports)]
#[macro_use]
extern crate pretty_env_logger;

//#[cfg(not(target_env = "msvc"))]
//use jemallocator::Jemalloc;

// #[cfg(not(target_env = "msvc"))]
// #[global_allocator]
// static GLOBAL: Jemalloc = Jemalloc;

/// Design handling
//mod design;
/// Graphical interface drawing
use ensnano_gui as gui;
use ensnano_interactor::consts;
//use design::Design;
//mod mediator;
mod multiplexer;
use ensnano_flatscene as flatscene;
use ensnano_interactor::{
    graphics::{ElementType, SplitMode},
    operation::Operation,
    ActionMode, CheckXoversParameter, Selection, SelectionMode,
};
/// 3D scene drawing
use ensnano_scene as scene;
mod scheduler;
use ensnano_utils as utils;
use scheduler::Scheduler;

#[cfg(test)]
mod main_tests;
// mod grid_panel; We don't use the grid panel atm

mod app_state;
mod controller;
use app_state::{
    AppState, AppStateTransition, CopyOperation, ErrOperation, OkOperation, PastePosition,
    PastingStatus, SimulationTarget, TransitionLabel,
};
use controller::Action;
use controller::Controller;

mod requests;
pub use requests::Requests;

mod dialog;

use flatscene::FlatScene;
use gui::{ColorOverlay, Gui, IcedMessages, OverlayType, UiSize};
use multiplexer::{Multiplexer, Overlay};
use scene::Scene;
use utils::{PhySize, TEXTURE_FORMAT};

use std::collections::HashMap as StdHashMap;

fn convert_size(size: PhySize) -> Size<f32> {
    Size::new(size.width as f32, size.height as f32)
}

fn convert_size_u32(size: PhySize) -> Size<u32> {
    Size::new(size.width, size.height)
}

/// Determine if log messages can be printed before the renderer setup.
///
/// Setting it to true will print information in the terminal that are not usefull for regular use.
/// By default the value is `false`. It can be set to `true` by enabling the
/// `log_after_renderer_setup` feature.
#[cfg(not(feature = "log_after_renderer_setup"))]
const EARLY_LOG: bool = true;
#[cfg(feature = "log_after_renderer_setup")]
const EARLY_LOG: bool = false;

/// Determine wgpu backends.
///
/// On some windows machine, only the DX12 backends will work. So the `dx12_only` feature forces
/// its use.
#[cfg(not(feature = "dx12_only"))]
const BACKEND: wgpu::Backends = wgpu::Backends::PRIMARY;
#[cfg(feature = "dx12_only")]
const BACKEND: wgpu::Backends = wgpu::Backends::DX12;

/// Determine if wgpu errors should panic.
///
/// Set to true because there should not be any "false-positive" in wgpu errors.
///
/// TODO: Make a feature that would set this constant to `false`.
const PANIC_ON_WGPU_ERRORS: bool = true;

/// Main function. Runs the event loop and holds the framebuffer.
///
/// # Intialization
///
/// Before running the event loop, the main fuction does the following:
///
/// * It requests a connection to the GPU and creates a framebuffer.
/// * It initializes a multiplexer.
/// * It initializes applications and GUI component, and associate regions of the screen to these
/// components
/// * It initializes the [Mediator](ensnano_interactor::application::AppId::Mediator), the
/// [Scheduler] and the [Gui manager](gui::Gui)
///
/// # Event loop
///
/// * The event loop waits for an event. If no event is recieved during 33ms, a new redraw is
/// requested.
/// * When a event is recieved, it is forwareded to the multiplexer. The Multiplexer may then
/// convert this event into a event for a specific screen region.
/// * When all window events have been handled, the main function reads messages that it recieved
/// from the [Gui Manager](gui::Gui).  The consequences of these messages are forwarded to the
/// applications.
/// * The main loops then reads the messages that it recieved from the [Mediator](ensnano_interactor::application::AppId::Mediator) and
/// forwards their consequences to the Gui components.
/// * Finally, a redraw is requested.
///
///
fn main() {
    if EARLY_LOG {
        pretty_env_logger::init();
    }
    // parse arugments, if an argument was given it is treated as a file to open
    let args: Vec<String> = env::args().collect();
    let path = if args.len() >= 2 {
        Some(PathBuf::from(&args[1]))
    } else {
        None
    };

    // Initialize winit
    let event_loop = EventLoop::new();
    let window = winit::window::Window::new(&event_loop).unwrap();
    let mut windows_title = String::from("ENSnano");
    window.set_title("ENSnano");
    window.set_min_inner_size(Some(PhySize::new(100, 100)));

    log::info!("scale factor {}", window.scale_factor());

    // Represents the current state of the keyboard modifiers (Shift, Ctrl, etc.)
    let kbd_modifiers = ModifiersState::default();

    let gpu = wgpu::Instance::new(BACKEND);
    let surface = unsafe { gpu.create_surface(&window) };
    // Initialize WGPU
    let (device, queue) = futures::executor::block_on(async {
        let adapter = gpu
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::LowPower,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Could not get adapter\n\
                     This might be because gpu drivers are missing.\n\
                     You need Vulkan, Metal (for MacOS) or DirectX (for Windows) drivers to run this software");

        adapter
            .request_device(
                &wgpu::DeviceDescriptor {
                    features: wgpu::Features::empty(),
                    limits: wgpu::Limits::default(),
                    label: None,
                },
                None,
            )
            .await
            .expect("Request device")
    });

    if !PANIC_ON_WGPU_ERRORS {
        device.on_uncaptured_error(|e| log::error!("wgpu error {:?}", e));
    }

    {
        let size = window.inner_size();

        surface.configure(
            &device,
            &wgpu::SurfaceConfiguration {
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                format: TEXTURE_FORMAT,
                width: size.width,
                height: size.height,
                present_mode: wgpu::PresentMode::Mailbox,
            },
        )
    }

    use consts::APP_NAME;
    let ui_size = confy::load(APP_NAME, APP_NAME)
        .map(|p: AppStateParameters| p.ui_size)
        .unwrap_or_default();

    let settings = Settings {
        antialiasing: Some(iced_graphics::Antialiasing::MSAAx4),
        default_text_size: ui_size.main_text(),
        default_font: Some(include_bytes!("../font/ensnano2.ttf")),
        ..Default::default()
    };
    let mut renderer =
        iced_wgpu::Renderer::new(iced_wgpu::Backend::new(&device, settings, TEXTURE_FORMAT));
    let device = Rc::new(device);
    let queue = Rc::new(queue);
    let mut resized = false;
    let mut scale_factor_changed = false;
    let mut staging_belt = wgpu::util::StagingBelt::new(5 * 1024);
    let mut local_pool = futures::executor::LocalPool::new();

    // Initialize the mediator
    let requests = Arc::new(Mutex::new(Requests::default()));
    let messages = Arc::new(Mutex::new(IcedMessages::new()));
    let mut scheduler = Scheduler::new();

    // Initialize the layout
    let mut multiplexer = Multiplexer::new(
        window.inner_size(),
        window.scale_factor(),
        device.clone(),
        requests.clone(),
        ui_size,
    );
    multiplexer.change_split(SplitMode::Both);

    // Initialize the scenes
    //
    // The `encoder` encodes a series of GPU operations.
    let mut encoder =
        device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    let scene_area = multiplexer.get_element_area(ElementType::Scene).unwrap();
    let scene = Arc::new(Mutex::new(Scene::new(
        device.clone(),
        queue.clone(),
        window.inner_size(),
        scene_area,
        requests.clone(),
        &mut encoder,
        Default::default(),
        scene::SceneKind::Cartesian,
    )));
    let stereographic_scene = Arc::new(Mutex::new(Scene::new(
        device.clone(),
        queue.clone(),
        window.inner_size(),
        scene_area,
        requests.clone(),
        &mut encoder,
        Default::default(),
        scene::SceneKind::Stereographic,
    )));

    queue.submit(Some(encoder.finish()));
    scheduler.add_application(scene.clone(), ElementType::Scene);
    scheduler.add_application(stereographic_scene.clone(), ElementType::StereographicScene);

    let flat_scene = Arc::new(Mutex::new(FlatScene::new(
        device.clone(),
        queue.clone(),
        window.inner_size(),
        scene_area,
        requests.clone(),
        Default::default(),
    )));
    scheduler.add_application(flat_scene.clone(), ElementType::FlatScene);

    // Initialize the UI
    //
    let main_state_constructor = MainStateConstructor {
        messages: messages.clone(),
    };

    let mut main_state = MainState::new(main_state_constructor);

    let mut gui = gui::Gui::new(
        device.clone(),
        &window,
        &multiplexer,
        requests.clone(),
        ui_size,
        &main_state.app_state,
        Default::default(),
    );

    let mut overlay_manager = OverlayManager::new(requests.clone(), &window, &mut renderer);

    // Run event loop
    let mut last_render_time = std::time::Instant::now();
    let mut mouse_interaction = iced::mouse::Interaction::Pointer;

    main_state.applications.insert(ElementType::Scene, scene);
    main_state
        .applications
        .insert(ElementType::FlatScene, flat_scene);
    main_state
        .applications
        .insert(ElementType::StereographicScene, stereographic_scene);

    // Add a design to the scene if one was given as a command line arguement
    if path.is_some() {
        main_state.push_action(Action::LoadDesign(path))
    }
    main_state.update();
    main_state.last_saved_state = main_state.app_state.clone();

    let mut controller = Controller::new();

    println!("{}", consts::WELCOME_MSG);
    if !EARLY_LOG {
        pretty_env_logger::init();
    }

    let mut first_iteration = true;

    let mut last_gui_state = (
        main_state.app_state.clone(),
        main_state.gui_state(&multiplexer),
    );
    messages
        .lock()
        .unwrap()
        .push_application_state(main_state.get_app_state(), last_gui_state.1.clone());

    event_loop.run(move |event, _, control_flow| {
        // Wait for event or redraw a frame every 33 ms (30 frame per seconds)
        *control_flow = ControlFlow::WaitUntil(Instant::now() + Duration::from_millis(33));

        let mut main_state_view = MainStateView {
            main_state: &mut main_state,
            control_flow,
            multiplexer: &mut multiplexer,
            gui: &mut gui,
            scheduler: &mut scheduler,
            window: &window,
            resized: false,
        };

        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => main_state_view
                .main_state
                .pending_actions
                .push_back(Action::Exit),
            Event::WindowEvent {
                event: WindowEvent::Focused(false),
                ..
            } => main_state_view.notify_apps(Notification::WindowFocusLost),
            Event::WindowEvent {
                event: WindowEvent::ModifiersChanged(modifiers),
                ..
            } => {
                main_state_view.multiplexer.update_modifiers(modifiers);
                messages.lock().unwrap().update_modifiers(modifiers);
                main_state_view.notify_apps(Notification::ModifersChanged(modifiers));
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { input, .. },
                ..
            } if input.virtual_keycode == Some(VirtualKeyCode::Escape)
                && window.fullscreen().is_some() =>
            {
                window.set_fullscreen(None)
            }
            Event::WindowEvent {
                event: WindowEvent::KeyboardInput { .. },
                ..
            }
            | Event::WindowEvent {
                event: WindowEvent::ReceivedCharacter(_),
                ..
            } if gui.has_keyboard_priority() => {
                if let Event::WindowEvent { event, .. } = event {
                    if let Some(event) = event.to_static() {
                        let event = iced_winit::conversion::window_event(
                            &event,
                            window.scale_factor(),
                            kbd_modifiers,
                        );
                        if let Some(event) = event {
                            gui.forward_event_all(event);
                        }
                    }
                }
            }
            Event::WindowEvent { event, .. } => {
                //let modifiers = multiplexer.modifiers();
                if let Some(event) = event.to_static() {
                    // Feed the event to the multiplexer
                    let event = multiplexer.event(event, &mut resized, &mut scale_factor_changed);

                    if let Some((event, area)) = event {
                        // pass the event to the area on which it happenened
                        if main_state.focused_element != Some(area) {
                            if let Some(app) = main_state
                                .focused_element
                                .as_ref()
                                .and_then(|elt| main_state.applications.get(elt))
                            {
                                app.lock().unwrap().on_notify(Notification::WindowFocusLost)
                            }
                            main_state.focused_element = Some(area);
                            main_state.update_candidates(vec![]);
                        }
                        main_state.applications_cursor = None;
                        match area {
                            area if area.is_gui() => {
                                let event = iced_winit::conversion::window_event(
                                    &event,
                                    window.scale_factor(),
                                    kbd_modifiers,
                                );
                                if let Some(event) = event {
                                    gui.forward_event(area, event);
                                }
                            }
                            ElementType::Overlay(n) => {
                                let event = iced_winit::conversion::window_event(
                                    &event,
                                    window.scale_factor(),
                                    kbd_modifiers,
                                );
                                if let Some(event) = event {
                                    overlay_manager.forward_event(event, n);
                                }
                            }
                            area if area.is_scene() => {
                                let cursor_position = multiplexer.get_cursor_position();
                                let state = main_state.get_app_state();
                                main_state.applications_cursor =
                                    scheduler.forward_event(&event, area, cursor_position, state);
                                if matches!(event, winit::event::WindowEvent::MouseInput { .. }) {
                                    gui.clear_foccus();
                                }
                            }
                            _ => unreachable!(),
                        }
                    }
                }
            }
            Event::MainEventsCleared => {
                scale_factor_changed |= multiplexer.check_scale_factor(&window);
                let mut redraw = resized || scale_factor_changed;
                redraw |= main_state.update_cursor(&multiplexer);
                redraw |= gui.fetch_change(&window, &multiplexer);

                // When there is no more event to deal with
                requests::poll_all(requests.lock().unwrap(), &mut main_state);

                let mut main_state_view = MainStateView {
                    main_state: &mut main_state,
                    control_flow,
                    multiplexer: &mut multiplexer,
                    gui: &mut gui,
                    scheduler: &mut scheduler,
                    window: &window,
                    resized: false,
                };

                if main_state_view.main_state.wants_fit {
                    main_state_view.notify_apps(Notification::FitRequest);
                    main_state_view.main_state.wants_fit = false;
                }
                controller.make_progress(&mut main_state_view);
                resized |= main_state_view.resized;
                resized |= first_iteration;
                first_iteration = false;

                for update in main_state.channel_reader.get_updates() {
                    if let ChannelReaderUpdate::ScaffoldShiftOptimizationProgress(x) = update {
                        main_state
                            .messages
                            .lock()
                            .unwrap()
                            .push_progress("Optimizing: ".to_string(), x);
                    } else if let ChannelReaderUpdate::ScaffoldShiftOptimizationResult(result) =
                        update
                    {
                        main_state.messages.lock().unwrap().finish_progess();
                        if let Ok(result) = result {
                            main_state.apply_operation(DesignOperation::SetScaffoldShift(
                                result.position,
                            ));
                            let msg = format!(
                                "Scaffold position set to {}\n {}",
                                result.position, result.score
                            );
                            main_state.pending_actions.push_back(Action::ErrorMsg(msg));
                        } else {
                            // unwrap because in this block, result is necessarilly an Err
                            log::warn!("{:?}", result.err().unwrap());
                        }
                    } else if let ChannelReaderUpdate::SimulationUpdate(update) = update {
                        main_state.app_state.apply_simulation_update(update)
                    } else if let ChannelReaderUpdate::SimulationExpired = update {
                        main_state.update_simulation(SimulationRequest::Stop)
                    }
                }

                log::trace!("call update from main");
                main_state.update();
                let new_title = if let Some(path) = main_state.get_current_file_name() {
                    let path_str = formated_path_end(path);
                    format!("ENSnano {}", path_str)
                } else {
                    format!("ENSnano {}", crate::consts::NO_DESIGN_TITLE)
                };

                if windows_title != new_title {
                    window.set_title(&new_title);
                    windows_title = new_title;
                }

                // Treat eventual event that happenend in the gui left panel.
                let _overlay_change =
                    overlay_manager.fetch_change(&multiplexer, &window, &mut renderer);
                {
                    let mut messages = messages.lock().unwrap();
                    gui.forward_messages(&mut messages);
                    overlay_manager.forward_messages(&mut messages);
                }

                let now = std::time::Instant::now();
                let dt = now - last_render_time;
                redraw |= scheduler.check_redraw(&multiplexer, dt, main_state.get_app_state());
                let new_gui_state = (
                    main_state.app_state.clone(),
                    main_state.gui_state(&multiplexer),
                );
                if new_gui_state != last_gui_state {
                    last_gui_state = new_gui_state;
                    messages.lock().unwrap().push_application_state(
                        main_state.get_app_state(),
                        last_gui_state.1.clone(),
                    );
                    redraw = true;
                };
                last_render_time = now;

                if redraw {
                    window.request_redraw();
                }
            }
            Event::RedrawRequested(_)
                if window.inner_size().width > 0 && window.inner_size().height > 0 =>
            {
                if resized {
                    multiplexer.generate_textures();
                    scheduler.forward_new_size(window.inner_size(), &multiplexer);
                    let window_size = window.inner_size();

                    surface.configure(
                        &device,
                        &wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format: TEXTURE_FORMAT,
                            width: window_size.width,
                            height: window_size.height,
                            present_mode: wgpu::PresentMode::Mailbox,
                        },
                    );

                    gui.resize(&multiplexer, &window);
                    log::trace!(
                        "Will draw on texture of size {}x {}",
                        window_size.width,
                        window_size.height
                    );
                }
                if scale_factor_changed {
                    multiplexer.generate_textures();
                    gui.notify_scale_factor_change(
                        &window,
                        &multiplexer,
                        &main_state.app_state,
                        main_state.gui_state(&multiplexer),
                    );
                    log::info!("Notified of scale factor change: {}", window.scale_factor());
                    scheduler.forward_new_size(window.inner_size(), &multiplexer);
                    let window_size = window.inner_size();

                    surface.configure(
                        &device,
                        &wgpu::SurfaceConfiguration {
                            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                            format: TEXTURE_FORMAT,
                            width: window_size.width,
                            height: window_size.height,
                            present_mode: wgpu::PresentMode::Mailbox,
                        },
                    );

                    gui.resize(&multiplexer, &window);
                }
                // Get viewports from the partition

                // If there are events pending
                gui.update(&multiplexer, &window);

                overlay_manager.process_event(&mut renderer, resized, &multiplexer, &window);

                resized = false;
                scale_factor_changed = false;

                if let Ok(frame) = surface.get_current_texture() {
                    let mut encoder = device
                        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });

                    // We draw the applications first
                    let now = std::time::Instant::now();
                    let dt = now - last_render_time;
                    scheduler.draw_apps(&mut encoder, &multiplexer, dt);

                    gui.render(
                        &mut encoder,
                        &window,
                        &multiplexer,
                        &mut staging_belt,
                        &mut mouse_interaction,
                    );

                    if multiplexer.resize(window.inner_size(), window.scale_factor()) {
                        resized = true;
                        window.request_redraw();
                        return;
                    }
                    log::trace!("window size {:?}", window.inner_size());
                    multiplexer.draw(
                        &mut encoder,
                        &frame
                            .texture
                            .create_view(&wgpu::TextureViewDescriptor::default()),
                        &window,
                    );
                    //overlay_manager.render(&device, &mut staging_belt, &mut encoder, &frame.output.view, &multiplexer, &window, &mut renderer);

                    // Then we submit the work
                    staging_belt.finish();
                    queue.submit(Some(encoder.finish()));
                    frame.present();

                    // And update the mouse cursor
                    main_state.gui_cursor =
                        iced_winit::conversion::mouse_interaction(mouse_interaction);
                    main_state.update_cursor(&multiplexer);
                    window.set_cursor_icon(main_state.cursor);
                    local_pool
                        .spawner()
                        .spawn(staging_belt.recall())
                        .expect("Recall staging buffers");

                    local_pool.run_until_stalled();
                } else {
                    log::warn!("Error getting next frame, attempt to recreate swap chain");
                    resized = true;
                }
            }
            _ => {}
        }
    })
}

pub struct OverlayManager {
    color_state: iced_native::program::State<ColorOverlay<Requests>>,
    color_debug: Debug,
    overlay_types: Vec<OverlayType>,
    overlays: Vec<Overlay>,
}

impl OverlayManager {
    pub fn new(
        requests: Arc<Mutex<Requests>>,
        window: &Window,
        renderer: &mut iced_wgpu::Renderer,
    ) -> Self {
        let color = ColorOverlay::new(
            requests,
            PhysicalSize::new(250., 250.).to_logical(window.scale_factor()),
        );
        let mut color_debug = Debug::new();
        let color_state = program::State::new(
            color,
            convert_size(PhysicalSize::new(250, 250)),
            renderer,
            &mut color_debug,
        );
        Self {
            color_state,
            color_debug,
            overlay_types: Vec::new(),
            overlays: Vec::new(),
        }
    }

    fn forward_event(&mut self, event: IcedEvent, n: usize) {
        match self.overlay_types.get(n) {
            None => {
                log::error!("recieve event from non existing overlay");
                unreachable!();
            }
            Some(OverlayType::Color) => self.color_state.queue_event(event),
        }
    }

    #[allow(dead_code)]
    fn add_overlay(&mut self, overlay_type: OverlayType, multiplexer: &mut Multiplexer) {
        match overlay_type {
            OverlayType::Color => self.overlays.push(Overlay {
                position: PhysicalPosition::new(500, 500),
                size: PhysicalSize::new(250, 250),
            }),
        }
        self.overlay_types.push(overlay_type);
        self.update_multiplexer(multiplexer);
    }

    fn process_event(
        &mut self,
        renderer: &mut iced_wgpu::Renderer,
        resized: bool,
        multiplexer: &Multiplexer,
        window: &Window,
    ) {
        for (n, overlay) in self.overlay_types.iter().enumerate() {
            let cursor_position = if multiplexer.foccused_element() == Some(ElementType::Overlay(n))
            {
                multiplexer.get_cursor_position()
            } else {
                PhysicalPosition::new(-1., -1.)
            };
            let mut clipboard = iced_native::clipboard::Null;
            match overlay {
                OverlayType::Color => {
                    if !self.color_state.is_queue_empty() || resized {
                        let _ = self.color_state.update(
                            convert_size(PhysicalSize::new(250, 250)),
                            conversion::cursor_position(cursor_position, window.scale_factor()),
                            renderer,
                            &mut clipboard,
                            &mut self.color_debug,
                        );
                    }
                }
            }
        }
    }

    #[allow(dead_code)]
    fn render(
        &self,
        device: &wgpu::Device,
        staging_belt: &mut wgpu::util::StagingBelt,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        multiplexer: &Multiplexer,
        window: &Window,
        renderer: &mut iced_wgpu::Renderer,
    ) {
        for overlay_type in self.overlay_types.iter() {
            match overlay_type {
                OverlayType::Color => {
                    let color_viewport = Viewport::with_physical_size(
                        convert_size_u32(multiplexer.window_size),
                        window.scale_factor(),
                    );
                    renderer.with_primitives(|backend, primitives| {
                        backend.present(
                            device,
                            staging_belt,
                            encoder,
                            target,
                            primitives,
                            &color_viewport,
                            &self.color_debug.overlay(),
                        )
                    });
                }
            }
        }
    }

    #[allow(dead_code)]
    fn rm_overlay(&mut self, overlay_type: OverlayType, multiplexer: &mut Multiplexer) {
        let mut rm_idx = Vec::new();
        for (idx, overlay_type_) in self.overlay_types.iter().rev().enumerate() {
            if *overlay_type_ == overlay_type {
                rm_idx.push(idx);
            }
        }
        for idx in rm_idx.iter() {
            self.overlays.remove(*idx);
            self.overlay_types.remove(*idx);
        }
        self.update_multiplexer(multiplexer);
    }

    #[allow(dead_code)]
    fn update_multiplexer(&self, multiplexer: &mut Multiplexer) {
        multiplexer.set_overlays(self.overlays.clone())
    }

    fn forward_messages(&mut self, _messages: &mut IcedMessages<AppState>) {
        ()
        /*
        for m in messages.color_overlay.drain(..) {
            self.color_state.queue_message(m);
        }*/
    }

    fn fetch_change(
        &mut self,
        multiplexer: &Multiplexer,
        window: &Window,
        renderer: &mut iced_wgpu::Renderer,
    ) -> bool {
        let mut ret = false;
        for (n, overlay) in self.overlay_types.iter().enumerate() {
            let cursor_position = if multiplexer.foccused_element() == Some(ElementType::Overlay(n))
            {
                multiplexer.get_cursor_position()
            } else {
                PhysicalPosition::new(-1., -1.)
            };
            let mut clipboard = iced_native::clipboard::Null;
            match overlay {
                OverlayType::Color => {
                    if !self.color_state.is_queue_empty() {
                        ret = true;
                        let _ = self.color_state.update(
                            convert_size(PhysicalSize::new(250, 250)),
                            conversion::cursor_position(cursor_position, window.scale_factor()),
                            renderer,
                            &mut clipboard,
                            &mut self.color_debug,
                        );
                    }
                }
            }
        }
        ret
    }
}

fn formated_path_end<P: AsRef<Path>>(path: P) -> String {
    let components: Vec<_> = path
        .as_ref()
        .components()
        .map(|comp| comp.as_os_str())
        .collect();
    let mut ret = if components.len() > 3 {
        vec!["..."]
    } else {
        vec![]
    };
    let mut iter = components.iter().rev().take(3).rev();
    for _ in 0..3 {
        if let Some(comp) = iter.next().and_then(|s| s.to_str()) {
            ret.push(comp);
        }
    }
    ret.join("/")
}

/// The state of the main event loop.
pub(crate) struct MainState {
    app_state: AppState,
    pending_actions: VecDeque<Action>,
    undo_stack: Vec<AppStateTransition>,
    redo_stack: Vec<AppStateTransition>,
    channel_reader: ChannelReader,
    messages: Arc<Mutex<IcedMessages<AppState>>>,
    applications: HashMap<ElementType, Arc<Mutex<dyn Application<AppState = AppState>>>>,
    focused_element: Option<ElementType>,
    last_saved_state: AppState,

    /// The name of the file containing the current design.
    ///
    /// For example, if the design is stored in `/home/alice/designs/origami.ens`, `file_name` is
    /// `origami.ens`.
    file_name: Option<PathBuf>,

    wants_fit: bool,
    last_backup_date: Instant,
    last_backed_up_state: AppState,
    simulation_cursor: Option<CursorIcon>,
    applications_cursor: Option<CursorIcon>,
    gui_cursor: CursorIcon,
    cursor: CursorIcon,
}

struct MainStateConstructor {
    messages: Arc<Mutex<IcedMessages<AppState>>>,
}

use controller::SaveDesignError;
impl MainState {
    fn new(constructor: MainStateConstructor) -> Self {
        let app_state = match AppState::with_preferred_parameters() {
            Ok(state) => state,
            Err(e) => {
                log::error!("Could not load preferrences {e}");
                Default::default()
            }
        };
        Self {
            app_state: app_state.clone(),
            pending_actions: VecDeque::new(),
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            channel_reader: Default::default(),
            messages: constructor.messages,
            applications: Default::default(),
            focused_element: None,
            last_saved_state: app_state.clone(),
            file_name: None,
            wants_fit: false,
            last_backup_date: Instant::now(),
            last_backed_up_state: app_state,
            simulation_cursor: None,
            applications_cursor: None,
            gui_cursor: Default::default(),
            cursor: Default::default(),
        }
    }

    fn update_cursor(&mut self, multiplexer: &Multiplexer) -> bool {
        self.update_simulation_cursor();
        // Usefull to remember to finish hyperboloid before trying to edit
        if self.app_state.is_building_hyperboloid()
            && multiplexer
                .foccused_element()
                .map(|e| e.is_scene())
                .unwrap_or(false)
        {
            self.applications_cursor = Some(CursorIcon::NotAllowed)
        }
        let new_cursor = if self.simulation_cursor.is_some() {
            multiplexer
                .icon
                .or_else(|| Some(self.gui_cursor).filter(|c| c != &Default::default()))
                .or(self.simulation_cursor)
                .unwrap_or_default()
        } else {
            self.applications_cursor
                .or(multiplexer.icon)
                .unwrap_or(self.gui_cursor)
        };
        let ret = self.cursor != new_cursor;
        self.cursor = new_cursor;
        ret
    }

    fn update_simulation_cursor(&mut self) {
        self.simulation_cursor = if self.app_state.get_simulation_state().is_runing() {
            Some(CursorIcon::Progress)
        } else {
            None
        }
    }

    fn push_action(&mut self, action: Action) {
        self.pending_actions.push_back(action)
    }

    fn get_app_state(&mut self) -> AppState {
        self.app_state.clone()
    }

    fn new_design(&mut self) {
        self.clear_app_state(Default::default());
        self.update_current_file_name();
    }

    fn clear_app_state(&mut self, new_state: AppState) {
        self.undo_stack.clear();
        self.redo_stack.clear();
        self.app_state = new_state.clone();
        self.last_saved_state = new_state;
    }

    fn update(&mut self) {
        // Appelé continuement
        log::trace!("call from main state");
        if let Some(camera_ptr) = self
            .applications
            .get(&ElementType::StereographicScene)
            .and_then(|s| s.lock().unwrap().get_camera())
        {
            self.applications
                .get(&ElementType::Scene)
                .unwrap()
                .lock()
                .unwrap()
                .on_notify(Notification::NewStereographicCamera(camera_ptr));
        }
        self.app_state.update()
    }

    fn update_candidates(&mut self, candidates: Vec<Selection>) {
        self.modify_state(|s| s.with_candidates(candidates), None);
    }

    fn transfer_selection_pivot_to_group(&mut self, group_id: ensnano_design::GroupId) {
        use scene::AppState;
        let scene_pivot = self
            .applications
            .get(&ElementType::Scene)
            .and_then(|app| app.lock().unwrap().get_current_selection_pivot());
        if let Some(pivot) = self.app_state.get_current_group_pivot().or(scene_pivot) {
            self.apply_operation(DesignOperation::SetGroupPivot { group_id, pivot })
        }
    }

    fn update_selection(
        &mut self,
        selection: Vec<Selection>,
        group_id: Option<ensnano_organizer::GroupId>,
    ) {
        self.modify_state(
            |s| s.with_selection(selection, group_id),
            Some("Selection".into()),
        );
    }

    fn update_center_of_selection(&mut self, center: Option<CenterOfSelection>) {
        self.modify_state(|s| s.with_center_of_selection(center), None)
    }

    fn apply_copy_operation(&mut self, operation: CopyOperation) {
        let result = self.app_state.apply_copy_operation(operation);
        self.apply_operation_result(result);
    }

    fn apply_operation(&mut self, operation: DesignOperation) {
        log::debug!("Applying operation {:?}", operation);
        let result = self.app_state.apply_design_op(operation.clone());
        if let Err(ErrOperation::FinishFirst) = result {
            self.modify_state(
                |s| s.notified(app_state::InteractorNotification::FinishOperation),
                None,
            );
            self.apply_operation(operation);
        } else {
            self.apply_operation_result(result);
        }
    }

    fn start_helix_simulation(&mut self, parameters: RigidBodyConstants) {
        let result = self.app_state.start_simulation(
            parameters,
            &mut self.channel_reader,
            SimulationTarget::Helices,
        );
        self.apply_operation_result(result)
    }

    fn start_grid_simulation(&mut self, parameters: RigidBodyConstants) {
        let result = self.app_state.start_simulation(
            parameters,
            &mut self.channel_reader,
            SimulationTarget::Grids,
        );
        self.apply_operation_result(result)
    }

    fn start_revolution_simulation(&mut self, desc: RevolutionSurfaceSystemDescriptor) {
        let result = self.app_state.start_simulation(
            Default::default(),
            &mut self.channel_reader,
            SimulationTarget::Revolution { desc },
        );
        self.apply_operation_result(result)
    }

    fn start_twist(&mut self, grid_id: GridId) {
        let result = self.app_state.start_simulation(
            Default::default(),
            &mut self.channel_reader,
            SimulationTarget::Twist { grid_id },
        );
        self.apply_operation_result(result)
    }

    fn start_roll_simulation(&mut self, target_helices: Option<Vec<usize>>) {
        let result = self.app_state.start_simulation(
            Default::default(),
            &mut self.channel_reader,
            SimulationTarget::Roll { target_helices },
        );
        self.apply_operation_result(result)
    }

    fn update_simulation(&mut self, request: SimulationRequest) {
        let result = self.app_state.update_simulation(request);
        self.apply_operation_result(result);
    }

    fn apply_silent_operation(&mut self, operation: DesignOperation) {
        match self.app_state.apply_design_op(operation.clone()) {
            Ok(_) => (),
            Err(ErrOperation::FinishFirst) => {
                self.modify_state(
                    |s| s.notified(app_state::InteractorNotification::FinishOperation),
                    None,
                );
                self.apply_silent_operation(operation)
            }
            Err(e) => log::warn!("{:?}", e),
        }
    }

    fn save_old_state(&mut self, old_state: AppState, label: TransitionLabel) {
        let camera_3d = self.get_camera_3d();
        self.undo_stack.push(AppStateTransition {
            state: old_state,
            label,
            camera_3d,
        });
        self.redo_stack.clear();
    }

    fn set_roll_of_selected_helices(&mut self, roll: f32) {
        if let Some((_, helices)) =
            ensnano_interactor::list_of_helices(self.app_state.get_selection().as_ref())
        {
            self.apply_operation(DesignOperation::SetRollHelices { helices, roll })
        }
    }

    fn undo(&mut self) {
        if let Some(mut transition) = self.undo_stack.pop() {
            transition.state.prepare_for_replacement(&self.app_state);
            let mut redo_state = std::mem::replace(&mut self.app_state, transition.state);
            redo_state = redo_state.notified(app_state::InteractorNotification::FinishOperation);
            self.set_camera_3d(transition.camera_3d.clone());
            self.messages
                .lock()
                .unwrap()
                .push_message(format!("UNDO: {}", transition.label.as_ref()));
            if redo_state.is_in_stable_state() {
                self.redo_stack.push(AppStateTransition {
                    state: redo_state,
                    label: transition.label,
                    camera_3d: transition.camera_3d,
                });
            }
        }
    }

    fn redo(&mut self) {
        if let Some(mut transition) = self.redo_stack.pop() {
            transition.state.prepare_for_replacement(&self.app_state);
            let undo_state = std::mem::replace(&mut self.app_state, transition.state);
            self.set_camera_3d(transition.camera_3d.clone());
            self.messages
                .lock()
                .unwrap()
                .push_message(format!("REDO: {}", transition.label.as_ref()));
            self.undo_stack.push(AppStateTransition {
                state: undo_state,
                camera_3d: transition.camera_3d,
                label: transition.label,
            });
        }
    }

    fn modify_state<F>(&mut self, modification: F, undo_label: Option<TransitionLabel>)
    where
        F: FnOnce(AppState) -> AppState,
    {
        let state = std::mem::take(&mut self.app_state);
        let old_state = state.clone();
        self.app_state = modification(state);
        if let Some(label) = undo_label {
            if old_state != self.app_state && old_state.is_in_stable_state() {
                let camera_3d = self.get_camera_3d();
                self.undo_stack.push(AppStateTransition {
                    state: old_state,
                    label,
                    camera_3d,
                });
                self.redo_stack.clear();
            }
        }
    }

    fn update_pending_operation(&mut self, operation: Arc<dyn Operation>) {
        let result = self.app_state.update_pending_operation(operation.clone());
        if let Err(ErrOperation::FinishFirst) = result {
            self.modify_state(
                |s| s.notified(app_state::InteractorNotification::FinishOperation),
                None,
            );
            self.update_pending_operation(operation)
        }
        self.apply_operation_result(result);
    }

    fn optimize_shift(&mut self) {
        let reader = &mut self.channel_reader;
        let result = self.app_state.optimize_shift(reader);
        self.apply_operation_result(result);
    }

    fn apply_operation_result(&mut self, result: Result<OkOperation, ErrOperation>) {
        match result {
            Ok(OkOperation::Undoable { state, label }) => self.save_old_state(state, label),
            Ok(OkOperation::NotUndoable) => (),
            Err(e) => log::warn!("{:?}", e),
        }
        if let Some(new_selection) = self.app_state.get_new_selection() {
            self.modify_state(|s| s.with_selection(new_selection, None), None)
        }
    }

    fn request_copy(&mut self) {
        let reader = self.app_state.get_design_reader();
        let selection = self.app_state.get_selection();
        if let Some((_, xover_ids)) =
            ensnano_interactor::list_of_xover_as_nucl_pairs(selection.as_ref(), &reader)
        {
            self.apply_copy_operation(CopyOperation::CopyXovers(xover_ids))
        } else if let Some(grid_ids) = ensnano_interactor::extract_only_grids(selection.as_ref()) {
            self.apply_copy_operation(CopyOperation::CopyGrids(grid_ids))
        } else if let Some((_, helices)) = ensnano_interactor::list_of_helices(selection.as_ref()) {
            self.apply_copy_operation(CopyOperation::CopyHelices(helices))
        } else {
            let strand_ids = ensnano_interactor::extract_strands_from_selection(
                self.app_state.get_selection().as_ref(),
            );
            self.apply_copy_operation(CopyOperation::CopyStrands(strand_ids))
        }
    }

    fn apply_paste(&mut self) {
        log::info!("apply paste");
        match self.app_state.get_pasting_status() {
            PastingStatus::Copy => self.apply_copy_operation(CopyOperation::Paste),
            PastingStatus::Duplication => self.apply_copy_operation(CopyOperation::Duplicate),
            _ => log::info!("Not pasting"),
        }
    }

    fn request_duplication(&mut self) {
        if self.app_state.can_iterate_duplication() {
            self.apply_copy_operation(CopyOperation::Duplicate)
        } else if let Some((_, nucl_pairs)) = ensnano_interactor::list_of_xover_as_nucl_pairs(
            self.app_state.get_selection().as_ref(),
            &self.app_state.get_design_reader(),
        ) {
            self.apply_copy_operation(CopyOperation::InitXoverDuplication(nucl_pairs))
        } else if let Some((_, helices)) =
            ensnano_interactor::list_of_helices(self.app_state.get_selection().as_ref())
        {
            self.apply_copy_operation(CopyOperation::InitHelicesDuplication(helices))
        } else {
            let strand_ids = ensnano_interactor::extract_strands_from_selection(
                self.app_state.get_selection().as_ref(),
            );
            self.apply_copy_operation(CopyOperation::InitStrandsDuplication(strand_ids))
        }
    }

    fn save_design(&mut self, path: &PathBuf) -> Result<(), SaveDesignError> {
        let camera = self
            .applications
            .get(&ElementType::Scene)
            .and_then(|s| s.lock().unwrap().get_camera())
            .map(|camera| Camera {
                id: Default::default(),
                name: String::from("Saved Camera"),
                position: camera.0.position,
                orientation: camera.0.orientation,
                pivot_position: camera.0.pivot_position,
            });
        let save_info = ensnano_design::SavingInformation { camera };
        self.app_state.save_design(path, save_info)?;

        if self.app_state.is_in_stable_state() {
            self.last_saved_state = self.app_state.clone();
        }
        self.update_current_file_name();
        Ok(())
    }

    fn save_backup(&mut self) -> Result<(), SaveDesignError> {
        let camera = self
            .applications
            .get(&ElementType::Scene)
            .and_then(|s| s.lock().unwrap().get_camera())
            .map(|camera| Camera {
                id: Default::default(),
                name: String::from("Saved Camera"),
                position: camera.0.position,
                orientation: camera.0.orientation,
                pivot_position: camera.0.pivot_position,
            });
        let save_info = ensnano_design::SavingInformation { camera };
        let path = if let Some(mut path) = self.app_state.path_to_current_design().cloned() {
            path.set_extension(crate::consts::ENS_BACKUP_EXTENSION);
            path
        } else {
            let mut ret = dirs::document_dir()
                .or_else(dirs::home_dir)
                .ok_or_else(|| {
                    self.last_backup_date =
                        Instant::now() + Duration::from_secs(crate::consts::SEC_PER_YEAR);
                    SaveDesignError::cannot_open_default_dir()
                })?;
            ret.push(crate::consts::ENS_UNNAMED_FILE_NAME);
            ret.set_extension(crate::consts::ENS_BACKUP_EXTENSION);
            ret
        };
        if self.app_state.is_in_stable_state() {
            self.app_state.save_design(&path, save_info)?;
            self.last_backed_up_state = self.app_state.clone();
            println!("Saved backup to {}", path.to_string_lossy());
        } else {
            // Do nothing. We do not want to save backup in transitory states.
        }

        Ok(())
    }

    fn change_selection_mode(&mut self, mode: SelectionMode) {
        self.modify_state(|s| s.with_selection_mode(mode), None)
    }

    fn change_action_mode(&mut self, mode: ActionMode) {
        self.modify_state(|s| s.with_action_mode(mode), None)
    }

    fn change_double_strand_parameters(&mut self, parameters: Option<(isize, usize)>) {
        self.modify_state(|s| s.with_strand_on_helix(parameters), None)
    }

    fn toggle_widget_basis(&mut self) {
        self.modify_state(|s| s.with_toggled_widget_basis(), None)
    }

    fn set_visibility_sieve(&mut self, selection: Vec<Selection>, compl: bool) {
        let result = self.app_state.set_visibility_sieve(selection, compl);
        self.apply_operation_result(result)
    }

    fn need_save(&self) -> bool {
        self.app_state.design_was_modified(&self.last_saved_state)
    }

    fn get_current_file_name(&self) -> Option<&Path> {
        self.file_name.as_ref().map(|p| p.as_ref())
    }

    fn update_current_file_name(&mut self) {
        self.file_name = self
            .app_state
            .path_to_current_design()
            .as_ref()
            .filter(|p| p.is_file())
            .map(|p| p.into())
    }

    fn set_suggestion_parameters(&mut self, param: SuggestionParameters) {
        self.modify_state(|s| s.with_suggestion_parameters(param), None)
    }

    fn set_check_xovers_parameters(&mut self, param: CheckXoversParameter) {
        self.modify_state(|s| s.with_check_xovers_parameters(param), None)
    }

    fn set_follow_stereographic_camera(&mut self, follow: bool) {
        self.modify_state(|s| s.with_follow_stereographic_camera(follow), None)
    }

    fn set_show_stereographic_camera(&mut self, show: bool) {
        self.modify_state(|s| s.with_show_stereographic_camera(show), None)
    }

    fn set_show_h_bonds(&mut self, show: ensnano_interactor::graphics::HBondDisplay) {
        self.modify_state(|s| s.with_show_h_bonds(show), None)
    }

    fn set_show_bezier_paths(&mut self, show: bool) {
        self.modify_state(|s| s.with_show_bezier_paths(show), None)
    }

    fn set_all_helices_on_axis(&mut self, off_axis: bool) {
        self.modify_state(|s| s.all_helices_on_axis(off_axis), None)
    }

    fn set_bezier_revolution_id(&mut self, id: Option<usize>) {
        self.modify_state(|s| s.set_bezier_revolution_id(id), None)
    }

    fn set_bezier_revolution_radius(&mut self, radius: f64) {
        self.modify_state(|s| s.set_bezier_revolution_radius(radius), None)
    }

    fn set_revolution_axis_position(&mut self, position: f64) {
        self.modify_state(|s| s.set_revolution_axis_position(position), None)
    }

    /// Create a bezier plane where the user is looking at if there are no bezier plane yet.
    fn create_default_bezier_plane(&mut self) {
        use ensnano_scene::DesignReader;
        if self.app_state.get_design_reader().get_bezier_planes().len() == 0 {
            if let Some((position, orientation)) = self.get_bezier_sheet_creation_position() {
                self.apply_operation(DesignOperation::AddBezierPlane {
                    desc: ensnano_design::BezierPlaneDescriptor {
                        position,
                        orientation,
                    },
                })
            }
        }
    }

    fn set_unrooted_surface(&mut self, surface: Option<UnrootedRevolutionSurfaceDescriptor>) {
        self.modify_state(|s| s.set_unrooted_surface(surface), None)
    }

    fn get_grid_creation_position(&self) -> Option<(Vec3, Rotor3)> {
        self.applications
            .get(&ElementType::Scene)
            .and_then(|s| s.lock().unwrap().get_position_for_new_grid())
    }

    fn get_bezier_sheet_creation_position(&self) -> Option<(Vec3, Rotor3)> {
        self.get_grid_creation_position()
            .map(|(position, orientation)| {
                (
                    position - 30. * Vec3::unit_x().rotated_by(orientation),
                    orientation,
                )
            })
    }

    fn toggle_all_helices_on_axis(&mut self) {
        self.modify_state(|s| s.with_toggled_all_helices_on_axis(), None)
    }

    fn set_background_3d(&mut self, bg: ensnano_interactor::graphics::Background3D) {
        self.modify_state(|s| s.with_background3d(bg), None)
    }

    fn set_rendering_mode(&mut self, rendering_mode: ensnano_interactor::graphics::RenderingMode) {
        self.modify_state(|s| s.with_rendering_mode(rendering_mode), None)
    }

    fn set_scroll_sensitivity(&mut self, sensitivity: f32) {
        self.modify_state(|s| s.with_scroll_sensitivity(sensitivity), None)
    }

    fn set_invert_y_scroll(&mut self, inverted: bool) {
        self.modify_state(|s| s.with_inverted_y_scroll(inverted), None)
    }

    fn gui_state(&self, multiplexer: &Multiplexer) -> gui::MainState {
        gui::MainState {
            can_undo: !self.undo_stack.is_empty(),
            can_redo: !self.redo_stack.is_empty(),
            need_save: self.need_save(),
            can_reload: self.get_current_file_name().is_some(),
            can_split2d: multiplexer.is_showing(&ElementType::FlatScene),
            splited_2d: self
                .applications
                .get(&ElementType::FlatScene)
                .map(|app| app.lock().unwrap().is_splited())
                .unwrap_or(false),
            can_toggle_2d: multiplexer.is_showing(&ElementType::FlatScene)
                || multiplexer.is_showing(&ElementType::StereographicScene),
        }
    }

    fn get_camera_3d(&self) -> ensnano_interactor::application::Camera3D {
        self.applications
            .get(&ElementType::Scene)
            .expect("Could not get scene element")
            .lock()
            .unwrap()
            .get_camera()
            .unwrap()
            .as_ref()
            .clone()
            .0
    }

    fn set_camera_3d(&self, camera: ensnano_interactor::application::Camera3D) {
        self.applications
            .get(&ElementType::Scene)
            .expect("Could not get scene element")
            .lock()
            .unwrap()
            .on_notify(Notification::TeleportCamera(camera));
    }
}

/// A temporary view of the main state and the control flow.
struct MainStateView<'a> {
    main_state: &'a mut MainState,
    control_flow: &'a mut ControlFlow,
    multiplexer: &'a mut Multiplexer,
    scheduler: &'a mut Scheduler,
    gui: &'a mut Gui<Requests, AppState>,
    window: &'a Window,
    resized: bool,
}

use controller::{LoadDesignError, MainState as MainStateInterface, StaplesDownloader};
impl<'a> MainStateInterface for MainStateView<'a> {
    fn pop_action(&mut self) -> Option<Action> {
        if !self.main_state.pending_actions.is_empty() {
            log::debug!("pending actions {:?}", self.main_state.pending_actions);
        }
        self.main_state.pending_actions.pop_front()
    }

    fn check_backup(&mut self) {
        if !self
            .main_state
            .last_backed_up_state
            .design_was_modified(&self.main_state.app_state)
            || !self
                .main_state
                .last_saved_state
                .design_was_modified(&self.main_state.app_state)
        {
            self.main_state.last_backup_date = Instant::now()
        }
    }

    fn need_backup(&self) -> bool {
        Instant::now() - self.main_state.last_backup_date
            > Duration::from_secs(crate::consts::SEC_BETWEEN_BACKUPS)
    }

    fn exit_control_flow(&mut self) {
        *self.control_flow = ControlFlow::Exit
    }

    fn new_design(&mut self) {
        self.notify_apps(Notification::ClearDesigns);
        self.main_state.new_design()
    }

    fn export(&mut self, path: &PathBuf, export_type: ExportType) -> ExportResult {
        let ret = self.main_state.app_state.export(path, export_type);
        self.set_exporting(false);
        ret
    }

    fn load_design(&mut self, path: PathBuf) -> Result<(), LoadDesignError> {
        let state = AppState::import_design(path)?;
        self.notify_apps(Notification::ClearDesigns);
        self.main_state.clear_app_state(state);
        if let Some((position, orientation)) = self
            .main_state
            .app_state
            .get_design_reader()
            .get_favourite_camera()
        {
            self.notify_apps(Notification::TeleportCamera(
                ensnano_interactor::application::Camera3D {
                    position,
                    orientation,
                    pivot_position: None,
                },
            ));
        } else {
            self.main_state.wants_fit = true;
        }
        self.main_state.update_current_file_name();
        Ok(())
    }

    fn get_chanel_reader(&mut self) -> &mut ChannelReader {
        &mut self.main_state.channel_reader
    }

    fn apply_operation(&mut self, operation: DesignOperation) {
        self.main_state.apply_operation(operation)
    }

    fn apply_silent_operation(&mut self, operation: DesignOperation) {
        self.main_state.apply_silent_operation(operation)
    }

    fn undo(&mut self) {
        self.main_state.undo();
    }

    fn redo(&mut self) {
        self.main_state.redo();
    }

    fn get_staple_downloader(&self) -> Box<dyn StaplesDownloader> {
        Box::new(self.main_state.app_state.get_design_reader())
    }

    fn save_design(&mut self, path: &PathBuf) -> Result<(), SaveDesignError> {
        self.main_state.save_design(path)?;
        self.main_state.last_backup_date = Instant::now();
        Ok(())
    }

    fn save_backup(&mut self) -> Result<(), SaveDesignError> {
        self.main_state.save_backup()?;
        self.main_state.last_backup_date = Instant::now();
        Ok(())
    }

    fn toggle_split_mode(&mut self, mode: SplitMode) {
        self.multiplexer.change_split(mode);
        self.scheduler
            .forward_new_size(self.window.inner_size(), self.multiplexer);
        self.gui.resize(self.multiplexer, self.window);
    }

    fn change_ui_size(&mut self, ui_size: UiSize) {
        self.gui.new_ui_size(
            ui_size,
            self.window,
            self.multiplexer,
            &self.main_state.app_state,
            self.main_state.gui_state(self.multiplexer),
        );
        self.multiplexer.change_ui_size(ui_size, self.window);
        self.main_state
            .messages
            .lock()
            .unwrap()
            .new_ui_size(ui_size);
        self.main_state
            .modify_state(|s| s.with_ui_size(ui_size), None);
        self.resized = true;
        //messages.lock().unwrap().new_ui_size(ui_size);
    }

    fn notify_apps(&mut self, notification: Notification) {
        log::info!("Notify apps {:?}", notification);
        for app in self.main_state.applications.values_mut() {
            app.lock().unwrap().on_notify(notification.clone())
        }
    }

    fn get_selection(&mut self) -> Box<dyn AsRef<[Selection]>> {
        Box::new(self.main_state.app_state.get_selection())
    }

    fn get_design_reader(&mut self) -> Box<dyn DesignReader> {
        Box::new(self.main_state.app_state.get_design_reader())
    }

    fn get_grid_creation_position(&self) -> Option<(Vec3, Rotor3)> {
        self.main_state.get_grid_creation_position()
    }

    fn get_bezier_sheet_creation_position(&self) -> Option<(Vec3, Rotor3)> {
        self.main_state.get_bezier_sheet_creation_position()
    }

    fn finish_operation(&mut self) {
        self.main_state.modify_state(
            |s| s.notified(app_state::InteractorNotification::FinishOperation),
            None,
        );
        self.main_state.app_state.finish_operation();
    }

    fn request_copy(&mut self) {
        self.main_state.request_copy()
    }

    fn init_paste(&mut self) {
        self.main_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(None));
    }

    fn apply_paste(&mut self) {
        self.main_state.apply_paste();
    }

    fn duplicate(&mut self) {
        self.main_state.request_duplication();
    }

    fn request_pasting_candidate(&mut self, candidate: Option<PastePosition>) {
        self.main_state
            .apply_copy_operation(CopyOperation::PositionPastingPoint(candidate))
    }

    fn delete_selection(&mut self) {
        let selection = self.get_selection();
        if let Some((_, nucl_pairs)) = ensnano_interactor::list_of_xover_as_nucl_pairs(
            selection.as_ref().as_ref(),
            self.get_design_reader().as_ref(),
        ) {
            self.main_state.update_selection(vec![], None);
            self.main_state
                .apply_operation(DesignOperation::RmXovers { xovers: nucl_pairs })
        } else if let Some((_, strand_ids)) =
            ensnano_interactor::list_of_strands(selection.as_ref().as_ref())
        {
            self.main_state.update_selection(vec![], None);
            self.main_state
                .apply_operation(DesignOperation::RmStrands { strand_ids })
        } else if let Some((_, h_ids)) =
            ensnano_interactor::list_of_helices(selection.as_ref().as_ref())
        {
            self.main_state.update_selection(vec![], None);
            self.main_state
                .apply_operation(DesignOperation::RmHelices { h_ids })
        } else if let Some(grid_ids) =
            ensnano_interactor::list_of_free_grids(selection.as_ref().as_ref())
        {
            self.main_state.update_selection(vec![], None);
            self.main_state
                .apply_operation(DesignOperation::RmFreeGrids { grid_ids })
        } else if let Some(vertices) =
            ensnano_interactor::list_of_bezier_vertices(selection.as_ref().as_ref())
        {
            self.main_state.update_selection(vec![], None);
            self.main_state
                .apply_operation(DesignOperation::RmBezierVertices { vertices })
        }
    }

    fn scaffold_to_selection(&mut self) {
        let scaffold_id = self
            .main_state
            .get_app_state()
            .get_design_reader()
            .get_scaffold_info()
            .map(|info| info.id);
        if let Some(s_id) = scaffold_id {
            self.main_state
                .update_selection(vec![Selection::Strand(0, s_id as u32)], None)
        }
    }

    fn start_helix_simulation(&mut self, parameters: RigidBodyConstants) {
        self.main_state.start_helix_simulation(parameters);
    }

    fn start_grid_simulation(&mut self, parameters: RigidBodyConstants) {
        self.main_state.start_grid_simulation(parameters);
    }

    fn start_revolution_simulation(&mut self, desc: RevolutionSurfaceSystemDescriptor) {
        self.main_state.start_revolution_simulation(desc)
    }

    fn start_roll_simulation(&mut self, target_helices: Option<Vec<usize>>) {
        self.main_state.start_roll_simulation(target_helices);
    }

    fn update_simulation(&mut self, request: SimulationRequest) {
        self.main_state.update_simulation(request)
    }

    fn set_roll_of_selected_helices(&mut self, roll: f32) {
        self.main_state.set_roll_of_selected_helices(roll)
    }

    fn turn_selection_into_anchor(&mut self) {
        let selection = self.get_selection();
        let nucls = ensnano_interactor::extract_nucls_from_selection(selection.as_ref().as_ref());

        self.main_state
            .apply_operation(DesignOperation::FlipAnchors { nucls });
    }

    fn set_visibility_sieve(&mut self, compl: bool) {
        let selection = self.get_selection().as_ref().as_ref().to_vec();
        self.main_state.set_visibility_sieve(selection, compl);
    }

    fn clear_visibility_sieve(&mut self) {
        self.main_state.set_visibility_sieve(vec![], true);
    }

    fn need_save(&self) -> Option<Option<PathBuf>> {
        if self.main_state.need_save() {
            Some(self.get_current_file_name().map(Path::to_path_buf))
        } else {
            None
        }
    }

    fn get_current_design_directory(&self) -> Option<&Path> {
        let mut ancestors = self
            .main_state
            .app_state
            .path_to_current_design()
            .as_ref()
            .map(|p| p.ancestors())?;
        let first_ancestor = ancestors.next()?;
        if first_ancestor.is_dir() {
            Some(first_ancestor)
        } else {
            let second_ancestor = ancestors.next()?;
            if second_ancestor.is_dir() {
                Some(second_ancestor)
            } else {
                None
            }
        }
    }

    fn get_current_file_name(&self) -> Option<&Path> {
        self.main_state.get_current_file_name()
    }

    fn get_design_path_and_notify(&mut self, notificator: fn(Option<Arc<Path>>) -> Notification) {
        if let Some(filename) = self.get_current_file_name() {
            self.main_state
                .push_action(Action::NotifyApps(notificator(Some(Arc::from(filename)))));
        } else {
            println!("Design has not been saved yet");
            self.main_state
                .push_action(Action::NotifyApps(notificator(None)));
        }
    }

    fn set_current_group_pivot(&mut self, pivot: ensnano_design::group_attributes::GroupPivot) {
        if let Some(group_id) = self.main_state.app_state.get_current_group_id() {
            self.apply_operation(DesignOperation::SetGroupPivot { group_id, pivot })
        } else {
            self.main_state.app_state.set_current_group_pivot(pivot);
        }
    }

    fn translate_group_pivot(&mut self, translation: Vec3) {
        use ensnano_interactor::{DesignTranslation, IsometryTarget};
        if let Some(group_id) = self.main_state.app_state.get_current_group_id() {
            self.apply_operation(DesignOperation::Translation(DesignTranslation {
                target: IsometryTarget::GroupPivot(group_id),
                translation,
                group_id: None,
            }))
        } else {
            self.main_state.app_state.translate_group_pivot(translation);
        }
    }

    fn rotate_group_pivot(&mut self, rotation: Rotor3) {
        use ensnano_interactor::{DesignRotation, IsometryTarget};
        if let Some(group_id) = self.main_state.app_state.get_current_group_id() {
            self.apply_operation(DesignOperation::Rotation(DesignRotation {
                target: IsometryTarget::GroupPivot(group_id),
                rotation,
                origin: Vec3::zero(),
                group_id: None,
            }))
        } else {
            self.main_state.app_state.rotate_group_pivot(rotation);
        }
    }

    fn create_new_camera(&mut self) {
        if let Some(camera) = self
            .main_state
            .applications
            .get(&ElementType::Scene)
            .and_then(|s| s.lock().unwrap().get_camera())
        {
            self.main_state
                .apply_operation(DesignOperation::CreateNewCamera {
                    position: camera.0.position,
                    orientation: camera.0.orientation,
                    pivot_position: camera.0.pivot_position,
                })
        } else {
            log::error!("Could not get current camera position");
        }
    }

    fn select_camera(&mut self, camera_id: ensnano_design::CameraId) {
        let reader = self.main_state.app_state.get_design_reader();
        if let Some(camera) = reader.get_camera_with_id(camera_id) {
            self.notify_apps(Notification::TeleportCamera(camera))
        } else {
            log::error!("Could not get camera {:?}", camera_id)
        }
    }

    fn update_camera(&mut self, camera_id: ensnano_design::CameraId) {
        if let Some(camera) = self
            .main_state
            .applications
            .get(&ElementType::Scene)
            .and_then(|s| s.lock().unwrap().get_camera())
        {
            self.main_state
                .apply_operation(DesignOperation::UpdateCamera {
                    camera_id,
                    position: camera.0.position,
                    orientation: camera.0.orientation,
                })
        } else {
            log::error!("Could not get current camera position");
        }
    }

    fn select_favorite_camera(&mut self, n_camera: u32) {
        let reader = self.main_state.app_state.get_design_reader();
        if let Some(camera) = reader.get_nth_camera(n_camera) {
            self.notify_apps(Notification::TeleportCamera(camera))
        } else {
            log::error!("Design has less than {} cameras", n_camera + 1);
        }
    }

    fn toggle_2d(&mut self) {
        self.multiplexer.toggle_2d();
        self.scheduler
            .forward_new_size(self.window.inner_size(), self.multiplexer);
    }

    fn make_all_suggested_xover(&mut self, doubled: bool) {
        use scene::DesignReader;
        let reader = self.main_state.app_state.get_design_reader();
        let xovers = reader.get_suggestions();
        self.apply_operation(DesignOperation::MakeSeveralXovers { xovers, doubled })
    }

    fn flip_split_views(&mut self) {
        self.notify_apps(Notification::FlipSplitViews)
    }

    fn start_twist(&mut self, g_id: GridId) {
        self.main_state.start_twist(g_id);
    }

    fn set_expand_insertions(&mut self, expand: bool) {
        self.main_state
            .modify_state(|app| app.with_expand_insertion_set(expand), None);
    }

    fn set_exporting(&mut self, exporting: bool) {
        self.main_state
            .modify_state(|app| app.exporting(exporting), None)
    }

    fn load_3d_object(&mut self, path: PathBuf) {
        let design_path = self
            .get_current_design_directory()
            .map(Path::to_path_buf)
            .or_else(dirs::home_dir)
            .unwrap();
        self.apply_operation(DesignOperation::Add3DObject {
            file_path: path,
            design_path,
        })
    }

    fn load_svg(&mut self, path: PathBuf) {
        self.apply_operation(DesignOperation::ImportSvgPath { path });
    }
}

use controller::{SetScaffoldSequenceError, SetScaffoldSequenceOk};

use crate::controller::TargetScaffoldLength;
impl<'a> controller::ScaffoldSetter for MainStateView<'a> {
    fn set_scaffold_sequence(
        &mut self,
        sequence: String,
        shift: usize,
    ) -> Result<SetScaffoldSequenceOk, SetScaffoldSequenceError> {
        let len = sequence.chars().filter(|c| c.is_alphabetic()).count();
        match self
            .main_state
            .app_state
            .apply_design_op(DesignOperation::SetScaffoldSequence { sequence, shift })
        {
            Ok(OkOperation::Undoable { state, label }) => {
                self.main_state.save_old_state(state, label)
            }
            Ok(OkOperation::NotUndoable) => (),
            Err(e) => return Err(SetScaffoldSequenceError(format!("{:?}", e))),
        };
        let default_shift = self.get_staple_downloader().default_shift();
        let scaffold_length = self.get_scaffold_length().unwrap_or(0);
        let target_scaffold_length = if len == scaffold_length {
            TargetScaffoldLength::Ok
        } else {
            TargetScaffoldLength::NotOk {
                design_length: scaffold_length,
                input_scaffold_length: len,
            }
        };
        Ok(SetScaffoldSequenceOk {
            default_shift,
            target_scaffold_length,
        })
    }

    fn optimize_shift(&mut self) {
        self.main_state.optimize_shift();
    }

    fn get_scaffold_length(&self) -> Option<usize> {
        use gui::AppState;
        self.main_state
            .app_state
            .get_scaffold_info()
            .map(|info| info.length)
    }
}

fn apply_update<T: Clone, F>(obj: &mut T, update_func: F)
where
    F: FnOnce(T) -> T,
{
    let tmp = obj.clone();
    *obj = update_func(tmp);
}
