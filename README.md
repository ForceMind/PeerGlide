# PeerGlide · 屏跃

**Multiple Computers. One Seamless Experience.**

Rust software KVM for independent computers. Each peer can capture local input
and receive authorized remote input. Bluetooth and USB devices stay paired
with their original computer; operating system input is forwarded over LAN.

## Status

Experimental **0.1.0-dev**, not a stable release. CLI, authenticated QUIC transport,
bounded protocol and recovery state machine are runnable. Windows/macOS native
adapters are implemented; real Windows ↔ macOS dual-machine acceptance is
**pending**. Linux currently supports transport/protocol diagnostics only.
See [validation](docs/VALIDATION.md).

Primary-display switching, basic HID keys, three mouse buttons and scrolling
form the initial native prototype. Multi-monitor topology, IME composition,
clipboard, files, mDNS, pairing codes, tray, reconnect and signed installers are
not implemented in this milestone.

## Build

```sh
cargo build --release --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
```

Use stable Rust. CI artifacts are unsigned prototype binaries, not installers.

## Explicit two-peer enrollment

On each computer create a **different** identity:

```sh
peerglide init --identity identity
```

Exchange only `node.crt.der` through a separately trusted channel. Independently
compare its printed BLAKE3 fingerprint with the other computer. Never share
`node.key.der`. Store the other certificate as `peer.crt.der`. This manual
enrollment precedes the future pairing GUI. There is no blind TOFU mode.

Test without input permission, on A:

```sh
peerglide listen --peer-cert peer.crt.der
```

On B (replace the LAN IP):

```sh
peerglide connect 192.168.1.10:24870 --peer-cert peer.crt.der --bind 0.0.0.0:0 --probe
```

`--probe` checks mutual TLS, version negotiation and a reliable roundtrip without
injecting input. Permit UDP 24870 for the process if the OS firewall prompts;
do not disable the firewall. The CLI exits on disconnect; restart for a new session.

## Experimental native control

Both ends use `--native`. Add `--allow-control` on each computer permitted to
receive remote input. Listener/connector roles only establish the connection;
they do not create a permanent input master. `--edge` specifies the peer's side.

For A left of B:

```sh
# A
peerglide listen --peer-cert peer.crt.der --native --allow-control --edge right
# B
peerglide connect 192.168.1.10:24870 --peer-cert peer.crt.der --bind 0.0.0.0:0 --native --allow-control --edge left
```

- Release held keys/buttons before crossing the primary display edge.
- Keyboard follows the pointer. Move inside the remote screen then back over
  its entry edge to return. Edge rearming requires interior movement.
- **Ctrl+Alt+Right**, then release the chord, toggles the local control target.
- **Ctrl+Alt+Esc** immediately stops capture and requests remote release.
- Physical input at the receiver reclaims control.
- Capture watchdog: 1 second. Receiver lease: 1.5 seconds. Exit/error paths
  release tracked remote keys and buttons.

macOS requires Accessibility and Input Monitoring permission for the exact
app/terminal running the prototype, followed by restart. Secure Input remains
outside its control. Windows input cannot cross privilege boundaries or control
UAC/secure desktop. Keep OS security protections enabled.

## Documentation

- [Product](docs/PRODUCT.md)
- [Architecture](docs/ARCHITECTURE.md)
- [Roadmap](docs/ROADMAP.md) and [development plan](docs/DEVELOPMENT_PLAN.md)
- [Research and licensing](docs/REFERENCE_RESEARCH.md)
- [Security](docs/SECURITY.md) and [validation](docs/VALIDATION.md)

MIT licensed. GPL references are studied, not copied or linked. Dependency
licenses are separate and must accompany distributions.
