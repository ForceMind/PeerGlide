#!/usr/bin/env python3
"""Run under xvfb-run on the Linux CI runner; never touches a user desktop."""
import os
import pathlib
import subprocess
import tempfile
import time

root = pathlib.Path(__file__).resolve().parent.parent
with tempfile.TemporaryDirectory(prefix="peerglide-gui-ci-") as temporary:
    env = os.environ.copy()
    env.update(XDG_DATA_HOME=temporary, LIBGL_ALWAYS_SOFTWARE="1")
    log = pathlib.Path(temporary) / "gui.log"
    with log.open("w") as output:
        gui = subprocess.Popen([str(root / "target/release/peerglide-gui")], env=env, stdout=output, stderr=subprocess.STDOUT)
        try:
            for _ in range(100):
                if gui.poll() is not None:
                    raise RuntimeError(log.read_text())
                found = subprocess.run(["xdotool", "search", "--onlyvisible", "--pid", str(gui.pid), "--name", "PeerGlide"], capture_output=True, text=True)
                if found.returncode == 0 and found.stdout.strip():
                    window = found.stdout.splitlines()[0]
                    break
                time.sleep(0.1)
            else:
                raise TimeoutError("GUI window did not appear")
            time.sleep(1)
            dest = root / "dist"
            dest.mkdir(exist_ok=True)
            # winit can create an unmapped helper window with the same name/PID.
            # Require a visible app window and capture the virtual display root.
            subprocess.run(["import", "-window", "root", str(dest / "peerglide-home.png")], check=True)
            assert (dest / "peerglide-home.png").stat().st_size > 1000
            subprocess.run(["xdotool", "windowclose", window], check=True)
            # Some bare Xvfb servers cannot deliver a window-manager close request.
            try:
                gui.wait(timeout=3)
            except subprocess.TimeoutExpired:
                gui.terminate()
                gui.wait(timeout=3)
            print("PASS: native GUI launched and rendered under isolated Xvfb")
        finally:
            if gui.poll() is None:
                gui.kill()
                gui.wait()
