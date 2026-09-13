//! « SUPPORT LEVEL -3 » — client (wgpu + winit).
//! Coop horreur support informatique en sous-sol.

pub mod app;
pub mod assets;
pub mod assets_bundle;
pub mod audio;
pub mod config;
pub mod game;
pub mod gpu;
pub mod hud;
pub mod lang;
pub mod net;

/// Affiche sur stderr les avertissements/erreurs wgpu (validation shader,
/// buffers...) — invisibles sinon faute de logger installé.
struct StderrLogger;

impl log::Log for StderrLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}] {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}

pub fn main_entry() {
    let _ = log::set_logger(&StderrLogger)
        .map(|_| log::set_max_level(log::LevelFilter::Warn));
    app::run();
}
