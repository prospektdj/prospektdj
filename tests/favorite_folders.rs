use prospekt_dj::settings::Settings;
#[test]
fn folder_bookmarks_and_cue_palette_colors_survive_settings_without_changing_track_favorites() {
    let mut settings: Settings = serde_json::from_str("{}").unwrap();
    assert!(settings.favorite_folders.is_empty());
    assert!(settings.folder_colors.is_empty());
    let favorite = std::path::PathBuf::from("Muzyka/Zażółć 🎵");
    settings.favorite_folders.insert(favorite.clone());
    for slot in 0..8 {
        settings.folder_colors.insert(
            std::path::PathBuf::from(format!("Muzyka/Folder-{slot}")),
            slot,
        );
    }
    settings.favorites.insert("utwór.flac".into());
    settings.tree_open.insert("favorite-folders-section".into());
    let restored: Settings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert!(restored.favorite_folders.contains(&favorite));
    assert_eq!(restored.folder_colors, settings.folder_colors);
    assert_eq!(restored.favorites, settings.favorites);
    assert!(restored.tree_open.contains("favorite-folders-section"));
}
