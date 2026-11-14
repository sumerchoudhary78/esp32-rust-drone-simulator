# ESP32 Rust Drone Simulator roadmap

The aim is a useful learning and experimentation platform that can eventually
share measured flight-control logic with an ESP32 bench build. Better simulation
evidence comes before physical free flight. There are no committed dates.

## Current foundation · v0.1

- [x] Allocation-free `no_std` Rust controller and MPU6050 register decoder.
- [x] Quaternion attitude estimation, rate PID and X motor mixing.
- [x] Noisy IMU, motor lag, battery depletion and 6-DOF virtual quadcopter.
- [x] 3D browser flight deck and Rust + Slint desktop/Android controller.
- [x] Command sequence checks, ownership, arming gates and latched motor-cut
      failsafes.
- [x] Link delay/loss, sensor, battery and motor fault injection.
- [x] Telemetry, attitude plots, CSV capture and automated controller/integration
      tests.

This is host software-in-the-loop. No flashable ESP32 adapter, real altitude
sensor, position hold or physically validated flight behavior is included.

## 1. Reproducible experiments · next

**Problem:** a person can fly and export telemetry, but sharing a screenshot is
not enough to reproduce the exact inputs, model and disturbance.

- [ ] Versioned scenario files: seed, initial state, vehicle parameters, command
      timeline, environment and fault schedule.
- [ ] Headless scenario runner with JSON/CSV results and nonzero exit status
      when an expected bound is exceeded.
- [ ] Input record/replay tied to the scenario seed and configuration.
- [ ] Numerical metrics: angle tracking error, overshoot, settling time, height
      deviation, saturation duration and watchdog response.
- [ ] Saved beginner examples for hover, roll/pitch steps, impulse recovery,
      lost link and depleted battery.

**Complete when:** the same scenario and seed reproduce the same results, a
contributor can run it without the UI, and a regression has a small sharable
fixture. Existing safety tests must continue to pass.

## 2. Model fidelity and learning tools

**Problem:** the current constants are illustrative rather than measurements of
a particular motor, propeller, battery and frame.

- [ ] Validated vehicle configuration for mass, inertia, arm geometry, motor
      response and measured thrust curves.
- [ ] Sensor mounting transforms, vibration, gyro saturation, stale samples and
      timing jitter with explicit units and deterministic seeds.
- [ ] Plots of estimated attitude versus ground truth, rate targets, PID terms,
      integrator state and actuator saturation.
- [ ] More demanding tests for coupled-axis inputs, saturation recovery and
      performance under wind/model mismatch.
- [ ] Short practical tutorials connecting each equation to a visible response.

**Complete when:** configurations reject invalid physical values, fixtures
explain the source of their parameters, and experiments expose model limits
without using ground truth inside the ordinary attitude controller.

## 3. ESP32 bench firmware · propellers removed

**Problem:** portable control logic does not prove physical sensor I/O, output
timing, power behavior or watchdog independence.

- [ ] Select and document one exact ESP32 chip, DevKit and supported toolchain.
- [ ] Implement verified MPU6050 startup/configuration and fresh I²C samples.
- [ ] Add calibrated battery sensing and a tested ESC output peripheral driver.
- [ ] Keep Wi-Fi processing outside the stabilization deadline using a bounded
      newest-command handoff.
- [ ] Add independent output inhibition for watchdog/reset/boot and emergency
      stop conditions.
- [ ] Measure worst-case loop jitter, sensor age and missed deadlines under
      Wi-Fi load; publish bench evidence.

**Complete when:** a reproducible propellers-off procedure verifies sensor axes,
motor order, pulse timing, timeout behavior, startup/reset inhibition and power
boundaries on the named hardware. Simulator gains are not automatically approved
for the physical frame. See [the port plan](docs/esp32-port.md).

## 4. Hardware-in-the-loop

- [ ] Run the actual ESP32 controller against virtual sensor samples and feed its
      motor outputs back into the host physics model.
- [ ] Make timing, sequence, units and transport limits explicit at the bridge.
- [ ] Replay reference scenarios with bus failures, stale input, congestion and
      missed deadlines.
- [ ] Compare host and device traces with documented numerical/timing tolerances.

**Complete when:** the firmware executes on a physical ESP32, virtual outputs
drive only the simulation during the test, and the traces show the measured
differences rather than assuming host results transfer to hardware.

## 5. Additional sensing and a justified airborne failsafe

- [ ] Choose suitable height sensing and characterize its noise, range, delay
      and failure modes; assess absolute heading/position sensors separately.
- [ ] Add a height/vertical-speed estimator without consulting simulation truth.
- [ ] Specify which failures permit a controlled descent and which require a
      different response, with explicit limits and a manual override.
- [ ] Validate low-voltage behavior and estimator/sensor failures across the
      supported operating range before any restrained hardware trial.

**Complete when:** the sensing makes the claimed behavior observable and tests
plus physical evidence support a narrowly defined policy. A quadcopter losing a
motor may lack the authority needed for ordinary stabilized flight. The current
motor-cut policy does **not** guarantee a safe landing.

## Later, after those foundations

- Cross-platform downloadable simulator builds and repeatable Android packaging.
- A clearly isolated browser demo and searchable tutorials with recorded runs.
- More complete touch controls, accessibility checks and controller layouts.
- Optional ROS 2 or MAVLink interoperability with an explicit protocol contract.
- Position hold and waypoint experiments only after suitable sensing and
  validated estimator/control behavior.

Camera/AI features and autonomous missions are lower priority than reproducible
experiments, trustworthy timing and measurable control behavior.

## Propose a change

Use the [feature template](https://github.com/sumerchoudhary78/esp32-rust-drone-simulator/issues/new?template=feature_request.yml).
State who benefits, the smallest useful change, how it will be measured, and
whether it affects simulation or hardware. See [CONTRIBUTING.md](CONTRIBUTING.md).
