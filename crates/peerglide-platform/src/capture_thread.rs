//! Cooperative native-loop shutdown with a bounded join; never waits in a hook.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;

#[cfg(any(windows, target_os = "macos"))]
static RESERVED: AtomicBool = AtomicBool::new(false);
#[cfg(any(windows, target_os = "macos"))]
pub(super) struct Reservation;
#[cfg(any(windows, target_os = "macos"))]
impl Reservation {
    pub(super) fn acquire() -> anyhow::Result<Self> {
        anyhow::ensure!(
            !RESERVED.swap(true, Ordering::AcqRel),
            "previous native capture is still running"
        );
        Ok(Self)
    }
}
#[cfg(any(windows, target_os = "macos"))]
impl Drop for Reservation {
    fn drop(&mut self) {
        RESERVED.store(false, Ordering::Release);
    }
}

pub(super) struct CaptureThread {
    stop: Arc<AtomicBool>,
    done: mpsc::Receiver<()>,
    thread: Option<JoinHandle<()>>,
}
impl CaptureThread {
    pub(super) fn spawn(task: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (tx, done) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || {
            task(flag);
            let _ = tx.send(());
        });
        Self {
            stop,
            done,
            thread: Some(thread),
        }
    }
    pub(super) fn shutdown(&mut self, timeout: Duration) -> bool {
        self.stop.store(true, Ordering::Release);
        if self.thread.is_none() {
            return true;
        }
        if matches!(
            self.done.recv_timeout(timeout),
            Err(mpsc::RecvTimeoutError::Timeout)
        ) {
            return false;
        }
        self.thread.take().is_none_or(|t| t.join().is_ok())
    }
}
impl Drop for CaptureThread {
    fn drop(&mut self) {
        if !self.shutdown(Duration::from_millis(500)) {
            eprintln!("Native capture thread did not stop cleanly within its shutdown budget.");
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(windows, target_os = "macos"))]
    #[test]
    fn reservation_refuses_overlap_and_releases_after_owner_exit() {
        let owner = Reservation::acquire().unwrap();
        assert!(Reservation::acquire().is_err());
        drop(owner);
        assert!(Reservation::acquire().is_ok());
    }
    #[test]
    fn repeated_start_stop_joins_and_is_idempotent() {
        for _ in 0..20 {
            let mut t = CaptureThread::spawn(|flag| {
                while !flag.load(Ordering::Acquire) {
                    std::thread::yield_now();
                }
            });
            assert!(t.shutdown(Duration::from_secs(1)));
            assert!(t.thread.is_none());
            assert!(t.shutdown(Duration::ZERO));
        }
    }
    #[test]
    fn partial_initialization_exit_is_joined() {
        let mut t = CaptureThread::spawn(|_| ());
        assert!(t.shutdown(Duration::from_secs(1)));
    }
    #[test]
    fn blocked_backend_reports_timeout_without_waiting_forever() {
        let (tx, rx) = mpsc::sync_channel(1);
        let mut t = CaptureThread::spawn(move |_| {
            let _ = rx.recv();
        });
        assert!(!t.shutdown(Duration::ZERO));
        tx.send(()).unwrap();
        assert!(t.shutdown(Duration::from_secs(1)));
    }
}
