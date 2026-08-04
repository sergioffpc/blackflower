use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;

use anyhow::{Context as _, Error, Result};
use image::ImageFormat;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalSize};
use winit::event::{DeviceEvent, ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{CursorGrabMode, Icon, Window, WindowId};

use crate::input::{InputContext, InputState};
use crate::lifecycle::{ClientLifecycle, ClientLifecycleState, ResumeAction};
use crate::runtime::{ApplicationRuntime, FrameClock};

const WINDOW_TITLE: &str = "Blackflower";
const INITIAL_WIDTH: f64 = 1_920.0;
const INITIAL_HEIGHT: f64 = 1_080.0;
const MINIMUM_WIDTH: f64 = 1_280.0;
const MINIMUM_HEIGHT: f64 = 720.0;
const WINDOW_ICON: &[u8] = include_bytes!("../assets/icons/png/blackflower-icon-64.png");
const FOREGROUND_SHUTDOWN_POLL: std::time::Duration = std::time::Duration::from_millis(100);

struct NativeWindow {
    window: Window,
    physical_size: PhysicalSize<u32>,
    scale_factor: f64,
    occluded: bool,
}

impl NativeWindow {
    fn new(window: Window) -> Self {
        Self {
            physical_size: window.inner_size(),
            scale_factor: window.scale_factor(),
            window,
            occluded: false,
        }
    }
}

pub(crate) struct ClientApplication {
    lifecycle: ClientLifecycle,
    window: Option<NativeWindow>,
    window_icon: Icon,
    input: InputState,
    runtime: Box<dyn ApplicationRuntime>,
    frame_clock: FrameClock,
    started: Instant,
    shutdown_requested: Option<Arc<AtomicBool>>,
    failure: Option<Error>,
}

impl ClientApplication {
    pub(crate) fn with_runtime(
        runtime: Box<dyn ApplicationRuntime>,
        shutdown_requested: Option<Arc<AtomicBool>>,
    ) -> Result<Self> {
        Ok(Self {
            lifecycle: ClientLifecycle::default(),
            window: None,
            window_icon: load_window_icon()?,
            input: InputState::default(),
            runtime,
            frame_clock: FrameClock::default(),
            started: Instant::now(),
            shutdown_requested,
            failure: None,
        })
    }

    pub(crate) fn finish(mut self) -> Result<()> {
        match self.failure.take() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }

    fn create_window(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        let attributes = Window::default_attributes()
            .with_title(WINDOW_TITLE)
            .with_inner_size(LogicalSize::new(INITIAL_WIDTH, INITIAL_HEIGHT))
            .with_min_inner_size(LogicalSize::new(MINIMUM_WIDTH, MINIMUM_HEIGHT))
            .with_resizable(true)
            .with_visible(true)
            .with_window_icon(Some(self.window_icon.clone()));
        let window = event_loop
            .create_window(attributes)
            .context("native window creation failed")?;
        self.input.set_focused(window.has_focus());
        let native = NativeWindow::new(window);
        tracing::info!(
            target: "blackflower_client",
            event_name = "window_created",
            window_id = ?native.window.id(),
            width = native.physical_size.width,
            height = native.physical_size.height,
            scale_factor = native.scale_factor,
            "client window created",
        );
        self.window = Some(native);
        self.lifecycle.window_created();
        self.frame_clock.resume(Instant::now());
        Ok(())
    }

    fn handle_keyboard(&mut self, physical_key: PhysicalKey, state: ElementState, synthetic: bool) {
        self.input.keyboard_input(physical_key, state, synthetic);
        if physical_key == PhysicalKey::Code(KeyCode::Escape)
            && state == ElementState::Pressed
            && !synthetic
            && self.input.context() == InputContext::GameplayCaptured
        {
            self.release_cursor();
        }
    }

    fn handle_mouse_button(&mut self, button: MouseButton, state: ElementState) {
        self.input.mouse_input(button, state);
        if button == MouseButton::Left
            && state == ElementState::Pressed
            && self.input.focused()
            && self.input.context() == InputContext::UserInterface
        {
            self.capture_cursor();
        }
    }

    fn capture_cursor(&mut self) {
        let Some(native) = &self.window else {
            return;
        };
        let mode = match native.window.set_cursor_grab(CursorGrabMode::Locked) {
            Ok(()) => Some(CursorGrabMode::Locked),
            Err(locked_error) => match native.window.set_cursor_grab(CursorGrabMode::Confined) {
                Ok(()) => Some(CursorGrabMode::Confined),
                Err(confined_error) => {
                    tracing::warn!(
                        target: "blackflower_client",
                        event_name = "cursor_capture_failed",
                        locked_error = %locked_error,
                        confined_error = %confined_error,
                        "cursor capture unavailable",
                    );
                    None
                }
            },
        };
        if let Some(mode) = mode {
            native.window.set_cursor_visible(false);
            self.input.set_context(InputContext::GameplayCaptured);
            tracing::info!(
                target: "blackflower_client",
                event_name = "cursor_captured",
                ?mode,
                "gameplay input captured",
            );
        }
    }

    fn release_cursor(&mut self) {
        self.input.set_context(InputContext::UserInterface);
        let Some(native) = &self.window else {
            return;
        };
        if let Err(error) = native.window.set_cursor_grab(CursorGrabMode::None) {
            tracing::warn!(
                target: "blackflower_client",
                event_name = "cursor_release_failed",
                %error,
                "cursor release failed",
            );
        }
        native.window.set_cursor_visible(true);
    }

    fn begin_shutdown(&mut self, event_loop: &ActiveEventLoop, reason: &'static str) {
        if let Some(shutdown_requested) = &self.shutdown_requested {
            shutdown_requested.store(true, Ordering::Release);
        }
        if self.lifecycle.request_stop() {
            self.input.suspend();
            self.frame_clock.suspend();
            self.release_cursor();
            tracing::info!(
                target: "blackflower_client",
                event_name = "client_stopping",
                reason,
                "client stopping",
            );
        }
        event_loop.exit();
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: Error) {
        tracing::error!(
            target: "blackflower_client",
            event_name = "client_failure",
            error = %error,
            "client failed",
        );
        if self.failure.is_none() {
            self.failure = Some(error);
        }
        self.begin_shutdown(event_loop, "fatal_error");
    }

    fn owns_window(&self, window_id: WindowId) -> bool {
        self.window
            .as_ref()
            .is_some_and(|native| native.window.id() == window_id)
    }

    fn window_destroyed(&mut self, event_loop: &ActiveEventLoop) {
        self.lifecycle.window_destroyed();
        self.window = None;
        self.input.suspend();
        self.begin_shutdown(event_loop, "window_destroyed");
    }

    fn focus_changed(&mut self, focused: bool) {
        self.input.set_focused(focused);
        if !focused {
            self.release_cursor();
        }
    }

    fn resume_retained_window(&mut self) {
        if let Some(native) = &self.window {
            self.input.set_focused(native.window.has_focus());
        }
        self.frame_clock.resume(Instant::now());
        tracing::debug!(
            target: "blackflower_client",
            event_name = "client_resumed",
            "client resumed",
        );
    }

    #[allow(
        clippy::wildcard_enum_match_arm,
        reason = "unhandled winit window events are traced and future variants are non-fatal"
    )]
    fn handle_window_event(&mut self, event_loop: &ActiveEventLoop, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => self.begin_shutdown(event_loop, "close_requested"),
            WindowEvent::Destroyed => self.window_destroyed(event_loop),
            WindowEvent::Focused(focused) => self.focus_changed(focused),
            WindowEvent::Resized(size) => self.resize(size),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.scale_factor_changed(scale_factor);
            }
            WindowEvent::Occluded(occluded) => self.occluded(occluded),
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => self.handle_keyboard(event.physical_key, event.state, is_synthetic),
            WindowEvent::ModifiersChanged(modifiers) => {
                self.input.modifiers_changed(modifiers.state());
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.input.cursor_moved((position.x, position.y));
            }
            WindowEvent::CursorEntered { .. } => self.input.cursor_entered(),
            WindowEvent::CursorLeft { .. } => self.input.cursor_left(),
            WindowEvent::MouseInput { state, button, .. } => {
                self.handle_mouse_button(button, state);
            }
            WindowEvent::MouseWheel { delta, .. } => self.input.mouse_wheel(delta),
            WindowEvent::RedrawRequested => self.redraw_requested(),
            unhandled => {
                tracing::trace!(
                    target: "blackflower_client",
                    event_name = "window_event_ignored",
                    event = ?unhandled,
                    "window event ignored",
                );
            }
        }
    }

    fn resize(&mut self, size: PhysicalSize<u32>) {
        if let Some(native) = &mut self.window {
            native.physical_size = size;
        }
        tracing::info!(
            target: "blackflower_client",
            event_name = "window_resized",
            width = size.width,
            height = size.height,
            "client window resized",
        );
    }

    fn scale_factor_changed(&mut self, scale_factor: f64) {
        if let Some(native) = &mut self.window {
            native.scale_factor = scale_factor;
            native.physical_size = native.window.inner_size();
        }
        tracing::info!(
            target: "blackflower_client",
            event_name = "window_scale_changed",
            scale_factor,
            "client window scale changed",
        );
    }

    fn occluded(&mut self, occluded: bool) {
        if let Some(native) = &mut self.window {
            native.occluded = occluded;
        }
        if occluded {
            self.frame_clock.suspend();
        } else {
            self.frame_clock.resume(Instant::now());
        }
        tracing::info!(
            target: "blackflower_client",
            event_name = "window_occlusion_changed",
            occluded,
            "client window occlusion changed",
        );
    }

    fn redraw_requested(&self) {
        let Some(native) = &self.window else {
            return;
        };
        tracing::trace!(
            target: "blackflower_client",
            event_name = "redraw_deferred",
            width = native.physical_size.width,
            height = native.physical_size.height,
            scale_factor = native.scale_factor,
            occluded = native.occluded,
            presentation_frame = self.runtime.current_frame().get(),
            "presentation frame ready; renderer submission remains external",
        );
    }

    fn advance_runtime(&mut self, event_loop: &ActiveEventLoop) -> Result<()> {
        if self
            .shutdown_requested
            .as_ref()
            .is_some_and(|requested| requested.load(Ordering::Acquire))
        {
            self.begin_shutdown(event_loop, "external_request");
            return Ok(());
        }
        let can_present = self.window.as_ref().is_some_and(|native| !native.occluded)
            && self.lifecycle.state() == ClientLifecycleState::Active;
        if !can_present {
            if self.shutdown_requested.is_some() {
                event_loop.set_control_flow(ControlFlow::WaitUntil(
                    Instant::now() + FOREGROUND_SHUTDOWN_POLL,
                ));
            } else {
                event_loop.set_control_flow(ControlFlow::Wait);
            }
            return Ok(());
        }

        let now = Instant::now();
        let delta = self.frame_clock.delta(now)?;
        if !self
            .runtime
            .frame(now.duration_since(self.started), delta)?
        {
            self.begin_shutdown(event_loop, "presentation_stopped");
            return Ok(());
        }
        if let Some(native) = &self.window {
            native.window.request_redraw();
        }
        event_loop.set_control_flow(ControlFlow::WaitUntil(FrameClock::next_deadline(now)));
        Ok(())
    }
}

impl ApplicationHandler for ClientApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        match self.lifecycle.resumed() {
            ResumeAction::CreateWindow => {
                if let Err(error) = self.create_window(event_loop) {
                    self.fail(event_loop, error);
                }
            }
            ResumeAction::RetainWindow => self.resume_retained_window(),
            ResumeAction::Ignore => {}
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: WindowId,
        event: WindowEvent,
    ) {
        if self.owns_window(window_id) {
            self.handle_window_event(event_loop, event);
        }
    }

    #[allow(
        clippy::wildcard_enum_match_arm,
        reason = "unhandled raw device events are traced and future variants are non-fatal"
    )]
    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        match event {
            DeviceEvent::MouseMotion { delta } => self.input.raw_mouse_motion(delta),
            unhandled => {
                tracing::trace!(
                    target: "blackflower_client",
                    event_name = "device_event_ignored",
                    event = ?unhandled,
                    "device event ignored",
                );
            }
        }
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if let Err(error) = self.advance_runtime(event_loop) {
            self.fail(event_loop, error);
        }
    }

    fn suspended(&mut self, _event_loop: &ActiveEventLoop) {
        self.lifecycle.suspended();
        self.input.suspend();
        self.frame_clock.suspend();
        self.release_cursor();
        tracing::info!(
            target: "blackflower_client",
            event_name = "client_suspended",
            "client suspended",
        );
    }

    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.release_cursor();
        self.input.suspend();
        self.frame_clock.suspend();
        self.window = None;
        self.lifecycle.exited();
        tracing::info!(
            target: "blackflower_client",
            event_name = "client_exited",
            "client exited",
        );
    }

    fn memory_warning(&mut self, _event_loop: &ActiveEventLoop) {
        tracing::warn!(
            target: "blackflower_client",
            event_name = "memory_warning",
            "platform memory warning",
        );
    }
}

fn load_window_icon() -> Result<Icon> {
    let image = image::load_from_memory_with_format(WINDOW_ICON, ImageFormat::Png)
        .context("window icon PNG decode failed")?
        .into_rgba8();
    let (width, height) = image.dimensions();
    Icon::from_rgba(image.into_raw(), width, height).context("window icon creation failed")
}
