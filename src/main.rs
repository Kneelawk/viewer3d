#[macro_use]
extern crate tracing;

use anyhow::Context;
use clap::Parser;
use std::path::PathBuf;
use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::window::{Window, WindowId};

/// Viewer for various 3d file formats
#[derive(Parser)]
struct Cli {
    /// The 3d file to view
    file: PathBuf,
}

fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::fmt().try_init().unwrap();

    let cli = Cli::parse();

    info!("Initializing...");

    let mut app = App::new(cli);

    let event_loop = EventLoop::new().context("Initializing event loop")?;

    event_loop.set_control_flow(ControlFlow::Poll);

    info!("Opening window...");

    event_loop.run_app(&mut app).context("Running event loop")?;

    Ok(())
}

struct App {
    cli: Cli,
    window: Option<Window>,
}

impl App {
    fn new(cli: Cli) -> Self {
        App { cli, window: None }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        self.window = Some(event_loop.create_window(Window::default_attributes().with_visible(true)).unwrap());
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _window_id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                info!("Exiting...");
                event_loop.exit();
            }
            WindowEvent::RedrawRequested => {
            }
            _ => {}
        }
    }
}
