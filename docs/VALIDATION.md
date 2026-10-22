# Validation record

2026-10-22, isolated cloud Linux development environment. No user Mac/PC changes.

| Check | Result |
|---|---|
| cargo test --workspace | 8 tests passed: framing, lease authorization/conflict/replay/expiry, pressed release, edge gate, authenticated QUIC/datagrams and untrusted-client rejection |
| cargo fmt --all; cargo clippy --workspace --all-targets -- -D warnings | Passed |
| platform clippy/check, x86_64-pc-windows-gnu | Passed (cross compile; no desktop run) |
| platform clippy/check, aarch64-apple-darwin | Passed (cross compile; no desktop run) |
| Native Windows/macOS/Linux GitHub CI | Pending first PR run |
| Real Windows ↔ macOS with Bluetooth/USB | Not run: no physical desktop sessions available |

Pending physical checks: all four edges and return; held keys/buttons; Ctrl/Alt/
Command mappings; simultaneous local input; IME layouts; 100–200% Windows scaling
and macOS Retina; permission denial; Secure Input; elevated windows; fullscreen;
packet loss/jitter; process/network failure while dragging or holding modifiers;
sleep/wake. Full topology, numpad/media/OEM key coverage, native drag, clipboard,
files, GUI, discovery, auto-reconnect, signing and installers remain incomplete.

Cross compilation proves source typechecking only. Native CI proves compilation,
linking and noninteractive tests only. Neither proves physical desktop behavior.
