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
