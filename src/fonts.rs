use egui::{FontData, FontDefinitions, FontFamily};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub fn install(ctx: &egui::Context, custom: Option<&Path>) -> String {
    let mut fonts = FontDefinitions::default();
    for (name, data) in [
        (
            "Chinese",
            include_bytes!("../assets/fonts/ProspektChinese.otf").as_slice(),
        ),
        (
            "Hindi",
            include_bytes!("../assets/fonts/NotoSansDevanagari.ttf").as_slice(),
        ),
        (
            "Bengali",
            include_bytes!("../assets/fonts/NotoSansBengali.ttf").as_slice(),
        ),
        (
            "Arabic",
            include_bytes!("../assets/fonts/NotoSansArabic.ttf").as_slice(),
        ),
    ] {
        fonts
            .font_data
            .insert(name.into(), Arc::new(FontData::from_static(data)));
        for family in [FontFamily::Proportional, FontFamily::Monospace] {
            fonts.families.get_mut(&family).unwrap().push(name.into());
        }
    }
    let candidates = [
        "/Library/Fonts/HelveticaNeueLTStd-Blk.otf",
        "C:/Windows/Fonts/HelveticaNeueLTStd-Blk.otf",
        "/usr/local/share/fonts/HelveticaNeueLTStd-Blk.otf",
    ];
    let found = custom
        .map(PathBuf::from)
        .or_else(|| candidates.iter().map(PathBuf::from).find(|p| p.exists()));
    let bytes = found
        .as_ref()
        .and_then(|p| std::fs::read(p).ok())
        .filter(|b| b.len() < 16 * 1024 * 1024 && ttf_parser::Face::parse(b, 0).is_ok());
    let label = if bytes.is_some() {
        found.unwrap().display().to_string()
    } else {
        "DejaVu Sans Bold (open font fallback)".into()
    };
    let data = bytes.map(FontData::from_owned).unwrap_or_else(|| {
        FontData::from_static(include_bytes!("../assets/fonts/DejaVuSans-Bold.ttf"))
    });
    fonts.font_data.insert("logo".into(), Arc::new(data));
    fonts
        .families
        .insert(FontFamily::Name("logo".into()), vec!["logo".into()]);
    ctx.set_fonts(fonts);
    label
}
