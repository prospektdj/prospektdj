//! Original Prospekt P icons in pink, blue and green.
use egui::{ColorImage, Context, TextureHandle};
pub struct Brand {
    pub icon: TextureHandle,
    pub blue: TextureHandle,
    pub green: TextureHandle,
    pub drive: TextureHandle,
}
impl Brand {
    pub fn load(ctx: &Context) -> Self {
        let icon_image = image::load_from_memory(include_bytes!("../assets/icon.png"))
            .expect("embedded brand icon")
            .into_rgba8();
        let icon = ctx.load_texture(
            "prospekt-watermark-icon",
            ColorImage::from_rgba_unmultiplied(
                [icon_image.width() as usize, icon_image.height() as usize],
                icon_image.as_raw(),
            ),
            prospekt_dj::theme::image_texture_options(),
        );
        let variant = |name, bytes: &[u8]| {
            let image = image::load_from_memory(bytes)
                .expect("embedded icon variant")
                .thumbnail(512, 512)
                .into_rgba8();
            ctx.load_texture(
                name,
                ColorImage::from_rgba_unmultiplied(
                    [image.width() as usize, image.height() as usize],
                    image.as_raw(),
                ),
                prospekt_dj::theme::image_texture_options(),
            )
        };
        Self {
            icon,
            blue: variant(
                "prospekt-icon-blue",
                include_bytes!("../assets/icon-blue.png"),
            ),
            green: variant(
                "prospekt-icon-green",
                include_bytes!("../assets/icon-green.png"),
            ),
            drive: variant(
                "google-drive-product-icon",
                include_bytes!("../assets/google-drive.png"),
            ),
        }
    }
}
