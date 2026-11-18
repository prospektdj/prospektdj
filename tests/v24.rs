use prospekt_dj::{
    engine::{Command, DeckControl, Engine},
    media::Track,
};
use std::sync::Arc;
fn track() -> Arc<Track> {
    Arc::new(Track::from_frames(
        "Cue test".into(),
        vec![[0.25, 0.25]; 48000],
        48000,
    ))
}
#[test]
fn cues_follow_audio_across_decks_deletion_and_database_restart() {
    let dir = std::env::temp_dir().join(format!("prospekt-cue-v24-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let db = dir.join("tracks.sqlite");
    let _ = std::fs::remove_file(&db);
    let t = track();
    {
        let mut e = Engine::with_store(48000, &db).unwrap();
        e.apply(Command::Deck(0, DeckControl::Load(t.clone())));
        e.apply(Command::Deck(1, DeckControl::Load(t.clone())));
        for (slot, pos) in [(0, 0.), (2, 0.123), (7, 0.875)] {
            e.apply(Command::Deck(0, DeckControl::Seek(pos)));
            e.apply(Command::Deck(0, DeckControl::HotCue(slot, false)));
            assert_eq!(e.snapshot().decks[1].hot[slot], Some(pos));
        }
        e.apply(Command::Deck(1, DeckControl::HotCue(2, true)));
        assert_eq!(e.snapshot().decks[0].hot[2], None);
        e.flush_store().unwrap();
    }
    let mut e = Engine::with_store(48000, &db).unwrap();
    e.apply(Command::DeckCount(4));
    e.apply(Command::Deck(3, DeckControl::Load(t)));
    assert_eq!(
        e.snapshot().decks[3].hot,
        [Some(0.), None, None, None, None, None, None, Some(0.875)]
    );
    drop(e);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn audio_identity_ignores_titles_but_distinguishes_audio() {
    let a = Track::from_frames("Old title".into(), vec![[0.25, 0.25]; 100], 48000);
    let b = Track::from_frames("New title".into(), vec![[0.25, 0.25]; 100], 48000);
    let c = Track::from_frames("Old title".into(), vec![[0.5, 0.25]; 100], 48000);
    assert_eq!(a.id, b.id);
    assert_ne!(a.id, c.id);
}
#[test]
fn independent_gain_keeps_fader_mute_and_survives_track_reload() {
    let mut e = Engine::new(48000);
    e.apply(Command::Deck(0, DeckControl::Load(track())));
    assert_eq!(e.snapshot().decks[0].gain_db, 0.);
    e.apply(Command::Deck(0, DeckControl::Gain(12.)));
    e.apply(Command::Deck(0, DeckControl::Volume(0.)));
    e.apply(Command::Deck(0, DeckControl::Load(track())));
    e.apply(Command::Deck(0, DeckControl::Play));
    assert_eq!(e.snapshot().decks[0].gain_db, 12.);
    for _ in 0..1000 {
        assert_eq!(e.render().0, [0., 0.]);
    }
}

#[test]
fn history_counts_master_time_once_across_loop_seek_and_pause_but_not_pfl_only() {
    let path = std::env::temp_dir().join(format!(
        "prospekt-history-v24-{}.sqlite",
        std::process::id()
    ));
    let _ = std::fs::remove_file(&path);
    let mut e = Engine::with_store(48000, &path).unwrap();
    e.apply(Command::Deck(0, DeckControl::Load(track())));
    e.apply(Command::Deck(0, DeckControl::Volume(0.)));
    e.apply(Command::Deck(0, DeckControl::Pfl));
    e.apply(Command::Deck(0, DeckControl::Loop(1.)));
    e.apply(Command::Deck(0, DeckControl::Play));
    for _ in 0..48000 {
        e.render();
    }
    e.flush_store().unwrap();
    let connection = rusqlite::Connection::open(&path).unwrap();
    let count = || {
        connection
            .query_row("SELECT count(*) FROM plays", [], |r| r.get::<_, u64>(0))
            .unwrap()
    };
    assert_eq!(count(), 0);
    e.apply(Command::Deck(0, DeckControl::Volume(0.8)));
    for _ in 0..24001 {
        e.render();
    }
    e.apply(Command::Deck(0, DeckControl::Pause));
    e.apply(Command::Deck(0, DeckControl::Seek(0.)));
    e.apply(Command::Deck(0, DeckControl::HotCue(0, false)));
    e.apply(Command::Deck(0, DeckControl::Play));
    for _ in 0..48000 {
        e.render();
    }
    e.flush_store().unwrap();
    assert_eq!(count(), 1);
    drop(e);
    drop(connection);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn preview_cache_user_edits_and_cues_survive_without_loading_a_deck() {
    use prospekt_dj::library_data::LibraryData;
    let root = std::env::temp_dir().join(format!("prospekt-preview-v24-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let path = root.join("Zażółć.wav");
    let database = root.join("tracks.sqlite");
    let _ = std::fs::remove_file(&database);
    let spec = hound::WavSpec {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut writer = hound::WavWriter::create(&path, spec).unwrap();
    for n in 0..48000 {
        let x = ((n as f32 * 0.1).sin() * 10000.) as i16;
        writer.write_sample(x).unwrap();
        writer.write_sample(x).unwrap();
    }
    writer.finalize().unwrap();
    let wait = |data: &mut LibraryData| {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        loop {
            data.poll();
            if data.records.contains_key(&path) {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "{:?}", data.error);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    };
    {
        let mut data = LibraryData::new(database.clone());
        data.request(&path, 123);
        wait(&mut data);
        let record = &data.records[&path];
        assert!(!record.analysis.peaks.is_empty());
        assert_eq!(record.user.first_seen, 123);
        let mut user = record.user.clone();
        user.rating = 4;
        user.comment = "Świetny utwór 🎧".into();
        assert!(data.edit(&path, user));
    }
    let mut data = LibraryData::new(database);
    data.request(&path, 999);
    wait(&mut data);
    assert_eq!(data.records[&path].user.rating, 4);
    assert_eq!(data.records[&path].user.comment, "Świetny utwór 🎧");
    assert_eq!(data.records[&path].user.first_seen, 123);
    drop(data);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn waveform_presets_change_color_without_changing_cue_palette() {
    use prospekt_dj::wave_style::{Preset, WaveStyle};
    let cues = prospekt_dj::music_view::CUE_RGB;
    let mut style = WaveStyle::default();
    let standard = style.color([1., 0.4, 0.1], [80; 3]);
    style.preset = Preset::Monochrome;
    let gray = style.color([1., 0.4, 0.1], [80; 3]);
    assert_eq!(gray[0], gray[1]);
    assert_eq!(gray[1], gray[2]);
    assert_ne!(standard, gray);
    style.preset = Preset::Dark;
    assert_ne!(style.color([1., 0.4, 0.1], [80; 3]), standard);
    assert_eq!(prospekt_dj::music_view::CUE_RGB, cues);
}

#[test]
fn ratings_are_exact_filters_separate_from_favorites_and_survive_rescan() {
    use prospekt_dj::library::{Catalog, Entry, Query, Scope, Sort};
    let a = std::path::PathBuf::from("a.wav");
    let b = std::path::PathBuf::from("b.wav");
    let paths = vec![a.clone(), b.clone()];
    let mut catalog = Catalog::default();
    let mut entry = Entry::placeholder(a.clone());
    entry.rating = 3;
    entry.first_seen = 123;
    catalog.merge(entry);
    catalog.merge(Entry::placeholder(b.clone()));
    catalog.merge(Entry::placeholder(a.clone()));
    assert_eq!(catalog.entries[&a].rating, 3);
    assert_eq!(catalog.entries[&a].first_seen, 123);
    let favorites = std::collections::BTreeSet::from([b]);
    let q = Query {
        text: "",
        scope: &Scope::Rating(3),
        favorites: &favorites,
        loaded: [None; 4],
        sort: Sort::Title,
        descending: false,
    };
    assert_eq!(catalog.visible(&paths, &q), vec![0]);
}

#[test]
fn future_database_versions_are_rejected_without_downgrading() {
    let path =
        std::env::temp_dir().join(format!("prospekt-future-v24-{}.sqlite", std::process::id()));
    let c = rusqlite::Connection::open(&path).unwrap();
    c.execute_batch("PRAGMA user_version=99").unwrap();
    assert!(prospekt_dj::track_store::Store::open(&path).is_err());
    assert_eq!(
        c.query_row("PRAGMA user_version", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        99
    );
    drop(c);
    std::fs::remove_file(path).unwrap();
}

#[test]
fn manual_loop_displays_in_and_reenters_after_exit() {
    use prospekt_dj::engine::PadMode;
    let mut engine = Engine::new(48000);
    engine.apply(Command::Deck(0, DeckControl::Load(track())));
    engine.apply(Command::Deck(0, DeckControl::PadMode(PadMode::ManualLoop)));
    engine.apply(Command::Deck(0, DeckControl::Seek(0.1)));
    engine.apply(Command::Deck(0, DeckControl::Pad(0, false)));
    assert_eq!(engine.snapshot().decks[0].manual_in, Some(0.1));
    engine.apply(Command::Deck(0, DeckControl::Seek(0.6)));
    engine.apply(Command::Deck(0, DeckControl::Pad(1, false)));
    let range = engine.snapshot().decks[0].loop_range;
    assert_eq!(range, Some((0.1, 0.6)));
    engine.apply(Command::Deck(0, DeckControl::Pad(2, false)));
    assert_eq!(engine.snapshot().decks[0].loop_range, None);
    engine.apply(Command::Deck(0, DeckControl::Pad(3, false)));
    assert_eq!(engine.snapshot().decks[0].loop_range, range);
    assert_eq!(engine.snapshot().decks[0].position, 0.1);
}

#[test]
fn delayed_import_cannot_restore_a_cue_deleted_after_import_started() {
    let mut e = Engine::new(48000);
    let t = track();
    e.apply(Command::Deck(0, DeckControl::Load(t.clone())));
    let started = e.snapshot().cue_revision;
    e.apply(Command::Deck(0, DeckControl::HotCue(0, false)));
    e.apply(Command::Deck(0, DeckControl::HotCue(0, true)));
    let mut old_bank = [None; 8];
    old_bank[0] = Some(0.);
    e.apply(Command::ImportedCues(t.id.clone(), old_bank, started));
    assert_eq!(e.snapshot().decks[0].hot[0], None);
    e.apply(Command::Deck(1, DeckControl::Load(t)));
    assert_eq!(e.snapshot().decks[1].hot[0], None);
}
