use std::{net::SocketAddr, sync::Arc};

use anyhow::Context;
use blackflower_graphics::renderer::Renderer;
use blackflower_input::{InputHandle, InputSnapshot, components::InputButtons};
use blackflower_network::client::{self, ClientHandle};
use blackflower_tick::TickScheduler;
use blackflower_world::{PresentationWorld, WorldSnapshot};
use clap::Parser;
use tracing::{error, info};
use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

#[derive(Parser)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(long, default_value_t = 1280)]
    width: u32,

    #[arg(long, default_value_t = 720)]
    height: u32,

    #[arg(long, default_value_t = 60)]
    tick_rate_hz: u64,

    #[arg(long, default_value = "127.0.0.1:3512")]
    server_addr: SocketAddr,
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();

    tracing_subscriber::registry()
        .with(fmt::layer())
        .with(EnvFilter::from_default_env())
        .init();

    let input_handle = Arc::new(InputHandle::default());
    let network_handle =
        Arc::new(client::connect(args.server_addr).context("connecting to server")?);

    let network_handle_clone = network_handle.clone();
    let input_handle_clone = input_handle.clone();
    std::thread::Builder::new()
        .name("blackflowerc::input".to_owned())
        .spawn(move || {
            TickScheduler::new(args.tick_rate_hz).start(|tick, _elapsed| {
                let snapshot = input_handle_clone.snapshot(tick);
                if tick % args.tick_rate_hz == 0 {
                    info!(tick = %tick, input = ?snapshot, "input snapshot");
                }

                network_handle_clone.try_send_input_snapshot(snapshot);
            })
        })?;

    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let mut app = App::new(input_handle, network_handle);
    event_loop.run_app(&mut app).map_err(Into::into)
}

struct App {
    window: Option<Arc<Window>>,
    renderer: Option<Renderer>,

    input_handle: Arc<InputHandle>,
    network_handle: Arc<ClientHandle<InputSnapshot, WorldSnapshot>>,

    world: PresentationWorld,
}

impl App {
    fn new(
        input_handle: Arc<InputHandle>,
        network_handle: Arc<ClientHandle<InputSnapshot, WorldSnapshot>>,
    ) -> Self {
        Self {
            window: None,
            renderer: None,

            input_handle,
            network_handle,

            world: PresentationWorld::default(),
        }
    }
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let args = Args::parse();
        let window_attributes = WindowAttributes::default()
            .with_resizable(false)
            .with_decorations(false)
            .with_inner_size(PhysicalSize::new(args.width, args.height));

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                error!(error = %e, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        let renderer = match Renderer::new_blocking(Arc::clone(&window), args.width, args.height) {
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
            WindowEvent::Focused(false) => self.input_handle.clear(),
            WindowEvent::KeyboardInput {
                event:
                    KeyEvent {
                        physical_key: PhysicalKey::Code(key),
                        state,
                        repeat: false,
                        ..
                    },
                ..
            } => {
                let button = match key {
                    KeyCode::KeyW => Some(InputButtons::FORWARD),
                    KeyCode::KeyS => Some(InputButtons::BACKWARD),
                    KeyCode::KeyA => Some(InputButtons::LEFT),
                    KeyCode::KeyD => Some(InputButtons::RIGHT),
                    _ => None,
                };
                if let Some(button) = button {
                    match state {
                        ElementState::Pressed => self.input_handle.press(button),
                        ElementState::Released => self.input_handle.release(button),
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                self.network_handle
                    .try_recv_world_snapshots()
                    .iter()
                    .for_each(|snapshot| self.world.apply(snapshot));
                renderer.render(&self.world);

                window.request_redraw();
            }
            _ => {}
        }
    }
}
