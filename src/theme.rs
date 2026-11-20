//! Application theme is independent of the Windows OS theme.
use egui::{Color32, Stroke};
pub fn apply(ctx: &egui::Context, light: bool) {
    // Pin the app preference before setting visuals. Otherwise a later Windows
    // system-theme event switches to egui's untouched light style on black panels.
    ctx.set_theme(if light {
        egui::ThemePreference::Light
    } else {
        egui::ThemePreference::Dark
    });
    ctx.send_viewport_cmd(egui::ViewportCommand::SetTheme(if light {
        egui::viewport::SystemTheme::Light
    } else {
        egui::viewport::SystemTheme::Dark
    }));
    let mut v = if light {
        egui::Visuals::light()
    } else {
        egui::Visuals::dark()
    };
    if !light {
        v.panel_fill = Color32::from_gray(8);
        v.window_fill = Color32::from_gray(8);
        v.extreme_bg_color = Color32::BLACK;
        v.faint_bg_color = Color32::BLACK;
        v.widgets.inactive.bg_fill = Color32::BLACK;
        v.widgets.inactive.weak_bg_fill = Color32::BLACK;
        v.widgets.inactive.bg_stroke = Stroke::new(1_f32, Color32::BLACK);
        v.widgets.hovered.bg_fill = Color32::BLACK;
        v.override_text_color = Some(Color32::from_rgb(216, 226, 237));
    }
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::ZERO;
    }
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.22 };
    v.selection.bg_fill = if light {
        Color32::from_rgb(27, 91, 126)
    } else {
        Color32::BLACK
    };
    if !light {
        v.window_fill = Color32::from_gray(8);
        v.panel_fill = Color32::from_gray(8);
        v.extreme_bg_color = Color32::BLACK;
        v.faint_bg_color = Color32::BLACK;
        v.code_bg_color = Color32::BLACK;
        for w in [
            &mut v.widgets.noninteractive,
            &mut v.widgets.inactive,
            &mut v.widgets.hovered,
            &mut v.widgets.active,
            &mut v.widgets.open,
        ] {
            w.bg_fill = Color32::BLACK;
            w.weak_bg_fill = Color32::BLACK;
        }
        v.selection.stroke = Stroke::new(1_f32, Color32::from_rgb(49, 188, 255));
    }
    ctx.set_visuals(v);
}
