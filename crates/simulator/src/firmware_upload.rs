use crate::emulator::{Image, flash_tool_path};
use sha2::{Digest, Sha256};
use std::{path::PathBuf, process::Command};

/// Validate identity before passing an image to the optional native emulator.
/// This is a format check, not a native-emulator security sandbox.
pub fn prepare(id: String, name: String, bytes: Vec<u8>) -> Result<Image, String> {
    if name.is_empty()
        || name.len() > 120
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-_. ".contains(c))
    {
        return Err("Use a plain .elf or .bin filename (120 characters maximum)".into());
    }
    if bytes.len() < 24 || bytes.len() > 16 * 1024 * 1024 {
        return Err("Firmware must be 24 bytes–16 MiB".into());
    }
    let folder = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../output/firmware-uploads");
    std::fs::create_dir_all(&folder).map_err(|e| e.to_string())?;
    let bin = folder.join(format!("{id}.bin"));
    let elf = if name.to_ascii_lowercase().ends_with(".elf") {
        if bytes.len() < 52
            || &bytes[..7] != b"\x7fELF\x01\x01\x01"
            || u16::from_le_bytes([bytes[18], bytes[19]]) != 94
        {
            return Err("Expected a 32-bit little-endian Xtensa ESP32 ELF; RISC-V / S3 images are unsupported".into());
        }
        if !flash_tool_path().is_file() {
            return Err("ELF conversion needs espflash: run python3 scripts/setup_emulator.py (or upload an ESP32 .bin)".into());
        }
        let elf = folder.join(format!("{id}.elf"));
        std::fs::write(&elf, bytes).map_err(|e| e.to_string())?;
        let output = Command::new(flash_tool_path())
            .args([
                "save-image",
                "--chip",
                "esp32",
                "--flash-size",
                "4mb",
                "--flash-mode",
                "dio",
                "--flash-freq",
                "40mhz",
                "--skip-update-check",
            ])
            .arg(&elf)
            .arg(&bin)
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            let _ = std::fs::remove_file(&elf);
            let _ = std::fs::remove_file(&bin);
            return Err(
                "espflash rejected the ELF; build for xtensa-esp32-none-elf / classic ESP32, 4 MiB"
                    .into(),
            );
        }
        Some(elf)
    } else if name.to_ascii_lowercase().ends_with(".bin") {
        validate_bin(&bytes)?;
        std::fs::write(&bin, bytes).map_err(|e| e.to_string())?;
        None
    } else {
        return Err("Upload a compiled ESP32 .elf or .bin file".into());
    };
    Ok(Image { id, name, bin, elf })
}
pub fn validate_bin(bytes: &[u8]) -> Result<(), String> {
    if bytes.len() > 4 * 1024 * 1024 {
        return Err("Selected virtual ESP32 has 4 MiB flash".into());
    }
    let image = if bytes.first() == Some(&0xe9) {
        bytes
    } else if bytes.len() > 0x10018
        && bytes[..0x1000].iter().all(|b| *b == 0xff)
        && bytes[0x1000] == 0xe9
    {
        &bytes[0x10000..]
    } else {
        return Err(
            "Expected an ESP32 app image, or a merged image with factory app at 0x10000".into(),
        );
    };
    if image.len() < 24
        || image[0] != 0xe9
        || image[1] == 0
        || image[1] > 16
        || image[12..14] != [0, 0]
    {
        return Err("Firmware header does not identify a classic ESP32 image".into());
    }
    if image.len() > 4 * 1024 * 1024 {
        return Err("Selected virtual ESP32 has 4 MiB flash".into());
    }
    let mut offset = 24usize;
    let mut checksum = 0xef;
    for _ in 0..image[1] {
        let header = image
            .get(offset..offset + 8)
            .ok_or("Truncated ESP32 segment header")?;
        let size = u32::from_le_bytes(header[4..8].try_into().unwrap()) as usize;
        let begin = offset + 8;
        offset = offset
            .checked_add(8)
            .and_then(|v| v.checked_add(size))
            .filter(|v| *v <= image.len())
            .ok_or("Truncated ESP32 segment data")?;
        for b in &image[begin..offset] {
            checksum ^= b;
        }
    }
    let checksum_at = (offset / 16 + 1) * 16 - 1;
    if checksum_at >= image.len() {
        return Err("Missing ESP32 image checksum".into());
    }
    if image[checksum_at] != checksum {
        return Err("ESP32 segment checksum mismatch".into());
    }
    if image[23] == 1 {
        let end = checksum_at + 1;
        if image.get(end..end + 32) != Some(Sha256::digest(&image[..end]).as_slice()) {
            return Err("ESP32 appended SHA-256 mismatch".into());
        }
    }
    Ok(())
}
