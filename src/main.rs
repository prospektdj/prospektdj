#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]
mod ui;
mod widgets;
use prospekt_dj::settings;

fn main() -> eframe::Result<()> {
    let settings = settings::load();
    let icon = eframe::icon_data::from_png_bytes(include_bytes!("../assets/icon.png"))
        .expect("embedded icon");
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("Prospekt DJ · Rust Edition")
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
