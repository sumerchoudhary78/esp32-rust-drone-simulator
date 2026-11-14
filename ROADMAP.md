# ESP32 Rust Drone roadmap

The real ESP32 bench-controller software is implemented; the next gate is physical measurement. Simulation supports
that work; it does not establish physical flight readiness. No dates are promised.

## Available now

- [x] Shared `no_std` Rust fusion, calibration, PID, X mixer and arming/failsafes.
- [x] 3D F450 physics, noisy MPU6050 samples, ESC/motor lag and battery depletion.
- [x] Browser controls, Rust + Slint Android controller, telemetry, faults and CSV.
- [x] Assemble 13 devices with real pin names; validate power/signal nets, apply
      connections, save/share circuits and inspect live devices.
- [x] Optional classic ESP32 LX6 instruction execution against MPU I²C, ADC1,
      UART0 and LEDC PWM, with the plant advancing alongside the guest clock.
- [x] Actual compiled HAL peripheral example and guest/browser regression tests.

Device models are functional approximations; guest Wi-Fi, electrical transients,
ESC firmware and cycle-accurate timing are outside the current adapter.

## 1. One exact board, flashable bootstrap · current hardware milestone

- [x] ESP32-DevKitC-32E / WROOM-32E, ECO3, 4 MiB target and reserved output pins.
- [x] Pinned Xtensa compiler/tools, reproducible ELF/merged-image build and hashes.
- [x] Boot-only serial heartbeat with motor pins static LOW after initialization.
- [x] Host behavior tests, image integrity checks and emulated application startup.
- [ ] Physical flash/boot log from the named board.
- [ ] Pin measurements during power-up, reset, application startup and panic.

**Complete when:** someone can build and flash the documented image and record
those measurements. It intentionally cannot arm or run a flight loop.
See [firmware instructions](firmware/esp32-devkitc/README.md).

## 2. Real MPU6050 acquisition and ESC outputs · propellers removed

- [x] Implement physical sensor identity, reset/configuration/readback and fresh burst reads.
- [ ] Verify mounting transforms and roll/pitch/yaw signs on the actual module.
- [x] Implement four-channel LEDC behind an external GPIO32 output inhibit.
- [ ] Measure pulse width, update frequency, idle and motor order on actual ESCs.
- [x] Automated sensor identity/readback/NACK/stale/saturation and guest disconnect tests.
- [ ] Physical stuck-bus, output-error and signal-loss fault measurements.

The actual dual-core controller binary exercises calibration, authenticated arming,
PID/LEDC, ADC, link/sensor/battery faults and an independent ISR in the emulator.
Physical measurements remain a separate gate. See the [bench guide](docs/physical-bench.md).

## 3. Calibration, battery sensing, expiry and independent motor watchdog

- [x] Motion-rejecting calibration, 98 Hz gyro DLPF and ±2000°/s / ±8 g decoding.
- [ ] Measure stationary variance and vibration with the actual mounting.
- [ ] Verified voltage divider/filtering, ADC characterization and voltage policy.
- [x] Separate-core WPA2/UDP + UART, bounded atomic newest-command handoff,
      HMAC/session/replay checks and local command timestamps; one PSK command producer.
- [x] Implement timer ISR inhibition, 40 ms MWDT fallback, physical interlock,
      deadline checks, 250 ms expiry and sticky emergency stop.
- [ ] Measure actual inhibition through hangs, reset, brownout and emergency stop.
- [ ] Worst-case loop jitter, sensor age and watchdog latency under Wi-Fi traffic.

**Complete when:** fault injection stops outputs on the actual board even when
flight code stops making progress. Host emulator guards do not meet this gate.

## 4. Verified wiring, build/flash guide and repeatable bench evidence

- [ ] Compatible motor/prop/ESC/battery/regulator/frame bill of materials.
- [x] Exact-pin wiring reference, private build profiles, flash/console/calibration tools.
- [ ] Physically verify wiring and power delivery for the assembled build.
- [ ] Propellers-off logs/scope captures covering startup, reset, order/signs,
      timing, calibration, voltage, dropped commands and failsafe recovery.
- [ ] A versioned hardware test report attached to the firmware revision.

## 5. Restrained trials, then controlled flights

- [ ] Measure thrust/current, mass/inertia, vibration and controller response.
- [ ] Retune using restrained tests; preserve separate simulation/hardware gains.
- [ ] Select additional sensing if claiming height, heading or position behavior.
- [ ] Define and validate an airborne failure policy within those sensing limits.
- [ ] Document controlled flight results and unresolved failures before describing
      a release as flight-ready. Motor cut does not guarantee a safe landing.

## Simulation improvements alongside those gates

- Record/replay with seeded scenarios, headless runner and numerical regressions.
- Measured vehicle configurations, sensor mounting/vibration/saturation models.
- More guest firmware examples, MPU FIFO/interrupt behavior, verified MCPWM/DShot
  adapters and explicit diagnostics for unsupported device operations.
- Physical ESP32 hardware-in-the-loop: virtual sensor input and measured motor
  output, with sequence/time/unit bounds and comparison to host/guest traces.
- Controller protocol bridge for guest firmware; networking must have an explicit
  contract rather than pretending arbitrary uploaded firmware understands joysticks.
- Downloadable desktop builds and beginner tutorials with reproducible evidence.

Camera/AI, missions, ROS 2/MAVLink and position hold follow measured control and
appropriate sensing. See [CONTRIBUTING.md](CONTRIBUTING.md) to propose a focused change.
