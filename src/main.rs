#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod brand;
mod fonts;
mod ui;
mod widgets;
use prospekt_dj::settings;

fn main() -> eframe::Result<()> {
    let settings = settings::load();
    let pixels = image::load_from_memory(include_bytes!("../assets/icon.png"))
        .expect("embedded icon")
        .resize_exact(128, 128, image::imageops::FilterType::Lanczos3)
        .into_rgba8();
    let icon = egui::IconData {
        rgba: pixels.into_raw(),
        width: 128,
        height: 128,
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Prospekt DJ · BETA · Rust Edition")
            .with_inner_size([settings.width, settings.height])
            .with_min_inner_size([1000., 720.])
            .with_icon(icon),
        renderer: eframe::Renderer::Glow,
        ..Default::default()
    };
    eframe::run_native(
        "Prospekt DJ Rust",
        options,
        Box::new(move |cc| Ok(Box::new(ui::Studio::new(cc, settings)))),
    )
}
