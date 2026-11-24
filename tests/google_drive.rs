use prospekt_dj::google_drive::{local_path, DriveFile, OAuthClient};
#[test]
fn desktop_configuration_required_and_tokens_not_accepted_as_configuration() {
    assert!(
        OAuthClient::from_json(br#"{"web":{"client_id":"x.apps.googleusercontent.com"}}"#).is_err()
    );
    assert!(OAuthClient::from_json(br#"{"installed":{"client_id":"invalid"}}"#).is_err());
    assert!(OAuthClient::from_json(br#"{"installed":{"client_id":"test.apps.googleusercontent.com","client_secret":"desktop-client"}}"#).is_ok());
}
#[test]
fn remote_names_stay_inside_cache_and_duplicate_names_have_distinct_paths() {
    let root = std::path::Path::new("cache/CLOUD/GoogleDrive");
    let mut file = DriveFile {
        id: "abc-123_456".into(),
        name: "../../CON.mp3".into(),
        mime: "audio/mpeg".into(),
        size: Some("100".into()),
        modified: "2026-11-23".into(),
    };
    let first = local_path(root, &file).unwrap();
    assert!(first.starts_with(root.join(&file.id)));
    assert_eq!(first.components().count(), root.components().count() + 2);
    assert!(file.audio());
    assert!(!file.folder());
    file.id = "different-file".into();
    assert_ne!(first, local_path(root, &file).unwrap());
    file.id = "../../escape".into();
    assert!(local_path(root, &file).is_err());
    file.id = "valid".into();
    file.name = "CON.mp3".into();
    assert_eq!(
        local_path(root, &file).unwrap().file_name().unwrap(),
        "gdrive-CON.mp3"
    );
}
