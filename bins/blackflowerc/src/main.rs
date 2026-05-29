use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use blackflower_core::ecs::PresentationWorld;
use blackflower_net::client::{self, ClientHandle};
use blackflower_render::renderer::Renderer;
use clap::Parser;
use tracing::error;
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::WindowEvent,
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    window::{Window, WindowAttributes, WindowId},
};

#[derive(Copy, Clone, Parser)]
#[command(version, about, long_about = None)]
struct Args {
    /// Server address to bind/connect to.
    #[arg(long, default_value = "127.0.0.1:3512")]
    server_addr: SocketAddr,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(args)?;
    event_loop.run_app(&mut app).map_err(Into::into)
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,

    client_handle: ClientHandle,
    world: PresentationWorld,
}

impl App {
    fn new(args: Args) -> anyhow::Result<Self> {
        let client_handle = client::connect(args.server_addr).context("connecting to server")?;
        Ok(Self {
            window: None,
            renderer: None,
            client_handle,
            world: PresentationWorld::default(),
        })
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_width = 1280;
        let window_height = 720;
        let window_attributes = WindowAttributes::default()
            .with_resizable(false)
            .with_decorations(false)
            .with_inner_size(PhysicalSize::new(window_width, window_height));

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                error!(error = %e, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        let renderer =
            match Renderer::new_blocking(Arc::clone(&window), window_width, window_height) {
                Ok(r) => r,
                Err(e) => {
                    error!(error = %e, "failed to create renderer");
                    event_loop.exit();
                    return;
                }
            };

        self.window = Some(window);
        self.renderer = Some(renderer);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        let Some(window) = self.window.as_ref() else {
            return;
        };
        let Some(renderer) = self.renderer.as_mut() else {
            return;
        };

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::RedrawRequested => {
                renderer.render();
                window.request_redraw();
            }
            _ => {}
        }
    }
}
