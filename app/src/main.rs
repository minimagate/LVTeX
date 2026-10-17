//! Mathstral: a native macOS editor for prose with inline math.
mod app;
mod buffer;
mod config;
mod export;
mod file;
#[cfg_attr(not(test), allow(dead_code))] // not wired into the app until the manual-math input slice
mod keymap;
mod layout;
mod markdown;
mod math;
mod menu;
mod render;
#[cfg_attr(not(test), allow(dead_code))] // not wired into the app until the manual-math input slice
mod structure;
#[cfg(test)]
mod verification;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let event_loop = winit::event_loop::EventLoop::<menu::Action>::with_user_event().build()?;
    let mut app = app::App::new(event_loop.create_proxy());
    event_loop.run_app(&mut app)?;
    Ok(())
}
