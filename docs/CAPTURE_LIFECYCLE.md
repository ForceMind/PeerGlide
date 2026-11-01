# C03 capture lifecycle

Status: implemented-unverified for real native capture. The worker owns an
explicit stop flag, completion channel and reaper-owned JoinHandle. CaptureSession
shutdown disables suppression/restores the source pointer before requesting stop;
the caller waits at most 500ms per attempt and reports errors. Drop provides bounded
fallback cleanup. A still-running thread retains the process-wide reservation,
preventing another capture from installing competing hooks during a timeout.

Windows dispatches input immediately via PeekMessage and waits for input with a
50ms maximum idle wait. macOS runs the existing event tap in bounded 50ms run-loop
slices, disables the tap and releases its source/port before completion. Native
initialization failure or readiness timeout drops the thread owner and requests
stop too. A blocked OS call cannot be force-killed safely: the join may time out,
but the gate is disabled and new capture remains refused until the native thread
is reaped. The background reaper can remain blocked; the caller cannot.

Pure lifecycle tests cover 20 start/stop/join rounds, repeated shutdown, early
initialization exit and a deliberately blocked backend's timeout/recovery.
They are not native hook/handle-count measurements. Real permission-granted OS
start/stop cycles, resource counts, permission revocation and session reconnection
remain pending under C03/#3. For the subsequent C04 implementation and remaining
native UX acceptance, see
[WORKER_IPC.md](WORKER_IPC.md).


Task completion is not treated as thread completion: a thread-local destructor
may still block after the completion notification. Neither that notification nor JoinHandle::is_finished is a join
guarantee. A dedicated reaper joins the native thread; the caller only waits on
its acknowledgement with a timeout. The reaper retains capture ownership through
all native thread-local destructors. One reaper thread is added per active capture;
its resource overhead has not been benchmarked. A regression uses a deliberately blocked thread-local destructor and an independent rescue to
prove the previous implementation failed this boundary without hanging CI.
