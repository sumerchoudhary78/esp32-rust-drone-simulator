# ESP32-DevKitC-32E Rust firmware

**Target:** Espressif **ESP32-DevKitC-32E**, ESP32-WROOM-32E module, classic
ESP32 ECO3, 4 MiB flash, `xtensa-esp32-none-elf`. A generic ESP32-S3/C3 DevKit is
not this target. Board reference:
[Espressif DevKitC guide](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32/esp32-devkitc/user_guide.html).

Two physical build targets are available:

- **Flight-controller bench:** actual 500 Hz MPU6050/fusion/PID/mixer loop on
  Core 1, ADC1 battery sensing, four gated LEDC outputs, physical arming input,
  calibrated voltage policy, 250 ms authenticated command expiry, independent
  timer inhibition and a hardware MWDT reset fallback. Optional WPA2 AP/UDP and
  UART run on Core 0. See the [complete physical bench guide](../../docs/physical-bench.md)
  and [packet contract](../../docs/firmware-protocol.md).
- **Milestone-1 bootstrap:** retained as the default `build`/`flash` target. It
  prints a heartbeat and holds motor pins LOW without configuring peripherals.

Physical UART/Wi-Fi images cross-build and the real dual-core UART controller
runs against emulated MPU6050, ADC, ESC PWM and GPIOs. **No physical board has
been flashed or measured.** Bench outputs default to a 1150 µs cap and require
an external gate plus measured ADC calibration before arming. Current gains,
ESC compatibility, pad voltages and real-time behavior remain unqualified.

```sh
python3 scripts/esp32_firmware.py init-profile
python3 scripts/esp32_firmware.py build-bench --profile output/firmware-config/device.json
python3 scripts/esp32_firmware.py build-bench --profile output/firmware-config/device.json --wifi
python3 scripts/esp32_firmware.py build-bench --emulated --fault-injection
```

Setup below is common to both. The subsequent `build`/`flash` examples refer to
the motor-inhibited bootstrap; use the linked bench guide for the controller.

## Setup and build

Use Python 3.11+, rustup, the platform C build tools and internet access for the
initial downloads. Helpers support macOS / glibc Linux, ARM64 / x86-64. Run from
the repository root:

```sh
python3 scripts/esp32_firmware.py setup
python3 scripts/esp32_firmware.py test
python3 scripts/esp32_firmware.py build
```

`tools.lock.json` pins espup 0.19.0, espflash 4.7.0 and the Xtensa Rust fork
1.97.0.0. Downloaded espup/espflash assets are checked against recorded SHA-256s.
espup installs an additive toolchain named **aerolab-esp-1.97.0.0**; it does not
replace stable or an existing `esp` toolchain. The generated GCC/LLVM environment
is sourced only inside the helper's child shell. Cargo dependencies are locked.

Build outputs in `output/firmware/esp32-devkitc-m1/`:

- `aerolab-esp32-devkitc.elf` — executable, used by the flash helper.
- `aerolab-esp32-devkitc-merged.bin` — bootloader, partitions and factory app;
  full image offset **0x0**, factory application at **0x10000**.
- `manifest.json` — image properties, integrity checks and explicit test limits.
- `SHA256SUMS` — artifact hashes; corruption detection, not a signature.

The offline verifier checks ELF class/machine/entry, classic-ESP32 image headers,
DIO/4 MiB/40 MHz settings, segment XOR checksums, appended SHA-256, partition
MD5/non-overlap/bounds and the application descriptor. It does not infer the
behavior of arbitrary programs from their bytes.

## Flash and check the desk milestone

Use the named board on **USB power only** for this milestone. Leave ESCs, motors,
battery and sensor disconnected. Identify its serial port, then explicitly run:

```sh
# Example Linux port; replace it with your board's actual port.
python3 scripts/esp32_firmware.py flash --port /dev/ttyUSB0
# macOS often uses a /dev/cu.usbserial-* or /dev/cu.SLAB_USBtoUART port.
```

Only explicit `flash`, `flash-bench` and bench-console transport commands access hardware. Setup, build, tests and emulator
setup do not. The helper rechecks images, retains espflash's chip/revision checks,
flashes the ELF and opens a **115200 baud** monitor. It does not auto-select a
serial device or bypass compatibility checks.

Expected output includes:

```text
AEROLAB firmware 0.2.0 milestone=1 mode=boot_only
board=ESP32-DevKitC-32E chip=esp32 configured_flash_bytes=4194304 cpu_mhz=240 uart_baud=115200
reserved_motor_gpios=[25, 26, 27, 14] signal=static_low pwm=disabled
imu=unavailable battery=unavailable wifi=disabled stabilization=not_running flight_ready=false
heartbeat=1 uptime_ms=... core=Calibrating gpio_output_registers_low=true requested_pwm_us=[1000, 1000, 1000, 1000] flight_ready=false
```

Record the board/module markings, build revision, serial log and whether reset
restarts the heartbeat. Measure the reserved pins through reset/startup before
enabling any real actuator driver. Host tests and emulator output do not replace
this measurement.

## Run the built image virtually

Follow [device workbench setup](../../docs/device-workbench.md), apply the F450
reference circuit, and upload the milestone-1 ELF or merged binary. The UART
heartbeat should appear while all virtual motor PWM peripherals remain off.
The emulator enters the application directly; it does not test the bootloader.

A separate **simulation-only peripheral exerciser** is available:

```sh
python3 scripts/esp32_firmware.py build-fixture
```

`output/firmware/simulation-fixture/aerolab-sim-fixture.elf` uses actual HAL I²C,
ADC and LEDC register accesses against the virtual devices. UART `0/5/7` selects
1000/1500/1700 µs for all motors. UART `r` selects ±16 g / ±2000°/s for range experiments. It intentionally has no flight controller,
arming gate or flight failsafe. It is never selected by the flash helper.

## Physical qualification still required

Follow the [bench measurement sequence](../../docs/physical-bench.md), preserve
raw traces in a [hardware report](../../docs/bench-report-template.md), then
measure and retune the actual vehicle before restrained tests. Firmware and
emulated fault coverage do not establish flight readiness. See the
[staged roadmap](../../ROADMAP.md).
