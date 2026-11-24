use prospekt_dj::storage::count_audio;
#[test]
fn inventory_counts_only_requested_formats_recursively_without_following_links() {
    let root = std::env::temp_dir().join(format!("prospekt-storage-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(root.join("nested")).unwrap();
    for name in [
        "one.MP3",
        "two.wav",
        "three.FLAC",
        "four.aac",
        "five.ogg",
        "ignore.mp4",
        "nested/six.mp3",
    ] {
        std::fs::write(root.join(name), b"fixture").unwrap();
    }
    std::fs::create_dir(root.join("not-a-file.mp3")).unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, root.join("nested/loop")).unwrap();
    assert_eq!(count_audio(&root), ([2, 1, 1, 1, 1], 0));
    std::fs::remove_dir_all(&root).unwrap();
}
#[test]
fn missing_media_reports_incomplete_scan() {
    let root =
        std::env::temp_dir().join(format!("prospekt-storage-missing-{}", std::process::id()));
    let (counts, errors) = count_audio(&root);
    assert_eq!(counts, [0; 5]);
    assert!(errors > 0);
}
