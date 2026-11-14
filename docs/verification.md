# Verification record · 14 November 2026

All results below are local software-in-the-loop evidence. No physical drone,
ESP32 board, sensor, ESC or motor was connected.

| Check | Result |
| --- | --- |
| `cargo test --workspace` | 22 passed: 9 controller tests and 13 plant/scenario tests |
| `npm test` | 5 passed against a real local HTTP/WebSocket server |
| `npm run test:browser` | 7 passed in Chrome, including full flight, faults, ownership and 320/390/768/1440 px layouts |
| `cargo clippy --workspace --all-targets -- -D warnings` | Passed |
| Native controller Clippy with warnings denied | Passed |
| Rust formatting checks | Passed |
| `flight-core` cross-check to `wasm32-unknown-unknown` with the installed Rustup toolchain | Passed; allocation-free core builds separately from the host server |
| Postman local collection | 3 HTTP requests / 3 assertions passed; no cloud login required for the local results |
| Optimized simulator build | Passed; self-contained executable with vendored browser assets |
| Android APK | ARM64 debug APK packaged, signed with a newly generated local development key, and signature verified |
| Android 11 native end-to-end | Passed on dedicated `Aerolab_Android_11`, `emulator-5580` |

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
