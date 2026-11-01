# Input compatibility and C02 status

C02 is implementing, not accepted. The current finite HID map does not imply
complete OEM/numpad/media/IME support. Unknown physical key events cancel the
current control/switch intent, immediately disable local suppression and pass
through locally. A payload-free diagnostic explains that no forwarding occurred.
Remote pressed inputs are released through existing session recovery. Unknown
keys are tracked separately from HID usages and block acquisition while held;
startup sampling includes unknown keys in the sampled OS keycode ranges.

This is a safe fallback, not remote support for those keys. Consumer/media events
not delivered through the existing native keyboard callbacks remain outside this
coverage. Full extended-key mapping, capability negotiation, keyboard layouts and
physical Windows/macOS acceptance are still pending under C02/#3. No wire or
permission change is introduced. Protocol v3 normal-return ordering is unchanged.

Native hook tests cover cancellation, held-key guard, autorepeat, no raw input
forwarding, armed-toggle cancellation and bounded-queue fail-open behavior.
Cross compilation is only typechecking; platform CI and real hardware acceptance
are separate gates. C03 thread shutdown and C04 structured events remain pending.
