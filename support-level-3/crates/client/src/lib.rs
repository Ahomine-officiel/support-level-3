//! « SUPPORT LEVEL -3 » — client (wgpu + winit).
//! Coop horreur support informatique en sous-sol.

pub mod app;
pub mod audio;
pub mod config;
pub mod game;
pub mod gpu;
pub mod hud;
pub mod lang;
pub mod net;

pub fn main_entry() {
    app::run();
}
