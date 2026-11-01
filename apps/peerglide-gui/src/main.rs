#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]
use anyhow::{Context, Result};
use eframe::egui::{self, Color32, RichText, Vec2};
use peerglide_transport::{self as transport, Identity};
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    io::{BufRead, BufReader, Write},
    path::PathBuf,
    process::{Child, Command, Stdio},
    sync::mpsc,
    time::{Duration, Instant},
};

#[derive(Serialize, Deserialize)]
#[serde(default)]
struct Config {
    peer_name: String,
    address: String,
    listen: bool,
    allow: bool,
    native: bool,
    edge: String,
    dark: bool,
    fingerprint: Option<String>,
}
impl Default for Config {
    fn default() -> Self {
        Self {
            peer_name: "My other computer".into(),
            address: "192.168.1.10:24870".into(),
            listen: true,
            allow: false,
            native: false,
            edge: "right".into(),
            dark: true,
            fingerprint: None,
        }
    }
}
#[derive(Clone, Copy, PartialEq)]
enum Page {
    Home,
    Devices,
    Displays,
    Transfers,
    Settings,
}
struct App {
    dir: PathBuf,
    config: Config,
    page: Page,
    import_path: String,
    compared: bool,
    notice: String,
    child: Option<Child>,
    events: Option<mpsc::Receiver<String>>,
    status: String,
    connected: bool,
    logs: VecDeque<String>,
    stopping: Option<Instant>,
}
impl App {
    fn new(cc: &eframe::CreationContext<'_>, dir: PathBuf) -> Self {
        let app = Self::load(dir);
        app.theme(&cc.egui_ctx);
        app
    }
    fn load(dir: PathBuf) -> Self {
        let config = std::fs::read(dir.join("settings.json"))
            .ok()
            .and_then(|v| serde_json::from_slice(&v).ok())
            .unwrap_or_default();
        let mut app = Self {
            dir,
            config,
            page: Page::Home,
            import_path: String::new(),
            compared: false,
            notice: String::new(),
            child: None,
            events: None,
            status: "Offline".into(),
            connected: false,
            logs: VecDeque::new(),
            stopping: None,
        };
        if let Err(e) = std::fs::create_dir_all(&app.dir) {
            app.notice = e.to_string();
        }
        app
    }
    fn theme(&self, ctx: &egui::Context) {
        ctx.set_visuals(if self.config.dark {
            egui::Visuals::dark()
        } else {
            egui::Visuals::light()
        });
    }
    fn save(&mut self) {
        if let Err(e) = self.save_config() {
            self.notice = e.to_string();
        }
    }
    fn save_config(&self) -> Result<()> {
        let data = serde_json::to_vec_pretty(&self.config)?;
        std::fs::write(self.dir.join("settings.tmp"), data)?;
        std::fs::rename(
            self.dir.join("settings.tmp"),
            self.dir.join("settings.json"),
        )?;
        Ok(())
    }
    fn identity_dir(&self) -> PathBuf {
        self.dir.join("identity")
    }
    fn local_fingerprint(&self) -> Option<String> {
        Identity::load(&self.identity_dir())
            .ok()
            .map(|i| i.fingerprint())
    }
    fn candidate(&self) -> Result<Vec<u8>> {
        let bytes = std::fs::read(self.import_path.trim())?;
        anyhow::ensure!(bytes.len() <= 16 * 1024, "certificate file too large");
        transport::validate_certificate(&bytes)?;
        Ok(bytes)
    }
    fn enroll(&mut self) -> Result<()> {
        anyhow::ensure!(
            self.compared,
            "compare the fingerprint on both computers first"
        );
        let bytes = self.candidate()?;
        if let Ok(local) = Identity::load(&self.identity_dir()) {
            anyhow::ensure!(
                bytes != local.certificate,
                "choose the other computer's certificate"
            );
        }
        std::fs::write(self.dir.join("trusted-peer.crt.der"), &bytes)?;
        self.config.fingerprint = Some(transport::fingerprint(&bytes));
        self.save_config()?;
        self.compared = false;
        Ok(())
    }
    fn start(&mut self, probe: bool) -> Result<()> {
        anyhow::ensure!(self.child.is_none(), "a session is already running");
        Identity::load(&self.identity_dir())
            .context("create this computer's identity in Devices first")?;
        let cert = self.dir.join("trusted-peer.crt.der");
        let bytes = std::fs::read(&cert).context("enroll a peer in Devices first")?;
        anyhow::ensure!(
            self.config.fingerprint.as_deref() == Some(transport::fingerprint(&bytes).as_str()),
            "enrolled certificate changed; explicitly re-enroll it"
        );
        if !self.config.listen || probe {
            self.config
                .address
                .parse::<std::net::SocketAddr>()
                .context("enter peer IP:port")?;
        }
        self.save_config()?;
        let executable = std::env::current_exe()?.with_file_name(if cfg!(windows) {
            "peerglide.exe"
        } else {
            "peerglide"
        });
        anyhow::ensure!(
            executable.is_file(),
            "peerglide CLI must be next to peerglide-gui; build the whole workspace"
        );
        let mut cmd = Command::new(executable);
        if self.config.listen && !probe {
            cmd.arg("listen");
        } else {
            cmd.arg("connect")
                .arg(&self.config.address)
                .args(["--bind", "0.0.0.0:0"]);
        }
        cmd.arg("--identity")
            .arg(self.identity_dir())
            .arg("--peer-cert")
            .arg(cert)
            .arg("--edge")
            .arg(&self.config.edge)
            .arg("--managed");
        if probe {
            cmd.arg("--probe");
        } else {
            if self.config.allow {
                cmd.arg("--allow-control");
            }
            if self.config.native {
                cmd.arg("--native");
            }
        }
        cmd.stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(0x08000000);
        }
        let mut child = cmd.spawn()?;
        let (tx, rx) = mpsc::channel();
        let stdout = child.stdout.take().unwrap();
        let stderr = child.stderr.take().unwrap();
        let output_tx = tx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                let _ = output_tx.send(line);
            }
        });
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        });
        self.child = Some(child);
        self.events = Some(rx);
        self.logs.clear();
        self.connected = false;
        self.stopping = None;
        self.status = "Starting".into();
        Ok(())
    }
    fn stop(&mut self) {
        if let Some(child) = &mut self.child {
            if let Some(stdin) = &mut child.stdin {
                let _ = stdin.write_all(b"stop\n");
                let _ = stdin.flush();
            }
            self.stopping = Some(Instant::now());
            self.status = "Restoring local control".into();
        }
    }
    fn poll(&mut self) {
        if let Some(rx) = &self.events {
            while let Ok(line) = rx.try_recv() {
                if self.child.is_some()
                    && self.stopping.is_none()
                    && line.starts_with("Listening on ")
                {
                    self.status = "Waiting for peer".into();
                }
                if self.child.is_some()
                    && self.stopping.is_none()
                    && line.starts_with("Authenticated peer connected")
                {
                    self.connected = true;
                    self.status = "Authenticated".into();
                }
                if line.starts_with("PASS:") {
                    self.notice = "Connection probe passed. No input was injected.".into();
                }
                self.logs.push_back(line.chars().take(600).collect());
                if self.logs.len() > 100 {
                    self.logs.pop_front();
                }
            }
        }
        if let Some(child) = &mut self.child {
            if self
                .stopping
                .is_some_and(|s| s.elapsed() > Duration::from_secs(3))
            {
                let _ = child.kill();
                self.notice = "Worker required forced stop; remote input lease will expire.".into();
            }
            match child.try_wait() {
                Ok(Some(exit)) => {
                    self.connected = false;
                    self.status = if exit.success() {
                        "Offline".into()
                    } else {
                        format!("Worker exited: {exit}")
                    };
                    self.child = None;
                    self.stopping = None;
                }
                Err(e) => {
                    self.notice = e.to_string();
                }
                _ => (),
            }
        }
    }
    fn feedback(&mut self, result: Result<()>, success: &str) {
        self.notice = match result {
            Ok(()) => success.into(),
            Err(e) => format!("{e:#}"),
        };
    }
    fn heading(ui: &mut egui::Ui, title: &str, subtitle: &str) {
        ui.heading(RichText::new(title).size(30.0));
        ui.add_space(8.0);
        ui.label(subtitle);
        ui.add_space(24.0);
    }
    fn home(&mut self, ui: &mut egui::Ui) {
        Self::heading(
            ui,
            "Your workspace, connected.",
            "Multiple Computers. One Seamless Experience.",
        );
        egui::Frame::group(ui.style())
            .inner_margin(20.0)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.vertical(|ui| {
                        ui.strong("THIS COMPUTER");
                        ui.add_space(8.0);
                        ui.label(std::env::consts::OS);
                        ui.label(if self.local_fingerprint().is_some() {
                            "Identity ready"
                        } else {
                            "Create an identity to get started"
                        });
                    });
                    ui.add_space(50.0);
                    ui.vertical(|ui| {
                        ui.strong(&self.config.peer_name);
                        ui.add_space(8.0);
                        ui.colored_label(
                            if self.connected {
                                Color32::from_rgb(94, 211, 161)
                            } else {
                                ui.visuals().text_color()
                            },
                            &self.status,
                        );
                        ui.label(if self.config.fingerprint.is_some() {
                            "Explicitly enrolled peer"
                        } else {
                            "No enrolled peer"
                        });
                    });
                });
            });
        ui.add_space(22.0);
        ui.add_enabled_ui(self.child.is_none(), |ui| {
            ui.horizontal(|ui| {
                ui.selectable_value(&mut self.config.listen, true, "Listen on this computer");
                ui.selectable_value(&mut self.config.listen, false, "Connect to peer");
            });
            if !self.config.listen {
                ui.horizontal(|ui| {
                    ui.label("Peer IP:port");
                    ui.text_edit_singleline(&mut self.config.address);
                });
            }
            ui.add_space(12.0);
            if ui
                .button(RichText::new("Start session").size(18.0))
                .clicked()
            {
                let r = self.start(false);
                self.feedback(r, "Session worker started.");
            }
            if ui.button("Run connection probe").clicked() {
                let r = self.start(true);
                self.feedback(
                    r,
                    "Probing the peer address. Its listener must already be running.",
                );
            }
        });
        if self.child.is_some() && ui.button("Disconnect & restore local control").clicked() {
            self.stop();
        }
        ui.add_space(18.0);
        ui.label(if self.config.native {
            "Native input enabled. Ctrl+Alt+Esc restores local control."
        } else {
            "Diagnostic mode: native capture/injection is off. Enable it explicitly in Settings."
        });
        egui::CollapsingHeader::new("Session diagnostics — no keystroke payloads").show(ui, |ui| {
            egui::ScrollArea::vertical()
                .max_height(180.0)
                .show(ui, |ui| {
                    for line in &self.logs {
                        ui.monospace(line);
                    }
                });
        });
    }
    fn devices(&mut self, ui: &mut egui::Ui) {
        Self::heading(
            ui,
            "Trusted devices",
            "Trust is explicit. Discovery and LAN access never authorize control.",
        );
        ui.strong("This computer");
        if let Some(fp) = self.local_fingerprint() {
            ui.label("Public certificate");
            ui.monospace(
                self.identity_dir()
                    .join("node.crt.der")
                    .display()
                    .to_string(),
            );
            ui.label("BLAKE3 fingerprint");
            ui.monospace(fp);
        } else if ui.button("Create local identity").clicked() {
            let r = Identity::create(&self.identity_dir()).map(|_| ());
            self.feedback(
                r,
                "Identity created. Exchange only node.crt.der; keep the private key here.",
            );
        }
        ui.add_space(18.0);
        ui.separator();
        ui.add_space(18.0);
        ui.add_enabled_ui(self.child.is_none(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Peer name");
                ui.text_edit_singleline(&mut self.config.peer_name);
            });
            ui.label("Other computer's public certificate file");
            if ui.text_edit_singleline(&mut self.import_path).changed() {
                self.compared = false;
            }
            if let Ok(bytes) = self.candidate() {
                ui.monospace(transport::fingerprint(&bytes));
            }
            ui.checkbox(
                &mut self.compared,
                "I independently compared this fingerprint on the other computer",
            );
            if ui
                .add_enabled(self.compared, egui::Button::new("Enroll trusted peer"))
                .clicked()
            {
                let r = self.enroll();
                self.feedback(
                    r,
                    "Peer enrolled. Input permission remains a separate setting.",
                );
            }
            if let Some(fp) = &self.config.fingerprint {
                ui.add_space(14.0);
                ui.label("Enrolled fingerprint");
                ui.monospace(fp);
            }
            if self.config.fingerprint.is_some() && ui.button("Revoke peer trust").clicked() {
                self.config.fingerprint = None;
                self.config.allow = false;
                let _ = std::fs::remove_file(self.dir.join("trusted-peer.crt.der"));
                self.save();
                self.notice = "Peer trust revoked.".into();
            }
        });
        ui.add_space(18.0);
        ui.weak("Automatic discovery and pairing codes are planned. Enrollment currently uses public certificate files.");
    }
    fn displays(&mut self, ui: &mut egui::Ui) {
        Self::heading(
            ui,
            "Arrange your displays",
            "Drag the peer around this computer to choose the switching edge.",
        );
        ui.add_enabled_ui(self.child.is_none(), |ui| {
            let (rect, _) = ui.allocate_exact_size(
                Vec2::new(ui.available_width().min(700.0), 340.0),
                egui::Sense::hover(),
            );
            let center = rect.center();
            let local = egui::Rect::from_center_size(center, Vec2::new(170.0, 100.0));
            let offset = match self.config.edge.as_str() {
                "left" => Vec2::new(-190.0, 0.0),
                "top" => Vec2::new(0.0, -110.0),
                "bottom" => Vec2::new(0.0, 110.0),
                _ => Vec2::new(190.0, 0.0),
            };
            let peer = egui::Rect::from_center_size(center + offset, Vec2::new(170.0, 100.0));
            let response = ui.interact(peer, ui.id().with("peer-display"), egui::Sense::drag());
            let displayed = if response.dragged() {
                peer.translate(response.drag_delta())
            } else {
                peer
            };
            let painter = ui.painter();
            painter.rect_filled(local, 10.0, Color32::from_rgb(49, 65, 91));
            painter.rect_filled(displayed, 10.0, Color32::from_rgb(41, 103, 130));
            painter.text(
                local.center(),
                egui::Align2::CENTER_CENTER,
                "This computer",
                egui::FontId::proportional(16.0),
                Color32::WHITE,
            );
            painter.text(
                displayed.center(),
                egui::Align2::CENTER_CENTER,
                &self.config.peer_name,
                egui::FontId::proportional(16.0),
                Color32::WHITE,
            );
            if response.drag_stopped() {
                if let Some(p) = response.interact_pointer_pos() {
                    let d = p - center;
                    self.config.edge = if d.x.abs() >= d.y.abs() {
                        if d.x < 0.0 {
                            "left"
                        } else {
                            "right"
                        }
                    } else if d.y < 0.0 {
                        "top"
                    } else {
                        "bottom"
                    }
                    .into();
                    self.save();
                }
            }
            ui.horizontal(|ui| {
                ui.label("Switching edge");
                for side in ["left", "right", "top", "bottom"] {
                    if ui
                        .selectable_value(&mut self.config.edge, side.into(), side)
                        .changed()
                    {
                        self.save();
                    }
                }
            });
        });
        ui.add_space(18.0);
        ui.label("Primary displays only in this prototype. The layout applies to the next session. Configure the opposite edge on your other computer.");
    }
    fn settings(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) {
        Self::heading(ui, "Make it yours", "Input permissions and appearance.");
        if ui
            .checkbox(&mut self.config.dark, "Dark appearance")
            .changed()
        {
            self.theme(ctx);
            self.save();
        }
        ui.add_space(18.0);
        ui.add_enabled_ui(self.child.is_none(), |ui| {
            if ui
                .checkbox(
                    &mut self.config.allow,
                    "Allow this enrolled peer to control this computer",
                )
                .changed()
            {
                self.save();
            }
            ui.add_enabled_ui(cfg!(any(windows, target_os = "macos")), |ui| {
                if ui
                    .checkbox(
                        &mut self.config.native,
                        "Enable native keyboard and mouse capture / injection",
                    )
                    .changed()
                {
                    self.save();
                }
            });
        });
        ui.add_space(18.0);
        ui.label("Ctrl+Alt+Right, then release: toggle target\nCtrl+Alt+Esc: emergency local recovery\nPhysical input at receiver: reclaim control");
        ui.add_space(14.0);
        ui.label("macOS: Accessibility and Input Monitoring permissions are required. Windows: elevated and secure desktops are outside this prototype's control.");
        ui.add_space(14.0);
        ui.label("Clipboard and file permissions are separate future features. No telemetry or cloud account.");
        ui.add_space(14.0);
        ui.weak(format!("Settings: {}", self.dir.display()));
    }
}
impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _: &mut eframe::Frame) {
        self.poll();
        ctx.request_repaint_after(Duration::from_millis(100));
        egui::SidePanel::left("navigation")
            .exact_width(180.0)
            .show(ctx, |ui| {
                ui.add_space(24.0);
                ui.heading(RichText::new("PeerGlide").size(25.0));
                ui.weak("SCREEN TO SCREEN");
                ui.add_space(34.0);
                for (page, label) in [
                    (Page::Home, "Home"),
                    (Page::Devices, "Devices"),
                    (Page::Displays, "Displays"),
                    (Page::Transfers, "Transfers"),
                    (Page::Settings, "Settings"),
                ] {
                    if ui
                        .selectable_label(self.page == page, RichText::new(label).size(18.0))
                        .clicked()
                    {
                        self.page = page;
                    }
                    ui.add_space(12.0);
                }
                ui.separator();
                ui.weak("0.1.0-dev\nExperimental prototype");
            });
        egui::TopBottomPanel::bottom("status").show(ctx, |ui| {
            ui.horizontal_wrapped(|ui| {
                ui.label(&self.status);
                if !self.notice.is_empty() {
                    ui.separator();
                    ui.label(&self.notice);
                }
            });
        });
        egui::CentralPanel::default().show(ctx,|ui|{ui.add_space(20.0);egui::ScrollArea::vertical().show(ui,|ui|match self.page {
            Page::Home=>self.home(ui),Page::Devices=>self.devices(ui),Page::Displays=>self.displays(ui),Page::Settings=>self.settings(ui,ctx),
            Page::Transfers=>{Self::heading(ui,"File transfers","Coming in M4");ui.label("File sending, resumable chunks, clipboard sharing and native cross-screen drag are not available in this prototype.");},
        });});
    }
}
impl Drop for App {
    fn drop(&mut self) {
        self.stop();
        if let Some(child) = &mut self.child {
            for _ in 0..40 {
                if child.try_wait().ok().flatten().is_some() {
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}
fn main() -> eframe::Result<()> {
    let dir = if cfg!(windows) {
        PathBuf::from(std::env::var_os("LOCALAPPDATA").expect("LOCALAPPDATA unavailable"))
            .join("PeerGlide")
    } else if cfg!(target_os = "macos") {
        PathBuf::from(std::env::var_os("HOME").expect("user directory unavailable"))
            .join("Library/Application Support/PeerGlide")
    } else {
        std::env::var_os("XDG_DATA_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                PathBuf::from(std::env::var_os("HOME").expect("user directory unavailable"))
                    .join(".local/share")
            })
            .join("peerglide")
    };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 720.0])
            .with_min_inner_size([860.0, 640.0]),
        ..Default::default()
    };
    eframe::run_native(
        "PeerGlide",
        options,
        Box::new(move |cc| Ok(Box::new(App::new(cc, dir)))),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) struct Fixture {
        pub(super) path: PathBuf,
        live: bool,
    }
    impl Fixture {
        pub(super) fn new() -> Result<Self> {
            static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            for _ in 0..100 {
                let path = std::env::temp_dir().join(format!(
                    "peerglide-ui-{}-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)?
                        .as_nanos(),
                    NEXT_ID.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
                match std::fs::create_dir(&path) {
                    Ok(()) => return Ok(Self { path, live: true }),
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    Err(e) => {
                        return Err(e)
                            .with_context(|| format!("create GUI fixture {}", path.display()))
                    }
                }
            }
            anyhow::bail!("unable to reserve a unique GUI test directory")
        }
        pub(super) fn close(mut self) -> Result<()> {
            std::fs::remove_dir_all(&self.path)
                .with_context(|| format!("cleanup GUI fixture {}", self.path.display()))?;
            self.live = false;
            Ok(())
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            if self.live {
                if let Err(e) = std::fs::remove_dir_all(&self.path) {
                    eprintln!("GUI fixture cleanup failed at {}: {e}", self.path.display());
                }
            }
        }
    }
    #[test]
    fn parallel_fixtures_have_exclusive_ownership_and_cleanup() -> Result<()> {
        let fixtures: Vec<_> = (0..32).map(|_| std::thread::spawn(Fixture::new)).collect();
        let fixtures: Vec<_> = fixtures
            .into_iter()
            .map(|t| t.join().expect("fixture thread panicked"))
            .collect::<Result<_>>()?;
        let paths: std::collections::BTreeSet<_> =
            fixtures.iter().map(|f| f.path.clone()).collect();
        assert_eq!(paths.len(), 32);
        for fixture in &fixtures {
            assert!(fixture.path.is_dir());
        }
        for fixture in fixtures {
            fixture.close()?;
        }
        for path in paths {
            assert!(!path.exists());
        }
        let path = {
            let fixture = Fixture::new()?;
            fixture.path.clone()
        };
        assert!(!path.exists(), "RAII cleanup on early return");
        Ok(())
    }
    #[test]
    fn enrollment_requires_confirmation_and_detects_changed_certificate() -> Result<()> {
        let fixture = Fixture::new()?;
        let dir = fixture.path.clone();
        {
            let mut app = App::load(dir.clone());
            let local = Identity::create(&app.identity_dir())?;
            let peer = Identity::generate()?;
            let candidate = dir.join("candidate.der");
            std::fs::write(&candidate, &peer.certificate)
                .context("write peer candidate fixture")?;
            app.import_path = candidate.display().to_string();
            assert!(app.enroll().is_err());
            app.compared = true;
            app.enroll()?;
            assert!(!app.config.allow);
            assert!(!app.config.native);
            let loaded = App::load(dir.clone());
            assert_eq!(loaded.config.fingerprint, Some(peer.fingerprint()));
            drop(loaded);
            std::fs::write(dir.join("trusted-peer.crt.der"), &local.certificate)
                .context("replace pinned certificate fixture")?;
            assert!(app
                .start(false)
                .unwrap_err()
                .to_string()
                .contains("changed"));
            app.import_path = app
                .identity_dir()
                .join("node.crt.der")
                .display()
                .to_string();
            app.compared = true;
            assert!(app.enroll().is_err());
        }
        fixture.close()?;
        Ok(())
    }
    #[test]
    fn all_configuration_pages_render_in_both_themes_without_a_display() -> Result<()> {
        let fixture = Fixture::new()?;
        let dir = fixture.path.clone();
        {
            let mut app = App::load(dir.clone());
            let ctx = egui::Context::default();
            for dark in [true, false] {
                app.config.dark = dark;
                app.theme(&ctx);
                for page in [Page::Home, Page::Devices, Page::Displays, Page::Settings] {
                    let output = ctx.run(
                        egui::RawInput {
                            screen_rect: Some(egui::Rect::from_min_size(
                                egui::Pos2::ZERO,
                                Vec2::new(1000.0, 720.0),
                            )),
                            ..Default::default()
                        },
                        |ctx| {
                            egui::CentralPanel::default().show(ctx, |ui| match page {
                                Page::Home => app.home(ui),
                                Page::Devices => app.devices(ui),
                                Page::Displays => app.displays(ui),
                                Page::Settings => app.settings(ui, ctx),
                                _ => (),
                            });
                        },
                    );
                    assert!(!output.shapes.is_empty());
                }
            }
        }
        fixture.close()?;
        Ok(())
    }
}

#[cfg(test)]
mod lifecycle_tests {
    use super::*;
    #[test]
    fn delayed_worker_output_cannot_report_a_dead_process_as_connected() -> Result<()> {
        let fixture = super::tests::Fixture::new()?;
        let dir = fixture.path.clone();
        {
            let mut app = App::load(dir.clone());
            let (tx, rx) = mpsc::channel();
            app.events = Some(rx);
            tx.send("Authenticated peer connected. Ctrl+C exits".into())?;
            app.poll();
            assert!(!app.connected);
            assert_eq!(app.status, "Offline");
        }
        fixture.close()?;
        Ok(())
    }
}
