"""Execute the actual dual-core bench firmware. No physical measurements.

Prerequisites: build-bench --emulated --fault-injection; setup_emulator.py.
"""
import json
import os
from pathlib import Path
import re
import selectors
import subprocess
import struct
import sys
import unittest

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT / "scripts"))
from bench_console import encode
from verify_esp32_image import watchdog_iram_address

BACKEND = ROOT / "output/emulator-build/aerolab-esp32"
IMAGE = ROOT / "output/firmware/flight-bench-emulated/aerolab-flight-bench.bin"
KEY = bytes([0x42] * 32)
SESSION = 0x1122334455667788
PINS = [25, 26, 27, 14]


class FlightBench:
    def __init__(self):
        self.process = subprocess.Popen([str(BACKEND), str(IMAGE), "21", "22", "104", "6", "1"],
            stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.console = ""
        self.sequence = 1
        self.arm_input = 0

    def step(self, steps=1, *, data=b"", adc=3300, powered=1, gyro=(0, 0, 0), accel=(0, 0, 9.80665), dt=20000):
        for i in range(steps):
            text = data.hex() if data and i == 0 else "-"
            self.process.stdin.write(f"{dt} {' '.join(map(str,accel))} {' '.join(map(str,gyro))} {adc} {powered} {text} {self.arm_input}\n")
            self.process.stdin.flush()
            if not self.selector.select(timeout=10):
                raise AssertionError("Guest failed to produce a bounded response")
            line = self.process.stdout.readline()
            if not line:
                raise AssertionError("Guest exited")
            self.frame = json.loads(line)
            self.console += bytes.fromhex(self.frame["uart_hex"]).decode(errors="replace")
            if self.frame["unhandled"] or self.frame["unmapped"] or self.frame["halted"]:
                raise AssertionError(self.frame)
        return self.frame

    def command(self, **values):
        packet = encode(KEY, SESSION, self.sequence, **values)
        self.sequence += 1
        self.step(data=b"@" + packet.hex().encode() + b"\n")
        return packet

    def ready(self):
        self.step(225)
        assert "state=1 calibration=2000" in self.console, self.console[-1200:]
        assert self.frame["app_cpu_started"]
        assert self.frame["gate_gpio32"] == 0

    def arm(self):
        self.arm_input = 0
        self.command(arm=False)
        self.arm_input = 1
        self.command(arm=True)
        assert self.frame["gate_gpio32"] == 1, self.console[-1200:]

    def close(self):
        self.process.kill()
        self.process.wait(timeout=5)
        self.process.stdin.close()
        self.process.stdout.close()
        self.selector.close()


class RealFirmwareTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not BACKEND.is_file() or not IMAGE.is_file():
            if os.environ.get("AEROLAB_REQUIRE_EMULATOR") == "1":
                raise AssertionError("Required real bench binary/backend missing")
            raise unittest.SkipTest("Build the emulated bench firmware and CPU backend first")

    def setUp(self):
        self.cpu = FlightBench()
        self.addCleanup(self.cpu.close)

    def assert_cut(self):
        self.assertEqual(self.cpu.frame["gate_gpio32"], 0)
        # The external AND gate forces all four ESC lines LOW. LEDC duty
        # latches can take one PWM period to update behind that closed gate.

    def test_linked_watchdog_is_in_iram_and_flash_placement_is_rejected(self):
        elf=IMAGE.with_suffix('.elf').read_bytes()
        address=watchdog_iram_address(elf)
        manifest=json.loads((IMAGE.parent/'manifest.json').read_text())
        self.assertTrue(manifest['watchdog_in_iram'])
        self.assertEqual(manifest['watchdog_handler_address'],hex(address))
        corrupt=elf.replace(struct.pack('<I',address),struct.pack('<I',0x400d0000))
        with self.assertRaisesRegex(ValueError,'IRAM'):
            watchdog_iram_address(corrupt)

    def test_actual_core1_sensor_calibration_adc_and_disarmed_waveforms(self):
        self.cpu.ready()
        self.assertGreater(self.cpu.frame["imu_reads"], 2000)
        self.assertIn("voltage=12.375", self.cpu.console)
        for pin in PINS:
            self.assertAlmostEqual(self.cpu.frame["pwm_us"][pin], 1000, delta=1)
        self.assert_cut()

    def test_authenticated_physical_interlock_replay_output_cap_and_expiry(self):
        self.cpu.ready()
        self.cpu.arm_input = 1
        self.cpu.command(arm=True)
        self.assert_cut()
        self.cpu.arm()
        packet = self.cpu.command(arm=True, throttle=1)
        for pin in PINS:
            self.assertAlmostEqual(self.cpu.frame["pwm_us"][pin], 1150, delta=1)
        corrupt = bytearray(encode(KEY, SESSION, self.cpu.sequence, emergency=True))
        corrupt[-1] ^= 1
        self.cpu.step(data=b"@" + corrupt.hex().encode() + b"\n")
        self.assertEqual(self.cpu.frame["gate_gpio32"], 1)
        self.cpu.step(12, data=b"@" + packet.hex().encode() + b"\n")
        self.assert_cut()  # replay did not refresh the local expiry clock
        self.cpu.command(arm=True)
        self.assert_cut()
        self.cpu.step(5)
        self.assertIn("rejected=2", self.cpu.console)

    def test_sensor_disconnect_cuts_and_reinitializes_only_while_inhibited(self):
        self.cpu.ready()
        self.cpu.arm()
        self.cpu.step(powered=0)
        self.assert_cut()
        before = self.cpu.frame["imu_reads"]
        self.cpu.step(50, powered=0)
        self.assertEqual(self.cpu.frame["imu_reads"], before)
        self.cpu.step(280)
        self.assert_cut()
        self.assertGreater(self.cpu.frame["imu_reads"], before)
        self.assertIn("state=1 calibration=2000", self.cpu.console[-1400:])
        self.assertNotIn("watchdog=true", self.cpu.console)

    def test_battery_rail_critical_and_physical_disarm_are_immediate(self):
        self.cpu.ready()
        for adc in [4095, 2600]:
            self.cpu.arm()
            self.cpu.step(adc=adc)
            self.assert_cut()
            self.cpu.arm_input = 0
            self.cpu.step(10)  # battery needs ten consecutive valid samples
        self.cpu.arm()
        self.cpu.arm_input = 0
        self.cpu.step(dt=2000)
        self.assert_cut()

    def test_emergency_stop_and_independent_timer_cut_a_stalled_core(self):
        self.cpu.ready()
        self.cpu.arm()
        self.cpu.command(emergency=True)
        self.assert_cut()
        self.cpu.arm()
        self.cpu.command(arm=True, throttle=.1)
        before = self.cpu.frame["guest_us"]
        # ! exists only in a capped, explicitly requested fault-injection build.
        # It stops Core 1 in a busy loop; Core 0 and TIMG1 ISR continue executing.
        self.cpu.step(data=b"!", dt=2000)
        for _ in range(6):
            if self.cpu.frame["gate_gpio32"] == 0: break
            self.cpu.step(dt=2000)
        self.assert_cut()
        self.assertTrue(all(self.cpu.frame["pwm_us"][pin] == 0 for pin in PINS))
        elapsed = self.cpu.frame["guest_us"] - before
        self.assertGreaterEqual(elapsed, 8000)
        self.assertLessEqual(elapsed, 14020)
        self.cpu.step(6)
        self.assertIn("watchdog=true", self.cpu.console[-1800:])
        self.assertTrue(any(int(n) > 2000 for n in re.findall(r"watchdog_ticks=(\d+)",self.cpu.console)))
        self.cpu.command(arm=True)
        self.assert_cut()
        self.assertIn("state=3", self.cpu.console[-1800:])  # ISR also marks the independent cut in telemetry

    def test_sensor_range_filter_and_real_pid_motor_mix(self):
        self.cpu.ready()
        self.cpu.arm()
        self.cpu.command(arm=True, throttle=.08)
        self.cpu.step(12, gyro=(.3, 0, 0), dt=2000)
        pulse = [self.cpu.frame["pwm_us"][pin] for pin in PINS]
        self.assertGreater(max(pulse) - min(pulse), 1)
        self.assertGreaterEqual(min(pulse), 999)
        self.assertLessEqual(max(pulse), 1151)
        self.cpu.step(8, accel=(0,0,6*9.80665), gyro=(20,0,0), dt=2000)
        self.assertGreater(self.cpu.frame["imu_reads"], 2000)
        self.assertNotIn("imu_errors=1", self.cpu.console[-600:])


if __name__ == "__main__":
    unittest.main()
