#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

// First, so its `t!` / `tf!` macros are visible in every module below.
#[macro_use]
mod i18n;
mod app;
mod autotest;
mod caption;
mod config;
mod download;
mod engine;
mod history;
mod inputs;
mod lora;
mod mask;
mod metadata;
mod safety;
mod theme;
mod update;

/// Window title and the name of the settings folder (Windows: %APPDATA%\<APP_NAME>\data;
/// Linux: ~/.local/share/rustimagestudio, lowercased without spaces by `eframe::storage_dir`).
pub const APP_NAME: &str = "Rust Image Studio";
/// Settings folder name used before the rename; carried over once.
const OLD_APP_NAME: &str = "Qwen Image Studio";

/// First start under the new name: copy the old settings (model paths, options, window,
/// folded cards) so the rename doesn't reset anything. The old folder is left as it was.
fn carry_over_settings() {
    if std::env::var_os("QIS_SHOT_DIR").is_some() {
        return; // a self-test run must not copy the settings early
    }
    let (Some(new), Some(old)) = (eframe::storage_dir(APP_NAME), eframe::storage_dir(OLD_APP_NAME)) else { return };
    let (new_file, old_file) = (new.join("app.ron"), old.join("app.ron"));
    if !new_file.exists() && old_file.is_file() {
        let _ = std::fs::create_dir_all(&new);
        let _ = std::fs::copy(&old_file, &new_file);
    }
}

/// The app logo (docs/logo.svg rendered to 256×256) for the title bar and taskbar.
fn window_icon() -> egui::IconData {
    let img = image::load_from_memory_with_format(include_bytes!("../assets/icon.png"), image::ImageFormat::Png)
        .expect("bundled icon.png decodes")
        .to_rgba8();
    let (width, height) = img.dimensions();
    egui::IconData { rgba: img.into_raw(), width, height }
}

fn main() -> eframe::Result {
    carry_over_settings();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title(APP_NAME)
            .with_inner_size([1480.0, 920.0])
            .with_min_inner_size([1100.0, 680.0])
            .with_icon(std::sync::Arc::new(window_icon())),
        // Self-test windows must not overwrite (or inherit) the user's window geometry.
        persist_window: std::env::var_os("QIS_SHOT_DIR").is_none() || std::env::var_os("QIS_KEEP_WINDOW").is_some(),
        ..Default::default()
    };
    eframe::run_native(APP_NAME, options, Box::new(|cc| Ok(Box::new(app::StudioApp::new(cc)))))
}
