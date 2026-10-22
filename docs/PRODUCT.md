# Product

PeerGlide / 屏跃 — 多台电脑，一种体验。

P0: Windows/macOS edge switching, keyboard/button/scroll forwarding, relative
screen side, resolution/scaling support, emergency and disconnect recovery.

P1: symmetric input roles, exclusive leases, no injected-event loops, trusted
reconnect, local activity reclaim. Bluetooth/USB devices remain connected to
their original computer. A receiver accepts only one remote lease at a time.

P2: Unicode text/image clipboard; file copy/send; resumable chunks, BLAKE3
integrity, progress/history. Native OS cross-screen drag is a separate feature
and cannot be claimed complete merely because normal file sending works.

P3: untrusted mDNS hints, authenticated identity, pairing confirmation,
independent input/clipboard/file grants and revocation, LAN without a cloud.

P4: three+ peers, multi-monitor topology, groups, shortcuts, clipboard history,
tray/autostart, notifications. Remote desktop/compute remain future research.

GUI: Home, Devices, Displays, Transfers, Settings; eframe, English first,
localizable labels, light/dark. Controls must call the real runtime; unimplemented
features must be clearly labeled or hidden. No decorative fake connection state.
