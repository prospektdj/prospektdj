//! Original grapefruit-pink brand artwork. UV bounds remove only blank display margins.
use egui::{ColorImage, Context, Rect, TextureHandle, TextureOptions};
pub struct Brand {
    pub texture: TextureHandle,
    pub uv: Rect,
    pub aspect: f32,
}
impl Brand {
    pub fn load(ctx: &Context) -> Self {
        let image = image::load_from_memory(include_bytes!("../assets/logo.png"))
            .expect("embedded brand logo")
            .thumbnail(1536, 512)
            .into_rgba8();
        let (w, h) = image.dimensions();
        let (mut left, mut top, mut right, mut bottom) = (w, h, 0, 0);
        for (x, y, p) in image.enumerate_pixels() {
            if p[3] > 0 && p[0].max(p[1]).max(p[2]) > 40 {
                left = left.min(x);
                top = top.min(y);
                right = right.max(x);
                bottom = bottom.max(y);
            }
        }
        let uv = Rect::from_min_max(
            egui::pos2(left as f32 / w as f32, top as f32 / h as f32),
            egui::pos2(
                (right + 1) as f32 / w as f32,
                (bottom + 1) as f32 / h as f32,
            ),
        );
        let aspect = (right - left + 1) as f32 / (bottom - top + 1).max(1) as f32;
        let texture = ctx.load_texture(
            "prospekt-brand-grapefruit",
            ColorImage::from_rgba_unmultiplied([w as usize, h as usize], image.as_raw()),
            TextureOptions::LINEAR,
        );
        Self {
            texture,
            uv,
            aspect,
        }
    }
}
