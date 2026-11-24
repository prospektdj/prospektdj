#[test]
fn appends_across_restarts_and_only_removes_owned_files_older_than_30_days() {
    use prospekt_dj::recommendation_log::{append, cleanup};
    let root =
        std::env::temp_dir().join(format!("prospekt-recommendations-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let at = chrono::Local::now();
    append(&root, at, "Artist — Tytuł 🎵\nInjected").unwrap();
    append(&root, at, "Second recommendation").unwrap();
    let text = std::fs::read_to_string(
        root.join(format!("recommendations-{}.txt", at.format("%Y-%m-%d"))),
    )
    .unwrap();
    assert_eq!(text.lines().count(), 2);
    assert!(text.contains("Tytuł 🎵 Injected"));
    let today = at.date_naive();
    let old = root.join(format!(
        "recommendations-{}.txt",
        today - chrono::Duration::days(31)
    ));
    let keep = root.join(format!(
        "recommendations-{}.txt",
        today - chrono::Duration::days(30)
    ));
    for path in [&old, &keep, &root.join("notes.txt")] {
        std::fs::write(path, "keep or expire").unwrap();
    }
    cleanup(&root, today).unwrap();
    assert!(!old.exists());
    assert!(keep.exists());
    assert!(root.join("notes.txt").exists());
    std::fs::remove_dir_all(root).unwrap();
}
