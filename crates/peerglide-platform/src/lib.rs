//! Native adapters. Capture is disabled automatically when the network watchdog expires.
use anyhow::Result;
use peerglide_protocol::Input;
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc, OnceLock,
};
use std::time::Instant;
use tokio::sync::mpsc;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(windows)]
mod windows;
#[cfg(target_os = "macos")]
use macos as native;
#[cfg(windows)]
use windows as native;

pub fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}
#[derive(Clone, Copy)]
pub enum Captured {
    Position {
        x: f64,
        y: f64,
        width: f64,
        height: f64,
    },
    Motion {
        dx: f64,
        dy: f64,
    },
    Input(Input),
    Emergency,
    Toggle,
}
#[derive(Default)]
struct GateState {
    deadline: AtomicU64,
    held: AtomicU64,
}
#[derive(Clone, Default)]
pub struct CaptureGate(Arc<GateState>);
impl CaptureGate {
    pub fn refresh(&self) {
        self.0.deadline.store(now_ms() + 1000, Ordering::Release);
    }
    pub fn disable(&self) {
        self.0.deadline.store(0, Ordering::Release);
    }
    pub fn active(&self) -> bool {
        self.0.deadline.load(Ordering::Acquire) > now_ms()
    }
    pub fn idle(&self) -> bool {
        self.0.held.load(Ordering::Acquire) == 0
    }
}
impl Drop for CaptureSession {
    fn drop(&mut self) {
        self.gate.disable();
    }
}
pub struct CaptureSession {
    pub gate: CaptureGate,
}
pub fn capture(tx: mpsc::Sender<Captured>) -> Result<CaptureSession> {
    let gate = CaptureGate::default();
    #[cfg(any(windows, target_os = "macos"))]
    {
        native::capture(tx, gate.clone())?;
        Ok(CaptureSession { gate })
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (tx, gate);
        anyhow::bail!("native input currently requires Windows or macOS; use probe on Linux")
    }
}
pub fn inject(event: Input) -> Result<()> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        native::inject(event)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = event;
        anyhow::bail!("native injection unavailable")
    }
}
pub fn pointer(x: f64, y: f64) -> Result<()> {
    #[cfg(any(windows, target_os = "macos"))]
    {
        native::pointer(x, y)
    }
    #[cfg(not(any(windows, target_os = "macos")))]
    {
        let _ = (x, y);
        anyhow::bail!("native injection unavailable")
    }
}
#[cfg(any(windows, target_os = "macos"))]
struct HookState {
    tx: mpsc::Sender<Captured>,
    gate: CaptureGate,
    keys: std::collections::BTreeSet<u16>,
    buttons: std::collections::BTreeSet<u8>,
    toggle_armed: bool,
}
#[cfg(any(windows, target_os = "macos"))]
impl HookState {
    fn button(&mut self, button: u8, down: bool) {
        if down {
            self.buttons.insert(button);
        } else {
            self.buttons.remove(&button);
        }
        self.update_held();
        self.emit(Captured::Input(Input::Button { button, down }));
    }
    fn update_held(&self) {
        self.gate.0.held.store(
            (self.keys.len() + self.buttons.len()) as u64,
            Ordering::Release,
        );
    }
    fn emit(&self, e: Captured) {
        if self.tx.try_send(e).is_err() {
            self.gate.disable();
        }
    }
    fn key(&mut self, usage: u16, down: bool) -> bool {
        if down {
            self.keys.insert(usage);
        } else {
            self.keys.remove(&usage);
        }
        self.update_held();
        let ctrl = self.keys.contains(&0xe0) || self.keys.contains(&0xe4);
        let alt = self.keys.contains(&0xe2) || self.keys.contains(&0xe6);
        if ctrl && alt && down && usage == 0x29 {
            self.gate.disable();
            self.emit(Captured::Emergency);
            return false;
        }
        if ctrl && alt && down && usage == 0x4f {
            self.toggle_armed = true;
        }
        self.emit(Captured::Input(Input::Key { usage, down }));
        if self.toggle_armed && self.keys.is_empty() {
            self.toggle_armed = false;
            self.emit(Captured::Toggle);
        }
        self.gate.active()
    }
}
