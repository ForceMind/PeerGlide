# Architecture

`peerglide-protocol`: messages, bounded decode, USB HID vocabulary.
`peerglide-core`: receiver-authoritative lease, pressed input tracking, motion
sequence filtering, edge gate. `peerglide-transport`: identities, QUIC/TLS, exact
enrolled certificate verification, bounded stream framing. `peerglide-platform`:
native tagged capture/injection and fail-open watchdog. `peerglide-desktop`:
executable session orchestration, enrollment and probe.

Both peers authenticate with explicitly exchanged certificates, TLS 1.3, ALPN
`peerglide/1`. No 0-RTT input or insecure verifier. A fingerprint must be checked
over an independently trusted channel; the same untrusted channel is insufficient.

The listener accepts one connection and the connector opens a reliable stream.
Both exchange versioned Hello. Either may request input control regardless of
connection direction. Receiving nodes grant only when explicitly authorized and
idle. Simultaneous acquisition requests are safely denied; deterministic
multi-peer arbitration is future work.

Keys/buttons/scroll use the ordered stream. Motion uses absolute normalized QUIC
datagrams with per-lease sequence filtering. Loss does not permanently lose
displacement; stale/early packets are ignored and repaired by the next position.
Bulk file transfers will use separate streams and scheduling; none exist yet.

The initial coordinate model covers primary displays with differing resolution:
Windows uses DPI-aware physical capture, macOS CoreGraphics display units. Mixed
DPI correctness still requires real-machine acceptance. Full topology is pending.
HID positions are forwarded; Unicode text/IME composition is not transmitted.
Ctrl/Command remain distinct; configurable shortcut remapping is pending.

Callbacks never wait for network IO. Bounded nonblocking channel overload disables
capture. A monotonic 1-second gate watchdog is checked for every event independently
of the async session. Receiver lease: 1.5 seconds. Timeout, disconnect, error,
escape, local activity or session drop release tracked remote keys/buttons.
Tagged injected events are excluded from forwarding. Acquisition waits until
physical keys/buttons are released, avoiding local-down/remote-up splitting.

OS permissions, Secure Input, UAC and privilege boundaries are respected. Failure
terminates control rather than bypassing system security.
