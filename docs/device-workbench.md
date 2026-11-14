# ESP32 drone device workbench

Assemble a quadcopter's electronics and run either the built-in Rust controller
or a compiled classic-ESP32 program against the connected devices. Open
**Virtual hardware** in the local Aerolab app.

## Build a circuit

The **F450 reference build** has 13 devices: ESP32-DevKitC-32E, MPU6050,
four ESCs, four A2212 motor/propeller models, a 3S battery, a 5 V regulator,
and a battery voltage divider. **Empty workspace** lets you add them yourself.

1. Add devices from the palette. ESC/motor positions are M1 front left (CW),
   M2 front right (CCW), M3 rear right (CW), M4 rear left (CCW), viewed above.
2. Drag device bodies to arrange them. **View full size** makes pin labels
   easier to read; the canvas scrolls horizontally on smaller screens.
3. Click a pin and then another pin to connect them. Keyboard: focus a pin and
   press Enter. Escape cancels a pending connection. Crossings do not join nets.
4. Click/focus a wire to remove it. Undo restores the previous edit. Selecting
   a device opens its model description and removal control.
5. Correct the connectivity errors, then **Apply circuit**. Applying stops
   motors, stops any guest process, and restarts calibration when using the built-in controller.
   Draft edits take effect only when applied.
6. Export/import version-1 JSON to share a circuit. Edits also save a local
   browser draft; **Restore saved draft** retrieves it without starting motors.

The validator resolves undirected nets and common board grounds. It rejects
power-rail shorts, GPIO-to-rail connections, tied GPIOs, phase shorts,
unknown pins, duplicate parts/positions, invalid coordinates and excessive
devices/wires. Incomplete, valid circuits can be applied for device experiments;
they cannot arm the built-in flight controller. Disconnected devices do not
quietly fall back to fake sensor data or motor drive.

## Reference connections

| Source                            | Destination                                                 |
| --------------------------------- | ----------------------------------------------------------- |
| Battery +                         | Regulator IN, four ESC BAT+, divider IN                     |
| Battery −                         | Regulator GND, board GND, MPU GND/AD0, ESC GND, divider GND |
| Regulator OUT5V                   | ESP32 5V                                                    |
| ESP32 3V3                         | MPU6050 VCC                                                 |
| ESP32 GPIO21 / GPIO22             | MPU SDA / SCL                                               |
| GPIO25 / GPIO26 / GPIO27 / GPIO14 | ESC1 / ESC2 / ESC3 / ESC4 SIGNAL                            |
| Each ESC A / B / C                | Its corresponding motor A / B / C                           |
| Divider OUT                       | GPIO34 (ADC1 channel 6)                                     |

These are **functional simulation connections**. The sensor model includes
3.3 V pull-ups; it is not a claim about the regulators/pull-ups on an arbitrary
MPU6050 breakout. The divider is 100 kΩ high side / 27 kΩ low side. The virtual
ADC uses `Vbattery × 27/127 / 3.3 × 4095`; physical ESP32 ADCs need attenuation,
calibration and a characterized input circuit. The fixed propeller model
requires A→A/B→B/C→C; phase swapping/reversible propellers are not simulated.

## Run your own ESP32 firmware

The built-in controller is host Rust software-in-the-loop. The optional
[Flexe](https://github.com/levkropp/flexe) backend executes Xtensa instructions
from a compiled ESP32 image in a separate process. It is not a visual stand-in
for firmware execution.

Requirements: Python 3.11+, C17 compiler, CMake, OpenSSL and zlib development
files, Rust for the server; macOS or glibc Linux on ARM64/x86-64.

```sh
# macOS prerequisites, if needed
brew install cmake openssl@3
# Debian/Ubuntu prerequisites, if needed
sudo apt-get install build-essential cmake libssl-dev zlib1g-dev python3

python3 scripts/setup_emulator.py
cargo run --release --locked
```

The script verifies a pinned Flexe source archive SHA-256, builds the adapter,
and installs the pinned prebuilt espflash packager for ELF conversion. Downloads
and build products stay under ignored `output/`. It never opens a serial port
or installs the ESP32 compiler. To build our Rust guest examples as well, follow
the [firmware setup](../firmware/esp32-devkitc/README.md).

1. Apply a circuit with a powered ESP32. A sensor-only assembly is allowed.
2. On the computer running Aerolab, choose a classic-ESP32 **`.elf`** or **`.bin`**.
   ELF is packaged as an app image by espflash. Binary images may be app-only
   or merged with a factory app at `0x10000`. The selected board has 4 MiB flash.
   RISC-V targets and ESP32-S2/S3 firmware are outside this adapter's scope.
   Xtensa ELF machine identification alone cannot prove a matching ESP32 variant.
3. Select **Start firmware**. The built-in flight helpers and arming controls
   are disabled. Your firmware owns its arming, stabilization and motor commands.
4. Inspect UART output, CPU cycles, guest time, MPU transfers, PWM and motor RPM
   estimates. Send your firmware's serial protocol through **UART0 input**.
   The app does not translate its joysticks into an arbitrary guest protocol.
5. Stop firmware, emergency-stop, or **Use built-in controller** to leave it.
   Starting again always creates a fresh CPU session.

For an included peripheral example:

```sh
python3 scripts/esp32_firmware.py setup
python3 scripts/esp32_firmware.py build-fixture
```

Upload `output/firmware/simulation-fixture/aerolab-sim-fixture.elf`. It uses
esp-hal's actual I²C, LEDC and ADC drivers: WHO_AM_I must be `0x68`, a stationary
accelerometer Z sample is near `16384`, and the battery ADC changes with voltage.
UART `0`, `5`, `7` select 1000, 1500, 1700 µs for all four motors. UART `r` selects ±16 g / ±2000°/s for range experiments. The buttons
below the console send these bytes. **This fixture intentionally has no flight
arming/stabilization; it is for simulation only.** It is separate from the
motor-inhibited milestone-1 boot firmware, and the flash helper cannot select it.

## Supported device behavior and limits

| Device/boundary        | Implemented                                                                                                              | Limits                                                                                                                |
| ---------------------- | ------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| ESP32                  | Classic LX6 instruction execution, GPIO matrix, ECO3 identity, 4 MiB image, application entry startup                    | Single guest core; modeled ROM calls; ROM/second-stage bootloader is not executed                                     |
| MPU6050                | I²C0/1, address 0x68/0x69, pointer/write/read, WHO_AM_I, sleep/reset, selectable ranges, quantized accel/gyro/temp burst | No FIFO/DMP, interrupt wire behavior, analog filtering or exact bus/sample timing                                     |
| ESC                    | GPIO-routed 40–500 Hz LEDC PWM, valid 1000–2000 µs pulse decode, wiring gates                                            | No DShot, ESC firmware, calibration handshake, commutation or switching electronics                                   |
| Other output protocols | Motor drive inhibited                                                                                                    | MCPWM, bit-banged PWM, RMT and DShot need separate measured/verified adapters; use LEDC for the current firmware path |
| Motors/propellers      | Quadratic thrust, fixed spin/order, 45 ms lag, rigid-body motion, zero drive at exhausted modeled charge                 | Illustrative A2212/F450 parameters; no measured thrust/current curve                                                  |
| Battery / regulator    | Charge, load sag, divider, ideal supplies, common-ground/power validation                                                | No thermal, charging, transient, brownout or protection-circuit model                                                 |
| ADC1                   | Connected divider selects ADC1 channel; injected 12-bit values read by guest HAL                                         | Ideal voltage conversion; no real ESP32 ADC nonlinearity/calibration                                                  |
| UART0                  | Bounded TX console and RX input                                                                                          | Serial protocol belongs to guest firmware                                                                             |
| Networking             | Existing host joystick link in built-in mode                                                                             | No guest Wi-Fi/RF bridge; ELF function hooks for host sockets/filesystems are deliberately not loaded                 |

The firmware and plant advance together in 2 ms simulation steps. Host speed
changes the time needed to complete a run. This is functional co-simulation;
guest cycles and peripheral events do not provide cycle-accurate hardware timing
or a real-time qualification. IMU ranges/sleep/reset affect guest register reads.
Wiring and GPIO routing affect whether I²C can ACK and which ESC receives PWM.

The host stops the guest on disconnect, ownership transfer, emergency stop,
500 ms host command-heartbeat expiry, backend timeout/halt/reset, unsupported
MMIO or unmapped memory accesses. Backend output older than 100 ms cannot drive
motors. Those host guards protect this virtual session; they are **not an
independent watchdog in aircraft firmware**.

Firmware is a user-selected native-emulator workload. Uploads are limited to
localhost, 16 MiB each and 16 accepted images per server session. Names cannot
choose host paths; image headers/bounds/checksums are checked. ELF conversion and
CPU execution never use a shell. Use trusted firmware locally: process separation
is not a security sandbox for hostile images. Uploaded files remain in
`output/firmware-uploads/`; restarting clears the in-memory list. Remove that
directory to delete saved uploads. Keep any LAN controller access on a trusted
network.

## Verify the path

```sh
cargo test --workspace --locked
python3 scripts/esp32_firmware.py build
python3 scripts/esp32_firmware.py build-fixture
python3 scripts/esp32_firmware.py build-bench --emulated --fault-injection
python3 -m unittest discover -s tests -p '*_test.py' -v
npm ci
AEROLAB_REQUIRE_EMULATOR=1 npx playwright test tests/browser/workbench.spec.mjs
```

The guest tests execute compiled binaries, check the boot-only GPIO/PWM policy,
read MPU/ADC data, change motor pulses with UART, and check wrong pins, wrong
I²C address and sensor power loss. Browser checks assemble all 13 devices from
empty, wire them, reject a power short, export a circuit and run uploaded ELF
through to the motor model. They do not establish physical flight readiness.


## Actual flight-controller binary

The [bench controller](physical-bench.md) now runs on both virtual ESP32 cores
with real MPU6050/HAL, ADC, calibration, fusion/PID, LEDC and authenticated UART.
`tests/flight_bench_test.py` supplies GPIO13 arming and signed AERO v2 frames to
the native backend and observes GPIO32 plus the motor pads. It also stalls Core 1
to verify TIMG1 interrupt-driven pad inhibition while Core 0 continues.

The adapter models classic-ESP32 DPORT boot/cache handoff and TIMG T0/T1
interrupt/prescaler quirks needed by esp-hal. Core 1 executes actual instructions;
there are no ELF symbol hooks or host substitutes for control/watchdog decisions.
The classic MWDT reset fallback and RF remain physical qualification items.
The browser workbench currently provides neither a GPIO13 switch control nor an
AERO v2 packet generator; use the native test harness for this binary. The older
SIM-only fixture remains the interactive UART `0/5/7` workbench example.
