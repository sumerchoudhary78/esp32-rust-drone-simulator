#!/usr/bin/env python3
"""Build the optional classic-ESP32 CPU backend; no serial devices are opened."""
import argparse
import platform
import shutil
import subprocess
import tarfile
import zipfile
from esp32_firmware import ROOT, LOCK, download, host_triple, tool_paths

COMMIT = "82917d1ac37863a86b8c58f107b0d64fc50fb081"
SHA256 = "1d1acacf3a5843d3ed4159bc49f17b2a851d2550259209472455349a45674e9d"

def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--jobs", type=int, default=4)
    args = parser.parse_args()
    if not 1 <= args.jobs <= 32:
        parser.error("--jobs must be 1–32")
    if not shutil.which("cmake"):
        raise SystemExit("Install CMake, a C17 compiler, OpenSSL development files and zlib development files. See docs/device-workbench.md.")
    output = ROOT / "output"
    archive = output / "flexe-pinned.tar.gz"
    download(f"https://codeload.github.com/levkropp/flexe/tar.gz/{COMMIT}", archive, SHA256)
    sources = output / "emulator-source"
    source = sources / f"flexe-{COMMIT}"
    if not source.is_dir():
        sources.mkdir(parents=True, exist_ok=True)
        with tarfile.open(archive) as package:
            for member in package.getmembers():
                path = sources / member.name
                if not path.resolve().is_relative_to(sources.resolve()) or not (member.isdir() or member.isfile()):
                    raise SystemExit("Unexpected archive member")
                if member.isdir():
                    path.mkdir(parents=True, exist_ok=True)
                else:
                    path.parent.mkdir(parents=True, exist_ok=True)
                    with package.extractfile(member) as src, path.open("wb") as dst:
                        shutil.copyfileobj(src, dst)
    build = output / "emulator-build"
    command = ["cmake", "-S", str(ROOT / "emulation/flexe"), "-B", str(build), f"-DFLEXE_SOURCE={source}"]
    if platform.system() == "Darwin" and shutil.which("brew"):
        openssl = subprocess.check_output(["brew", "--prefix", "openssl@3"], text=True).strip()
        command.append(f"-DOPENSSL_ROOT_DIR={openssl}")
    subprocess.run(command, check=True)
    subprocess.run(["cmake", "--build", str(build), "--target", "aerolab-esp32", "-j", str(args.jobs)], check=True)
    # ELF uploads only need this prebuilt packager, not the Xtensa compiler.
    folder, _, espflash, _ = tool_paths()
    triple = host_triple()
    zipped = folder / "espflash.zip"
    download(f"https://github.com/esp-rs/espflash/releases/download/v{LOCK['espflash_version']}/espflash-{triple}.zip", zipped, LOCK["platforms"][triple]["espflash_sha256"])
    with zipfile.ZipFile(zipped) as package:
        espflash.write_bytes(package.read("espflash"))
    espflash.chmod(0o755)
    (build / "source-manifest.json").write_text(__import__("json").dumps({"repository":"https://github.com/levkropp/flexe", "commit":COMMIT,"archive_sha256":SHA256}, indent=2)+"\n")
    print("CPU backend ready. Restart / run cargo run --release --locked, then open Virtual hardware.")

if __name__ == "__main__":
    main()
