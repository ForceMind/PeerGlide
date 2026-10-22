# Roadmap

| Milestone | Implementation | Acceptance still required |
|---|---|---|
| M0 workspace/protocol/security/state/CI | Implemented | Native matrix results in VALIDATION.md |
| M1 two-machine input | Prototype implemented | Real Windows/macOS sessions, Bluetooth/USB, DPI, recovery |
| M2 peer collaboration | Partial: symmetric leases and local preemption | mDNS, interactive pairing, reconnect, deterministic arbitration |
| M3 GUI | Basic eframe GUI implemented | Native runtime validation; pairing codes, discovery, tray, full topology |
| M4 clipboard/files | Planned | Unicode/images; resumable integrity-checked files; native drag separate |
| M5 release | Planned | Performance/endurance, installers, signing/notarization, Linux input |

The first supported release requires P0 real-machine acceptance. CI artifacts
are prototype binaries and do not prove desktop compatibility.

## M1 implementation/acceptance checklist

- [x] Windows/macOS capture and injection source with platform CI
- [x] Authenticated two-process communication
- [x] Primary display edge and keyboard-follow session code
- [x] Relative side and normalized resolution model
- [x] Recovery state machine and fault-path tests
- [ ] Physical Windows/macOS Bluetooth and USB control
- [ ] Real mixed-DPI, IME/shortcut, fullscreen and permissions checks
- [ ] Real two-machine fault/recovery acceptance

5/8 tracking gates complete (about 63%). This measures engineering gates, not
product readiness. No physical acceptance gate is checked without evidence.
