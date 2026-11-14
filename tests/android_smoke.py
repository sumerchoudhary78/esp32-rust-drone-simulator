"""End-to-end check of the dedicated 1080x2280 Aerolab Android 11 AVD.

Run scripts/run-android-emulator.sh first, with a fresh simulator on port 8787.
The test takes control of that simulator and leaves it disarmed.
"""
import json
import os
from pathlib import Path
import subprocess
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
ADB = Path(os.environ.get("ANDROID_HOME", str(Path.home() / "Library/Android/sdk"))) / "platform-tools/adb"
DEVICE = "emulator-5580"
OUTPUT = ROOT / "output/android"


def adb(*args):
    return subprocess.check_output([str(ADB), "-s", DEVICE, *args], timeout=15)


def telemetry():
    with urllib.request.urlopen("http://127.0.0.1:8787/api/telemetry", timeout=2) as response:
        return json.load(response)


def wait(predicate, timeout=5):
    deadline = time.monotonic() + timeout
    while time.monotonic() < deadline:
        current = telemetry()
        if predicate(current):
            return current
        time.sleep(0.04)
    raise AssertionError(f"Timed out; final snapshot: {telemetry()}")


def tap(x, y):
    adb("shell", "input", "tap", str(x), str(y))


def screenshot(name):
    (OUTPUT / name).write_bytes(adb("exec-out", "screencap", "-p"))


def main():
    assert b"Aerolab_Android_11" in adb("emu", "avd", "name")
    assert b"1080x2280" in adb("shell", "wm", "size")
    previous_event = telemetry()["events"][0]["id"]
    tap(790, 458)  # Take control, as observed on this dedicated AVD.
    state = wait(lambda t: t["controller_id"] is not None and any(e["id"] > previous_event and e["message"].startswith("Controller ") for e in t["events"]))
    assert state["state"] == "disarmed"
    owner = state["controller_id"]
    state = wait(lambda t: t["command_age_ms"] is not None and t["command_age_ms"] < 100)
    assert state["settings"]["altitude_assist"], "Reset the simulator to its default assisted mode first"
    received = state["packets_received"]
    started = time.monotonic()
    time.sleep(0.6)
    rate = (telemetry()["packets_received"] - received) / (time.monotonic() - started)
    assert 30 < rate < 70, rate
    tap(540, 950)  # Arm at zero throttle.
    wait(lambda t: t["state"] == "armed")
    tap(290, 1100)  # Assisted takeoff.
    state = wait(lambda t: t["position_m"]["z"] > 1.4, 12)
    assert abs(state["attitude_deg"]["x"]) < 3
    screenshot("controller-hover.png")
    tap(800, 1323)  # Positive roll slider input.
    wait(lambda t: t["attitude_deg"]["x"] > 3)
    tap(540, 1735)  # Center attitude controls.
    wait(lambda t: abs(t["attitude_deg"]["x"]) < 2)
    adb("shell", "input", "keyevent", "3")  # Background the actual Android app.
    state = wait(lambda t: t["reason"] == "link_timeout", 3)
    assert state["motors"]["pwm_us"] == [1000] * 4
    adb("shell", "am", "start", "-n", "dev.aerolab.controller/android.app.NativeActivity")
    time.sleep(0.3)
    assert telemetry()["state"] == "failsafe", "Resume must not automatically clear the latch"
    screenshot("controller-paused.png")
    tap(540, 950)  # Explicit Resume / disarm.
    state = wait(lambda t: t["state"] == "disarmed")
    assert state["motors"]["pwm_us"] == [1000] * 4
    assert state["controller_id"] == owner
    screenshot("controller-disarmed.png")
    result = {"device": DEVICE, "avd": "Aerolab_Android_11", "mode": "simulation", "command_hz": round(rate, 1), "checks": ["native launch and WebSocket connection", "exclusive ownership", "zero-throttle arming", "assisted takeoff above 1.4 m", "roll control and centering", "real Android background triggers link timeout", "return preserves failsafe latch", "explicit resume disarms"]}
    (OUTPUT / "verification.json").write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
