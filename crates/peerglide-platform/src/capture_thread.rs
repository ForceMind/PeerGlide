//! Cooperative native-loop shutdown with a bounded join; never waits in a hook.
use std::sync::{
    atomic::{AtomicBool, Ordering},
    mpsc, Arc,
};
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
    done: mpsc::Receiver<bool>,
    completed: Option<bool>,
}
impl CaptureThread {
    #[cfg(test)]
    pub(super) fn spawn(task: impl FnOnce(Arc<AtomicBool>) + Send + 'static) -> Self {
        Self::spawn_owned(task, ())
    }
    pub(super) fn spawn_owned(
        task: impl FnOnce(Arc<AtomicBool>) + Send + 'static,
        owner: impl Send + 'static,
    ) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let flag = stop.clone();
        let (tx, done) = mpsc::sync_channel(1);
        let thread = std::thread::spawn(move || task(flag));
        // Only the reaper can block on join. Its ownership guard survives all
        // native thread-local destructors, even if the caller times out/drops.
        std::thread::spawn(move || {
            let clean = thread.join().is_ok();
            drop(owner);
            let _ = tx.send(clean);
        });
        Self {
            stop,
            done,
            completed: None,
        }
    }
    pub(super) fn shutdown(&mut self, timeout: Duration) -> bool {
        self.stop.store(true, Ordering::Release);
        if let Some(clean) = self.completed {
            return clean;
        }
        match self.done.recv_timeout(timeout) {
            Ok(clean) => {
                self.completed = Some(clean);
                clean
            }
            Err(mpsc::RecvTimeoutError::Timeout) => false,
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.completed = Some(false);
                false
            }
        }
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
            assert_eq!(t.completed, Some(true));
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
        struct Owned(Arc<AtomicBool>);
        impl Drop for Owned {
            fn drop(&mut self) {
                self.0.store(false, Ordering::Release);
            }
        }
        let owned = Arc::new(AtomicBool::new(true));
        let mut t = CaptureThread::spawn_owned(
            move |_| EXIT.set(Some(OnExit(started, blocked))),
            Owned(owned.clone()),
        );
        waiting.recv_timeout(Duration::from_secs(1)).unwrap();
        // Rescue prevents a regression from hanging the test process forever.
        let (kick, rescue_wait) = mpsc::sync_channel(1);
        let rescue = std::thread::spawn(move || {
            let _ = rescue_wait.recv_timeout(Duration::from_secs(1));
            let _ = release.send(());
        });
        let stopped = t.shutdown(Duration::ZERO);
        let retained = owned.load(Ordering::Acquire);
        let _ = kick.send(());
        rescue.join().unwrap();
        assert!(!stopped, "a completed task is not a completed OS thread");
        assert!(
            retained,
            "capture ownership must survive thread-local cleanup"
        );
        assert!(t.shutdown(Duration::from_secs(1)));
        assert!(!owned.load(Ordering::Acquire));
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
