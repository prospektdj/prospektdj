use prospekt_dj::library::{index, Catalog, Entry, Query, Scope, Sort};
use std::{collections::BTreeSet, path::PathBuf};
fn catalog() -> (Catalog, Vec<PathBuf>) {
    let mut catalog = Catalog::default();
    let mut paths = Vec::new();
    for (title, artist, bpm, path) in [
        ("Night Train", "Nova", 128., "/music/house/one.wav"),
        ("Slow Signal", "Luna", 118., "/music/house/two.wav"),
        ("Bright Day", "Nova", 124., "/music/house-extra/three.wav"),
    ] {
        let path = PathBuf::from(path);
        paths.push(path.clone());
        catalog.merge(Entry {
            path,
            title: title.into(),
            artist: artist.into(),
            album: "Collection".into(),
            genre: "House".into(),
            bpm: Some(bpm),
            duration: Some(123.),
            ..Default::default()
        });
    }
    (catalog, paths)
}
#[test]
fn search_uses_all_words_across_metadata_and_sort_is_numeric() {
    let (c, p) = catalog();
    let favorites = BTreeSet::new();
    let mut q = Query {
        text: "nova night",
        scope: &Scope::All,
        sort: Sort::Title,
        descending: false,
        favorites: &favorites,
        loaded: [None, None],
    };
    assert_eq!(c.visible(&p, &q), vec![0]);
    q.text = "";
    q.sort = Sort::Bpm;
    assert_eq!(c.visible(&p, &q), vec![1, 2, 0]);
    q.descending = true;
    assert_eq!(c.visible(&p, &q), vec![0, 2, 1]);
}
#[test]
fn scopes_distinguish_folder_boundaries_crates_loaded_and_missing() {
    let (mut c, p) = catalog();
    let id = c.add_crate("WARM UP").unwrap();
    c.crates[0].tracks.insert(p[1].clone());
    c.entries.get_mut(&p[2]).unwrap().missing = true;
    let favorites = BTreeSet::from([p[0].clone()]);
    let visible = |scope| {
        c.visible(
            &p,
            &Query {
                text: "",
                scope: &scope,
                sort: Sort::Title,
                descending: false,
                favorites: &favorites,
                loaded: [Some(p[1].as_path()), None],
            },
        )
    };
    assert_eq!(visible(Scope::Crate(id)), vec![1]);
    assert_eq!(visible(Scope::Favorites), vec![0]);
    assert_eq!(visible(Scope::Loaded), vec![1]);
    assert_eq!(visible(Scope::Missing), vec![2]);
    assert_eq!(visible(Scope::Folder("/music/house".into())), vec![0, 1]);
}
#[test]
fn cache_and_crates_survive_restart_and_removal_does_not_delete_audio() {
    let root = std::env::temp_dir().join(format!("prospekt-library-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("track.wav");
    std::fs::write(&file, b"test fixture").unwrap();
    let mut c = Catalog::default();
    c.merge(Entry::placeholder(file.clone()));
    let id = c.add_crate("  TEST  ").unwrap();
    assert!(c.add_crate("test").is_none());
    c.crates[0].tracks.insert(file.clone());
    c.save(&root.join("library.json")).unwrap();
    c.crates[0].name = "Renamed crate".into();
    c.save(&root.join("library.json")).unwrap();
    let mut restored = Catalog::load(&root.join("library.json"));
    assert_eq!(restored.crates[0].id, id);
    assert_eq!(restored.crates[0].name, "Renamed crate");
    assert!(restored.crates[0].tracks.contains(&file));
    restored.remove(&file);
    assert!(file.is_file());
    assert!(restored.crates[0].tracks.is_empty());
    assert!(restored.excluded.contains(&file));
    let mut entries = Vec::new();
    index(vec![root.clone()], Vec::new(), restored, false, |batch| {
        entries.extend(batch)
    });
    assert!(entries.is_empty());
    let _ = std::fs::remove_dir_all(root);
}
#[test]
fn indexing_deduplicates_and_preserves_missing_track_metadata() {
    let root = std::env::temp_dir().join(format!("prospekt-index-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let file = root.join("tone.wav");
    let mut w = hound::WavWriter::create(
        &file,
        hound::WavSpec {
            channels: 1,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..4800 {
        w.write_sample(100i16).unwrap();
    }
    w.finalize().unwrap();
    let mut entries = Vec::new();
    index(
        vec![root.clone(), file.clone()],
        vec![file.clone()],
        Catalog::default(),
        false,
        |batch| entries.extend(batch),
    );
    assert_eq!(entries.len(), 1);
    assert!((entries[0].duration.unwrap() - 0.1).abs() < 0.001);
    let mut c = Catalog::default();
    let mut entry = entries.remove(0);
    entry.title = "Known title".into();
    entry.artist = "Known artist".into();
    c.merge(entry);
    std::fs::remove_file(&file).unwrap();
    let mut missing = Vec::new();
    index(vec![file], Vec::new(), c, false, |batch| {
        missing.extend(batch)
    });
    assert!(missing[0].missing);
    assert_eq!(missing[0].artist, "Known artist");
    let _ = std::fs::remove_dir_all(root);
}
