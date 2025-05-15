#[macro_use]
extern crate tracing;

use anyhow::Context;
use cfg_if::cfg_if;
use std::path::PathBuf;
use std::sync::Arc;
use wgpu::{Adapter, Instance, Surface};
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

    let mut app = App::new(args);

    let event_loop = EventLoop::new().context("Initializing event loop")?;

    event_loop.set_control_flow(ControlFlow::Poll);

    info!("Opening window...");

    event_loop.run_app(&mut app).context("Running event loop")?;

    Ok(())
}

struct App {
    args: StartupArgs,
    instance: Instance,
    surface: Option<AppSurface>,
}

struct AppSurface {
    window: Arc<Window>,
    surface: Surface<'static>,
    adapter: Adapter,
}

impl App {
    fn new(cli: StartupArgs) -> Self {
        let instance = Instance::new(&wgpu::InstanceDescriptor {
            #[cfg(not(target_arch = "wasm32"))]
            backends: wgpu::Backends::PRIMARY,
            #[cfg(target_arch = "wasm32")]
            backends: wgpu::Backends::GL,
            ..Default::default()
        });

        App {
            args: cli,
            instance,
            surface: None,
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        let window = Arc::new(
            event_loop
                .create_window(
                    Window::default_attributes()
                        .with_visible(true)
                        .with_inner_size(PhysicalSize::new(WINDOW_WIDTH, WINDOW_HEIGHT)),
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
                    let canvas = web_sys::Element::from(window.canvas());
                    dst.append_child(&canvas).ok()?;
                    Some(())
                })
                .expect("Can't append canvas to document body")
        }

        let surface = self
            .instance
            .create_surface(window.clone())
            .expect("Error creating surface");

        let adapter = self.instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: Default::default(),
            force_fallback_adapter: false,
            compatible_surface: Some(&surface),
        });

        self.surface = Some(AppSurface { window, surface, adapter });
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        // drop surface
        self.surface = None;
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Exiting...");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {}
            _ => {}
        }
    }
}
