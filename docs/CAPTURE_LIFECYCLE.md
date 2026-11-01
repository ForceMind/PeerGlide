# C03 capture lifecycle

Status: implemented-unverified for real native capture. The worker owns an
explicit cooperative stop flag, completion channel and JoinHandle. CaptureSession
shutdown disables suppression/restores the source pointer before requesting stop;
join waits at most 500ms per attempt and errors are reported. Drop provides bounded
fallback cleanup. A still-running thread retains the process-wide reservation,
preventing another capture from installing competing hooks during a timeout.

Windows dispatches input immediately via PeekMessage and waits for input with a
50ms maximum idle wait. macOS runs the existing event tap in bounded 50ms run-loop
slices, disables the tap and releases its source/port before completion. Native
initialization failure or readiness timeout drops the thread owner and requests
stop too. A blocked OS call cannot be force-killed safely: the join may time out,
but the gate is disabled and new capture remains refused until that thread exits.

Pure lifecycle tests cover 20 start/stop/join rounds, repeated shutdown, early
initialization exit and a deliberately blocked backend's timeout/recovery.
They are not native hook/handle-count measurements. Real permission-granted OS
start/stop cycles, resource counts, permission revocation and session reconnection
remain pending under C03/#3. C04 structured state IPC has not been implemented.
