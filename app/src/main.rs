//! Mathstral: a native macOS editor for prose with inline math.
mod app;
mod config;
mod document;
mod export;
mod file;
mod keymap;
mod layout;
mod markdown;
mod math;
mod menu;
mod render;
mod structure;
#[cfg(test)]
mod verification;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = winit::event_loop::EventLoop::<menu::Action>::with_user_event().build()?;
    let mut app = app::App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}
