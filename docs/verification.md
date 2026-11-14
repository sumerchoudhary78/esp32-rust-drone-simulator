# Verification record · 14 November 2026

All results below are local software-in-the-loop evidence. No physical drone,
ESP32 board, sensor, ESC or motor was connected.

| Check                                                                                     | Result                                                                                                |
| ----------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------- |
| `cargo test --workspace`                                                                  | 22 passed: 9 controller tests and 13 plant/scenario tests                                             |
| `npm test`                                                                                | 5 passed against a real local HTTP/WebSocket server                                                   |
| `npm run test:browser`                                                                    | 7 passed in Chrome, including full flight, faults, ownership and 320/390/768/1440 px layouts          |
| `cargo clippy --workspace --all-targets -- -D warnings`                                   | Passed                                                                                                |
| Native controller Clippy with warnings denied                                             | Passed                                                                                                |
| Rust formatting checks                                                                    | Passed                                                                                                |
| `flight-core` cross-check to `wasm32-unknown-unknown` with the installed Rustup toolchain | Passed; allocation-free core builds separately from the host server                                   |
| Postman local collection                                                                  | 3 HTTP requests / 3 assertions passed; no cloud login required for the local results                  |
| Optimized simulator build                                                                 | Passed; self-contained executable with vendored browser assets                                        |
| Android APK                                                                               | ARM64 debug APK packaged, signed with a newly generated local development key, and signature verified |
| Android 11 native end-to-end                                                              | Passed on dedicated `Aerolab_Android_11`, `emulator-5580`                                             |

The native Android flow verified actual Slint rendering, connection to the host
through `10.0.2.2`, ownership, zero-throttle arming, assisted takeoff above 1.4 m,
roll control, centering, watchdog motor cut after actual Android backgrounding,
persistent failsafe on return, and explicit resume/disarm. The final native
worker measured **49.5 commands/s** against its 50 Hz nominal schedule.

Rust scenarios additionally cover natural battery depletion through the critical
voltage threshold, bounded output mixing, stationary calibration interruption,
missing/non-finite sensor input, stale/replayed packets, host timing faults,
delayed commands after emergency stop, and manual throttle/wind behavior.

Generated artifacts (ignored by Git):

- `output/flight-deck-{320,390,768,1440}.png`: responsive browser screenshots.
- `output/android/Aerolab-Controller-debug.apk`: installable development app.
- `output/android/controller-hover.png`: native flight screenshot.
- `output/android/controller-paused.png`: retained failsafe after backgrounding.
- `output/android/controller-disarmed.png`: explicit recovery.
- `output/android/verification.json`: Android result details.

The tests establish controller behavior against this approximate model. They
do not establish physical stability, ESP32 worst-case execution time, RF
reliability, sensor-bus timing or component compatibility.

The installed Postman CLI is 1.73.0. Its local collection runner passed all
checks; this version does not expose the newer `postman status` command. No
Postman cloud workspace was created or required.

## Device workbench and firmware follow-up · 14 November 2026

The exact-board boot image and functional CPU/device workbench were added after
the initial SITL/Android checks above. Current local results:

| Check                                               | Result                                                                                                                                                                                                     |
| --------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Root Rust workspace                                 | 32 passed: 9 flight-core, 13 scenarios, 10 circuit/upload/PWM/sensor checks                                                                                                                                |
| Boot-only firmware host behavior                    | 3 passed; exact board and inhibited output requests                                                                                                                                                        |
| Compiled image / CPU checks                         | 7 passed: merged-image integrity/corruption tests plus actual guest UART, GPIO, MPU6050, ADC and LEDC execution                                                                                            |
| Live HTTP/WebSocket                                 | 7 passed, including read-only net validation and upload/origin rejection                                                                                                                                   |
| Browser                                             | 11 passed; complete flight flow plus assemble/wire 13 parts from empty, drag/zoom, reject shorts, reconnect sensor, export, upload ELF → CPU → PWM → motor model, emergency stop and host-heartbeat expiry |
| Formatting, warnings-denied Clippy, optimized build | Passed                                                                                                                                                                                                     |
| Xtensa firmware cross-build                         | Boot-only image and separate simulation-only HAL fixture built successfully                                                                                                                                |

The guest firmware returned `WHO_AM_I=0x68`, stationary `az_raw=16384`, the
requested ADC values `3300` then `1234`, and all four routed LEDC pulses near
1000/1500 µs after UART commands. Wrong SDA, wrong I²C address and sensor absence
returned errors without new samples. A range regression verifies that 6 g /
20 rad/s inputs saturate at the default range, then read as `12288` / `18793`
after the guest selects ±16 g / ±2000°/s. Guest input is quantized only by its
selected device range, not clipped by the built-in controller's fixed decoder. The boot-only image produced repeated
heartbeats with the emulated GPIO outputs LOW and no motor PWM peripheral output.
Guest ROM/second-stage bootloader execution is not included: the loader enters
the application and models its clock/revision handoff.

Screenshots: `output/device-workbench-desktop.png` and
`output/device-workbench-mobile.png`. Firmware files, manifests and hashes are
under `output/firmware/`; the CI firmware job retains them as clearly separated
boot-only and simulation-fixture artifacts. Exact setup/limits are in
[device-workbench.md](device-workbench.md) and the
[firmware guide](../firmware/esp32-devkitc/README.md).

No physical board was flashed or measured. The earlier Android result above is
retained historical evidence; the native Android app was not rerun for this
follow-up. Independent aircraft motor inhibition, physical sensor/ADC/ESC
qualification, real-time measurements and restrained flights remain open.
