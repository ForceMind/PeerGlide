use super::*;
use anyhow::{bail, ensure};
use std::{cell::RefCell, mem::size_of, ptr::null_mut};
use windows_sys::Win32::{
    Foundation::*,
    UI::{HiDpi::*, Input::KeyboardAndMouse::*, WindowsAndMessaging::*},
};

const TAG: usize = 0x50474c44;
thread_local! { static WHEEL:RefCell<[i32; 2]>=const {RefCell::new([0; 2])}; }
thread_local! { static STATE:RefCell<Option<HookState>>=const {RefCell::new(None)}; }
pub fn capture(tx: mpsc::Sender<Captured>, gate: CaptureGate) -> Result<CaptureThread> {
    let reservation = capture_thread::Reservation::acquire()?;
    let (ready, rx) = std::sync::mpsc::sync_channel(1);
    let thread = CaptureThread::spawn(move |stop| unsafe {
        let _reservation = reservation;
        SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
        let state = HookState {
            tx,
            gate,
            keys: initial_keys(),
            buttons: initial_buttons(),
            toggle_armed: false,
        };
        state.update_held();
        STATE.set(Some(state));
        let mouse = SetWindowsHookExW(WH_MOUSE_LL, Some(mouse_hook), null_mut(), 0);
        let keyboard = SetWindowsHookExW(WH_KEYBOARD_LL, Some(key_hook), null_mut(), 0);
        if mouse.is_null() || keyboard.is_null() {
            if !mouse.is_null() {
                UnhookWindowsHookEx(mouse);
            }
            if !keyboard.is_null() {
                UnhookWindowsHookEx(keyboard);
            }
            let _ = ready.send(false);
            return;
        }
        let _ = ready.send(true);
        let mut msg: MSG = std::mem::zeroed();
        while !stop.load(Ordering::Acquire) {
            if PeekMessageW(&mut msg, null_mut(), 0, 0, PM_REMOVE) != 0 {
                if msg.message == WM_QUIT {
                    break;
                }
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            } else {
                // Input wakes immediately; the timeout bounds cooperative shutdown.
                MsgWaitForMultipleObjectsEx(
                    0,
                    std::ptr::null(),
                    50,
                    QS_ALLINPUT,
                    MWMO_INPUTAVAILABLE,
                );
            }
        }
        STATE.with_borrow(|s| {
            if let Some(s) = s {
                s.gate.disable();
            }
        });
        UnhookWindowsHookEx(mouse);
        UnhookWindowsHookEx(keyboard);
    });
    ensure!(
        rx.recv_timeout(std::time::Duration::from_secs(3))?,
        "unable to install input hooks"
    );
    Ok(thread)
}
unsafe extern "system" fn key_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let e = &*(l as *const KBDLLHOOKSTRUCT);
        if e.flags & LLKHF_INJECTED == 0 && e.dwExtraInfo != TAG {
            let block = STATE.with_borrow_mut(|s| {
                s.as_mut().is_some_and(|s| {
                    let _callback = s.gate.begin_event();
                    let down = w as u32 == WM_KEYDOWN || w as u32 == WM_SYSKEYDOWN;
                    match from_vk(e.vkCode as u16) {
                        Some(usage) => s.key(usage, down),
                        None => s.unsupported_key(e.vkCode as u16, down),
                    }
                })
            });
            if block {
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, w, l)
}
unsafe extern "system" fn mouse_hook(code: i32, w: WPARAM, l: LPARAM) -> LRESULT {
    if code >= 0 {
        let e = &*(l as *const MSLLHOOKSTRUCT);
        if e.flags & LLMHF_INJECTED == 0 && e.dwExtraInfo != TAG {
            let block = STATE.with_borrow_mut(|s| {
                let Some(s) = s else {
                    return false;
                };
                let _callback = s.gate.begin_event();
                let active = s.gate.active();
                match w as u32 {
                    WM_MOUSEMOVE => {
                        if active {
                            let mut p = POINT { x: 0, y: 0 };
                            GetCursorPos(&mut p);
                            s.emit(Captured::Motion {
                                dx: (e.pt.x - p.x) as f64,
                                dy: (e.pt.y - p.y) as f64,
                            });
                        } else {
                            s.emit(Captured::Position {
                                x: e.pt.x as f64,
                                y: e.pt.y as f64,
                                width: GetSystemMetrics(SM_CXSCREEN) as f64,
                                height: GetSystemMetrics(SM_CYSCREEN) as f64,
                            });
                        }
                    }
                    WM_LBUTTONDOWN | WM_LBUTTONUP | WM_RBUTTONDOWN | WM_RBUTTONUP
                    | WM_MBUTTONDOWN | WM_MBUTTONUP => {
                        let (button, down) = match w as u32 {
                            WM_LBUTTONDOWN => (1, true),
                            WM_LBUTTONUP => (1, false),
                            WM_RBUTTONDOWN => (2, true),
                            WM_RBUTTONUP => (2, false),
                            WM_MBUTTONDOWN => (3, true),
                            _ => (3, false),
                        };
                        s.button(button, down);
                    }
                    WM_MOUSEWHEEL => s.emit(Captured::Input(Input::Scroll {
                        x: 0,
                        y: WHEEL.with_borrow_mut(|r| {
                            wheel_steps(&mut r[1], (e.mouseData >> 16) as i16)
                        }),
                    })),
                    WM_MOUSEHWHEEL => s.emit(Captured::Input(Input::Scroll {
                        x: WHEEL.with_borrow_mut(|r| {
                            wheel_steps(&mut r[0], (e.mouseData >> 16) as i16)
                        }),
                        y: 0,
                    })),
                    _ => (),
                }
                active && s.gate.active()
            });
            if block {
                return 1;
            }
        }
    }
    CallNextHookEx(null_mut(), code, w, l)
}
// Preserve sub-detent wheel increments rather than dropping high-resolution input.
fn wheel_steps(remainder: &mut i32, delta: i16) -> i16 {
    let total = *remainder + i32::from(delta);
    *remainder = total % 120;
    (total / 120) as i16
}
pub fn inject(event: Input) -> Result<()> {
    unsafe {
        let mut input: INPUT = std::mem::zeroed();
        match event {
            Input::Key { usage, down } => {
                let vk = to_vk(usage).ok_or_else(|| anyhow::anyhow!("unsupported HID key"))?;
                input.r#type = INPUT_KEYBOARD;
                input.Anonymous.ki = KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if down { 0 } else { KEYEVENTF_KEYUP },
                    time: 0,
                    dwExtraInfo: TAG,
                };
            }
            Input::Button { button, down } => {
                input.r#type = INPUT_MOUSE;
                let flags = match (button, down) {
                    (1, true) => MOUSEEVENTF_LEFTDOWN,
                    (1, false) => MOUSEEVENTF_LEFTUP,
                    (2, true) => MOUSEEVENTF_RIGHTDOWN,
                    (2, false) => MOUSEEVENTF_RIGHTUP,
                    (3, true) => MOUSEEVENTF_MIDDLEDOWN,
                    (3, false) => MOUSEEVENTF_MIDDLEUP,
                    _ => bail!("unsupported mouse button"),
                };
                input.Anonymous.mi = MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: 0,
                    dwFlags: flags,
                    time: 0,
                    dwExtraInfo: TAG,
                };
            }
            Input::Scroll { x, y } => {
                if x != 0 && y != 0 {
                    inject(Input::Scroll { x, y: 0 })?;
                }
                input.r#type = INPUT_MOUSE;
                input.Anonymous.mi = MOUSEINPUT {
                    dx: 0,
                    dy: 0,
                    mouseData: (if y != 0 { y } else { x } as i32 * 120) as u32,
                    dwFlags: if y != 0 {
                        MOUSEEVENTF_WHEEL
                    } else {
                        MOUSEEVENTF_HWHEEL
                    },
                    time: 0,
                    dwExtraInfo: TAG,
                };
            }
        }
        ensure!(
            SendInput(1, &input, size_of::<INPUT>() as i32) == 1,
            "SendInput rejected (integrity level / secure desktop)"
        );
        Ok(())
    }
}
pub fn pointer(x: f64, y: f64) -> Result<()> {
    unsafe {
        let input = INPUT {
            r#type: INPUT_MOUSE,
            Anonymous: INPUT_0 {
                mi: MOUSEINPUT {
                    dx: (x * 65535.0).round() as i32,
                    dy: (y * 65535.0).round() as i32,
                    mouseData: 0,
                    dwFlags: MOUSEEVENTF_MOVE | MOUSEEVENTF_ABSOLUTE,
                    time: 0,
                    dwExtraInfo: TAG,
                },
            },
        };
        ensure!(
            SendInput(1, &input, size_of::<INPUT>() as i32) == 1,
            "pointer injection rejected"
        );
        Ok(())
    }
}
fn to_vk(u: u16) -> Option<u16> {
    Some(match u {
        4..=29 => u - 4 + 0x41,
        30..=38 => u - 30 + 0x31,
        39 => 0x30,
        40 => 0x0d,
        41 => 0x1b,
        42 => 8,
        43 => 9,
        44 => 0x20,
        45 => 0xbd,
        46 => 0xbb,
        47 => 0xdb,
        48 => 0xdd,
        49 => 0xdc,
        51 => 0xba,
        52 => 0xde,
        53 => 0xc0,
        54 => 0xbc,
        55 => 0xbe,
        56 => 0xbf,
        57 => 0x14,
        58..=69 => u - 58 + 0x70,
        73 => 0x2d,
        74 => 0x24,
        75 => 0x21,
        76 => 0x2e,
        77 => 0x23,
        78 => 0x22,
        79 => 0x27,
        80 => 0x25,
        81 => 0x28,
        82 => 0x26,
        0xe0 => 0xa2,
        0xe1 => 0xa0,
        0xe2 => 0xa4,
        0xe3 => 0x5b,
        0xe4 => 0xa3,
        0xe5 => 0xa1,
        0xe6 => 0xa5,
        0xe7 => 0x5c,
        _ => return None,
    })
}
fn from_vk(v: u16) -> Option<u16> {
    (4..=0xe7).find(|u| to_vk(*u) == Some(v))
}

unsafe fn initial_keys() -> std::collections::BTreeSet<u16> {
    // Exclude mouse VKs and generic modifier aliases (the side-specific VKs
    // below own their releases). Unknown keys held at startup also block capture.
    (7u16..=255)
        .filter(|v| ![0x10, 0x11, 0x12].contains(v))
        .filter(|v| GetAsyncKeyState(i32::from(*v)) < 0)
        .map(|v| from_vk(v).unwrap_or(0x8000 | v))
        .collect()
}
unsafe fn initial_buttons() -> std::collections::BTreeSet<u8> {
    [(1, 1), (2, 2), (3, 4)]
        .into_iter()
        .filter(|(_, v)| GetAsyncKeyState(*v) < 0)
        .map(|(b, _)| b)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn high_resolution_wheel_preserves_remainder_and_direction() {
        let mut remainder = 0;
        assert_eq!(wheel_steps(&mut remainder, 30), 0);
        assert_eq!(wheel_steps(&mut remainder, 30), 0);
        assert_eq!(wheel_steps(&mut remainder, 90), 1);
        assert_eq!(remainder, 30);
        assert_eq!(wheel_steps(&mut remainder, -60), 0);
        assert_eq!(wheel_steps(&mut remainder, -90), -1);
        assert_eq!(remainder, 0);
        assert_eq!(wheel_steps(&mut remainder, 240), 2);
    }
    #[test]
    fn key_positions_roundtrip() {
        for u in 4..=0xe7 {
            if let Some(v) = to_vk(u) {
                assert_eq!(from_vk(v), Some(u));
            }
        }
    }
}
