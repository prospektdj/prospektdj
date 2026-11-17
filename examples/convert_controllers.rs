//! Rebuild the declarative catalog from the user's extracted mapping directory.
use prospekt_dj::midi::{actions, ControllerProfile};
fn main() -> anyhow::Result<()> {
    let mut paths = std::fs::read_dir(std::env::args().nth(1).expect("input directory"))?
        .flatten()
        .map(|e| e.path())
        .collect::<Vec<_>>();
    paths.sort();
    let mut profiles = Vec::new();
    for path in paths {
        if !matches!(
            path.extension().and_then(|s| s.to_str()),
            Some("xml" | "opendj-mapping")
        ) {
            continue;
        }
        let file = path.file_name().unwrap().to_string_lossy();
        let data = std::fs::read(&path)?;
        let mut profile = ControllerProfile::import(&data, &file, &actions())
            .map_err(|e| anyhow::anyhow!("{file}: {e:#}"))?;
        profile.source = format!("builtin:{file}");
        profiles.push(profile);
    }
    profiles.sort_by_key(|p| p.label().to_lowercase());
    std::fs::write(
        "assets/controller-profiles.json",
        serde_json::to_vec(&profiles)?,
    )?;
    println!(
        "{} profiles, {} bindings; {} profiles with usable MIDI",
        profiles.len(),
        profiles.iter().map(|p| p.mappings.len()).sum::<usize>(),
        profiles.iter().filter(|p| !p.mappings.is_empty()).count()
    );
    Ok(())
}
