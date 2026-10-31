# Development plan

1. Reproducible M0 tests and native CI; fix compile/link failures.
2. Executable loopback probe and negative authentication checks.
3. Native input sessions, extended HID and shortcut mappings, permissions UX.
4. Two-way edge return, local reclaim, collision denial, held-key disconnect
   recovery on real Windows/macOS computers with Bluetooth/USB and mixed DPI.
5. Functional eframe control panel using the same runtime; explicit persisted
   public-certificate trust and independent permissions.
6. mDNS, authenticated pairing UI, backoff reconnect with fresh leases.
7. Clipboard/files with grants, size limits, separate streams and consent.
8. Installable preview after physical P0 acceptance; signing/notarization and
   Linux backend. No hosted service deployment.

Use incremental branches and PRs, fmt, clippy with denied warnings, tests and
native matrix builds. Do not force updates or change repository visibility.


## Input correctness gate (before M2 expansion)

1. Retain/retry failed native releases and fail the session visibly; add injected
   transient/permanent failure tests. Implemented in the recovery follow-up.
2. Preserve Windows high-resolution wheel remainder. Implemented; native runtime
   acceptance still pending.
3. Reproduce reliable-stream/datagram ordering at edge return, then implement a
   causally ordered handoff if the test confirms premature lease termination.
4. Define unknown-key/capability behavior; expand layout and extended-key tests.
5. Add explicit capture-thread shutdown and reclamation before in-process reconnect.
6. Replace GUI stdout-prefix state parsing with structured worker events; allow
   immediate disconnect-and-revoke.
7. Run physical Windows/macOS acceptance matrix #3. No date or completion claim
   substitutes for recorded real desktop results.
