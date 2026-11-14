# ESP32 firmware: current milestone and bench plan

The virtual setup is complete without an ESP32 toolchain. `flight-core` is
`#![no_std]`, uses no allocation, and receives SI sensor samples and locally
timestamped commands. It outputs normalized motor commands and a pulse-width
representation. The host simulator and optional CPU/device backend supply virtual device behavior.

The [milestone-1 firmware](../firmware/esp32-devkitc/README.md) now targets
**ESP32-DevKitC-32E / WROOM-32E, ECO3, 4 MiB** with pinned tools and a flashable
boot-only image. Motor pins remain LOW after initialization; sensor/PWM/network
flight drivers are not enabled. Cross-build, offline image checks and emulated
application startup pass; physical flash/boot and pad timing remain unverified.
The [device workbench](device-workbench.md) executes an additional simulation-only
HAL peripheral example against virtual MPU/ADC/ESC devices. These are separate
programs and separate evidence from a physical flight-controller implementation.

Sharing the controller is useful; copying host scheduling or the virtual height
helper into an aircraft would be incorrect.

## Proposed interfaces

At each 2 ms deadline:

1. Read all 14 MPU6050 bytes beginning at `0x3B`. Configure the physical chip
   for the ranges assumed by `decode_mpu6050`: ±2 g and ±250°/s. Verify WHO_AM_I,
   sample-rate and digital filter settings on the exact module. An I²C failure
   supplies `None`, never an old cached sample.
2. Rotate sensor axes to body **X forward, Y left, Z up**. Verify signs by hand
   with every propeller removed. Units: rad/s and specific force m/s².
3. Read verified ADC battery voltage. Packet timestamps must use an ESP32
   monotonic clock, not phone time. Accept a newest validated command from a
   bounded mailbox. Reject duplicates and expire old samples.
4. Call `FlightController::step`. Wire emergency stop/deadlines to a separate
   motor inhibit that also works if the controller task hangs.
5. Apply outputs through a tested ESC peripheral driver. The model's
   1000–2000 µs convention does not prove a real ESC's update-rate support,
   calibration, idle behavior or stop response. Determine those on the actual
   ESCs with a scope/logic analyzer and propellers removed.

The network task parses packets outside the flight task and must never perform
blocking Wi-Fi, heap allocation or I/O in the stabilization deadline. The host's
JSON protocol is a development interface; define authentication, ownership,
packet format and a hard watchdog for the hardware link separately.

## Tentative classic ESP32 DevKit pin plan

| Signal                      | Proposed pin                    | Constraint                                                                                        |
| --------------------------- | ------------------------------- | ------------------------------------------------------------------------------------------------- |
| MPU6050 SDA / SCL           | GPIO 21 / 22                    | Verified 3.3 V logic and pull-ups; module regulators vary                                         |
| Motor 1 · front left · CW   | GPIO 25                         | Output peripheral and exact board must be verified                                                |
| Motor 2 · front right · CCW | GPIO 26                         | Output peripheral and exact board must be verified                                                |
| Motor 3 · rear right · CW   | GPIO 27                         | Output peripheral and exact board must be verified                                                |
| Motor 4 · rear left · CCW   | GPIO 14                         | Output peripheral and exact board must be verified                                                |
| Battery sensing             | GPIO34 planned (ADC1 channel 6) | Virtual 100k / 27k divider only; physical divider/filtering and ADC calibration remain unverified |

Spins are viewed from above. Confirm the matching propeller handedness before
any powered test. The visual front arms are orange, the rear arms white.

Power is battery → distribution → ESC power inputs, with an appropriate regulated
supply to the ESP32 board and a verified sensor rail. Motors never run from an
ESP32 pin. All logic needs a common signal ground. Do not parallel unknown ESC
BEC outputs. The exact board's allowable input rail, MPU module voltage, cable
ratings and regulator capacity still need verification.

## Hardware gates still open

- Verify physical flash/boot on the selected ESP32-DevKitC-32E; the pinned Xtensa toolchain and boot-only image are implemented. Other DevKit variants are not interchangeable.
- Verify the bill of materials with measured motor/prop thrust/current curves,
  ESC/battery ratings, frame clearance, center of mass and actual inertia.
- Measure timing under Wi-Fi traffic: worst-case loop jitter, sensor sample
  age, missed deadlines and watchdog motor inhibit, including brownouts.
- Implement robust sensor-startup checks, motion rejection in calibration,
  vibration filtering, adequate gyro range and actual sensor-bus fault handling.
- Calibrate the physical sensor/ADC/ESCs and retune on a restrained bench. The
  virtual battery and noise are simplified models, not protection electronics.
- Decide and validate an airborne failsafe policy with the sensing needed for
  it. Motor cut, as modeled here, does not guarantee a safe landing.
- Perform propellers-off motor order/sign checks, then controlled restrained
  testing before considering any free flight.

Toolchain reference: [official Rust on ESP book](https://docs.espressif.com/projects/rust/book/).
