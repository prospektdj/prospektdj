//! Application theme is independent of the Windows OS theme.
//! Skins: the original flat black look is kept as `Skin::Classic`.
use egui::{Color32, Stroke};
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU8, Ordering};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Skin {
    #[default]
    Modern,
    Classic,
    Carbon,
    Midnight,
    Neon,
    Dracula,
    Nord,
    Gruvbox,
    TokyoNight,
}
impl Skin {
    pub const ALL: [(Self, &'static str); 9] = [
        (Self::Modern, "Nowoczesna"),
        (Self::Classic, "Oryginalna (100%, sprzed zmian)"),
        (Self::Carbon, "Carbon"),
        (Self::Midnight, "Midnight"),
        (Self::Neon, "Neon"),
        (Self::Dracula, "Dracula"),
        (Self::Nord, "Nord"),
        (Self::Gruvbox, "Gruvbox Dark"),
        (Self::TokyoNight, "Tokyo Night"),
    ];
    pub fn name(self) -> &'static str {
        Self::ALL
            .iter()
            .find(|(skin, _)| *skin == self)
            .map_or("", |(_, name)| name)
    }
}

/// Surface colors of a rounded skin.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub deck_bg: Color32,
    pub panel: Color32,
    pub raised: Color32,
    pub raised_hover: Color32,
    pub edge: Color32,
    pub focus: Color32,
    pub focus_text: Color32,
    pub row_alt: Color32,
}

static SKIN: AtomicU8 = AtomicU8::new(0);

pub fn set_skin(skin: Skin) {
    SKIN.store(skin as u8, Ordering::Relaxed);
}
pub fn skin() -> Skin {
    match SKIN.load(Ordering::Relaxed) {
        1 => Skin::Classic,
        2 => Skin::Carbon,
        3 => Skin::Midnight,
        4 => Skin::Neon,
        5 => Skin::Dracula,
        6 => Skin::Nord,
        7 => Skin::Gruvbox,
        8 => Skin::TokyoNight,
        _ => Skin::Modern,
    }
}
pub fn classic() -> bool {
    skin() == Skin::Classic
}
pub fn palette() -> Palette {
    let rgb = Color32::from_rgb;
    match skin() {
        Skin::Carbon => Palette {
            deck_bg: rgb(12, 12, 12),
            panel: rgb(22, 22, 22),
            raised: rgb(40, 40, 40),
            raised_hover: rgb(58, 58, 58),
            edge: rgb(66, 66, 66),
            focus: rgb(214, 110, 24),
            focus_text: rgb(255, 196, 140),
            row_alt: rgb(29, 29, 29),
        },
        Skin::Midnight => Palette {
            deck_bg: rgb(9, 13, 24),
            panel: rgb(16, 22, 38),
            raised: rgb(30, 40, 64),
            raised_hover: rgb(44, 58, 90),
            edge: rgb(52, 66, 100),
            focus: rgb(0, 150, 200),
            focus_text: rgb(150, 230, 255),
            row_alt: rgb(21, 28, 46),
        },
        Skin::Neon => Palette {
            deck_bg: rgb(12, 8, 18),
            panel: rgb(22, 15, 32),
            raised: rgb(42, 30, 60),
            raised_hover: rgb(62, 44, 88),
            edge: rgb(84, 56, 120),
            focus: rgb(156, 60, 230),
            focus_text: rgb(230, 180, 255),
            row_alt: rgb(29, 20, 42),
        },
        Skin::Dracula => Palette {
            deck_bg: rgb(33, 34, 44),
            panel: rgb(40, 42, 54),
            raised: rgb(68, 71, 90),
            raised_hover: rgb(82, 86, 110),
            edge: rgb(98, 114, 164),
            focus: rgb(189, 147, 249),
            focus_text: rgb(224, 204, 255),
            row_alt: rgb(45, 47, 61),
        },
        Skin::Nord => Palette {
            deck_bg: rgb(36, 41, 51),
            panel: rgb(46, 52, 64),
            raised: rgb(59, 66, 82),
            raised_hover: rgb(67, 76, 94),
            edge: rgb(76, 86, 106),
            focus: rgb(136, 192, 208),
            focus_text: rgb(143, 188, 187),
            row_alt: rgb(53, 60, 74),
        },
        Skin::Gruvbox => Palette {
            deck_bg: rgb(29, 32, 33),
            panel: rgb(40, 40, 40),
            raised: rgb(60, 56, 54),
            raised_hover: rgb(80, 73, 69),
            edge: rgb(102, 92, 84),
            focus: rgb(250, 189, 47),
            focus_text: rgb(250, 189, 47),
            row_alt: rgb(50, 48, 47),
        },
        Skin::TokyoNight => Palette {
            deck_bg: rgb(22, 22, 30),
            panel: rgb(26, 27, 38),
            raised: rgb(36, 40, 59),
            raised_hover: rgb(41, 46, 66),
            edge: rgb(59, 66, 97),
            focus: rgb(122, 162, 247),
            focus_text: rgb(125, 207, 255),
            row_alt: rgb(31, 35, 53),
        },
        // Classic only uses these where the original code had no color.
        Skin::Modern | Skin::Classic => Palette {
            deck_bg: rgb(13, 14, 17),
            panel: rgb(24, 25, 29),
            raised: rgb(40, 42, 48),
            raised_hover: rgb(56, 59, 67),
            edge: rgb(62, 65, 74),
            focus: rgb(33, 112, 196),
            focus_text: rgb(140, 200, 255),
            row_alt: rgb(30, 31, 36),
        },
    }
}
/// Spacing unit for the modern skins: the same gap separates decks, platters,
/// the mixer, pads and labels.
pub const GAP: f32 = 10.;

/// Corner radius for buttons, pads and panels: square in the classic skin.
pub fn radius() -> u8 {
    if classic() {
        0
    } else {
        3
    }
}
pub fn radius_f(rounded: f32) -> f32 {
    if classic() {
        0.
    } else {
        rounded
    }
}
/// Deck surface fill: black in the classic skin.
pub fn deck_bg() -> Color32 {
    if classic() {
        Color32::BLACK
    } else {
        palette().deck_bg
    }
}
/// Deck row background behind decks and mixer.
pub fn row_bg() -> Color32 {
    if classic() {
        Color32::from_gray(26)
    } else {
        palette().deck_bg
    }
}

/// Readable label color on top of a solid pad color.
pub fn on_color(fill: Color32) -> Color32 {
    let luma = 0.299 * fill.r() as f32 + 0.587 * fill.g() as f32 + 0.114 * fill.b() as f32;
    if luma > 150. {
        Color32::from_gray(12)
    } else {
        Color32::WHITE
    }
}

pub fn apply(ctx: &egui::Context, light: bool) {
    if classic() {
        apply_classic(ctx, light)
    } else {
        apply_modern(ctx, light)
    }
}
fn apply_modern(ctx: &egui::Context, light: bool) {
    let p = palette();
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
        v.panel_fill = p.panel;
        v.window_fill = p.panel;
        v.extreme_bg_color = p.deck_bg;
        v.faint_bg_color = p.row_alt;
        v.widgets.inactive.bg_fill = p.raised;
        v.widgets.inactive.weak_bg_fill = p.raised;
        v.widgets.inactive.bg_stroke = Stroke::new(1_f32, p.edge);
        v.widgets.hovered.bg_fill = p.raised_hover;
        v.widgets.hovered.weak_bg_fill = p.raised_hover;
        v.widgets.active.bg_fill = p.focus;
        v.widgets.active.weak_bg_fill = p.focus;
        v.override_text_color = Some(Color32::from_rgb(216, 226, 237));
    }
    for widget in [
        &mut v.widgets.noninteractive,
        &mut v.widgets.inactive,
        &mut v.widgets.hovered,
        &mut v.widgets.active,
        &mut v.widgets.open,
    ] {
        widget.corner_radius = egui::CornerRadius::same(3);
    }
    v.handle_shape = egui::style::HandleShape::Rect { aspect_ratio: 0.22 };
    v.selection.bg_fill = if light {
        Color32::from_rgb(27, 91, 126)
    } else {
        p.focus
    };
    if !light {
        v.code_bg_color = p.deck_bg;
        v.widgets.noninteractive.bg_fill = p.panel;
        v.widgets.noninteractive.weak_bg_fill = p.panel;
        v.widgets.noninteractive.bg_stroke = Stroke::new(1_f32, Color32::from_rgb(38, 40, 46));
        v.widgets.open.bg_fill = p.raised_hover;
        v.widgets.open.weak_bg_fill = p.raised_hover;
        v.selection.stroke = Stroke::new(1_f32, p.focus_text);
        v.window_stroke = Stroke::new(1_f32, p.edge);
    }
    ctx.set_visuals(v);
}

fn apply_classic(ctx: &egui::Context, light: bool) {
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
