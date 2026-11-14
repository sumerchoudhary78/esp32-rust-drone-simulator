# ESP32 firmware status

The hardware adapter now targets **ESP32-DevKitC-32E / WROOM-32E, ECO3, 4 MiB**.
Actual Rust code acquires fresh MPU6050 samples over I²C at ±8 g / ±2000°/s,
calibrates, estimates attitude, runs PID/mixing and drives gated LEDC ESC signals
at a 500 Hz control rate. ADC1 battery calibration, physical arming, authenticated
UART/optional Wi-Fi UDP, command expiry and independent timer/MWDT paths are
implemented. The boot-only target remains available separately.

- [Build, wiring, calibration and propellers-removed bench guide](physical-bench.md)
- [AERO v2 physical command protocol](firmware-protocol.md)
- [Toolchain and bootstrap reference](../firmware/esp32-devkitc/README.md)
- [Physical measurement report template](bench-report-template.md)
- [Automated evidence and limits](verification.md)
- [Vehicle qualification roadmap](../ROADMAP.md)

The actual dual-core UART binary is exercised by the CPU emulator. The Wi-Fi
variant is cross-built; RF and timing under radio traffic are not emulated.
Neither is a physical flight-readiness result. There is no connected board or
completed physical report. Mounting signs, ADC/ESC compatibility, pulse timing,
watchdog reset behavior, supply integrity and gains still require measurements.
The host's height helper and browser/Slint JSON controllers remain simulation
features and are not silently enabled in the physical firmware.
