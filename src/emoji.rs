//! Offline color emoji rendering from attributed, unmodified Twemoji PNG assets.
use egui::{Color32, FontId, Rect, TextureHandle, Vec2};
use std::collections::HashMap;
use unicode_segmentation::UnicodeSegmentation;
#[derive(Default)]
pub struct EmojiAtlas {
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
            Default::default(),
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
    pub fn button(&mut self, ui: &mut egui::Ui, text: &str) -> egui::Response {
        let response = if let Some(texture) = self.texture(ui.ctx(), text) {
            ui.add(egui::ImageButton::new((texture.id(), Vec2::splat(23.))))
        } else {
            ui.button(text)
        };
        response.on_hover_text(text)
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
