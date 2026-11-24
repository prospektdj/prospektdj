use prospekt_dj::audio::is_asio4all;

#[test]
fn recognizes_registered_asio4all_versions_without_matching_other_drivers() {
    for name in ["ASIO4ALL v2", "asio4all", "ASIO4ALL v2.16"] {
        assert!(is_asio4all(name));
    }
    for name in [
        "Realtek Audio",
        "ASIO Fireface USB",
        "Generic Low Latency ASIO Driver",
        "",
    ] {
        assert!(!is_asio4all(name));
    }
}
