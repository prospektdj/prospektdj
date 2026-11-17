pub mod audio;
pub mod dsp;
pub mod effects;
pub mod engine;
pub mod library;
pub mod media;
pub mod midi;
pub mod recording;
pub mod settings;

pub mod i18n;
pub mod telemetry;

pub mod notices;

pub mod controller_import;

pub mod artwork;
pub mod folder_tree;
pub mod music_view;

pub mod emoji_assets {
    include!(concat!(env!("OUT_DIR"), "/emoji_assets.rs"));
}
