use prospekt_dj::{
    midi::{self, ControllerProfile, Encoding, Mapping},
    settings,
};
#[test]
fn bundled_catalog_preserves_models_and_validates_every_usable_binding() {
    let profiles = midi::bundled_profiles();
    assert_eq!(profiles.len(), 334);
    assert!(profiles.iter().filter(|p| !p.mappings.is_empty()).count() >= 289);
    for profile in profiles {
        assert!(!profile.name.is_empty());
        assert!(profile.source.starts_with("builtin:"));
        midi::validate(&profile.mappings, &midi::actions()).unwrap();
        let mut keys = std::collections::BTreeSet::new();
        for m in &profile.mappings {
            assert!(
                keys.insert((m.status, m.number)),
                "duplicate input in {}",
                profile.source
            );
        }
    }
    let numark = profiles
        .iter()
        .find(|p| p.source == "builtin:Numark_DJ2GO2_Touch.xml")
        .unwrap();
    assert_eq!(numark.manufacturer, "Numark");
    assert!(numark
        .mappings
        .iter()
        .any(|m| m.status == 0xb0 && m.number == 6 && m.action == "0.jog"));
    assert!(numark
        .mappings
        .iter()
        .any(|m| m.status == 0x95 && m.number == 0x23 && m.action == "1.pad6"));
    assert!(profiles
        .iter()
        .filter(|p| p.source.ends_with(".hid.xml"))
        .all(|p| p.mappings.is_empty() && !p.unsupported.is_empty()));
}
#[test]
fn mixxx_internal_constants_and_inverted_pitch_are_imported_without_scripts() {
    let xml=br#"<!DOCTYPE test [<!ENTITY cc "0xB2">]><MixxxControllerPreset><info><name>Test Deck</name><author>Mapping author</author></info><controller><controls>
 <control><group>[Channel3]</group><key>rate</key><status>&cc;</status><midino>0x04</midino></control>
 <control><group>[Channel3]</group><key>Vendor.script()</key><status>0x92</status><midino>0x05</midino><options><script-binding/></options></control>
 <control><group>[Channel3]</group><key>play</key><status>0x82</status><midino>0x06</midino></control>
 </controls></controller></MixxxControllerPreset>"#;
    let p = ControllerProfile::import(xml, "vendor.midi.xml", &midi::actions()).unwrap();
    assert_eq!(p.authors, "Mapping author");
    assert_eq!(p.mappings.len(), 1);
    assert!(p.mappings[0].inverted);
    assert_eq!(p.mappings[0].action, "2.pitch");
    assert_eq!(p.skipped, 1);
}
#[test]
fn conflicting_layer_assignments_are_reported_instead_of_picking_a_function() {
    let xml=br#"<OpenFadeRMapping><info><name>Conflict</name></info><controls><control status="0x90" midino="1" action="play" deck="0"/><control status="0x90" midino="1" action="cue" deck="0"/></controls></OpenFadeRMapping>"#;
    let p = ControllerProfile::import(xml, "test.xml", &midi::actions()).unwrap();
    assert!(p.mappings.is_empty());
    assert!(p.unsupported.iter().any(|s| s.contains("Conflicting")));
}
#[test]
fn invalid_or_external_xml_entities_do_not_become_midi_actions() {
    let xml=br#"<!DOCTYPE x [<!ENTITY x SYSTEM "file:///etc/passwd">]><OpenFadeRMapping><info><name>&x;</name></info></OpenFadeRMapping>"#;
    assert!(ControllerProfile::import(xml, "test.xml", &midi::actions()).is_err());
    assert!(ControllerProfile::import(b"<invalid/>", "test.xml", &midi::actions()).is_err());
}
#[test]
fn relative_and_inverted_controls_keep_their_declared_semantics() {
    let mut map = Mapping {
        status: 0xb0,
        number: 1,
        action: "0.filter".into(),
        encoding: Encoding::TwosComplement,
        inverted: true,
        relative: false,
    };
    assert_eq!(map.normalized(0, 0.5), 1.);
    assert_eq!(map.normalized(127, 0.5), 0.);
    map.relative = true;
    assert!((map.normalized(1, 0.5) - 0.49).abs() < 1e-6);
    assert!((map.normalized(127, 0.5) - 0.51).abs() < 1e-6);
}
#[test]
fn built_in_catalog_is_not_duplicated_in_periodically_saved_settings() {
    let dir = std::env::temp_dir().join(format!("prospekt-profiles-{}", std::process::id()));
    std::env::set_var("PROSPEKT_CONFIG_DIR", &dir);
    let s = settings::Settings {
        controller_profiles: midi::bundled_profiles().to_vec(),
        controller_profile: Some(0),
        ..Default::default()
    };
    let expected = s.controller_profiles[0].source.clone();
    settings::save(&s).unwrap();
    let bytes = std::fs::read(settings::config_path()).unwrap();
    assert!(bytes.len() < 32000);
    let loaded: settings::Settings = serde_json::from_slice(&bytes).unwrap();
    assert!(loaded.controller_profiles.is_empty());
    assert_eq!(loaded.controller_profile_source, expected);
    std::env::remove_var("PROSPEKT_CONFIG_DIR");
    std::fs::remove_dir_all(dir).unwrap();
}
