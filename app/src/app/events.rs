//! Native window lifecycle and event dispatch.
use super::*;
use winit::{
    application::ApplicationHandler,
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    event_loop::{ActiveEventLoop, ControlFlow},
    platform::macos::WindowAttributesExtMacOS,
    window::WindowId,
};

impl ApplicationHandler<Action> for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }
        let mut attrs = Window::default_attributes()
            .with_title("Mathstral")
            .with_inner_size(LogicalSize::new(
                self.window_geometry.width,
                self.window_geometry.height,
            ))
            .with_min_inner_size(LogicalSize::new(300.0, 240.0))
            .with_titlebar_transparent(true)
            .with_title_hidden(true)
            .with_titlebar_hidden(false)
            .with_titlebar_buttons_hidden(false)
            .with_fullsize_content_view(true);
        if let Some((x, y)) = self.window_geometry.position {
            // Ignore an off-screen position after a display has been disconnected.
            let visible = event_loop.available_monitors().any(|monitor| {
                let pos = monitor.position().to_logical::<f64>(monitor.scale_factor());
                let size = monitor.size().to_logical::<f64>(monitor.scale_factor());
                x + 100.0 > pos.x
                    && x < pos.x + size.width
                    && y + 32.0 > pos.y
                    && y < pos.y + size.height
            });
            if visible {
                attrs = attrs.with_position(LogicalPosition::new(x, y));
            }
        }
        let window = Rc::new(event_loop.create_window(attrs).expect("native window"));
        window.set_ime_allowed(true);
        let context = softbuffer::Context::new(window.clone()).expect("CPU drawing context");
        self.surface =
            Some(softbuffer::Surface::new(&context, window.clone()).expect("CPU drawing surface"));
        self.window = Some(window);
        self.menu = Some(menu::install(self.proxy.clone()));
        self.changed(true);
    }
    fn user_event(&mut self, event_loop: &ActiveEventLoop, action: Action) {
        self.action(event_loop, action);
    }
    fn window_event(&mut self, event_loop: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        match event {
            WindowEvent::CloseRequested => {
                self.finish_math();
                if self.may_discard() {
                    event_loop.exit();
                }
            }
            WindowEvent::RedrawRequested => self.draw(),
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.capture_window_geometry();
                self.layout_dirty = true;
                self.line_hint = None;
                self.changed(false);
            }
            WindowEvent::Moved(_) => self.capture_window_geometry(),
            WindowEvent::ModifiersChanged(modifiers) => self.modifiers = modifiers.state(),
            WindowEvent::KeyboardInput { event, .. } => self.key(event),
            WindowEvent::Ime(Ime::Preedit(text, _)) => {
                self.composing = !text.is_empty();
            }
            WindowEvent::Ime(Ime::Commit(text)) => {
                self.composing = false;
                self.text_input(&text);
            }
            WindowEvent::Ime(Ime::Disabled) => self.composing = false,
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                if !focused {
                    self.doc.break_group();
                    self.dragging = false;
                }
                self.window.as_ref().unwrap().request_redraw();
            }
            WindowEvent::CursorMoved { position, .. } => {
                let scale = self.window.as_ref().unwrap().scale_factor();
                self.mouse = ((position.x / scale) as f32, (position.y / scale) as f32);
                if self.dragging {
                    self.mouse_caret(true);
                } else {
                    self.mls_hover();
                }
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => {
                if state == ElementState::Pressed {
                    if self.mls_click() {
                        return;
                    }
                    if self.modifiers.super_key() || self.mouse.1 < 32.0 {
                        let _ = self.window.as_ref().unwrap().drag_window();
                    } else {
                        self.finish_math();
                        self.dragging = true;
                        self.mouse_caret(self.modifiers.shift_key());
                    }
                } else {
                    self.dragging = false;
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let delta = match delta {
                    MouseScrollDelta::LineDelta(_, y) => y * LINE_HEIGHT,
                    MouseScrollDelta::PixelDelta(p) => {
                        (p.y / self.window.as_ref().unwrap().scale_factor()) as f32
                    }
                };
                self.scroll = (self.scroll - delta).max(0.0);
                self.page_dirty = true;
                self.reveal = false;
                self.window.as_ref().unwrap().request_redraw();
            }
            _ => {}
        }
    }
    fn exiting(&mut self, _event_loop: &ActiveEventLoop) {
        self.capture_window_geometry();
        file::remember_window_geometry(self.window_geometry);
    }
    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        if self
            .size_save_at
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            file::remember_window_geometry(self.window_geometry);
            self.size_save_at = None;
        }
        let wake = [self.size_save_at, self.animate()]
            .into_iter()
            .flatten()
            .min();
        event_loop.set_control_flow(wake.map_or(ControlFlow::Wait, ControlFlow::WaitUntil));
    }
}
