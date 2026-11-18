//! OS statistics are refreshed on a worker, never in the audio callback.
use std::{path::PathBuf, time::Duration};
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub cpu: f32,
    pub ram_used: u64,
    pub ram_total: u64,
    pub disk_used: f32,
    pub available: u64,
    pub disk: String,
}
pub struct Monitor {
    rx: crossbeam_channel::Receiver<Stats>,
    stop: crossbeam_channel::Sender<()>,
    thread: Option<std::thread::JoinHandle<()>>,
    pub stats: Stats,
}
impl Monitor {
    pub fn new(path: PathBuf) -> Self {
        let (tx, rx) = crossbeam_channel::bounded(1);
        let (stop, done) = crossbeam_channel::bounded(1);
        let thread = std::thread::spawn(move || {
            let mut system = sysinfo::System::new();
            let mut disks = sysinfo::Disks::new_with_refreshed_list();
            system.refresh_cpu_usage();
            while done.recv_timeout(Duration::from_secs(1)).is_err() {
                system.refresh_cpu_usage();
                system.refresh_memory();
                disks.refresh(true);
                let disk = disks
                    .list()
                    .iter()
                    .filter(|d| path.starts_with(d.mount_point()))
                    .max_by_key(|d| d.mount_point().as_os_str().len());
                let mut stats = Stats {
                    cpu: system.global_cpu_usage().clamp(0., 100.),
                    ram_used: system.used_memory(),
                    ram_total: system.total_memory(),
                    ..Default::default()
                };
                if let Some(d) = disk {
                    stats.available = d.available_space();
                    stats.disk = d.mount_point().display().to_string();
                    stats.disk_used = if d.total_space() > 0 {
                        100. * (1. - d.available_space() as f64 / d.total_space() as f64) as f32
                    } else {
                        0.
                    };
                }
                let _ = tx.try_send(stats);
            }
        });
        Self {
            rx,
            stop,
            thread: Some(thread),
            stats: Stats::default(),
        }
    }
    pub fn poll(&mut self) {
        while let Ok(stats) = self.rx.try_recv() {
            self.stats = stats;
        }
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        let _ = self.stop.send(());
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}
