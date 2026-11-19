use prospekt_dj::dj_import::{self, Source};
fn root(name: &str) -> std::path::PathBuf {
    let path = std::env::temp_dir().join(format!("prospekt-import-{name}-{}", std::process::id()));
    std::fs::create_dir_all(&path).unwrap();
    path
}
#[test]
fn rekordbox_xml_handles_encoded_uri_ratings_and_memory_cues() {
    let dir = root("rekordbox");
    let file = dir.join("collection.xml");
    let expected = dir.join("Zażółć.mp3");
    let uri = url::Url::from_file_path(&expected)
        .unwrap()
        .to_string()
        .replacen("file:///", "file://localhost/", 1);
    std::fs::write(&file,r#"<DJ_PLAYLISTS Version="1.0.0"><COLLECTION><TRACK TrackID="1" Location="__URI__" Name="Track" Rating="204" AverageBpm="128" DateAdded="2024-01-02" PlayCount="7"><POSITION_MARK Num="-1" Type="0" Start="1"/><POSITION_MARK Num="0" Type="0" Start="0"/><POSITION_MARK Num="7" Type="0" Start="12.345"/></TRACK></COLLECTION><PLAYLISTS><NODE Type="1" Name="Set"><TRACK Key="1"/></NODE></PLAYLISTS></DJ_PLAYLISTS>"#.replace("__URI__", &uri)).unwrap();
    let r = dj_import::inspect(&file, Source::RekordboxXml).unwrap();
    assert_eq!(r.tracks[0].rating, Some(4));
    assert_eq!(r.tracks[0].hot[0], Some(0.));
    assert_eq!(r.tracks[0].hot[7], Some(12.345));
    assert_eq!(r.tracks[0].path, expected);
    assert_eq!(r.tracks[0].added.as_deref(), Some("2024-01-02"));
    assert_eq!(r.playlists[0].1, vec!["1"]);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn virtualdj_uses_seconds_per_beat_and_one_based_cue_numbers() {
    let dir = root("virtualdj");
    let file = dir.join("database.xml");
    std::fs::write(&file,r#"<VirtualDJ_Database Version="8.4"><Song FilePath="/tmp/song.mp3"><Tags Title="Song" Author="Artist" Bpm="0.5" Stars="3"/><Infos SongLength="120" FirstSeen="1700000000" PlayCount="4"/><Poi Type="cue" Num="1" Pos="0"/><Poi Type="beatgrid" Pos="1"/><Comment>Mix note</Comment></Song></VirtualDJ_Database>"#).unwrap();
    let r = dj_import::inspect(&file, Source::VirtualDjXml).unwrap();
    assert_eq!(r.tracks[0].bpm, Some(120.));
    assert_eq!(r.tracks[0].hot[0], Some(0.));
    assert_eq!(r.tracks[0].comment, "Mix note");
    assert_eq!(r.tracks[0].play_count, Some(4));
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn mixxx_reads_committed_wal_without_writing_the_source() {
    let dir = root("mixxx");
    let file = dir.join("mixxxdb.sqlite");
    let c = rusqlite::Connection::open(&file).unwrap();
    c.execute_batch("PRAGMA journal_mode=WAL; CREATE TABLE track_locations(id INTEGER,location TEXT); CREATE TABLE library(id INTEGER,location INTEGER,title TEXT,artist TEXT,rating INTEGER,timesplayed INTEGER,last_played_at TEXT); INSERT INTO track_locations VALUES(1,'/tmp/Zażółć.mp3'); INSERT INTO library VALUES(9,1,'Title','Artist',5,7,'2024-01-02 12:00:00'); CREATE TABLE crates(id INTEGER,name TEXT); CREATE TABLE crate_tracks(crate_id INTEGER,track_id INTEGER); INSERT INTO crates VALUES(1,'Warm up');INSERT INTO crate_tracks VALUES(1,9);").unwrap();
    let before = std::fs::read(&file).unwrap();
    let r = dj_import::inspect(&file, Source::MixxxSqlite).unwrap();
    assert_eq!(r.tracks[0].play_count, Some(7));
    assert_eq!(r.tracks[0].rating, Some(5));
    assert_eq!(r.playlists[0].1, vec!["9"]);
    assert_eq!(before, std::fs::read(&file).unwrap());
    drop(c);
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn serato_csv_preserves_quotes_unicode_and_rejects_unsafe_title_only_matching() {
    let dir = root("serato");
    let file = dir.join("history.csv");
    std::fs::write(&file,"Location,Title,Artist,Start Time\n/tmp/Zażółć.mp3,\"Title, Remix\",Artist,2024-01-02 12:34\n").unwrap();
    let r = dj_import::inspect(&file, Source::SeratoCsv).unwrap();
    assert_eq!(r.tracks[0].title, "Title, Remix");
    assert_eq!(r.tracks[0].play_count, None);
    std::fs::write(&file, "Title,Artist\nSong,Artist\n").unwrap();
    assert!(dj_import::inspect(&file, Source::SeratoCsv).is_err());
    std::fs::remove_dir_all(dir).unwrap();
}
#[test]
fn repeated_import_is_idempotent_and_keeps_user_edits() {
    let dir = root("repeat");
    let audio = dir.join("audio.wav");
    let mut w = hound::WavWriter::create(
        &audio,
        hound::WavSpec {
            channels: 2,
            sample_rate: 48000,
            bits_per_sample: 16,
            sample_format: hound::SampleFormat::Int,
        },
    )
    .unwrap();
    for _ in 0..4800 {
        w.write_sample(1000_i16).unwrap();
        w.write_sample(1000_i16).unwrap();
    }
    w.finalize().unwrap();
    let file = dir.join("rekordbox.xml");
    let uri = url::Url::from_file_path(&audio).unwrap();
    std::fs::write(&file,format!(r#"<DJ_PLAYLISTS><COLLECTION><TRACK TrackID="1" Location="{uri}" Name="Track" Rating="255" Comments="Source" PlayCount="8"><POSITION_MARK Type="0" Num="0" Start="0"/></TRACK></COLLECTION></DJ_PLAYLISTS>"#)).unwrap();
    let report = dj_import::inspect(&file, Source::RekordboxXml).unwrap();
    let db = dir.join("tracks.sqlite");
    let cancel = std::sync::atomic::AtomicBool::new(false);
    let a = dj_import::apply(&report, &db, &cancel).unwrap();
    assert_eq!(a.entries.len(), 1);
    let c = rusqlite::Connection::open(&db).unwrap();
    let id = &a.banks[0].0;
    c.execute(
        "UPDATE user_tracks SET data=?2 WHERE track_id=?1",
        rusqlite::params![
            id,
            r#"{"rating":2,"comment":"Local","first_seen":1,"created":null}"#
        ],
    )
    .unwrap();
    let a = dj_import::apply(&report, &db, &cancel).unwrap();
    assert_eq!(a.entries[0].rating, 2);
    assert_eq!(
        c.query_row("SELECT count(*) FROM imported_tracks", [], |r| r
            .get::<_, u64>(0))
            .unwrap(),
        1
    );
    let user: String = c
        .query_row("SELECT data FROM user_tracks", [], |r| r.get(0))
        .unwrap();
    assert!(user.contains("Local"));
    drop(c);
    std::fs::remove_dir_all(dir).unwrap();
}
