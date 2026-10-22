# ADR 0001: explicit enrollment and independent input leases

Accepted 2026-10-22.

Quinn/QUIC with rustls TLS 1.3 mutual authentication. Manual out-of-band public
certificate exchange avoids shipping unauthenticated pairing. Root and leaf
identity are pinned. Later pairing must preserve authentication and consent.

Listener/connector roles do not assign an input master. Receivers arbitrate short
leases; source callbacks expire independently. Absolute datagrams serve motion;
discrete input uses an ordered stream.

Keep the core independently MIT licensed. Do not link GPL reference capture
crates. Native APIs sit behind one adapter crate. Primary display/finite HID
bound M1; topology, native drag and IME composition have separate gates.
