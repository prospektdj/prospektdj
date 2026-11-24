#[test]
fn rename_preserves_bytes_and_refuses_overwrite_and_path_escape() {
    let root = std::env::temp_dir().join(format!("prospekt-rename-{}", std::process::id()));
    std::fs::create_dir_all(&root).unwrap();
    let old = root.join("sample.mp3");
    std::fs::write(&old, b"original audio and tags").unwrap();
    let occupied = root.join("existing.mp3");
    std::fs::write(&occupied, b"keep").unwrap();
    for name in ["existing.mp3", "../escape.mp3", "wrong.wav", "CON.mp3"] {
        assert!(prospekt_dj::file_actions::rename(&old, name).is_err());
        assert!(old.exists());
    }
    let new = prospekt_dj::file_actions::rename(&old, "Nowa nazwa 🎵.mp3").unwrap();
    assert!(!old.exists());
    assert_eq!(std::fs::read(new).unwrap(), b"original audio and tags");
    assert_eq!(std::fs::read(occupied).unwrap(), b"keep");
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn folders_copy_move_and_subfolder_never_overwrite_or_recurse() {
    let root = std::env::temp_dir().join(format!("prospekt-folder-ops-{}", std::process::id()));
    std::fs::create_dir_all(root.join("source/child")).unwrap();
    std::fs::write(root.join("source/child/sample.wav"), b"audio").unwrap();
    let copy = root.join("copy");
    prospekt_dj::file_actions::transfer(&root.join("source"), &copy, false).unwrap();
    assert_eq!(
        std::fs::read(copy.join("child/sample.wav")).unwrap(),
        b"audio"
    );
    assert!(prospekt_dj::file_actions::transfer(&root.join("source"), &copy, false).is_err());
    assert!(prospekt_dj::file_actions::transfer(
        &root.join("source"),
        &root.join("source/inside"),
        false
    )
    .is_err());
    prospekt_dj::file_actions::transfer(&copy, &root.join("moved"), true).unwrap();
    assert!(!copy.exists());
    assert!(root.join("moved/child/sample.wav").exists());
    assert!(prospekt_dj::file_actions::create_folder(&root, "../escape").is_err());
    prospekt_dj::file_actions::create_folder(&root, "Nowy folder").unwrap();
    assert!(prospekt_dj::file_actions::create_folder(&root, "Nowy folder").is_err());
    std::fs::remove_dir_all(root).unwrap();
}
