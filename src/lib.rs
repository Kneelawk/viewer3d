#![feature(let_chains)]

#[macro_use]
extern crate tracing;

use anyhow::Context;
use cfg_if::cfg_if;
use egui::ViewportId;
use egui_wgpu::ScreenDescriptor;
use std::cmp::min;
use std::iter::once;
use std::path::PathBuf;
use std::sync::Arc;
#[cfg(target_arch = "wasm32")]
use tokio::runtime;
use tokio::runtime::Runtime;
use wgpu::{Device, Instance, Queue, Surface};
use winit::application::ApplicationHandler;
use winit::dpi::PhysicalSize;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

const WINDOW_WIDTH: u32 = 1280;
const WINDOW_HEIGHT: u32 = 720;

#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[derive(Default)]
pub struct StartupArgs {
    pub file: Option<PathBuf>,
}

#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn run() {
    match run_impl(Default::default()) {
        Ok(_) => {}
        Err(err) => {
            error!("Error running viewer {:?}", err);
        }
    }
}

pub fn run_impl(args: StartupArgs) -> anyhow::Result<()> {
    cfg_if! {
        if #[cfg(target_arch = "wasm32")] {
            std::panic::set_hook(Box::new(console_error_panic_hook::hook));
            tracing_wasm::set_as_global_default();
        } else {
            tracing_subscriber::fmt::fmt().try_init().unwrap();
        }
    }

    info!("Initializing...");

    let mut app = App::new(args).context("Creating app")?;

    let event_loop = EventLoop::new().context("Initializing event loop")?;

    event_loop.set_control_flow(ControlFlow::Poll);

    info!("Opening window...");

    event_loop.run_app(&mut app).context("Running event loop")?;

    info!("Done.");

    Ok(())
}

struct App {
    args: StartupArgs,
    runtime: Runtime,
    instance: Instance,
    surface: Option<AppSurface>,
}

struct AppSurface {
    window: Arc<Window>,
    surface: Surface<'static>,
    device: Device,
    queue: Queue,
    size: PhysicalSize<u32>,
    config: wgpu::SurfaceConfiguration,
    egui_ctx: egui::Context,
    egui_state: egui_winit::State,
    egui_renderer: egui_wgpu::Renderer,
    redrawn: bool,
    resized: bool,
}

impl App {
    fn new(args: StartupArgs) -> anyhow::Result<Self> {
        let runtime;
        #[cfg(target_arch = "wasm32")]
        {
            runtime = runtime::Builder::new_current_thread()
                .build()
                .context("Creating tokio runtime")?;
        }

        #[cfg(not(target_arch = "wasm32"))]
        {
            runtime = Runtime::new()?;
        }

        let instance = Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(all(target_arch = "wasm32", feature = "webgl")))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(all(target_arch = "wasm32", feature = "webgl"))]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        Ok(App {
            args,
            runtime,
            instance,
            surface: None,
        })
    }

    fn resize(&mut self, new_size: PhysicalSize<u32>) {
        if let Some(surface) = &mut self.surface
            && new_size.width > 0
            && new_size.height > 0
        {
            surface.size = new_size;
            surface.config.width = new_size.width;
            surface.config.height = new_size.height;
            surface.surface.configure(&surface.device, &surface.config);
        }
    }

    fn draw(&mut self, output: wgpu::SurfaceTexture) -> anyhow::Result<()> {
        if let Some(surface) = &mut self.surface {
            let view = output.texture.create_view(&Default::default());

            let mut encoder =
                surface
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Render Encoder"),
                    });

            {
                let _render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("Render Pass"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color {
                                r: 0.1,
                                g: 0.1,
                                b: 0.1,
                                a: 1.0,
                            }),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                });
            }

            let egui_output =
                surface
                    .egui_ctx
                    .run(surface.egui_state.take_egui_input(&surface.window), |ctx| {
                        egui::Window::new("Settings").show(ctx, |ui| {
                            ctx.settings_ui(ui);
                        });
                    });

            let scale_factor = surface.window.scale_factor() as f32;

            let paint_jobs = surface
                .egui_ctx
                .tessellate(egui_output.shapes, scale_factor);

            let descriptor = ScreenDescriptor {
                size_in_pixels: [surface.size.width, surface.size.height],
                pixels_per_point: scale_factor,
            };
            surface.egui_renderer.update_buffers(
                &surface.device,
                &surface.queue,
                &mut encoder,
                &paint_jobs,
                &descriptor,
            );

            for (tex_id, delta) in egui_output.textures_delta.set {
                surface.egui_renderer.update_texture(
                    &surface.device,
                    &surface.queue,
                    tex_id,
                    &delta,
                );
            }

            {
                let mut render_pass = encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Egui Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Load,
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                    })
                    .forget_lifetime();

                surface
                    .egui_renderer
                    .render(&mut render_pass, &paint_jobs, &descriptor);
            }

            surface
                .egui_state
                .handle_platform_output(&surface.window, egui_output.platform_output);

            surface.queue.submit(once(encoder.finish()));
            output.present();
        }

        Ok(())
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_visible(true)
                        .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
                        .with_min_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT))
                        .with_max_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
                )
                .expect("Creating window"),
        );

        #[cfg(target_arch = "wasm32")]
        {
            use winit::platform::web::WindowExtWebSys;
            web_sys::window()
                .and_then(|win| win.document())
                .and_then(|doc| {
                    let dst = doc.get_element_by_id("viewer3d-wasm")?;
                    let canvas = web_sys::Element::from(
                        window.canvas().expect("Unable to get window canvas"),
                    );
                    dst.append_child(&canvas).ok()?;
                    Some(())
                })
                .expect("Can't append canvas to document body")
        }

        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("Error creating surface");

        let adapter = self
            .runtime
            .block_on(self.instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: Some(&surface),
            }))
            .expect("Error creating adapter");

        let limits = if cfg!(all(target_arch = "wasm32", feature = "webgl")) {
            wgpu::Limits::downlevel_webgl2_defaults()
        } else {
            wgpu::Limits::default()
        };

        let (device, queue) = self
            .runtime
            .block_on(adapter.request_device(&wgpu::DeviceDescriptor {
                label: None,
                required_features: wgpu::Features::empty(),
                required_limits: limits.clone(),
                memory_hints: Default::default(),
                trace: wgpu::Trace::Off,
            }))
            .expect("Error requesting device");

        let surface_caps = surface.get_capabilities(&adapter);

        let surface_format = surface_caps
            .formats
            .iter()
            .find(|f| !f.is_srgb())
            .copied()
            .unwrap_or(surface_caps.formats[0]);

        let size = window.inner_size();

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format: surface_format,
            width: size.width,
            height: size.height,
            present_mode: surface_caps.present_modes[0],
            desired_maximum_frame_latency: 2,
            alpha_mode: surface_caps.alpha_modes[0],
            view_formats: vec![],
        };

        let max_tex_len = min(
            adapter.limits().max_texture_dimension_2d,
            limits.max_texture_dimension_2d,
        );

        // setup egui
        let egui_ctx = egui::Context::default();
        let egui_state = egui_winit::State::new(
            egui_ctx.clone(),
            ViewportId::ROOT,
            &window,
            Some(window.scale_factor() as f32),
            window.theme(),
            Some(max_tex_len as usize),
        );
        let egui_renderer = egui_wgpu::Renderer::new(&device, surface_format, None, 1, false);

        self.surface = Some(AppSurface {
            window,
            surface,
            device,
            queue,
            size,
            config,
            egui_ctx,
            egui_state,
            egui_renderer,
            redrawn: false,
            resized: false,
        });
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let response = if let Some(surface) = &mut self.surface {
            Some(surface.egui_state.on_window_event(&surface.window, &event))
        } else {
            None
        };

        if let Some(surface) = &mut self.surface && surface.redrawn && !surface.resized {
            let window = &surface.window;
            window.set_min_inner_size::<PhysicalSize<u32>>(None);
            window.set_max_inner_size::<PhysicalSize<u32>>(None);
            surface.resized = true;
        }

        if response.is_none_or(|res| !res.consumed) {
            match event {
                WindowEvent::CloseRequested => {
                    info!("Exiting...");
                    event_loop.exit();
                }
                WindowEvent::RedrawRequested => {
                    if self.surface.is_none() {
                        return;
                    }

                    match self.surface.as_ref().unwrap().surface.get_current_texture() {
                        Ok(texture) => {
                            if let Err(err) = self.draw(texture) {
                                error!("Draw Error: {:?}", err);
                                event_loop.exit();
                            }
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            self.resize(self.surface.as_ref().unwrap().window.inner_size());
                        }
                        Err(wgpu::SurfaceError::OutOfMemory | wgpu::SurfaceError::Other) => {
                            error!("Out of memory!");
                            event_loop.exit();
                        }
                        Err(wgpu::SurfaceError::Timeout) => {
                            warn!("Surface timeout");
                        }
                    }

                    self.surface.as_mut().unwrap().redrawn = true;
                }
                WindowEvent::Resized(new_size) => {
                    self.resize(new_size);
                }
                _ => {}
            }
        }

        if let Some(response) = response
            && response.repaint
        {
            self.surface.as_ref().unwrap().window.request_redraw();
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // drop surface
        self.surface = None;
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        // drop surface on exit
        self.surface = None;
    }
}
