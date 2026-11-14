#!/usr/bin/env python3
"""Offline ESP32 image integrity and linked bench-watchdog placement checks.

Image format: https://docs.espressif.com/projects/esptool/en/latest/esp32/advanced-topics/firmware-image-format.html
Checksums detect corruption; they do not provide secure-boot authentication.
"""
import argparse
import hashlib
import json
from pathlib import Path
import struct

FLASH_BYTES = 4 * 1024 * 1024
BOOTLOADER_OFFSET = 0x1000
PARTITION_OFFSET = 0x8000
APP_OFFSET = 0x10000


def require(condition, message):
    if not condition:
        raise ValueError(message)


def watchdog_iram_address(elf):
    """Check the linked bench ISR, independently of its source attributes.

    This is a build-time ELF symbol check, not an emulator firmware hook.
    It establishes placement only; physical response still needs measurement.
    """
    require(len(elf) >= 52, "Truncated ELF header")
    start = struct.unpack_from("<I", elf, 32)[0]
    width, count = struct.unpack_from("<HH", elf, 46)
    require(width == 40 and count > 0 and start + width * count <= len(elf), "Invalid ELF section table")
    sections = [struct.unpack_from("<10I", elf, start + i * width) for i in range(count)]
    addresses = []
    for section in sections:
        if section[1] != 2:  # SHT_SYMTAB
            continue
        require(section[6] < count and section[9] == 16, "Invalid ELF symbol table")
        names = sections[section[6]]
        require(names[4] + names[5] <= len(elf) and section[4] + section[5] <= len(elf), "Truncated ELF symbols")
        strings = elf[names[4]:names[4] + names[5]]
        for offset in range(section[4], section[4] + section[5], 16):
            name, address = struct.unpack_from("<II", elf, offset)
            require(name < len(strings), "Invalid ELF symbol name")
            if b"__esp_hal_internal_motor_watchdog" in strings[name:].split(b"\0", 1)[0]:
                addresses.append(address)
    require(len(addresses) == 1, "Expected one linked motor-watchdog ISR")
    require(0x40080000 <= addresses[0] < 0x400A0000, "Motor-watchdog ISR must be in IRAM, not flash")
    return addresses[0]


def parse_image(data, offset):
    require(offset + 24 <= len(data), "Truncated ESP image header")
    header = data[offset:offset + 24]
    require(header[0] == 0xE9, f"ESP image magic missing at {offset:#x}")
    require(1 <= header[1] <= 16, "Invalid ESP segment count")
    require(struct.unpack_from("<H", header, 12)[0] == 0, "Image targets another ESP chip, not classic ESP32")
    require(header[2] == 2 and header[3] == 0x20, "Expected DIO / 4 MB / 40 MHz image settings")
    cursor = offset + 24
    checksum = 0xEF
    segments = []
    for _ in range(header[1]):
        require(cursor + 8 <= len(data), "Truncated segment header")
        address, size = struct.unpack_from("<II", data, cursor)
        cursor += 8
        require(cursor + size <= len(data), "Truncated segment payload")
        payload = data[cursor:cursor + size]
        for byte in payload:
            checksum ^= byte
        segments.append({"address": address, "size": size, "payload": payload})
        cursor += size
    checksum_at = offset + ((cursor - offset) // 16 + 1) * 16 - 1
    require(checksum_at < len(data), "Missing segment checksum")
    require(data[checksum_at] == checksum, "ESP segment checksum mismatch")
    end = checksum_at + 1
    require(header[23] == 1, "Expected an appended SHA-256 image digest")
    require(end + 32 <= len(data), "Missing appended SHA-256 digest")
    require(hashlib.sha256(data[offset:end]).digest() == data[end:end + 32], "ESP image SHA-256 mismatch")
    return {"entry": struct.unpack_from("<I", header, 4)[0], "size": end + 32 - offset, "segments": segments}


def verify(elf_data, image):
    require(len(elf_data) >= 52 and elf_data[:6] == b"\x7fELF\x01\x01", "Expected a 32-bit little-endian ELF")
    require(struct.unpack_from("<H", elf_data, 16)[0] == 2, "ELF is not an executable")
    require(struct.unpack_from("<H", elf_data, 18)[0] == 94, "ELF is not Xtensa")
    require(APP_OFFSET < len(image) <= FLASH_BYTES, "Merged image does not fit the 4 MB target")
    require(image[:BOOTLOADER_OFFSET] == b"\xff" * BOOTLOADER_OFFSET, "Merged image must be flashed at offset 0x0")
    boot = parse_image(image, BOOTLOADER_OFFSET)
    require(BOOTLOADER_OFFSET + boot["size"] <= PARTITION_OFFSET, "Bootloader overlaps partition table")
    entries = []
    for cursor in range(PARTITION_OFFSET, PARTITION_OFFSET + 0x1000, 32):
        require(cursor + 32 <= len(image), "Truncated partition table")
        entry = image[cursor:cursor + 32]
        magic = struct.unpack_from("<H", entry)[0]
        if magic == 0xEBEB:
            require(hashlib.md5(image[PARTITION_OFFSET:cursor]).digest() == entry[16:32], "Partition table MD5 mismatch")
            break
        require(magic == 0x50AA, "Invalid partition entry or missing partition checksum")
        _, kind, subtype, address, size, name, _ = struct.unpack("<HBBII16sI", entry)
        require(size > 0 and address + size <= FLASH_BYTES, "Partition exceeds flash capacity")
        require(address % 0x1000 == 0, "Partition is not sector-aligned")
        require(all(address + size <= e["offset"] or address >= e["offset"] + e["size"] for e in entries), "Overlapping partitions")
        entries.append({"name": name.split(b"\x00")[0].decode("ascii"), "type": kind, "subtype": subtype, "offset": address, "size": size})
    else:
        raise ValueError("Partition checksum entry missing")
    apps = [entry for entry in entries if entry["type"] == 0]
    require(len(apps) == 1 and apps[0]["subtype"] == 0 and apps[0]["offset"] == APP_OFFSET, "Expected a factory application at 0x10000")
    app = parse_image(image, APP_OFFSET)
    require(app["size"] <= apps[0]["size"], "Application exceeds its partition")
    require(app["entry"] == struct.unpack_from("<I", elf_data, 24)[0], "ELF and packaged application entry points differ")
    descriptor = struct.pack("<I", 0xABCD5432)
    require(any(segment["payload"].startswith(descriptor) for segment in app["segments"]), "ESP application descriptor missing")
    return {
        "board": "ESP32-DevKitC-32E",
        "chip": "esp32",
        "target": "xtensa-esp32-none-elf",
                "flash_bytes": FLASH_BYTES,
        "merged_flash_offset": "0x0",
        "bootloader_offset": hex(BOOTLOADER_OFFSET),
        "application_offset": hex(APP_OFFSET),
        "application_bytes": app["size"],
        "partitions": entries,
        "validation": ["Xtensa ELF", "ESP32 image headers", "segment checksums", "image SHA-256", "partition MD5", "matching application entry"],
        "hardware_boot_test_included": False,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--elf", type=Path, required=True)
    parser.add_argument("--image", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--firmware-kind", choices=["unknown", "boot-only"], default="unknown")
    args = parser.parse_args()
    try:
        result = verify(args.elf.read_bytes(), args.image.read_bytes())
    except (ValueError, OSError, struct.error) as error:
        parser.exit(1, f"Firmware image verification failed: {error}\n")
    # Image checks establish structural integrity, not motor behavior.
    if args.firmware_kind == "boot-only":
        result.update(milestone=1, mode="boot_only_motor_inhibited",
            motor_policy="GPIO25/26/27/14 static LOW after application initialization; no PWM",
            motor_policy_source="firmware/esp32-devkitc/src/main.rs; build-supplied label, not inferred from image bytes")
    if args.manifest:
        args.manifest.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
