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
        let started = std::time::Instant::now();
        if matches!(
            self.done.recv_timeout(timeout),
            Err(mpsc::RecvTimeoutError::Timeout)
        ) {
            return false;
        }
        // Task completion precedes thread-local destructors. Join only after
        // the OS thread is finished, keeping TLS cleanup inside the same budget.
        while self.thread.as_ref().is_some_and(|t| !t.is_finished()) {
            let remaining = timeout.saturating_sub(started.elapsed());
            if remaining.is_zero() {
                return false;
            }
            std::thread::sleep(remaining.min(Duration::from_millis(1)));
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
    fn completion_notice_does_not_make_blocked_thread_local_cleanup_joinable() {
        struct OnExit(mpsc::SyncSender<()>, mpsc::Receiver<()>);
        impl Drop for OnExit {
            fn drop(&mut self) {
                let _ = self.0.send(());
                let _ = self.1.recv();
            }
        }
        thread_local! { static EXIT: std::cell::RefCell<Option<OnExit>> = const { std::cell::RefCell::new(None) }; }
        let (started, waiting) = mpsc::sync_channel(1);
        let (release, blocked) = mpsc::sync_channel(1);
        let mut t = CaptureThread::spawn(move |_| EXIT.set(Some(OnExit(started, blocked))));
        waiting.recv_timeout(Duration::from_secs(1)).unwrap();
        // Rescue prevents a regression from hanging the test process forever.
        let rescue = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            let _ = release.send(());
        });
        let stopped = t.shutdown(Duration::ZERO);
        rescue.join().unwrap();
        assert!(!stopped, "a completed task is not a completed OS thread");
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
