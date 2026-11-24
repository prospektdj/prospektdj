#[test]
fn reads_mp3_text_and_custom_tags_from_file() {
    if std::env::var_os("PROSPEKT_TEST_FFMPEG").is_none() {
        return;
    }
    let path = std::env::temp_dir().join(format!("prospekt-metadata-{}.mp3", std::process::id()));
    let result = prospekt_dj::media::ffmpeg_command()
        .args([
            "-v",
            "error",
            "-f",
            "lavfi",
            "-i",
            "sine=frequency=440:duration=0.2",
            "-metadata",
            "title=Preview title",
            "-metadata",
            "artist=Preview artist",
            "-metadata",
            "comment=Test comment",
            "-metadata",
            "DJ_CUSTOM=Custom value",
            "-y",
        ])
        .arg(&path)
        .output()
        .unwrap();
    assert!(result.status.success());
    let rows = prospekt_dj::metadata_preview::inspect(&path)
        .unwrap()
        .join("\n");
    for expected in [
        "Preview title",
        "Preview artist",
        "Test comment",
        "Custom value",
        "Próbkowanie:",
    ] {
        assert!(rows.contains(expected), "{expected} missing: {rows}");
    }
    std::fs::remove_file(path).unwrap();
}
#[test]
fn missing_file_is_reported() {
    assert!(prospekt_dj::metadata_preview::inspect(std::path::Path::new(
        "/nonexistent/prospekt-preview.mp3"
    ))
    .is_err());
}
