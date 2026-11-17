//! Lazy, bounded directory enumeration. Symlinks are deliberately not followed.
use crossbeam_channel::{bounded, Receiver, Sender};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
};
#[derive(Clone, Default)]
pub struct Listing {
    pub children: Vec<PathBuf>,
    pub error: Option<String>,
    pub truncated: bool,
}
pub struct Folders {
    tx: Sender<(PathBuf, usize)>,
    rx: Receiver<(PathBuf, usize, Listing)>,
    pub cache: BTreeMap<PathBuf, Listing>,
    pending: BTreeSet<PathBuf>,
    pub roots: Vec<PathBuf>,
}
impl Default for Folders {
    fn default() -> Self {
        let (tx, rx) = bounded::<(PathBuf, usize)>(32);
        let (out, events) = bounded(32);
        std::thread::spawn(move || {
            let mut roots = Vec::new();
            if let Some(user) = directories::UserDirs::new() {
                roots.push(user.home_dir().to_owned());
                if let Some(music) = user.audio_dir() {
                    roots.push(music.to_owned());
                }
            }
            for disk in sysinfo::Disks::new_with_refreshed_list().list() {
                roots.push(disk.mount_point().to_owned());
            }
            roots.sort();
            roots.dedup();
            let _ = out.send((
                PathBuf::new(),
                0,
                Listing {
                    children: roots,
                    ..Default::default()
                },
            ));
            while let Ok((path, offset)) = rx.recv() {
                if out
                    .send((path.clone(), offset, list_page(&path, offset)))
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            tx,
            rx: events,
            cache: BTreeMap::new(),
            pending: BTreeSet::new(),
            roots: Vec::new(),
        }
    }
}
impl Folders {
    pub fn request(&mut self, path: &PathBuf) {
        if self.cache.contains_key(path) || self.pending.contains(path) {
            return;
        }
        if self.tx.try_send((path.clone(), 0)).is_ok() {
            self.pending.insert(path.clone());
        }
    }
    pub fn more(&mut self, path: &PathBuf) {
        if self.pending.contains(path) {
            return;
        }
        let offset = self.cache.get(path).map(|v| v.children.len()).unwrap_or(0);
        if self.tx.try_send((path.clone(), offset)).is_ok() {
            self.pending.insert(path.clone());
        }
    }
    pub fn retry(&mut self, path: &PathBuf) {
        self.cache.remove(path);
        self.request(path);
    }
    pub fn poll(&mut self) {
        while let Ok((path, offset, mut result)) = self.rx.try_recv() {
            if path.as_os_str().is_empty() {
                self.roots = result.children;
                continue;
            }
            self.pending.remove(&path);
            if self.cache.len() > 1024 {
                self.cache.clear();
            }
            if offset > 0 {
                if let Some(old) = self.cache.remove(&path) {
                    result.children.extend(old.children);
                    result.children.sort();
                    result.children.dedup();
                }
            }
            self.cache.insert(path, result);
        }
    }
}
pub fn list(path: &std::path::Path) -> Listing {
    list_page(path, 0)
}
fn list_page(path: &std::path::Path, offset: usize) -> Listing {
    let mut result = Listing::default();
    let mut skipped = 0;
    match std::fs::read_dir(path) {
        Ok(entries) => {
            for entry in entries {
                match entry {
                    Ok(e) => match e.file_type() {
                        Ok(t) if t.is_dir() && !t.is_symlink() => {
                            if skipped < offset {
                                skipped += 1;
                                continue;
                            }
                            result.children.push(e.path());
                            if result.children.len() >= 512 {
                                result.truncated = true;
                                break;
                            }
                        }
                        Err(e) => result.error = Some(e.to_string()),
                        _ => {}
                    },
                    Err(e) => result.error = Some(e.to_string()),
                }
            }
            result
                .children
                .sort_by_key(|p| p.file_name().map(|s| s.to_string_lossy().to_lowercase()));
        }
        Err(e) => result.error = Some(e.to_string()),
    }
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn lazy_pages_do_not_drop_folders_beyond_first_batch() {
        let root = std::env::temp_dir().join(format!("prospekt-pages-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        for n in 0..513 {
            std::fs::create_dir_all(root.join(n.to_string())).unwrap();
        }
        let first = super::list_page(&root, 0);
        assert_eq!(first.children.len(), 512);
        assert!(first.truncated);
        let second = super::list_page(&root, 512);
        assert_eq!(second.children.len(), 1);
        assert!(!second.truncated);
        assert!(!first.children.contains(&second.children[0]));
        std::fs::remove_dir_all(root).unwrap();
    }
}
