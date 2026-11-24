use prospekt_dj::{
    midi::{mapping_bank, Encoding, Mapping},
    settings::Settings,
};
fn map(action: &str) -> Mapping {
    Mapping {
        status: 144,
        number: 36,
        action: action.into(),
        encoding: Encoding::TwosComplement,
        inverted: false,
        relative: false,
    }
}
#[test]
fn identical_messages_keep_device_specific_actions_and_persist_separately() {
    let mut settings = Settings::default();
    settings.midi.push(map("play0"));
    settings.midi_second.push(map("play1"));
    assert_eq!(
        mapping_bank(&mut settings.midi, &mut settings.midi_second, false)[0].action,
        "play0"
    );
    assert_eq!(
        mapping_bank(&mut settings.midi, &mut settings.midi_second, true)[0].action,
        "play1"
    );
    mapping_bank(&mut settings.midi, &mut settings.midi_second, true).push(map("cross"));
    assert_eq!(settings.midi.len(), 1);
    let saved = serde_json::to_string(&settings).unwrap();
    let restored: Settings = serde_json::from_str(&saved).unwrap();
    assert_eq!(restored.midi, settings.midi);
    assert_eq!(restored.midi_second, settings.midi_second);
    let mut old = serde_json::to_value(settings).unwrap();
    old.as_object_mut().unwrap().remove("midi_second");
    assert!(serde_json::from_value::<Settings>(old)
        .unwrap()
        .midi_second
        .is_empty());
}
