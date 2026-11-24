//! File operations invoked explicitly from the library context menu.
use anyhow::{ensure, Context, Result};
use std::path::{Path, PathBuf};

pub fn rename(path: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.trim().is_empty() && name == name.trim(),
        "Podaj nazwę bez spacji na początku i końcu"
    );
    ensure!(
        !name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
            && !name.ends_with('.'),
        "Niedozwolone znaki nazwy pliku"
    );
    let base = name.split('.').next().unwrap_or("").to_ascii_uppercase();
    ensure!(
        ![
            "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7",
            "COM8", "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9"
        ]
        .contains(&base.as_str()),
        "Nazwa zarezerwowana przez Windows"
    );
    ensure!(path.is_file(), "Nie znaleziono pliku");
    ensure!(
        Path::new(name).extension() == path.extension(),
        "Zachowaj rozszerzenie pliku"
    );
    let new = path.with_file_name(name);
    ensure!(
        new != path && !new.try_exists()?,
        "Plik o tej nazwie już istnieje lub nazwa się nie zmieniła"
    );
    // Creating a new hard link is atomic and never replaces an existing destination.
    if let Err(link_error) = std::fs::hard_link(path, &new) {
        // FAT/exFAT do not support hard links. Reserve the target without overwrite.
        let mut target = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&new)
            .with_context(|| format!("Nie można utworzyć pliku: {link_error}"))?;
        let copied = (|| -> std::io::Result<()> {
            let mut source = std::fs::File::open(path)?;
            std::io::copy(&mut source, &mut target)?;
            target.sync_all()?;
            std::fs::set_permissions(&new, source.metadata()?.permissions())?;
            Ok(())
        })();
        drop(target);
        if let Err(error) = copied {
            let _ = std::fs::remove_file(&new);
            return Err(error.into());
        }
    }
    if let Err(error) = std::fs::remove_file(path) {
        let _ = std::fs::remove_file(&new);
        return Err(error.into());
    }
    Ok(new)
}

pub fn properties(path: &Path) -> Result<()> {
    ensure!(path.exists(), "Nie znaleziono pliku");
    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("powershell.exe")
            .args(["-NoProfile", "-NonInteractive", "-Command", "$p=$env:PROSPEKT_PROPERTIES_PATH; $s=New-Object -ComObject Shell.Application; $f=$s.Namespace((Split-Path -LiteralPath $p)); $f.ParseName([System.IO.Path]::GetFileName($p)).InvokeVerb('properties')"])
            .env("PROSPEKT_PROPERTIES_PATH", path).spawn()?;
        Ok(())
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("osascript").args(["-e", "on run argv", "-e", "tell application \"Finder\" to open information window of (POSIX file (item 1 of argv) as alias)", "-e", "end run", "--"]).arg(path).spawn()?;
        return Ok(());
    }
    #[cfg(not(any(target_os = "windows", target_os = "macos")))]
    {
        for manager in ["nautilus", "nemo", "caja", "thunar"] {
            if std::process::Command::new(manager)
                .arg("--properties")
                .arg(path)
                .spawn()
                .is_ok()
            {
                return Ok(());
            }
        }
        anyhow::bail!(
            "Brak menadżera plików obsługującego właściwości (Nautilus, Nemo, Caja lub Thunar)"
        )
    }
}

pub fn create_folder(parent: &Path, name: &str) -> Result<PathBuf> {
    ensure!(
        !name.trim().is_empty() && name == name.trim() && name != "." && name != "..",
        "Podaj nazwę podfolderu"
    );
    ensure!(
        !name
            .chars()
            .any(|c| c.is_control() || "\\/:*?\"<>|".contains(c))
            && !name.ends_with('.'),
        "Niedozwolona nazwa folderu"
    );
    let path = parent.join(name);
    std::fs::create_dir(&path).context("Nie można utworzyć podfolderu (może już istnieje)")?;
    Ok(path)
}

/// Never merges or overwrites destinations; rejects symlinks and recursive copies.
pub fn transfer(source: &Path, target: &Path, moving: bool) -> Result<()> {
    ensure!(
        !std::fs::symlink_metadata(source)?.file_type().is_symlink(),
        "Dowiązania symboliczne nie są kopiowane"
    );
    let source = source.canonicalize()?;
    let parent = target
        .parent()
        .context("Brak folderu docelowego")?
        .canonicalize()?;
    let target = parent.join(target.file_name().context("Brak nazwy docelowej")?);
    ensure!(
        !target.starts_with(&source),
        "Nie można kopiować folderu do niego samego"
    );
    ensure!(
        !target.try_exists()?,
        "Plik lub folder docelowy już istnieje"
    );
    fn copy(source: &Path, target: &Path) -> Result<()> {
        let meta = std::fs::symlink_metadata(source)?;
        ensure!(
            !meta.file_type().is_symlink(),
            "Dowiązania symboliczne nie są kopiowane"
        );
        if meta.is_dir() {
            std::fs::create_dir(target)?;
            for item in std::fs::read_dir(source)? {
                let item = item?;
                copy(&item.path(), &target.join(item.file_name()))?;
            }
        } else {
            ensure!(meta.is_file(), "Nieobsługiwany typ pliku");
            let mut output = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(target)?;
            std::io::copy(&mut std::fs::File::open(source)?, &mut output)?;
            output.sync_all()?;
            std::fs::set_permissions(target, meta.permissions())?;
        }
        Ok(())
    }
    // Reserve destination before copying so failed operations never remove someone else's file.
    if source.is_dir() {
        std::fs::create_dir(&target)?;
        let result = (|| -> Result<()> {
            for item in std::fs::read_dir(&source)? {
                let item = item?;
                copy(&item.path(), &target.join(item.file_name()))?;
            }
            Ok(())
        })();
        if let Err(error) = result {
            let _ = std::fs::remove_dir_all(&target);
            return Err(error);
        }
    } else {
        copy(&source, &target)?;
    }
    if moving {
        if source.is_dir() {
            std::fs::remove_dir_all(&source)?;
        } else {
            std::fs::remove_file(&source)?;
        }
    }
    Ok(())
}
