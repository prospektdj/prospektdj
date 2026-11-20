use prospekt_dj::midi::{matching_profiles, ControllerProfile, Mapping};
fn profile(model: &str, usable: bool) -> ControllerProfile {
    ControllerProfile {
        source: model.into(),
        schema: String::new(),
        authors: String::new(),
        port: String::new(),
        skipped: 0,
        unsupported: vec![],
        manufacturer: "Pioneer".into(),
        model: model.into(),
        name: model.into(),
        mappings: if usable {
            vec![Mapping {
                status: 144,
                number: 1,
                action: "0.play".into(),
                encoding: Default::default(),
                inverted: false,
                relative: false,
            }]
        } else {
            vec![]
        },
    }
}
#[test]
fn matches_model_across_spacing_case_and_midi_port_suffix() {
    assert_eq!(
        matching_profiles("PIONEER DDJ 400 MIDI 1", &[profile("DDJ-400", true)]),
        vec![0]
    );
}
#[test]
fn never_matches_vendor_alone_wrong_model_or_unsupported_profile() {
    let profiles = vec![profile("DDJ-400", true), profile("DDJ-800", false)];
    for port in ["Pioneer MIDI", "DDJ-4000", "DDJ-400SRT", "DDJ-800"] {
        assert!(matching_profiles(port, &profiles).is_empty(), "{port}");
    }
}
#[test]
fn keeps_variant_ambiguity_and_prefers_specific_model() {
    let variants = vec![profile("DDJ-400", true), profile("DDJ-400", true)];
    assert_eq!(matching_profiles("DDJ-400", &variants), vec![0, 1]);
    let models = vec![profile("DDJ-1000", true), profile("DDJ-1000SRT", true)];
    assert_eq!(matching_profiles("DDJ-1000SRT MIDI", &models), vec![1]);
}
