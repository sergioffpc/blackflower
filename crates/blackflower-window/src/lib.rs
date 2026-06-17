use std::sync::{Arc, Mutex};

use raw_window_handle::{HasDisplayHandle, HasWindowHandle};
use tracing::error;
use winit::{
    application::ApplicationHandler,
    dpi::PhysicalSize,
    event::{ElementState, KeyEvent, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow, EventLoop},
    keyboard::{KeyCode, PhysicalKey},
    window::{Window, WindowAttributes, WindowId},
};

pub trait SurfaceHandle: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static {}

impl<T> SurfaceHandle for T where T: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static {}

pub trait WindowHandler {
    fn on_create(&mut self, target: Arc<dyn SurfaceHandle>, width: u32, height: u32);
    fn on_destroy(&mut self);

    fn on_resize(&mut self, width: u32, height: u32);

    fn on_gained_focus(&mut self);
    fn on_lost_focus(&mut self);

    fn on_draw(&mut self);

    fn on_key_down(&mut self, key: &str);
    fn on_key_up(&mut self, key: &str);
}

pub fn start(
    width: u32,
    height: u32,
    window_handle: Arc<Mutex<dyn WindowHandler>>,
) -> anyhow::Result<()> {
    let event_loop = EventLoop::new()?;
    event_loop.set_control_flow(ControlFlow::Poll);

    let window = None;
    let mut app = InnerApp {
        width,
        height,
        window,
        window_handle,
    };
    event_loop.run_app(&mut app).map_err(Into::into)
}

struct InnerApp {
    width: u32,
    height: u32,
    window: Option<Arc<Window>>,
    window_handle: Arc<Mutex<dyn WindowHandler>>,
}

impl ApplicationHandler for InnerApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let window_attributes = WindowAttributes::default()
            .with_title(concat!(
                env!("CARGO_PKG_NAME"),
                " ",
                env!("CARGO_PKG_VERSION")
            ))
            .with_inner_size(PhysicalSize::new(self.width, self.height));

        let window = match event_loop.create_window(window_attributes) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                error!(error = %e, "failed to create window");
                event_loop.exit();
                return;
            }
        };

        let surface: Arc<dyn SurfaceHandle> = window.clone();
        match self.window_handle.lock() {
            Ok(mut g) => g.on_create(surface, self.width, self.height),
            Err(e) => {
                error!(error = %e, "failed to aquire window lock");
                event_loop.exit();
            }
        }

        self.window = Some(window);
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

        match event {
            WindowEvent::Resized(PhysicalSize { width, height }) => {
                self.width = width;
                self.height = height;

                match self.window_handle.lock() {
                    Ok(mut g) => g.on_resize(width, height),
                    Err(e) => {
                        error!(error = %e, "failed to aquire window lock");
                        event_loop.exit();
                    }
                }

                window.request_redraw();
            }
            WindowEvent::CloseRequested => {
                match self.window_handle.lock() {
                    Ok(mut g) => g.on_destroy(),
                    Err(e) => {
                        error!(error = %e, "failed to aquire window lock");
                    }
                }

                event_loop.exit();
            }
            WindowEvent::Focused(focus) => match self.window_handle.lock() {
                Ok(mut g) => {
                    if focus {
                        g.on_gained_focus();
                    } else {
                        g.on_lost_focus();
                    }
                }
                Err(e) => {
                    error!(error = %e, "failed to aquire window lock");
                    event_loop.exit();
                }
            },
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
                let key = match key {
                    KeyCode::KeyW => "W",
                    KeyCode::KeyA => "A",
                    KeyCode::KeyS => "S",
                    KeyCode::KeyD => "D",
                    KeyCode::Space => "Space",
                    _ => "Unknown",
                };

                match self.window_handle.lock() {
                    Ok(mut g) => match state {
                        ElementState::Pressed => g.on_key_down(key),
                        ElementState::Released => g.on_key_up(key),
                    },
                    Err(e) => {
                        error!(error = %e, "failed to aquire window lock");
                        event_loop.exit();
                    }
                }
            }
            WindowEvent::RedrawRequested => {
                match self.window_handle.lock() {
                    Ok(mut g) => g.on_draw(),
                    Err(e) => {
                        error!(error = %e, "failed to aquire window lock");
                        event_loop.exit();
                    }
                }
                window.request_redraw();
            }
            _ => {}
        }
    }
}
