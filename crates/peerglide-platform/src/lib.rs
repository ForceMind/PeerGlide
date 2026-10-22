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
    control: AtomicU64,
}
#[derive(Clone, Default)]
pub struct CaptureGate(Arc<GateState>);
const ACTIVE: u64 = 1 << 63;
const IN_CALLBACK: u64 = 1 << 62;
const COUNT: u64 = !(ACTIVE | IN_CALLBACK);
impl CaptureGate {
    /// Never enable suppression while a callback or physically held input straddles the boundary.
    pub fn try_activate(&self) -> bool {
        self.0.deadline.store(now_ms() + 1000, Ordering::Release);
        self.0
            .control
            .compare_exchange(0, ACTIVE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }
    pub fn refresh(&self) {
        if self.active() {
            self.0.deadline.store(now_ms() + 1000, Ordering::Release);
        }
    }
    pub fn disable(&self) {
        self.0.control.fetch_and(!ACTIVE, Ordering::AcqRel);
        self.0.deadline.store(0, Ordering::Release);
    }
    pub fn active(&self) -> bool {
        if self.0.deadline.load(Ordering::Acquire) <= now_ms() {
            self.0.control.fetch_and(!ACTIVE, Ordering::AcqRel);
            return false;
        }
        self.0.control.load(Ordering::Acquire) & ACTIVE != 0
    }
    pub fn idle(&self) -> bool {
        self.0.control.load(Ordering::Acquire) & COUNT == 0
    }
    #[cfg(any(windows, target_os = "macos", test))]
    fn begin_event(&self) -> CallbackGuard {
        self.0.control.fetch_or(IN_CALLBACK, Ordering::AcqRel);
        CallbackGuard(self.clone())
    }
    #[cfg(any(windows, target_os = "macos", test))]
    fn set_held(&self, count: u64) {
        let mut old = self.0.control.load(Ordering::Acquire);
        loop {
            let new = (old & !COUNT) | (count & COUNT);
            match self.0.control.compare_exchange_weak(
                old,
                new,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => break,
                Err(current) => old = current,
            }
        }
    }
}
#[cfg(any(windows, target_os = "macos", test))]
struct CallbackGuard(CaptureGate);
#[cfg(any(windows, target_os = "macos", test))]
impl Drop for CallbackGuard {
    fn drop(&mut self) {
        self.0 .0.control.fetch_and(!IN_CALLBACK, Ordering::AcqRel);
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
        self.gate
            .set_held((self.keys.len() + self.buttons.len()) as u64);
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
            self.toggle_armed = false;
            self.emit(Captured::Emergency);
            return false;
        }
        if ctrl && alt && down && usage == 0x4f {
            self.toggle_armed = true;
        }
        let swallow_toggle = self.toggle_armed && usage == 0x4f;
        if !swallow_toggle {
            self.emit(Captured::Input(Input::Key { usage, down }));
        }
        if self.toggle_armed && self.keys.is_empty() {
            self.toggle_armed = false;
            self.emit(Captured::Toggle);
        }
        swallow_toggle || self.gate.active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn watchdog_is_fail_open_and_explicitly_releasable() {
        let gate = CaptureGate::default();
        assert!(!gate.active());
        assert!(gate.try_activate());
        assert!(gate.active());
        gate.disable();
        assert!(!gate.active());
        gate.0.deadline.store(now_ms(), Ordering::Release);
        assert!(!gate.active());
        gate.set_held(1);
        assert!(!gate.idle());
    }
}

#[cfg(all(test, any(windows, target_os = "macos")))]
mod hook_tests {
    use super::*;
    fn state(capacity: usize) -> (HookState, mpsc::Receiver<Captured>) {
        let (tx, rx) = mpsc::channel(capacity);
        (
            HookState {
                tx,
                gate: CaptureGate::default(),
                keys: Default::default(),
                buttons: Default::default(),
                toggle_armed: false,
            },
            rx,
        )
    }
    #[test]
    fn hotkey_waits_for_release_and_never_forwards_arrow() {
        let (mut s, mut rx) = state(20);
        s.key(0xe0, true);
        s.key(0xe2, true);
        assert!(s.key(0x4f, true));
        assert!(!s.gate.idle());
        assert!(s.key(0x4f, false));
        s.key(0xe0, false);
        s.key(0xe2, false);
        assert!(s.gate.idle());
        let mut toggles = 0;
        while let Ok(e) = rx.try_recv() {
            match e {
                Captured::Toggle => toggles += 1,
                Captured::Input(Input::Key { usage: 0x4f, .. }) => panic!("switch arrow leaked"),
                _ => (),
            }
        }
        assert_eq!(toggles, 1);
    }
    #[test]
    fn queue_overflow_disables_capture() {
        let (s, _rx) = state(1);
        assert!(s.gate.try_activate());
        s.emit(Captured::Emergency);
        s.emit(Captured::Emergency);
        assert!(!s.gate.active());
    }
}

#[cfg(test)]
mod handoff_tests {
    use super::*;
    #[test]
    fn activation_cannot_split_a_local_callback_or_held_key() {
        let gate = CaptureGate::default();
        {
            let _callback = gate.begin_event();
            assert!(!gate.try_activate());
            gate.set_held(1);
        }
        assert!(!gate.try_activate());
        {
            let _callback = gate.begin_event();
            gate.set_held(0);
            assert!(!gate.try_activate());
        }
        assert!(gate.try_activate());
        gate.0.deadline.store(now_ms(), Ordering::Release);
        assert!(!gate.active());
        gate.refresh();
        assert!(!gate.active());
    }
}
