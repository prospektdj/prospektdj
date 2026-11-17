use prospekt_dj::{
    artwork::{self, Transport},
    engine::{Command, DeckControl, Engine, PadMode},
    library::{Catalog, Entry, Query, Scope, Sort},
    media,
    music_view::{self, MusicalKey, Timeline},
};
use std::{collections::BTreeSet, path::PathBuf, sync::atomic::AtomicBool};
#[test]
fn cue_palette_timeline_and_boundary_hits_preserve_source_time() {
    assert_eq!(music_view::CUE_RGB[7], [190, 120, 255]);
    assert_eq!(music_view::CUE_RGB.iter().collect::<BTreeSet<_>>().len(), 8);
    let full = Timeline {
        start: 0.,
        span: 200.,
        duration: 200.,
    };
    assert_eq!(full.project(50.), Some(0.25));
    assert_eq!(full.at(0.25), 50.);
    let zoom = Timeline {
        start: 45.,
        span: 10.,
        duration: 200.,
    };
    assert_eq!(zoom.project(50.), Some(0.5));
    assert_eq!(zoom.project(30.), None);
    assert_eq!(zoom.project(201.), None);
    assert_eq!(full.project(f64::NAN), None);
    assert_eq!(
        Timeline {
            start: -5.,
            span: 10.,
            duration: 200.
        }
        .project(0.),
        Some(0.5)
    );
}
#[test]
fn unicode_picker_and_nested_crates_survive_json_without_normalization() {
    let mut name = "Zażółć 🎧 set".to_owned();
    let end = music_view::insert_unicode(&mut name, 7..8, "🔥🔥 ❤️ 🍕 👍🏽 👩‍💻");
    assert!(end > 8);
    assert!(name.contains("👩‍💻"));
    let mut c = Catalog::default();
    let parent = c.add_subcrate(None, &name).unwrap();
    let child = c.add_subcrate(Some(parent), "🌙 Noc").unwrap();
    c.crates[1].tracks.insert(PathBuf::from("one.flac"));
    assert!(!c.move_crate(parent, Some(child)));
    assert_eq!(c.crate_tracks(parent).len(), 1);
    let bytes = serde_json::to_vec(&c).unwrap();
    let mut restored: Catalog = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(restored.crates[0].name, name);
    assert!(restored.rename_crate(child, "🌅 Świt ❤️"));
    assert_eq!(restored.crates[1].id, child);
    restored.delete_crate(parent);
    assert_eq!(restored.crates[0].parent, None);
    assert!(restored.crates[0]
        .tracks
        .contains(&PathBuf::from("one.flac")));
    let legacy = r#"{"crates":[{"id":7,"name":"WARM UP","tracks":["one.flac"]}]}"#;
    let mut legacy: Catalog = serde_json::from_str(legacy).unwrap();
    legacy.repair_tree();
    assert_eq!(legacy.crates[0].id, 7);
    assert_eq!(legacy.crates[0].parent, None);
}
#[test]
fn four_deck_loaded_filter_and_descendant_filter_are_real() {
    let mut c = Catalog::default();
    let paths: Vec<_> = (0..4).map(|i| PathBuf::from(format!("{i}.wav"))).collect();
    for p in &paths {
        c.merge(Entry::placeholder(p.clone()));
    }
    let parent = c.add_subcrate(None, "Main").unwrap();
    c.add_subcrate(Some(parent), "Child");
    c.crates[1].tracks.insert(paths[3].clone());
    let favorites = BTreeSet::new();
    let mut q = Query {
        text: "",
        scope: &Scope::Loaded,
        sort: Sort::Title,
        descending: false,
        favorites: &favorites,
        loaded: [None, None, Some(&paths[2]), Some(&paths[3])],
    };
    assert_eq!(c.visible(&paths, &q), vec![2, 3]);
    let scope = Scope::Crate(parent);
    q.scope = &scope;
    assert_eq!(c.visible(&paths, &q), vec![3]);
}
#[test]
fn tagged_harmonic_keys_are_enharmonic_and_not_fake_analysis() {
    assert_eq!(MusicalKey::parse("Am").unwrap().camelot(), "8A");
    assert_eq!(MusicalKey::parse("C").unwrap().camelot(), "8B");
    assert_eq!(MusicalKey::parse("G♭ minor"), MusicalKey::parse("F#m"));
    assert_eq!(MusicalKey::parse("12A").unwrap().camelot(), "12A");
    assert!(MusicalKey::parse("").is_none());
    assert!(MusicalKey::parse("13B").is_none());
}
#[test]
fn marker_recall_is_mode_independent_and_preserves_transport() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(media::demo(0))));
    for (n, pos) in [(7, 0.5), (0, 2.0), (3, 1.0)] {
        e.apply(Command::Deck(0, DeckControl::Seek(pos)));
        e.apply(Command::Deck(0, DeckControl::HotCue(n, false)));
    }
    e.apply(Command::Deck(0, DeckControl::PadMode(PadMode::Sampler)));
    e.apply(Command::Deck(0, DeckControl::HotCue(7, false)));
    e.render();
    assert_eq!(e.snapshot().decks[0].position, 0.5);
    assert!(!e.snapshot().decks[0].playing);
    assert_eq!(e.snapshot().decks[0].mode, PadMode::Sampler);
    e.apply(Command::Deck(0, DeckControl::Play));
    e.apply(Command::Deck(0, DeckControl::HotCue(0, false)));
    assert!(e.snapshot().decks[0].playing);
    e.render();
    assert!((e.snapshot().decks[0].position - 2.).abs() < 0.001);
    e.apply(Command::Deck(0, DeckControl::HotCue(3, true)));
    assert!(e.snapshot().decks[0].hot[3].is_none());
    assert_eq!(e.snapshot().decks[0].hot[7], Some(0.5));
}
struct Fake {
    responses: std::collections::VecDeque<Result<Option<Vec<u8>>, String>>,
    urls: Vec<String>,
}
impl Transport for Fake {
    fn get(&mut self, url: &str) -> Result<Option<Vec<u8>>, String> {
        self.urls.push(url.into());
        self.responses.pop_front().expect("unexpected request")
    }
}
fn fixture(title: &str) -> Entry {
    Entry {
        artist: "Test Artist".into(),
        album: "Test Album".into(),
        title: title.into(),
        ..Default::default()
    }
}
fn response(v: serde_json::Value) -> Result<Option<Vec<u8>>, String> {
    Ok(Some(serde_json::to_vec(&v).unwrap()))
}
const ID: &str = "12345678-1234-1234-1234-123456789abc";
fn release(title: &str) -> serde_json::Value {
    serde_json::json!({"title":"Test Album","artist-credit":[{"artist":{"name":"Test Artist"}}],"media":[{"tracks":[{"title":title}]}]})
}
#[test]
fn artwork_verified_release_id_and_thumbnail_endpoint() {
    let mut e = fixture("Song (Club Mix)");
    e.musicbrainz_release = ID.into();
    let mut t = Fake {
        responses: vec![response(release(&e.title)), Ok(Some(vec![1, 2, 3]))].into(),
        urls: vec![],
    };
    assert_eq!(
        artwork::lookup(&mut t, &e, &AtomicBool::new(true)).unwrap(),
        Some(vec![1, 2, 3])
    );
    assert!(t.urls[1].ends_with("/front-250"));
    assert!(!t.urls.join("").contains(".flac"));
}
#[test]
fn artwork_refuses_remix_mismatch_ambiguous_search_and_missing_artist() {
    let mut e = fixture("Song (Bootleg)");
    e.musicbrainz_release = ID.into();
    let mut t = Fake {
        responses: vec![response(release("Song"))].into(),
        urls: vec![],
    };
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(true))
        .unwrap()
        .is_none());
    assert_eq!(t.urls.len(), 1);
    e.musicbrainz_release.clear();
    let candidate = serde_json::json!({"id":ID,"score":100,"title":"Test Album","artist-credit":[{"name":"Test Artist"}]});
    let mut t = Fake {
        responses: vec![response(
            serde_json::json!({"releases":[candidate,candidate]}),
        )]
        .into(),
        urls: vec![],
    };
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(true))
        .unwrap()
        .is_none());
    assert_eq!(t.urls.len(), 1);
    e.artist.clear();
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(true))
        .unwrap()
        .is_none());
    assert_eq!(t.urls.len(), 1);
}
#[test]
fn artwork_offline_404_disabled_and_metadata_identity() {
    let mut e = fixture("Song");
    e.musicbrainz_release = ID.into();
    let mut t = Fake {
        responses: vec![Err("offline".into())].into(),
        urls: vec![],
    };
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(true)).is_err());
    let mut t = Fake {
        responses: vec![Ok(None)].into(),
        urls: vec![],
    };
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(true))
        .unwrap()
        .is_none());
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(false))
        .unwrap()
        .is_none());
    let key = artwork::identity(&e);
    e.title += " Remix";
    assert_ne!(key, artwork::identity(&e));
    assert!(artwork::decode(b"bad image").is_none());
}
#[test]
fn lazy_folder_listing_reports_missing_and_skips_symlink_loops() {
    let root = std::env::temp_dir().join(format!("prospekt-tree-{}", std::process::id()));
    std::fs::create_dir_all(root.join("Music")).unwrap();
    std::fs::write(root.join("file.mp3"), b"fixture").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, root.join("loop")).unwrap();
    let list = prospekt_dj::folder_tree::list(&root);
    assert_eq!(list.children, vec![root.join("Music")]);
    assert!(list.error.is_none());
    assert!(prospekt_dj::folder_tree::list(&root.join("missing"))
        .error
        .is_some());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn recording_id_is_used_before_search_and_subtitle_version_is_verified() {
    let mut e = fixture("Song");
    e.musicbrainz_recording = ID.into();
    let recording = serde_json::json!({"title":"Song","artist-credit":[{"name":"Test Artist"}],"releases":[{"id":ID,"title":"Test Album"}]});
    let mut full = release("Song");
    full["media"][0]["tracks"][0]["recording"] = serde_json::json!({"id":ID});
    let mut t = Fake {
        responses: vec![response(recording), response(full), Ok(None)].into(),
        urls: vec![],
    };
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(true))
        .unwrap()
        .is_none());
    assert!(t.urls[0].contains(&format!("recording/{ID}?")));
    assert!(!t.urls[0].contains("query="));
    e.musicbrainz_release = ID.into();
    e.musicbrainz_recording.clear();
    e.version = "Bootleg".into();
    let mut t = Fake {
        responses: vec![response(release("Song"))].into(),
        urls: vec![],
    };
    assert!(artwork::lookup(&mut t, &e, &AtomicBool::new(true))
        .unwrap()
        .is_none());
    assert_eq!(t.urls.len(), 1);
}
#[test]
fn monochrome_emoji_font_contains_required_component_glyphs() {
    let font = ttf_parser::Face::parse(include_bytes!("../assets/fonts/NotoEmoji.ttf"), 0).unwrap();
    for c in ['🎧', '🔥', '❤', '🍕', '🌙', '👍', '🏽', '👩', '💻'] {
        assert!(font.glyph_index(c).is_some(), "missing {c}");
    }
}

#[test]
fn color_emoji_atlas_is_sorted_complete_and_contains_joined_sequences() {
    let atlas = prospekt_dj::emoji_assets::EMOJI_PNGS;
    assert_eq!(atlas.len(), 3846);
    assert!(atlas.windows(2).all(|pair| pair[0].0 < pair[1].0));
    for emoji in music_view::EMOJI {
        let png = music_view::emoji_png(emoji).unwrap_or_else(|| panic!("missing {emoji}"));
        let image = image::load_from_memory(png).unwrap().to_rgba8();
        assert_eq!(image.dimensions(), (72, 72));
        assert!(image.pixels().any(|p| p[3] > 0));
        if ["🔥", "❤️", "🍕"].contains(&emoji) {
            assert!(image.pixels().any(|p| p[3] > 0 && p[0].abs_diff(p[1]) > 30));
        }
    }
    assert_eq!(music_view::emoji_png("❤️"), music_view::emoji_png("❤"));
    assert!(music_view::emoji_png("not an emoji").is_none());
    let name = "Zażółć ❤️🍕👍🏽👩‍💻🔥🔥";
    use unicode_segmentation::UnicodeSegmentation;
    assert_eq!(
        name.graphemes(true)
            .filter(|g| music_view::emoji_png(g).is_some())
            .count(),
        6
    );
}
#[test]
fn jog_position_advances_with_playback_and_pitch_stops_and_reverses() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(media::demo(0))));
    e.apply(Command::Deck(0, DeckControl::Seek(4.)));
    for _ in 0..4800 {
        e.render();
    }
    assert_eq!(e.snapshot().decks[0].position, 4.);
    e.apply(Command::Deck(0, DeckControl::Rate(1.5)));
    e.apply(Command::Deck(0, DeckControl::Play));
    for _ in 0..4800 {
        e.render();
    }
    assert!((e.snapshot().decks[0].position - 4.15).abs() < 0.001);
    e.apply(Command::Deck(0, DeckControl::Pause));
    let at = e.snapshot().decks[0].position;
    for _ in 0..4800 {
        e.render();
    }
    assert_eq!(e.snapshot().decks[0].position, at);
    e.apply(Command::Deck(0, DeckControl::Jog(-1., 0.1)));
    for _ in 0..2400 {
        e.render();
    }
    assert!(e.snapshot().decks[0].position < at - 0.04);
}

#[test]
fn pitch_bend_returns_to_zero_on_release_focus_loss_and_keeps_base_tempo() {
    assert_eq!(music_view::momentary_bend(0.08, 0., 0., true, true), 0.08);
    assert_eq!(music_view::momentary_bend(0.08, 0., 0., false, true), 0.);
    assert_eq!(music_view::momentary_bend(0.08, 0., 0.04, true, false), 0.);
    assert_eq!(
        music_view::momentary_bend(0.08, 0., 0.04, false, true),
        0.04
    );
    assert_eq!(music_view::momentary_bend(f64::NAN, 0., 0., true, true), 0.);
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(media::demo(0))));
    e.apply(Command::Deck(0, DeckControl::Seek(2.)));
    e.apply(Command::Deck(0, DeckControl::Rate(1.1)));
    e.apply(Command::Deck(0, DeckControl::Bend(0.08)));
    e.apply(Command::Deck(0, DeckControl::Play));
    for _ in 0..4800 {
        e.render();
    }
    assert!((e.snapshot().decks[0].position - 2.118).abs() < 0.001);
    e.apply(Command::Deck(0, DeckControl::Bend(0.)));
    for _ in 0..4800 {
        e.render();
    }
    assert!((e.snapshot().decks[0].position - 2.228).abs() < 0.001);
    assert_eq!(e.snapshot().decks[0].rate, 1.1);
}
