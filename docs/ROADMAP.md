# Roadmap

## Planning extension — 2026-11-01

The existing M0–M5 identifiers below remain authoritative; none are renumbered
as gesture milestones. Read [the staged product/development baseline](PRODUCT_EVOLUTION.zh-CN.md),
[MouseInc/Aitiy research](RESEARCH_MOUSEINC_AITIY.zh-CN.md), and
[ADR-0002](ADR/0002-unified-input-actions.md) for the additive G1/G2 tracks.
These are plans, not implemented-feature claims.

Current baseline: `56174387563ba6a087c7db35c9c5e1e1031e593c`.
Main CI run `41226383167` has Windows/Linux success but macOS GUI test failure;
macOS release/smoke/package were skipped. Restore this gate in #10 before claiming
an all-green current baseline. Historical validation records are not erased.

| Milestone | Implementation | Acceptance still required |
|---|---|---|
| M0 workspace/protocol/security/state/CI | Implemented engineering foundation | Restore current macOS CI gate #10; native matrix results in VALIDATION.md |
| M1 two-machine input | Prototype implemented | Real Windows/macOS sessions, Bluetooth/USB, DPI, recovery #3 |
| M2 peer collaboration | Partial: symmetric leases and local preemption | mDNS, interactive pairing, reconnect, deterministic arbitration #4 |
| M3 GUI | Basic eframe GUI implemented | Native runtime validation; pairing codes, discovery, tray, full topology #5 |
| M4 clipboard/files | Planned | Unicode/images; resumable integrity-checked files; native drag separate #6 |
| M5 release | Planned | Performance/endurance, installers, signing/notarization, Linux input #7 |
| G1 optional local mouse enhancement | Planned, additive | Shared input broker, local-only runtime, gestures/actions, app rules and physical acceptance #11 |
| G2 cross-device semantic actions | Planned, after G1 and trusted sessions | Separate grants, target context, ordering, deduplication and Win/Mac acceptance #12 |

The first supported release requires P0 real-machine acceptance. CI artifacts
are prototype binaries and do not prove desktop compatibility. G1/G2, OCR and
scripts do not block a correctly scoped, accepted core preview. M5 hardening
runs throughout; Linux and full topology remain open original milestone work,
not implied support in the first two-peer primary-display slice.

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
The checked source/CI history item does not override the current CI failure
noted above.

## Dependency order

1. Restore current CI; complete the existing input correctness gate and #3.
2. Finish the daily-use M2/M3 connection, lifecycle and permission slice.
3. Add G1 locally, with M4 text clipboard independently deliverable.
4. Add image/file collaboration and separately authorized G2 actions.
5. Release only the accepted scope through M5; retain deferred native drag,
   topology, multi-peer, Linux and optional OCR/workflows as explicit open work.

Pure gesture recognition and action-schema tests may run in parallel, but native
interception cannot bypass the existing correctness/physical acceptance gates.
