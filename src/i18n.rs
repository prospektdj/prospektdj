//! Interface catalogs; DJ terms and user metadata are intentionally preserved.
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    LazyLock,
};
pub const LANGUAGES: [(&str, &str); 10] = [
    ("pl", "Polski"),
    ("en", "English"),
    ("zh", "中文"),
    ("hi", "हिन्दी"),
    ("es", "Español"),
    ("ar", "العربية"),
    ("fr", "Français"),
    ("pt", "Português"),
    ("ru", "Русский"),
    ("bn", "বাংলা"),
];
static LANGUAGE: AtomicUsize = AtomicUsize::new(0);
static CATALOG: LazyLock<std::collections::BTreeMap<String, Vec<String>>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../assets/translations.json"))
        .expect("validated translation catalog")
});
pub fn set_language(code: &str) {
    LANGUAGE.store(
        LANGUAGES.iter().position(|(c, _)| *c == code).unwrap_or(0),
        Ordering::Relaxed,
    );
}
pub fn tr(text: &str) -> String {
    shape(translate(text, LANGUAGE.load(Ordering::Relaxed)))
}
pub fn translate(text: &str, language: usize) -> &str {
    CATALOG
        .get(text)
        .and_then(|row| row.get(language))
        .map(String::as_str)
        .unwrap_or(text)
}

/// Shape Arabic joins and reorder RTL text for egui's glyph renderer.
pub fn shape(text: &str) -> String {
    if !text.chars().any(|c| ('\u{0600}'..='\u{06ff}').contains(&c)) {
        return text.to_owned();
    }
    let shaped = arabic_reshaper::arabic_reshape(text);
    let bidi = unicode_bidi::BidiInfo::new(&shaped, None);
    bidi.paragraphs
        .iter()
        .map(|p| bidi.reorder_line(p, p.range.clone()).into_owned())
        .collect::<Vec<_>>()
        .join("\n")
}
