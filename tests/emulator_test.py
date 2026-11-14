"""Real guest binaries, CPU execution and peripheral boundaries (no hardware).
Run after setup_emulator.py, esp32_firmware.py build and build-fixture.
"""
import json
from pathlib import Path
import selectors
import subprocess
import unittest

ROOT = Path(__file__).resolve().parents[1]
BACKEND = ROOT / "output/emulator-build/aerolab-esp32"
FIXTURE = ROOT / "output/firmware/simulation-fixture/aerolab-sim-fixture.bin"
BOOT = ROOT / "output/firmware/esp32-devkitc-m1/aerolab-esp32-devkitc-merged.bin"

class Bridge:
    def __init__(self, image=FIXTURE, sda=21, address=104):
        self.process = subprocess.Popen([str(BACKEND), str(image), str(sda), "22", str(address), "6", "1"], stdin=subprocess.PIPE, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
        self.selector = selectors.DefaultSelector()
        self.selector.register(self.process.stdout, selectors.EVENT_READ)
        self.console = ""

    def step(self, steps=1, uart="", powered=1, adc=3300, gyro=.25):
        for i in range(steps):
            encoded = uart.encode().hex() if uart and i == 0 else "-"
            self.process.stdin.write(f"2000 0 0 9.80665 0 0 {gyro} {adc} {powered} {encoded}\n")
            self.process.stdin.flush()
            if not self.selector.select(timeout=5):
                raise AssertionError("Guest backend failed to produce a bounded response")
            line = self.process.stdout.readline()
            if not line:
                raise AssertionError("Guest process exited")
            self.frame = json.loads(line)
            self.console += bytes.fromhex(self.frame["uart_hex"]).decode(errors="replace")
        return self.frame

    def __enter__(self):
        return self

    def __exit__(self, *_):
        self.process.kill()
        self.process.wait(timeout=5)
        self.process.stdin.close()
        self.process.stdout.close()
        self.selector.close()

class GuestTests(unittest.TestCase):
    def test_boot_only_binary_keeps_motor_peripherals_off_and_clock_runs(self):
        with Bridge(BOOT) as cpu:
            frame = cpu.step(530)
            self.assertIn("board=ESP32-DevKitC-32E", cpu.console)
            self.assertIn("heartbeat=2", cpu.console)
            self.assertIn("gpio_output_registers_low=true", cpu.console)
            self.assertEqual(frame["pwm_us"], [0] * 40)
            self.assertTrue(frame["reserved_gpio_outputs_low"])
            self.assertGreater(frame["guest_us"], 1_000_000)
            self.assertEqual(frame["unmapped"], 0)
            self.assertEqual(frame["unhandled"], 0)

    def test_unmodified_guest_hal_reads_mpu_adc_and_drives_pwm_from_uart(self):
        with Bridge() as cpu:
            frame = cpu.step(30)
            self.assertIn("WHO_AM_I=0x68", cpu.console)
            self.assertIn("az_raw=16384 gz_raw=1876", cpu.console)
            self.assertIn("battery_adc=3300", cpu.console)
            self.assertGreater(frame["imu_reads"], 1)
            frame = cpu.step(140, uart="5\n", adc=1234)
            self.assertIn("battery_adc=1234", cpu.console)
            for gpio in [25,26,27,14]:
                self.assertAlmostEqual(frame["pwm_us"][gpio], 1500, delta=2)
            frame = cpu.step(30, uart="0\n")
            for gpio in [25,26,27,14]:
                self.assertAlmostEqual(frame["pwm_us"][gpio], 1000, delta=2)
            self.assertFalse(frame["halted"])
            self.assertEqual(frame["unmapped"], 0)
            self.assertEqual(frame["unhandled"], 0)

    def test_wrong_wire_address_and_sensor_power_do_not_return_fake_samples(self):
        for sda,address in [(19,104),(21,105)]:
            with self.subTest(sda=sda,address=address), Bridge(sda=sda,address=address) as cpu:
                frame = cpu.step(40)
                self.assertNotIn("WHO_AM_I=0x68", cpu.console)
                self.assertEqual(frame["imu_reads"], 0)
        with Bridge() as cpu:
            before = cpu.step(30)["imu_reads"]
            frame = cpu.step(140,powered=0)
            self.assertEqual(frame["imu_reads"], before)
            self.assertIn("IMU NACK/error", cpu.console)

if __name__ == "__main__":
    unittest.main()
