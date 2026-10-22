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
