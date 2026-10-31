//! Receiver-authoritative, bounded leases; no permanent input master.
use peerglide_protocol::{Input, Motion, LEASE_MS};
use std::collections::BTreeSet;

#[derive(Default)]
pub struct Receiver {
    lease: Option<u64>,
    deadline: u64,
    last_lease: u64,
    sequence: u64,
    keys: BTreeSet<u16>,
    buttons: BTreeSet<u8>,
}
impl Receiver {
    pub fn acquire(&mut self, lease: u64, now: u64, authorized: bool) -> bool {
        if !authorized || self.lease.is_some() || lease <= self.last_lease {
            return false;
        }
        self.last_lease = lease;
        self.lease = Some(lease);
        self.sequence = 0;
        self.deadline = now.saturating_add(LEASE_MS);
        true
    }
    pub fn accepts(&self, lease: u64, now: u64) -> bool {
        self.lease == Some(lease) && now < self.deadline
    }
    pub fn heartbeat(&mut self, lease: u64, now: u64) -> bool {
        if !self.accepts(lease, now) {
            return false;
        }
        self.deadline = now.saturating_add(LEASE_MS);
        true
    }
    pub fn input(&mut self, lease: u64, event: Input, now: u64) -> bool {
        if !self.accepts(lease, now) || !event.valid() {
            return false;
        }
        match event {
            Input::Key { usage, down } => {
                if down {
                    self.keys.insert(usage);
                } else {
                    self.keys.remove(&usage);
                }
            }
            Input::Button { button, down } => {
                if down {
                    self.buttons.insert(button);
                } else {
                    self.buttons.remove(&button);
                }
            }
            Input::Scroll { .. } => (),
        }
        true
    }
    pub fn motion(&mut self, m: Motion, now: u64) -> bool {
        if !self.accepts(m.lease, now) || !m.valid() || m.sequence <= self.sequence {
            return false;
        }
        self.sequence = m.sequence;
        true
    }
    /// Reliable discrete input carries its own cursor snapshot, repairing lost motion.
    /// Never lower the datagram replay watermark if a newer motion packet arrived first.
    pub fn anchor(&mut self, m: Motion, now: u64) -> bool {
        if !self.accepts(m.lease, now) || !m.valid() {
            return false;
        }
        self.sequence = self.sequence.max(m.sequence);
        true
    }
    pub fn expired(&self, now: u64) -> bool {
        self.lease.is_some() && now >= self.deadline
    }
    pub fn active(&self) -> Option<u64> {
        self.lease
    }
    pub fn release(&mut self) -> Vec<Input> {
        self.lease = None;
        let events = self
            .keys
            .iter()
            .map(|&usage| Input::Key { usage, down: false })
            .chain(self.buttons.iter().map(|&button| Input::Button {
                button,
                down: false,
            }))
            .collect::<Vec<_>>();
        self.keys.clear();
        self.buttons.clear();
        events
    }
}

/// Entry gate re-arms only after moving inside, avoiding repeated edge switches.
#[derive(Default)]
pub struct EdgeGate {
    latched: bool,
    last: u64,
}
impl EdgeGate {
    pub fn update(&mut self, at_edge: bool, now: u64) -> bool {
        if !at_edge {
            self.latched = false;
            return false;
        }
        if self.latched || now.saturating_sub(self.last) < 300 {
            return false;
        }
        self.latched = true;
        self.last = now;
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn authorization_conflict_expiry_replay() {
        let mut r = Receiver::default();
        assert!(!r.acquire(1, 0, false));
        assert!(r.acquire(1, 0, true));
        assert!(!r.acquire(2, 1, true));
        assert!(!r.heartbeat(2, 1));
        assert!(!r.input(
            1,
            Input::Key {
                usage: 4,
                down: true
            },
            1500
        ));
        assert!(r.expired(1500));
        r.release();
        assert!(!r.acquire(1, 1501, true));
        assert!(r.acquire(2, 1501, true));
    }
    #[test]
    fn releases_every_pressed_input() {
        let mut r = Receiver::default();
        r.acquire(1, 0, true);
        r.input(
            1,
            Input::Key {
                usage: 0xe0,
                down: true,
            },
            1,
        );
        r.input(
            1,
            Input::Button {
                button: 1,
                down: true,
            },
            1,
        );
        assert_eq!(r.release().len(), 2);
        assert!(r.release().is_empty());
    }
    #[test]
    fn motion_loss_reorder_and_old_lease() {
        let mut r = Receiver::default();
        r.acquire(1, 0, true);
        let m = Motion {
            lease: 1,
            sequence: 8,
            x: 0.5,
            y: 0.5,
        };
        assert!(r.motion(m, 1));
        assert!(!r.motion(m, 2));
        r.release();
        r.acquire(2, 3, true);
        assert!(!r.motion(m, 4));
    }
    #[test]
    fn edge_rearms_after_interior() {
        let mut g = EdgeGate::default();
        assert!(g.update(true, 500));
        assert!(!g.update(true, 900));
        g.update(false, 1000);
        assert!(g.update(true, 1100));
    }
}

#[cfg(test)]
mod causal_tests {
    use super::*;
    #[test]
    fn reliable_cursor_anchor_repairs_loss_without_reopening_replay() {
        let mut r = Receiver::default();
        assert!(r.acquire(1, 0, true));
        assert!(r.motion(
            Motion {
                lease: 1,
                sequence: 9,
                x: 0.9,
                y: 0.5
            },
            1
        ));
        assert!(r.anchor(
            Motion {
                lease: 1,
                sequence: 8,
                x: 0.8,
                y: 0.5
            },
            2
        ));
        assert!(!r.motion(
            Motion {
                lease: 1,
                sequence: 8,
                x: 0.8,
                y: 0.5
            },
            3
        ));
        assert!(r.anchor(
            Motion {
                lease: 1,
                sequence: 10,
                x: 0.8,
                y: 0.5
            },
            4
        ));
        assert!(!r.anchor(
            Motion {
                lease: 2,
                sequence: 11,
                x: 0.8,
                y: 0.5
            },
            4
        ));
    }
}

/// Retains failed key/button releases until acknowledged by the native backend.
/// This is bounded by Receiver's finite tracked input set, never by event volume.
#[derive(Default)]
pub struct PendingReleases(Vec<Input>);
impl PendingReleases {
    pub fn extend(&mut self, events: impl IntoIterator<Item = Input>) {
        self.0.extend(events);
    }
    /// Retry each failed release at most three times; successful releases are removed.
    /// The callback can be a native injector or a fault-injecting test backend.
    pub fn flush<E>(
        &mut self,
        mut inject: impl FnMut(Input) -> Result<(), E>,
    ) -> Result<(), usize> {
        for _ in 0..3 {
            self.0.retain(|event| inject(*event).is_err());
            if self.0.is_empty() {
                return Ok(());
            }
        }
        Err(self.0.len())
    }
}

#[cfg(test)]
mod recovery_tests {
    use super::*;
    fn key() -> Input {
        Input::Key {
            usage: 0xe0,
            down: false,
        }
    }
    #[test]
    fn transient_failure_is_retried_and_success_is_not_replayed() {
        let mut pending = PendingReleases::default();
        pending.extend([key()]);
        let mut calls = 0;
        assert_eq!(
            pending.flush(|_| {
                calls += 1;
                if calls < 3 {
                    Err(())
                } else {
                    Ok(())
                }
            }),
            Ok(())
        );
        assert_eq!(calls, 3);
        assert_eq!(
            pending.flush::<()>(|_| panic!("successful release replayed")),
            Ok(())
        );
    }
    #[test]
    fn permanent_failure_is_bounded_and_retained_for_cleanup() {
        let mut pending = PendingReleases::default();
        pending.extend([key()]);
        let mut calls = 0;
        assert_eq!(
            pending.flush(|_| {
                calls += 1;
                Err(())
            }),
            Err(1)
        );
        assert_eq!(calls, 3);
        assert_eq!(
            pending.flush(|event| {
                assert!(matches!(event, Input::Key { down: false, .. }));
                Ok::<_, ()>(())
            }),
            Ok(())
        );
    }
    #[test]
    fn one_failed_release_does_not_block_other_releases() {
        let mut pending = PendingReleases::default();
        pending.extend([
            key(),
            Input::Button {
                button: 1,
                down: false,
            },
        ]);
        let mut buttons = 0;
        assert_eq!(
            pending.flush(|event| match event {
                Input::Key { .. } => Err(()),
                _ => {
                    buttons += 1;
                    Ok(())
                }
            }),
            Err(1)
        );
        assert_eq!(buttons, 1);
    }
}
