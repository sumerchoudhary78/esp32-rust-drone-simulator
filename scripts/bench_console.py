#!/usr/bin/env python3
"""Authenticated, finite-duration, propellers-removed ESP32 bench console.

UDP uses the Python standard library. Serial additionally requires pyserial.
No command touches a port until a transport is explicitly supplied.
"""
import argparse
import hashlib
import hmac
import json
import math
import os
from pathlib import Path
import re
import socket
import struct
import time

PACKET_BYTES = 60


def encode(key, session, sequence, *, arm=False, throttle=0., roll=0., pitch=0., yaw=0., emergency=False):
    if len(key) != 32 or not any(key):
        raise ValueError("A nonzero 32-byte device key is required")
    values = [throttle, roll, pitch, yaw]
    if not all(math.isfinite(v) for v in values) or not 0 <= throttle <= 1 or any(abs(v) > 1 for v in values[1:]):
        raise ValueError("Invalid control range")
    if arm and emergency:
        raise ValueError("Emergency stop must not set ARM")
    body = struct.pack("<4sBBHQIHhhh", b"AERO", 2, int(arm) | (int(emergency) << 1),
        PACKET_BYTES, session, sequence & 0xffffffff, *(round(v * 1000) for v in values))
    return body + hmac.digest(key, body, "sha256")


class Udp:
    def __init__(self, host, port):
        self.socket = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        self.socket.connect((host, port))
        self.socket.settimeout(.3)

    def send(self, packet):
        self.socket.send(packet)

    def status(self):
        for _ in range(6):
            self.socket.send(b"AERO2 HELLO")
            try:
                data = json.loads(self.socket.recv(1024))
                if data.get("protocol") == 2:
                    return data
            except (socket.timeout, json.JSONDecodeError):
                pass
        raise TimeoutError("No AERO2 status; join the board's Wi-Fi AP and check its build")

    def close(self):
        self.socket.close()


class Serial:
    def __init__(self, path, baud):
        try:
            import serial
        except ImportError as error:
            raise RuntimeError("Serial console needs pyserial: python3 -m pip install pyserial") from error
        self.port = serial.Serial(port=None, baudrate=baud, timeout=.2)
        # Configure control lines before opening, to avoid requesting reset.
        self.port.dtr = False
        self.port.rts = False
        self.port.port = path
        self.port.open()

    def send(self, packet):
        self.port.write(b"@" + packet.hex().encode() + b"\n")
        self.port.flush()

    def fault(self, masked=False):
        self.port.write(b"~" if masked else b"!")
        self.port.flush()

    def status(self):
        until = time.monotonic() + 3
        while time.monotonic() < until:
            line = self.port.readline().decode("ascii", errors="replace")
            if not line.startswith("bench "):
                continue
            values = dict(re.findall(r"(\w+)=([^\s]+)", line))
            if "session" in values and "next" in values:
                values["next_sequence"] = int(values.pop("next"))
                values["protocol"] = 2
                return values
        raise TimeoutError("No bench telemetry; close espflash monitor and check the firmware/port")

    def close(self):
        self.port.close()


def calibrate(path, low_raw, low_volts, high_raw, high_volts):
    if not (32 < low_raw < high_raw < 4063 and high_raw - low_raw > 100
            and 8 <= low_volts <= 11.5 and 11.5 <= high_volts <= 13 and high_volts - low_volts >= 1):
        raise ValueError("Calibration points must span at least 1 V and 101 ADC counts, without rail clipping")
    profile = json.loads(path.read_text())
    if profile.get("mode") == "emulated":
        raise ValueError("Physical calibration cannot be written into the emulated profile")
    profile["battery_calibration"] = dict(raw_low=low_raw, volts_low=low_volts, raw_high=high_raw, volts_high=high_volts)
    temporary = path.with_suffix(".json.tmp")
    with open(temporary, "x", opener=lambda name, flags: os.open(name, flags, 0o600)) as output:
        json.dump(profile, output, indent=2)
        output.write("\n")
    temporary.replace(path)
    print("Saved measured calibration. Rebuild and flash this device profile before arming.")


def execute(transport, key, action, seconds, throttle=0., roll=0., pitch=0., yaw=0.):
    status = transport.status()
    print(json.dumps(status, sort_keys=True))
    session = int(status["session"], 16)
    sequence = int(status["next_sequence"])

    def send(**command):
        nonlocal sequence
        transport.send(encode(key, session, sequence, **command))
        sequence = (sequence + 1) & 0xffffffff

    if action == "status":
        return
    if action == "emergency":
        for _ in range(3):
            send(emergency=True)
            time.sleep(.02)
        return
    start = due = time.monotonic()
    level = 0.
    try:
        while time.monotonic() - start < seconds:
            now = time.monotonic()
            if now - due > .1:
                raise TimeoutError("Console missed its send deadline; disarming")
            # First 400 ms at zero throttle allow the controller to arm before
            # a small, bounded bench command. It still needs the GPIO13 switch.
            target = throttle if now - start >= .4 else 0.
            level = min(target, level + .01)
            send(arm=action == "arm", throttle=level if action == "arm" else 0.,
                roll=roll, pitch=pitch, yaw=yaw)
            due += .02
            time.sleep(max(0, due - time.monotonic()))
    finally:
        for _ in range(3):
            send(arm=False)
            time.sleep(.02)
        print("Disarm sent; GPIO13 must return LOW before another arming sequence.")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--profile", type=Path, required=True)
    transport = parser.add_mutually_exclusive_group()
    transport.add_argument("--udp", metavar="HOST")
    transport.add_argument("--serial", metavar="PORT")
    parser.add_argument("--port", type=int, default=14550)
    parser.add_argument("--baud", type=int, default=115200)
    commands = parser.add_subparsers(dest="action", required=True)
    commands.add_parser("status")
    commands.add_parser("emergency")
    stop = commands.add_parser("disarm")
    stop.add_argument("--seconds", type=float, default=1)
    arm = commands.add_parser("arm", help="Finite, low-power bench run; always sends disarm on exit")
    arm.add_argument("--props-removed", action="store_true", required=True)
    arm.add_argument("--seconds", type=float, default=3)
    arm.add_argument("--throttle", type=float, default=0)
    for axis in ["roll", "pitch", "yaw"]:
        arm.add_argument(f"--{axis}", type=float, default=0)
    fault = commands.add_parser("fault", help="Stall Core 1 in an explicitly built serial fault-injection image")
    fault.add_argument("--props-removed", action="store_true", required=True)
    fault.add_argument("--mask-interrupts", action="store_true", help="Exercise the hardware MWDT reset fallback")
    cal = commands.add_parser("calibrate", help="Record TWO measured ADC/multimeter points; does not connect hardware")
    for flag, kind in [("low-raw", int), ("high-raw", int), ("low-volts", float), ("high-volts", float)]:
        cal.add_argument("--" + flag, type=kind, required=True)
    args = parser.parse_args()
    try:
        if args.action == "calibrate":
            calibrate(args.profile, args.low_raw, args.low_volts, args.high_raw, args.high_volts)
            return
        if args.action == "fault" and not args.serial:
            parser.error("Fault injection is available only through an explicit serial bench image")
        if not args.udp and not args.serial:
            parser.error("Choose --udp HOST or --serial PORT")
        seconds = getattr(args, "seconds", 1)
        throttle = getattr(args, "throttle", 0)
        axes = [getattr(args, axis, 0) for axis in ["roll", "pitch", "yaw"]]
        if not .02 <= seconds <= 10 or not 0 <= throttle <= .15 or any(not -.2 <= v <= .2 for v in axes):
            parser.error("Bench commands require 0.02–10 s, throttle 0–0.15, and axes within ±0.2")
        profile = json.loads(args.profile.read_text())
        if profile.get("mode") == "emulated":
            parser.error("This console opens physical transports; use the emulator tests for the shared simulation key")
        key = bytes.fromhex(profile["psk_hex"])
        connection = Udp(args.udp, args.port) if args.udp else Serial(args.serial, args.baud)
        try:
            if args.action == "fault":
                connection.fault(args.mask_interrupts)
                print("Fault injected. Capture GPIO33, GPIO32 and ESC lines; reset is needed after a latched timer cut.")
            else:
                execute(connection, key, args.action, seconds, throttle, *axes)
        finally:
            connection.close()
    except (OSError, ValueError, RuntimeError, KeyError) as error:
        parser.exit(1, f"Bench console: {error}\n")
    except KeyboardInterrupt:
        parser.exit(130, "Bench run interrupted. Verify the gate is LOW.\n")


if __name__ == "__main__":
    main()
