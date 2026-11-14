#!/usr/bin/env python3
"""Pinned tools and build/flash commands for ESP32-DevKitC-32E.

Default operations never access a serial port. Only `flash` / `flash-bench --port ...` write
hardware. espup installs a dedicated toolchain without replacing stable or esp.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import secrets
import subprocess
import sys
import urllib.request
import zipfile

ROOT = Path(__file__).resolve().parents[1]
FIRMWARE = ROOT / "firmware/esp32-devkitc"
LOCK = json.loads((FIRMWARE / "tools.lock.json").read_text())
TARGET = "xtensa-esp32-none-elf"
NAME = "aerolab-esp32-devkitc"
OUTPUT = ROOT / "output/firmware/esp32-devkitc-m1"


def run(args, **kwargs):
    print("+", " ".join(map(str, args)), flush=True)
    return subprocess.run(list(map(str, args)), check=True, **kwargs)


def digest(path):
    with path.open("rb") as source:
        return hashlib.file_digest(source, "sha256").hexdigest()


def host_triple():
    arch = {"arm64": "aarch64", "aarch64": "aarch64", "x86_64": "x86_64", "AMD64": "x86_64"}.get(platform.machine())
    suffix = {"Darwin": "apple-darwin", "Linux": "unknown-linux-gnu"}.get(platform.system())
    triple = f"{arch}-{suffix}"
    if triple not in LOCK["platforms"]:
        raise SystemExit("The helper supports macOS and glibc Linux on ARM64/x86_64. See the firmware README for manual setup.")
    return triple


def tool_paths():
    folder = ROOT / "output/esp32-tools" / host_triple()
    return folder, folder / "espup", folder / "espflash", folder / "export-esp.sh"


def download(url, path, expected):
    if path.exists() and digest(path) == expected:
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    request = urllib.request.Request(url, headers={"User-Agent": "Aerolab-ESP32-Setup"})
    partial = path.with_suffix(path.suffix + ".part")
    print(f"Downloading pinned tool: {path.name}", flush=True)
    with urllib.request.urlopen(request, timeout=60) as response, partial.open("wb") as output:
        shutil.copyfileobj(response, output)
    if digest(partial) != expected:
        partial.unlink()
        raise SystemExit(f"SHA-256 mismatch for {path.name}; tool was not executed")
    partial.replace(path)


def rustup_which(toolchain, command):
    return subprocess.check_output(["rustup", "which", "--toolchain", toolchain, command], text=True).strip()


def setup():
    if not shutil.which("rustup"):
        raise SystemExit("Install Rust with rustup before setting up the ESP32 toolchain.")
    triple = host_triple()
    folder, espup, espflash, exports = tool_paths()
    checksums = LOCK["platforms"][triple]
    download(f"https://github.com/esp-rs/espup/releases/download/v{LOCK['espup_version']}/espup-{triple}", espup, checksums["espup_sha256"])
    espup.chmod(0o755)
    archive = folder / "espflash.zip"
    download(f"https://github.com/esp-rs/espflash/releases/download/v{LOCK['espflash_version']}/espflash-{triple}.zip", archive, checksums["espflash_sha256"])
    # Read exactly one named member, avoiding arbitrary archive path extraction.
    with zipfile.ZipFile(archive) as zipped:
        espflash.write_bytes(zipped.read("espflash"))
    espflash.chmod(0o755)
    run([espup, "install", "--targets", "esp32", "--toolchain-version", LOCK["xtensa_rust_version"], "--name", LOCK["toolchain"], "--export-file", exports])
    print("Toolchain ready. Build with: python3 scripts/esp32_firmware.py build")


def build_environment():
    folder, espup, espflash, exports = tool_paths()
    if not espflash.is_file() or not exports.is_file():
        raise SystemExit("Run python3 scripts/esp32_firmware.py setup first.")
    rustc = rustup_which(LOCK["toolchain"], "rustc")
    cargo = Path(rustc).parent / "cargo"
    if not cargo.is_file():
        cargo = Path(rustup_which("stable", "cargo"))
    env = os.environ.copy()
    # Explicit paths also work when Homebrew Rust precedes rustup on PATH.
    env["RUSTC"] = rustc
    env["RUSTUP_TOOLCHAIN"] = LOCK["toolchain"]
    return cargo, exports, espflash, env


def firmware_command(arguments):
    cargo, exports, _, env = build_environment()
    # The generated espup file supplies GCC/LLVM paths only in this child shell.
    # Paths and arguments are positional parameters, never interpolated as code.
    run(["bash", "-c", 'source "$1"; shift; exec "$@"', "aerolab-esp32", exports, cargo, *arguments], cwd=FIRMWARE, env=env)


def build():
    firmware_command(["build", "--release", "--locked", "--bin", NAME])
    _, _, espflash, _ = build_environment()
    OUTPUT.mkdir(parents=True, exist_ok=True)
    elf = OUTPUT / f"{NAME}.elf"
    shutil.copyfile(FIRMWARE / "target" / TARGET / "release" / NAME, elf)
    image = OUTPUT / f"{NAME}-merged.bin"
    run([espflash, "save-image", "--chip", "esp32", "--flash-size", "4mb", "--flash-mode", "dio", "--flash-freq", "40mhz", "--merge", "--skip-padding", "--skip-update-check", elf, image])
    run([sys.executable, ROOT / "scripts/verify_esp32_image.py", "--elf", elf, "--image", image, "--manifest", OUTPUT / "manifest.json", "--firmware-kind", "boot-only"])
    with (OUTPUT / "SHA256SUMS").open("w") as checksums:
        for file in [elf, image, OUTPUT / "manifest.json"]:
            checksums.write(f"{digest(file)}  {file.name}\n")
    print(f"Milestone-1 boot-only artifacts: {OUTPUT}")


def build_fixture():
    name = "aerolab-sim-fixture"
    firmware_command(["build", "--release", "--locked", "--features", "simulation-fixture", "--bin", name])
    output = ROOT / "output/firmware/simulation-fixture"
    output.mkdir(parents=True, exist_ok=True)
    elf = output / f"{name}.elf"
    shutil.copyfile(FIRMWARE / "target" / TARGET / "release" / name, elf)
    _, _, espflash, _ = build_environment()
    run([espflash, "save-image", "--chip", "esp32", "--flash-size", "4mb", "--flash-mode", "dio", "--flash-freq", "40mhz", "--skip-update-check", elf, output / f"{name}.bin"])
    print(f"SIMULATION ONLY peripheral fixture (not flight firmware): {output}")


def host_check(action="test"):
    cargo = rustup_which("stable", "cargo")
    rustc = rustup_which("stable", "rustc")
    host = next(line.split(": ", 1)[1] for line in subprocess.check_output([rustc, "-vV"], text=True).splitlines() if line.startswith("host: "))
    env = os.environ.copy()
    env["RUSTC"] = rustc
    # Root cwd intentionally avoids the firmware's Xtensa build-std config.
    arguments = [cargo, action, "--manifest-path", FIRMWARE / "Cargo.toml", "--no-default-features", "--lib", "--tests", "--target", host, "--locked"]
    if action == "clippy":
        arguments += ["--", "-D", "warnings"]
    run(arguments, cwd=ROOT, env=env)


def flash(port):
    elf = OUTPUT / f"{NAME}.elf"
    image = OUTPUT / f"{NAME}-merged.bin"
    if not elf.is_file() or not image.is_file():
        raise SystemExit("Build the milestone-1 artifacts first.")
    run([sys.executable, ROOT / "scripts/verify_esp32_image.py", "--elf", elf, "--image", image])
    _, _, espflash, _ = build_environment()
    # Retain espflash's chip/revision compatibility checks and flash verification.
    run([espflash, "flash", "--chip", "esp32", "--port", port, "--flash-size", "4mb", "--flash-mode", "dio", "--flash-freq", "40mhz", "--monitor", "--monitor-baud", "115200", "--skip-update-check", elf])



def init_profile(path):
    """Create a private, uncalibrated physical bench profile, never a shared key."""
    path = path.resolve()
    path.parent.mkdir(parents=True, exist_ok=True)
    profile = dict(version=1, mode="bench", psk_hex=secrets.token_hex(32),
        max_pwm_us=1150, esc_hz=50, battery_calibration=None,
        wifi_ssid="Aerolab-ESP32", wifi_password=secrets.token_urlsafe(18))
    # Exclusive creation prevents silently changing the key of an existing board.
    with open(path, "x", opener=lambda name, flags: os.open(name, flags, 0o600)) as file:
        json.dump(profile, file, indent=2)
        file.write("\n")
    print(f"Created private profile: {path} (battery calibration missing; arming inhibited)")


def build_bench(profile_path, emulated=False, wifi=False, faults=False):
    from verify_esp32_image import verify, watchdog_iram_address
    profile_path = (FIRMWARE / "profiles/emulated.json" if emulated else profile_path).resolve()
    profile = json.loads(profile_path.read_text())
    if emulated != (profile.get("mode") == "emulated"):
        raise SystemExit("Use --emulated for the simulation profile; it is not flashable.")
    if wifi and emulated:
        raise SystemExit("The emulator does not model the Wi-Fi RF hardware.")
    suffix = "-emulated" if emulated else "-wifi" if wifi else ""
    output = ROOT / ("output/firmware/flight-bench" + suffix)
    features = ["bench-wifi" if wifi else "bench-firmware"]
    if faults:
        features.append("bench-fault-injection")
    previous = os.environ.get("AEROLAB_BENCH_PROFILE")
    os.environ["AEROLAB_BENCH_PROFILE"] = str(profile_path)
    name = "aerolab-flight-bench"
    try:
        firmware_command(["build", "--release", "--locked", "--features", ",".join(features), "--bin", name])
    finally:
        if previous is None:
            os.environ.pop("AEROLAB_BENCH_PROFILE", None)
        else:
            os.environ["AEROLAB_BENCH_PROFILE"] = previous
    output.mkdir(parents=True, exist_ok=True)
    elf = output / f"{name}.elf"
    shutil.copyfile(FIRMWARE / "target" / TARGET / "release" / name, elf)
    _, _, espflash, _ = build_environment()
    common = [espflash, "save-image", "--chip", "esp32", "--flash-size", "4mb", "--flash-mode", "dio", "--flash-freq", "40mhz", "--skip-update-check"]
    image = output / f"{name}-merged.bin"
    app = output / f"{name}.bin"
    run([*common, "--merge", "--skip-padding", elf, image])
    run([*common, elf, app])
    manifest = verify(elf.read_bytes(), image.read_bytes())
    watchdog_address = watchdog_iram_address(elf.read_bytes())
    manifest.update(milestone=2, mode=profile["mode"], transport="wifi_udp_and_uart" if wifi else "uart",
        flight_qualified=False, physical_bench_validated=False, fault_injection=faults,
        watchdog_handler_address=hex(watchdog_address), watchdog_in_iram=True,
        loop_hz=500, esc_hz=profile["esc_hz"], max_pwm_us=profile["max_pwm_us"],
        battery_calibrated=profile.get("battery_calibration") is not None,
        profile_sha256=digest(profile_path),
        motor_policy="GPIO32 hardware gate required; disarmed gate LOW; armed LEDC signals on GPIO25/26/27/14",
        motor_policy_source="firmware/esp32-devkitc/src/bench.rs; hardware measurement pending",
        sha256={file.name:digest(file) for file in [elf,image,app]})
    (output / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n")
    with (output / "SHA256SUMS").open("w") as checksums:
        for file in [elf,image,app,output / "manifest.json"]:
            checksums.write(f"{digest(file)}  {file.name}\n")
    print(f"{'SIMULATION ONLY' if emulated else 'Physical bench — NOT flight qualified'} artifacts: {output}")


def flash_bench(port, folder, allow_faults=False, experimental=False):
    from verify_esp32_image import verify
    folder = folder.resolve()
    manifest = json.loads((folder / "manifest.json").read_text())
    if manifest.get("milestone") != 2 or manifest.get("mode") not in ["bench", "experimental_flight"]:
        raise SystemExit("Refusing to flash an emulated, unknown or non-bench image.")
    if manifest.get("fault_injection") and not allow_faults:
        raise SystemExit("Fault-injection image: use --allow-fault-injection only on a propellers-removed bench.")
    if manifest["mode"] == "experimental_flight" and not experimental:
        raise SystemExit("Full-range, unqualified image: requires --experimental-flight and completed physical qualification.")
    elf = folder / "aerolab-flight-bench.elf"
    image = folder / "aerolab-flight-bench-merged.bin"
    for file in [elf, image]:
        if digest(file) != manifest["sha256"][file.name]:
            raise SystemExit("Artifact hash differs from its build manifest; rebuild before flashing.")
    verify(elf.read_bytes(), image.read_bytes())
    _, _, espflash, _ = build_environment()
    run([espflash, "flash", "--chip", "esp32", "--port", port, "--flash-size", "4mb", "--flash-mode", "dio", "--flash-freq", "40mhz", "--monitor", "--monitor-baud", "115200", "--skip-update-check", elf])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ["setup", "build", "build-fixture", "test", "lint"]:
        commands.add_parser(name)
    initial = commands.add_parser("init-profile", help="Generate private command/WPA2 keys; ADC remains uncalibrated")
    initial.add_argument("--output", type=Path, default=ROOT / "output/firmware-config/device.json")
    bench = commands.add_parser("build-bench", help="Build actual MPU6050 / ESC / watchdog firmware")
    choice = bench.add_mutually_exclusive_group(required=True)
    choice.add_argument("--profile", type=Path)
    choice.add_argument("--emulated", action="store_true")
    bench.add_argument("--wifi", action="store_true")
    bench.add_argument("--fault-injection", action="store_true")
    flash_parser = commands.add_parser("flash", help="Write the boot-only image to the named port")
    flash_parser.add_argument("--port", required=True)
    physical = commands.add_parser("flash-bench", help="Write a physical bench image to the named port")
    physical.add_argument("--port", required=True)
    physical.add_argument("--artifacts", type=Path, default=ROOT / "output/firmware/flight-bench")
    physical.add_argument("--props-removed", action="store_true", required=True)
    physical.add_argument("--allow-fault-injection", action="store_true")
    physical.add_argument("--experimental-flight", action="store_true")
    args = parser.parse_args()
    if args.command == "setup": setup()
    elif args.command == "build": build()
    elif args.command == "test": host_check()
    elif args.command == "lint": host_check("clippy")
    elif args.command == "build-fixture": build_fixture()
    elif args.command == "init-profile": init_profile(args.output)
    elif args.command == "build-bench": build_bench(args.profile, args.emulated, args.wifi, args.fault_injection)
    elif args.command == "flash-bench": flash_bench(args.port, args.artifacts, args.allow_fault_injection, args.experimental_flight)
    else: flash(args.port)


if __name__ == "__main__":
    main()
