#[test]
fn migration_keeps_comments_and_wal_data_without_overwriting_portable_settings() {
    let root = std::env::temp_dir().join(format!("prospekt-portable-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let old = root.join("old");
    let new = root.join("portable");
    std::fs::create_dir_all(old.join("history/session")).unwrap();
    std::fs::create_dir_all(&new).unwrap();
    std::fs::write(old.join("settings.json"), "old settings").unwrap();
    std::fs::write(new.join("settings.json"), "portable settings").unwrap();
    std::fs::write(old.join("history/session/history.jsonl"), "played").unwrap();
    let db = rusqlite::Connection::open(old.join("tracks.sqlite")).unwrap();
    db.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE comments(text TEXT); INSERT INTO comments VALUES('Komentarz ❤️');").unwrap();
    prospekt_dj::portable::migrate(&old, &new).unwrap();
    let copy = rusqlite::Connection::open(new.join("tracks.sqlite")).unwrap();
    let comment: String = copy
        .query_row("SELECT text FROM comments", [], |row| row.get(0))
        .unwrap();
    assert_eq!(comment, "Komentarz ❤️");
    assert_eq!(
        std::fs::read_to_string(new.join("settings.json")).unwrap(),
        "portable settings"
    );
    assert!(old.join("tracks.sqlite").exists());
    assert!(new.join("history/session/history.jsonl").exists());
    prospekt_dj::portable::migrate(&old, &new).unwrap();
    drop(copy);
    drop(db);
    std::fs::remove_dir_all(root).unwrap();
}
// Linux package layouts (AppImage, Flatpak, system packages) only exist on Linux.
#[cfg(target_os = "linux")]
#[test]
fn linux_formats_share_a_single_writable_data_root() {
    use prospekt_dj::portable::linux_root;
    use std::path::Path;
    let home = Some(Path::new("/home/dj/.local/share/prospekt-dj"));
    assert_eq!(
        linux_root(
            Path::new("/home/dj/Downloads/Prospekt/prospekt-dj"),
            None,
            false,
            home
        )
        .unwrap(),
        Path::new("/home/dj/Downloads/Prospekt")
    );
    assert_eq!(
        linux_root(
            Path::new("/tmp/.mount_123/usr/bin/prospekt-dj"),
            Some(Path::new("/home/dj/DJ.AppImage")),
            false,
            home
        )
        .unwrap(),
        Path::new("/home/dj/prospekt-dj-data")
    );
    assert_eq!(
        linux_root(Path::new("/usr/bin/prospekt-dj"), None, false, home).unwrap(),
        Path::new("/home/dj/.local/share/prospekt-dj")
    );
    let flatpak = Some(Path::new(
        "/home/dj/.var/app/org.prospektdj.ProspektDJ/data/prospekt-dj",
    ));
    assert_eq!(
        linux_root(Path::new("/app/bin/prospekt-dj"), None, true, flatpak).unwrap(),
        Path::new("/home/dj/.var/app/org.prospektdj.ProspektDJ/data/prospekt-dj")
    );
    assert!(linux_root(Path::new("/usr/bin/prospekt-dj"), None, false, None).is_err());
}
