# Security model

LAN peers are hostile until authenticated. Discovery grants no trust. TLS 1.3
mutually authenticates explicitly enrolled certificates, with post-handshake
leaf verification. No insecure verifier, shared default secret, automatic TOFU,
cloud relay or 0-RTT input. Fingerprints use BLAKE3; TLS uses rustls/ring.

`--allow-control` grants input for the session independently of certificate
trust. Clipboard/files are not implemented. Stop the session and remove/replace
the peer certificate to revoke CLI authorization. Persistent grant UI is pending.

Frames are capped before allocation at 4096 bytes, input codes and motion ranges
validated, stream count/windows bounded. Input payloads do not implement Debug
and are not logged. No password/clipboard collection, telemetry or uploads.

Keys use exclusive file creation and Unix mode 0600. On Windows store identity
files under a user-private directory with normal user ACLs; hardened ACL/keychain
provisioning is pending. Never exchange private keys.

Recovery uses callback-level escape/watchdog and receiver lease expiry with
tracked releases. OS-protected input is not bypassed. Process termination or OS
injection failure may prevent synthetic key release and needs physical testing.

Before stable release: independent security review, fuzzing/rate limiting, OS
key store, revocation UI, advisory/license review, signed/notarized packages.
Use private vulnerability reporting if available. Never put secrets or captured
input into public issues.
