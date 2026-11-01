# ADR-0003: reliable terminal cursor and protocol v4

Date: 2026-11-02. Status: implemented, native CI/physical acceptance tracked separately.

Protocol v3 correctly orders normal source-originated Release behind discrete
input. It cannot repair the final cursor when the last motion datagram is lost:
Release carries only a lease ID. Reusing a fake scroll or key event as a pointer
anchor would introduce unintended OS input and is rejected.

Protocol v4 adds an optional Motion snapshot to Release. Normal edge return and
hotkey return carry the final normalized cursor on the reliable input stream.
Receiver validates the active lease, cursor lease, range and deadline, applies the
cursor while pressed input is still owned, then releases keys/buttons. Invalid or
expired cursor metadata is ignored but never prevents cleanup. Pointer injection
failure still attempts all releases; failed releases remain pending for recovery.

Emergency, timeout and local preemption use no terminal cursor and remain prompt
safety interruptions. A lost connection cannot promise terminal-position delivery.
Late datagrams/duplicate releases/foreign leases cannot revive completed control.

This changes the wire format. Following ADR-0002, VERSION and ALPN become 4 and
peerglide/4. There is no silent v3 downgrade: both computers must update together.
A real TLS test rejects a v3 endpoint before capture or input. Existing node keys,
certificates and explicit grants are retained; no new permission is implied.
Local GUI-worker IPC remains version 1. Capability-negotiated feature fallback is
future M2 work, not provided by this version bump. v3 ordered-return safety is
preserved; only its incomplete terminal-position representation is superseded.

Tests cover total motion loss, invalid/expired/foreign anchors, pressed-input
cleanup after pointer failure, retained releases after injector failure, wire
roundtrip, real-session terminal release and old-ALPN rejection. These do not
replace Win/Mac dragging, permissions, DPI and real network-fault acceptance #3.
