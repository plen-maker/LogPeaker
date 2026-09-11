//! Host filesystem browsing and non-deleting rsync transfers over key-based SSH.
use eframe::egui::{self, RichText};
use std::{
    io::Read,
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
    thread,
    time::{Duration, Instant},
};

const SSH: [&str; 8] = [
    "-o",
    "BatchMode=yes",
    "-o",
    "ConnectTimeout=5",
    "-o",
    "ServerAliveInterval=10",
    "-o",
    "ServerAliveCountMax=2",
];
const LIMIT: usize = 2 * 1024 * 1024;
fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}
fn target(host: &str, user: &str) -> Result<String, String> {
    if host.is_empty()
        || user.is_empty()
        || host.starts_with('-')
        || user.starts_with('-')
        || !host
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || ".-:[]".contains(c))
        || !user
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-".contains(c))
    {
        return Err("Enter a valid SSH host and user in Build Server.".into());
    }
    Ok(format!("{user}@{host}"))
}
fn absolute(path: &str) -> Result<(), String> {
    if !path.starts_with('/') || path.contains('\0') {
        Err("Use an absolute path starting with /.".into())
    } else {
        Ok(())
    }
}
#[derive(Debug)]
struct Entry {
    name: String,
    kind: String,
    size: String,
}
fn parse(bytes: &[u8]) -> Result<Vec<Entry>, String> {
    let text = std::str::from_utf8(bytes)
        .map_err(|_| "Directory contains non-UTF-8 names; use a terminal to manage it.")?;
    let fields: Vec<_> = text.split_terminator('\0').collect();
    if fields.len() % 3 != 0 {
        return Err("Incomplete directory listing.".into());
    }
    let mut rows: Vec<_> = fields
        .chunks(3)
        .map(|f| Entry {
            kind: f[0].into(),
            size: f[1].into(),
            name: f[2].into(),
        })
        .collect();
    rows.sort_by(|a, b| (a.kind != "d", &a.name).cmp(&(b.kind != "d", &b.name)));
    Ok(rows)
}
fn drain(mut reader: impl Read) -> Vec<u8> {
    let mut kept = Vec::new();
    let mut buf = [0; 8192];
    while let Ok(n) = reader.read(&mut buf) {
        if n == 0 {
            break;
        }
        let take = n.min((LIMIT + 1).saturating_sub(kept.len()));
        kept.extend_from_slice(&buf[..take]);
    }
    kept
}
struct Job {
    rx: Receiver<Result<Vec<u8>, String>>,
    cancel: Arc<AtomicBool>,
    kind: Kind,
    key: String,
}
#[derive(Clone, Copy)]
enum Kind {
    List,
    Preview,
    Transfer,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
    }
}
fn launch(mut cmd: Command, kind: Kind, key: String) -> Job {
    let (tx, rx) = mpsc::channel();
    let cancel = Arc::new(AtomicBool::new(false));
    let stop = cancel.clone();
    thread::spawn(move || {
        let result = (|| {
            let mut child = cmd
                .stdin(Stdio::null())
                .stdout(Stdio::piped())
                .stderr(Stdio::piped())
                .spawn()
                .map_err(|e| e.to_string())?;
            let stdout = child.stdout.take().unwrap();
            let stderr = child.stderr.take().unwrap();
            let out = thread::spawn(move || drain(stdout));
            let err = thread::spawn(move || drain(stderr));
            let start = Instant::now();
            let mut interrupted = false;
            let status = loop {
                if stop.load(Ordering::Relaxed) || start.elapsed() > Duration::from_secs(1800) {
                    interrupted = true;
                    let _ = child.kill();
                    break child.wait();
                }
                match child.try_wait() {
                    Ok(Some(s)) => break Ok(s),
                    Ok(None) => thread::sleep(Duration::from_millis(50)),
                    Err(e) => {
                        let _ = child.kill();
                        let _ = child.wait();
                        break Err(e);
                    }
                }
            }
            .map_err(|e| e.to_string());
            let stdout = out.join().unwrap_or_default();
            let stderr = err.join().unwrap_or_default();
            if interrupted {
                return Err(
                    "Cancelled or timed out. Files already copied remain at the destination."
                        .into(),
                );
            }
            if !status?.success() {
                return Err(String::from_utf8_lossy(&stderr).trim().to_string());
            }
            if stdout.len() > LIMIT {
                return Err("Output exceeds 2 MiB; choose a smaller directory.".into());
            }
            Ok(stdout)
        })();
        let _ = tx.send(result);
    });
    Job {
        rx,
        cancel,
        kind,
        key,
    }
}

pub struct FilesPanel {
    remote: String,
    local: String,
    upload: bool,
    overwrite: bool,
    entries: Vec<Entry>,
    listing_key: String,
    preview_key: Option<String>,
    job: Option<Job>,
    message: String,
}
impl Default for FilesPanel {
    fn default() -> Self {
        Self {
            remote: "/home/yocto".into(),
            local: String::new(),
            upload: true,
            overwrite: false,
            entries: vec![],
            listing_key: String::new(),
            preview_key: None,
            job: None,
            message:
                "Browse files on the SSH host. For container files, use a host bind-mount path."
                    .into(),
        }
    }
}
impl FilesPanel {
    fn key(&self, host: &str, user: &str) -> String {
        format!(
            "{host:?}|{user:?}|{:?}|{:?}|{}|{}",
            self.remote, self.local, self.upload, self.overwrite
        )
    }
    fn list_key(&self, host: &str, user: &str) -> String {
        format!("{host:?}|{user:?}|{:?}", self.remote)
    }
    fn browse(&mut self, host: &str, user: &str) -> Result<(), String> {
        let dest = target(host, user)?;
        absolute(&self.remote)?;
        let mut cmd = Command::new("ssh");
        cmd.args(SSH).arg(dest).arg(format!(
            "LC_ALL=C find {} -mindepth 1 -maxdepth 1 -printf '%y\\0%s\\0%f\\0'",
            quote(&self.remote)
        ));
        self.entries.clear();
        self.message = "Reading directory…".into();
        self.job = Some(launch(cmd, Kind::List, self.list_key(host, user)));
        Ok(())
    }
    fn transfer(&mut self, host: &str, user: &str, preview: bool) -> Result<(), String> {
        let dest = target(host, user)?;
        absolute(&self.remote)?;
        absolute(&self.local)?;
        if !std::path::Path::new(&self.local).is_dir() {
            return Err("Local folder must already exist.".into());
        }
        if Command::new("rsync")
            .arg("--version")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .is_err()
        {
            return Err("rsync is not installed locally. Install rsync on this computer and the SSH server to enable transfers.".into());
        }
        let mut cmd = Command::new("rsync");
        cmd.args(["-rt", "--protect-args", "--itemize-changes", "--stats", "--timeout=30", "-e", "ssh -o BatchMode=yes -o ConnectTimeout=5 -o ServerAliveInterval=10 -o ServerAliveCountMax=2"]);
        if preview {
            cmd.arg("--dry-run");
        }
        if !self.overwrite {
            cmd.arg("--ignore-existing");
        }
        let local = format!("{}/", self.local.trim_end_matches('/'));
        let remote = format!("{dest}:{}/", self.remote.trim_end_matches('/'));
        cmd.arg("--");
        if self.upload {
            cmd.arg(local).arg(remote);
        } else {
            cmd.arg(remote).arg(local);
        }
        self.preview_key = None;
        self.message = if preview {
            "Previewing changes…"
        } else {
            "Copying files…"
        }
        .into();
        self.job = Some(launch(
            cmd,
            if preview {
                Kind::Preview
            } else {
                Kind::Transfer
            },
            self.key(host, user),
        ));
        Ok(())
    }
    pub fn ui(&mut self, ui: &mut egui::Ui, host: &str, user: &str) {
        if let Some(result) = self.job.as_ref().and_then(|j| j.rx.try_recv().ok()) {
            let job = self.job.take().unwrap();
            match result {
                Err(e) => {
                    self.message = format!("Failed: {e}");
                    self.preview_key = None;
                }
                Ok(bytes) => match job.kind {
                    Kind::List => {
                        if job.key == self.list_key(host, user) {
                            match parse(&bytes) {
                                Ok(rows) => {
                                    self.entries = rows;
                                    self.listing_key = job.key.clone();
                                    self.message = format!("{} entries", self.entries.len());
                                }
                                Err(e) => self.message = e,
                            }
                        }
                    }
                    Kind::Preview => {
                        self.preview_key = Some(job.key.clone());
                        self.message = format!(
                            "Preview — no files copied\n{}",
                            String::from_utf8_lossy(&bytes)
                        );
                    }
                    Kind::Transfer => {
                        self.message =
                            format!("Transfer complete\n{}", String::from_utf8_lossy(&bytes));
                        self.preview_key = None;
                        self.listing_key.clear();
                    }
                },
            }
        }
        ui.heading("Files & Transfer");
        ui.label("SSH host filesystem · directory contents · requires rsync on both machines");
        let busy = self.job.is_some();
        ui.add_enabled_ui(!busy, |ui| {
            ui.horizontal(|ui| { ui.label("Remote folder"); ui.text_edit_singleline(&mut self.remote);
                if ui.button("Up").clicked() { self.remote = std::path::Path::new(&self.remote).parent().unwrap_or(std::path::Path::new("/")).display().to_string(); }
                if ui.button("Browse").clicked() { if let Err(e) = self.browse(host,user) { self.message = e; } }
            });
            let mut enter = None;
            egui::ScrollArea::vertical().id_salt("remote_files").max_height(190.0).show(ui, |ui| {
                if self.listing_key == self.list_key(host,user) {
                    for row in &self.entries {
                        ui.horizontal(|ui| {
                            if row.kind == "d" { if ui.button(format!("▸ {}", row.name.escape_debug())).clicked() { enter = Some(row.name.clone()); } }
                            else { ui.label(format!("{}  {} bytes{}", row.name.escape_debug(), row.size, if row.kind == "l" { " (symlink, not copied)" } else { "" })); }
                        });
                    }
                } else { ui.weak("Browse to load this folder."); }
            });
            if let Some(name) = enter { self.remote = format!("{}/{}", self.remote.trim_end_matches('/'), name); if let Err(e) = self.browse(host,user) { self.message = e; } }
            ui.separator();
            ui.horizontal(|ui| { ui.label("Local folder"); ui.text_edit_singleline(&mut self.local); });
            ui.horizontal(|ui| { ui.selectable_value(&mut self.upload, true, "Upload → server"); ui.selectable_value(&mut self.upload, false, "Download → local"); });
            ui.checkbox(&mut self.overwrite, "Update existing files");
            ui.weak("Copies folder contents recursively. Destination-only files are kept. Symbolic links are skipped.");
            ui.horizontal(|ui| {
                if ui.button("Preview sync").clicked() { if let Err(e) = self.transfer(host,user,true) { self.message = e; } }
                let ready = self.preview_key.as_ref() == Some(&self.key(host,user));
                if ui.add_enabled(ready, egui::Button::new("Apply sync")).clicked() { if let Err(e) = self.transfer(host,user,false) { self.message = e; } }
            });
        });
        if let Some(job) = &self.job {
            ui.horizontal(|ui| {
                ui.spinner();
                if ui.button("Cancel").clicked() {
                    job.cancel.store(true, Ordering::Relaxed);
                }
            });
            ui.ctx().request_repaint_after(Duration::from_millis(100));
        }
        egui::ScrollArea::vertical()
            .id_salt("file_output")
            .max_height(240.0)
            .show(ui, |ui| {
                ui.label(RichText::new(&self.message).monospace());
            });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn filenames_with_newlines_survive() {
        let e = parse(b"f\012\0a\nb\0d\00\0folder\0").unwrap();
        assert_eq!(e[0].name, "folder");
        assert_eq!(e[1].name, "a\nb");
    }
    #[test]
    fn rejects_incomplete_listing() {
        assert!(parse(b"f\012\0").is_err());
    }
    #[test]
    fn destination_cannot_inject_options() {
        assert!(target("-oProxyCommand=x", "yocto").is_err());
        assert!(target("host", "x@other").is_err());
        assert!(target("192.168.7.1", "root").is_ok());
    }
    #[test]
    fn shell_quote_roundtrip() {
        let name = "/tmp/a'b\n$(echo nope)";
        let out = Command::new("sh")
            .arg("-c")
            .arg(format!("printf %s {}", quote(name)))
            .output()
            .unwrap();
        assert_eq!(out.stdout, name.as_bytes());
    }
}
#[cfg(test)]
mod worker_tests {
    use super::*;
    #[test]
    fn failed_process_is_reported() {
        let mut c = Command::new("sh");
        c.args(["-c", "echo failure >&2; exit 7"]);
        let j = launch(c, Kind::Preview, String::new());
        assert!(j
            .rx
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap_err()
            .contains("failure"));
    }
    #[test]
    fn cancellation_finishes_promptly() {
        let mut c = Command::new("sleep");
        c.arg("30");
        let j = launch(c, Kind::Transfer, String::new());
        j.cancel.store(true, Ordering::Relaxed);
        assert!(j
            .rx
            .recv_timeout(Duration::from_secs(3))
            .unwrap()
            .unwrap_err()
            .contains("Cancelled"));
    }
    #[test]
    fn preview_is_bound_to_connection_paths_and_direction() {
        let mut f = FilesPanel::default();
        let key = f.key("host", "user");
        assert_ne!(key, f.key("other", "user"));
        f.upload = false;
        assert_ne!(key, f.key("host", "user"));
        f.upload = true;
        f.local = "/tmp/new".into();
        assert_ne!(key, f.key("host", "user"));
    }
}
