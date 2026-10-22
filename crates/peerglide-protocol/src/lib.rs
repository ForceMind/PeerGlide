//! PeerGlide's independent, versioned wire protocol. Input payloads must never be logged.
use serde::{Deserialize, Serialize};

pub const VERSION: u16 = 1;
pub const MAX_FRAME: usize = 4096;
pub const LEASE_MS: u64 = 1500;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}
impl Edge {
    pub fn opposite(self) -> Self {
        match self {
            Self::Left => Self::Right,
            Self::Right => Self::Left,
            Self::Top => Self::Bottom,
            Self::Bottom => Self::Top,
        }
    }
}

/// USB HID keyboard page 0x07; buttons 1=left, 2=right, 3=middle.
#[derive(Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Input {
    Key { usage: u16, down: bool },
    Button { button: u8, down: bool },
    Scroll { x: i16, y: i16 },
}
impl Input {
    pub fn valid(self) -> bool {
        match self {
            Self::Key { usage, .. } => {
                (4..=0x73).contains(&usage) || (0xe0..=0xe7).contains(&usage)
            }
            Self::Button { button, .. } => (1..=3).contains(&button),
            Self::Scroll { x, y } => x.abs_diff(0) <= 1200 && y.abs_diff(0) <= 1200,
        }
    }
}
#[derive(Clone, Serialize, Deserialize)]
pub enum Message {
    Hello { version: u16 },
    Acquire { lease: u64, edge: Edge, along: f64 },
    Granted { lease: u64 },
    Denied { lease: u64 },
    Input { lease: u64, event: Input },
    Release { lease: u64 },
    Ping { lease: u64 },
    Pong { lease: u64 },
}
/// Absolute normalized motion tolerates packet loss without accumulating displacement error.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Motion {
    pub lease: u64,
    pub sequence: u64,
    pub x: f64,
    pub y: f64,
}
impl Motion {
    pub fn valid(self) -> bool {
        [self.x, self.y]
            .iter()
            .all(|n| n.is_finite() && (0.0..=1.0).contains(n))
    }
}

pub fn encode<T: Serialize>(value: &T) -> Result<Vec<u8>, serde_json::Error> {
    serde_json::to_vec(value)
}
pub fn decode<T: for<'a> Deserialize<'a>>(bytes: &[u8]) -> Result<T, &'static str> {
    if bytes.len() > MAX_FRAME {
        return Err("frame too large");
    }
    serde_json::from_slice(bytes).map_err(|_| "invalid frame")
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn bounds_and_malformed() {
        assert!(decode::<Message>(&vec![0; MAX_FRAME + 1]).is_err());
        assert!(decode::<Message>(b"{}").is_err());
        assert!(!Motion {
            lease: 1,
            sequence: 1,
            x: f64::NAN,
            y: 0.0
        }
        .valid());
        assert!(!Input::Key {
            usage: 0,
            down: true
        }
        .valid());
        assert!(!Input::Scroll { x: i16::MIN, y: 0 }.valid());
    }
    #[test]
    fn roundtrip() {
        let msg = Message::Input {
            lease: 42,
            event: Input::Key {
                usage: 4,
                down: true,
            },
        };
        assert!(matches!(
            decode::<Message>(&encode(&msg).unwrap()).unwrap(),
            Message::Input { lease: 42, .. }
        ));
    }
}
