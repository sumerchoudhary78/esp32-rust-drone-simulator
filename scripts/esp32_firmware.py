#!/usr/bin/env python3
"""Pinned tools and build/flash commands for the single milestone-1 board.

Default operations never access a serial port. Only `flash --port ...` writes
hardware. espup installs a dedicated toolchain without replacing stable or esp.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
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
    run([sys.executable, ROOT / "scripts/verify_esp32_image.py", "--elf", elf, "--image", image, "--manifest", OUTPUT / "manifest.json"])
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


def host_tests():
    cargo = rustup_which("stable", "cargo")
    rustc = rustup_which("stable", "rustc")
    host = next(line.split(": ", 1)[1] for line in subprocess.check_output([rustc, "-vV"], text=True).splitlines() if line.startswith("host: "))
    env = os.environ.copy()
    env["RUSTC"] = rustc
    # Root cwd intentionally avoids the firmware's Xtensa build-std config.
    run([cargo, "test", "--manifest-path", FIRMWARE / "Cargo.toml", "--no-default-features", "--lib", "--tests", "--target", host, "--locked"], cwd=ROOT, env=env)


def flash(port):
    elf = OUTPUT / f"{NAME}.elf"
    image = OUTPUT / f"{NAME}-merged.bin"
    if not elf.is_file() or not image.is_file():
        raise SystemExit("Build the milestone-1 artifacts first.")
    run([sys.executable, ROOT / "scripts/verify_esp32_image.py", "--elf", elf, "--image", image])
    _, _, espflash, _ = build_environment()
    # Retain espflash's chip/revision compatibility checks and flash verification.
    run([espflash, "flash", "--chip", "esp32", "--port", port, "--flash-size", "4mb", "--flash-mode", "dio", "--flash-freq", "40mhz", "--monitor", "--monitor-baud", "115200", "--skip-update-check", elf])


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    for name in ["setup", "build", "build-fixture", "test"]:
        commands.add_parser(name)
    flash_parser = commands.add_parser("flash", help="Write the boot-only firmware to the explicitly named board port")
    flash_parser.add_argument("--port", required=True)
    args = parser.parse_args()
    if args.command == "setup":
        setup()
    elif args.command == "build":
        build()
    elif args.command == "test":
        host_tests()
    elif args.command == "build-fixture":
        build_fixture()
    else:
        flash(args.port)


if __name__ == "__main__":
    main()
