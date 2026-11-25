//! Offline color emoji rendering from attributed, unmodified Twemoji PNG assets.
use egui::{Color32, FontId, Rect, TextureHandle, Vec2};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
#[derive(Default)]
pub struct EmojiAtlas {
    pub hidden: bool,
    /// 0 = full labels, 1 = small labels, 2 = icons only (labels with numbers stay).
    /// Keeps the header from overflowing onto the clock on narrow windows.
    pub compact: u8,
    textures: HashMap<String, (TextureHandle, u64)>,
    tick: u64,
}
impl EmojiAtlas {
    fn texture(&mut self, ctx: &egui::Context, text: &str) -> Option<TextureHandle> {
        let key = prospekt_dj::music_view::emoji_key(text);
        self.tick = self.tick.wrapping_add(1);
        if let Some((texture, used)) = self.textures.get_mut(&key) {
            *used = self.tick;
            return Some(texture.clone());
        }
        let bytes = prospekt_dj::music_view::emoji_png(text)?;
        let img = image::load_from_memory(bytes).ok()?.to_rgba8();
        let texture = ctx.load_texture(
            format!("emoji-{key}"),
            egui::ColorImage::from_rgba_unmultiplied(
                [img.width() as usize, img.height() as usize],
                img.as_raw(),
            ),
            prospekt_dj::theme::image_texture_options(),
        );
        if self.textures.len() >= 128 {
            if let Some(old) = self
                .textures
                .iter()
                .min_by_key(|(_, (_, used))| *used)
                .map(|(key, _)| key.clone())
            {
                self.textures.remove(&old);
            }
        }
        self.textures.insert(key, (texture.clone(), self.tick));
        Some(texture)
    }
    pub fn icon_button(
        &mut self,
        ctx: &egui::Context,
        icon: &str,
        label: impl Into<egui::WidgetText>,
    ) -> egui::Button<'static> {
        let mut label = label.into();
        let text = label.text().to_owned();
        if self.compact >= 1 {
            label = egui::RichText::new(&text).size(9.).into();
        }
        if !self.hidden {
            if let Some(texture) = self.texture(ctx, icon) {
                let image = egui::Image::new((texture.id(), Vec2::splat(16.)));
                if self.compact >= 2 && !text.chars().any(|c| c.is_ascii_digit()) {
                    return egui::Button::image(image).corner_radius(prospekt_dj::theme::radius());
                }
                return egui::Button::image_and_text(image, label)
                    .corner_radius(prospekt_dj::theme::radius());
            }
        }
        egui::Button::new(label).corner_radius(prospekt_dj::theme::radius())
    }
    pub fn labeled_button(
        &mut self,
        ui: &mut egui::Ui,
        emoji: &str,
        label: &str,
    ) -> egui::Response {
        if !self.hidden {
            if let Some(texture) = self.texture(ui.ctx(), emoji) {
                return ui.add(
                    egui::Button::image_and_text(
                        egui::Image::new((texture.id(), Vec2::splat(16.))),
                        label,
                    )
                    .corner_radius(prospekt_dj::theme::radius()),
                );
            }
        }
        ui.button(label)
    }
    pub fn button(&mut self, ui: &mut egui::Ui, text: &str) -> egui::Response {
        let response = if self.hidden {
            ui.add_enabled(false, egui::Button::new("—"))
        } else if let Some(texture) = self.texture(ui.ctx(), text) {
            ui.add(egui::ImageButton::new((texture.id(), Vec2::splat(23.))))
        } else {
            ui.button(text)
        };
        response.on_hover_text(text)
    }
    /// Bounded, wrapped color preview; keep grapheme/ZWJ sequences intact.
    pub fn preview(&mut self, ui: &mut egui::Ui, text: &str, width: f32, max_rows: usize) {
        if text.is_empty() {
            return;
        }
        let color = ui.visuals().text_color();
        let mut line = String::new();
        let mut used = 0_f32;
        let mut rows = 0;
        for grapheme in text.graphemes(true).take(512) {
            let size = if prospekt_dj::music_view::emoji_png(grapheme).is_some() {
                18.
            } else {
                ui.painter()
                    .layout_no_wrap(grapheme.to_owned(), FontId::proportional(11.), color)
                    .size()
                    .x
            };
            if grapheme == "\n" || (!line.is_empty() && used + size > width - 6.) {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 22.), egui::Sense::hover());
                self.paint(ui, rect, &line, color);
                rows += 1;
                if rows >= max_rows {
                    return;
                }
                line.clear();
                used = 0.;
                if grapheme == "\n" {
                    continue;
                }
            }
            line.push_str(grapheme);
            used += size;
        }
        if !line.is_empty() && rows < max_rows {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(width, 22.), egui::Sense::hover());
            self.paint(ui, rect, &line, color);
        }
    }
    pub fn paint(&mut self, ui: &egui::Ui, rect: Rect, text: &str, color: Color32) {
        if !ui.is_rect_visible(rect) {
            return;
        }
        let painter = ui.painter().with_clip_rect(rect);
        let mut x = rect.left() + 3.;
        let mut plain = String::new();
        let flush = |plain: &mut String, x: &mut f32| {
            if !plain.is_empty() {
                let galley =
                    painter.layout_no_wrap(std::mem::take(plain), FontId::proportional(11.), color);
                let y = rect.center().y - galley.size().y / 2.;
                let width = galley.size().x;
                painter.galley(egui::pos2(*x, y), galley, color);
                *x += width;
            }
        };
        for grapheme in text.graphemes(true) {
            if self.hidden && prospekt_dj::music_view::emoji_png(grapheme).is_some() {
                continue;
            }
            if grapheme.is_ascii() {
                plain.push_str(grapheme);
                continue;
            }
            if let Some(texture) = self.texture(ui.ctx(), grapheme) {
                flush(&mut plain, &mut x);
                let image =
                    Rect::from_center_size(egui::pos2(x + 8., rect.center().y), Vec2::splat(16.));
                painter.image(
                    texture.id(),
                    image,
                    Rect::from_min_max(egui::Pos2::ZERO, egui::pos2(1., 1.)),
                    Color32::WHITE,
                );
                x += 18.;
            } else {
                plain.push_str(grapheme);
            }
            if x > rect.right() {
                break;
            }
        }
        flush(&mut plain, &mut x);
    }
}
