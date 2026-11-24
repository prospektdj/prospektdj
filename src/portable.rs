//! Unified application data with portable and read-only Linux package support.
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
pub fn root() -> Result<PathBuf> {
    if let Some(path) = std::env::var_os("PROSPEKT_CONFIG_DIR") {
        return Ok(path.into());
    }
    let executable = std::env::current_exe()?;
    #[cfg(target_os = "linux")]
    {
        let user_data =
            directories::BaseDirs::new().map(|dirs| dirs.data_dir().join("prospekt-dj"));
        linux_root(
            &executable,
            std::env::var_os("APPIMAGE").as_deref().map(Path::new),
            std::env::var_os("FLATPAK_ID").is_some() || Path::new("/.flatpak-info").exists(),
            user_data.as_deref(),
        )
    }
    #[cfg(not(target_os = "linux"))]
    executable
        .parent()
        .map(Path::to_path_buf)
        .context("Nie można ustalić folderu programu")
}
/// Resolve read-only Linux package formats without writing into mounted/system payloads.
pub fn linux_root(
    executable: &Path,
    appimage: Option<&Path>,
    flatpak: bool,
    user_data: Option<&Path>,
) -> Result<PathBuf> {
    if flatpak {
        return user_data
            .map(Path::to_path_buf)
            .context("Nie można ustalić folderu danych Flatpak");
    }
    if let Some(image) = appimage.filter(|path| path.is_absolute()) {
        return image
            .parent()
            .map(|parent| parent.join("prospekt-dj-data"))
            .context("Nie można ustalić folderu AppImage");
    }
    if ["/usr", "/opt", "/app", "/bin", "/sbin"]
        .iter()
        .any(|prefix| executable.starts_with(prefix))
    {
        return user_data
            .map(Path::to_path_buf)
            .context("Nie można ustalić folderu danych użytkownika");
    }
    executable
        .parent()
        .map(Path::to_path_buf)
        .context("Nie można ustalić folderu programu")
}

/// Samples live beside the other portable application data.
/// Downloaded artwork shares the application's portable data root.
pub fn covers_dir() -> Result<PathBuf> {
    Ok(root()?.join("covers"))
}

pub fn sampler_dir() -> Result<PathBuf> {
    Ok(root()?.join("SAMPLER"))
}

pub fn prepare() -> Result<()> {
    let root = root()?;
    std::fs::create_dir_all(&root)?;
    std::fs::create_dir_all(root.join("CLOUD").join("GoogleDrive"))
        .context("Nie można utworzyć folderu CLOUD/GoogleDrive")?;
    std::fs::create_dir_all(root.join("covers"))
        .context("Nie można utworzyć folderu okładek covers")?;
    std::fs::create_dir_all(root.join("SAMPLER")).context("Nie można utworzyć folderu SAMPLER")?;
    let probe = root.join(format!(".prospekt-write-test-{}", std::process::id()));
    std::fs::OpenOptions::new().write(true).create_new(true).open(&probe)
        .context("Folder programu nie pozwala na zapis. Przenieś rozpakowany program do folderu z prawem zapisu.")?;
    std::fs::remove_file(probe)?;
    // Explicit test/deployment override is isolated from the user's old configuration.
    if std::env::var_os("PROSPEKT_CONFIG_DIR").is_some() {
        return Ok(());
    }
    let marker = root.join(".prospekt-portable-migrated");
    if marker.exists() {
        return Ok(());
    }
    if let Some(project) = directories::ProjectDirs::from("org", "Prospekt", "Prospekt DJ Rust") {
        let old = project.config_dir();
        if old != root {
            migrate(old, &root)?;
        }
    }
    std::fs::write(
        marker,
        b"Portable storage initialized. Legacy files were not deleted.\n",
    )?;
    Ok(())
}
pub fn migrate(old: &Path, target: &Path) -> Result<()> {
    std::fs::create_dir_all(target)?;
    for name in [
        "settings.json",
        "library.json",
        "tracks.sqlite",
        "covers",
        "history",
        "audio-diagnostics.txt",
        "last-import-report.json",
    ] {
        let source = old.join(name);
        let destination = target.join(name);
        if !source.exists() || (destination.exists() && !source.is_dir()) {
            continue;
        }
        if name == "tracks.sqlite" {
            // A consistent snapshot includes committed WAL data without copying live WAL/SHM files.
            let connection = rusqlite::Connection::open_with_flags(
                &source,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
            )?;
            let temp = target.join(".tracks-portable-migration.sqlite");
            if temp.exists() {
                std::fs::remove_file(&temp)?;
            }
            connection.execute(
                "VACUUM INTO ?1",
                [temp.to_str().context("Nieprawidłowa ścieżka bazy")?],
            )?;
            std::fs::rename(temp, destination)?;
        } else {
            copy_tree(&source, &destination)
                .with_context(|| format!("Nie można przenieść danych {}", source.display()))?;
        }
    }
    Ok(())
}
fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    let metadata = std::fs::symlink_metadata(source)?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.is_dir() {
        std::fs::create_dir_all(target)?;
        for entry in std::fs::read_dir(source)? {
            let entry = entry?;
            copy_tree(&entry.path(), &target.join(entry.file_name()))?;
        }
    } else if metadata.is_file() && !target.exists() {
        let mut input = std::fs::File::open(source)?;
        let temp = target.with_extension("portable-copy-tmp");
        let mut output = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&temp)?;
        std::io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        drop(output);
        std::fs::rename(temp, target)?;
    }
    Ok(())
}
