use egui::{FontData, FontDefinitions, FontFamily};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
pub fn install(
    ctx: &egui::Context,
    custom: Option<&Path>,
    settings: &prospekt_dj::settings::Settings,
) -> String {
    let mut fonts = FontDefinitions::default();
    for (name, data) in [
        (
            "Emoji",
            include_bytes!("../assets/fonts/NotoEmoji.ttf").as_slice(),
        ),
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
    let regular = face(settings.interface_font, false);
    fonts
        .font_data
        .insert("interface".into(), Arc::new(FontData::from_owned(regular)));
    fonts
        .families
        .get_mut(&FontFamily::Proportional)
        .unwrap()
        .insert(0, "interface".into());
    fonts.font_data.insert(
        "numbers".into(),
        Arc::new(FontData::from_owned(face(settings.numeric_font, false))),
    );
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.insert(
        FontFamily::Monospace,
        std::iter::once("numbers".into()).chain(fallback).collect(),
    );
    let bold = FontData::from_owned(face(settings.heading_font, true));
    fonts.font_data.insert("Arial Bold".into(), Arc::new(bold));
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.insert(
        FontFamily::Name("Arial Bold".into()),
        std::iter::once("Arial Bold".into())
            .chain(fallback)
            .collect(),
    );
    fonts.font_data.insert(
        "Sedgwick Ave Display".into(),
        Arc::new(FontData::from_static(include_bytes!(
            "../assets/fonts/SedgwickAveDisplay-Regular.ttf"
        ))),
    );
    let fallback = fonts.families[&FontFamily::Proportional].clone();
    fonts.families.insert(
        FontFamily::Name("Sedgwick Ave Display".into()),
        std::iter::once("Sedgwick Ave Display".into())
            .chain(fallback)
            .collect(),
    );
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
    ctx.style_mut(|style| {
        style.text_styles.insert(
            egui::TextStyle::Heading,
            egui::FontId::new(18., FontFamily::Name("Arial Bold".into())),
        );
    });
    label
}

fn face(choice: prospekt_dj::settings::Typeface, bold: bool) -> Vec<u8> {
    use prospekt_dj::settings::Typeface;
    let names: &[&str] = match (choice, bold) {
        (Typeface::Helvetica, false) => &[
            "Helvetica.ttf",
            "Helvetica.otf",
            "Helvetica.ttc",
            "HelveticaNeue.ttc",
        ],
        (Typeface::Helvetica, true) => &[
            "Helvetica-Bold.ttf",
            "Helvetica-Bold.otf",
            "HelveticaBold.ttf",
        ],
        (Typeface::SegoeUi, false) => &["segoeui.ttf"],
        (Typeface::SegoeUi, true) => &["segoeuib.ttf"],
        (Typeface::Arial, false) => &["arial.ttf", "Arial.ttf", "LiberationSans-Regular.ttf"],
        (Typeface::Arial, true) => &["arialbd.ttf", "Arial Bold.ttf", "LiberationSans-Bold.ttf"],
        (Typeface::NimbusSans, _) => &[],
    };
    system_font(names).unwrap_or_else(|| {
        if bold {
            include_bytes!("../assets/fonts/NimbusSans-Bold.otf").to_vec()
        } else {
            include_bytes!("../assets/fonts/NimbusSans-Regular.otf").to_vec()
        }
    })
}

fn system_font(names: &[&str]) -> Option<Vec<u8>> {
    let mut roots = vec![
        PathBuf::from("C:/Windows/Fonts"),
        PathBuf::from("/usr/share/fonts"),
        PathBuf::from("/usr/local/share/fonts"),
        PathBuf::from("/Library/Fonts"),
        PathBuf::from("/System/Library/Fonts/Supplemental"),
        PathBuf::from("/System/Library/Fonts"),
    ];
    if let Some(windir) = std::env::var_os("WINDIR") {
        roots.insert(0, PathBuf::from(windir).join("Fonts"));
    }
    if let Some(home) = std::env::var_os("HOME") {
        roots.push(PathBuf::from(&home).join(".local/share/fonts"));
        roots.push(PathBuf::from(home).join(".fonts"));
    }
    if let Some(local) = std::env::var_os("LOCALAPPDATA") {
        roots.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
    }
    fn find(root: &Path, name: &str, depth: usize) -> Option<Vec<u8>> {
        for entry in std::fs::read_dir(root).ok()?.flatten() {
            let kind = entry.file_type().ok()?;
            let path = entry.path();
            if kind.is_dir() && depth > 0 {
                if let Some(data) = find(&path, name, depth - 1) {
                    return Some(data);
                }
            } else if kind.is_file()
                && entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(name)
            {
                let data = std::fs::read(path).ok()?;
                if data.len() < 16 * 1024 * 1024 && ttf_parser::Face::parse(&data, 0).is_ok() {
                    return Some(data);
                }
            }
        }
        None
    }
    for name in names {
        for root in &roots {
            if let Some(data) = find(root, name, 4) {
                return Some(data);
            }
        }
    }
    None
}
