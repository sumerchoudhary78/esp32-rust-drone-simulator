# Physical ESP32 bench report

**Status: NOT RUN.** Copy this file for an actual test session. No physical
measurements are supplied by this repository's automated tests.

## Reproducible identity

| Field | Actual value |
| --- | --- |
| Date/operator | Pending |
| Git commit + clean/dirty status | Pending |
| ELF and merged-image SHA-256 | Pending |
| Profile SHA-256 (never include command/WPA2 secrets) | Pending |
| Manifest mode, PWM cap/rate, fault-injection flag | Pending |
| ESP32 board/module/revision/flash markings | Pending |
| MPU6050 module + physical mounting | Pending |
| ESC model/firmware + motor model | Pending |
| Power source, regulator, divider, AND gate | Pending |
| Multimeter/scope/logic-analyzer model and settings | Pending |
| Propellers removed, vehicle secured, power disconnect available | Pending |

## Measurements and evidence

Record the observed value and an evidence filename for **every** row. A checkbox,
serial message or emulator number alone does not establish a physical waveform.

| Check | Observed value / limit / raw evidence | Pass/fail |
| --- | --- | --- |
| Rail voltages, ripple and minimum under Wi-Fi load | Pending | Not run |
| GPIO32 + downstream ESC signals at power-up/reset/panic/brownout | Pending | Not run |
| MPU identity/config/readback and stationary gyro variance | Pending | Not run |
| Sensor mounting and roll/pitch/yaw signs | Pending | Not run |
| Motion interrupts calibration; early/held arm never enables | Pending | Not run |
| ADC calibration points with multimeter + interpolation residuals | Pending | Not run |
| 10.8 V arming threshold and 10.2 V critical cut | Pending | Not run |
| ADC disconnect/rails inhibit outputs | Pending | Not run |
| PWM widths/rate for all four pins; idle and 1150 µs cap | Pending | Not run |
| Correct M1/M2/M3/M4 order, spin direction and corrective mix | Pending | Not run |
| GPIO33 period/high time: idle, maximum Wi-Fi traffic, weak RF | Pending | Not run |
| Maximum I²C duration, sensor age, stuck-SDA/NACK response | Pending | Not run |
| Command loss → gate LOW latency (target ≤252 ms plus measured execution) | Pending | Not run |
| MAC/replay/old-session rejected without refreshing expiry | Pending | Not run |
| GPIO13 LOW / emergency → gate LOW latency | Pending | Not run |
| Core 1 `!` stall → gate/pads inhibited; ISR latency | Pending | Not run |
| Core 1 `~` interrupt-masked stall → MWDT reset and gate LOW | Pending | Not run |
| Downstream ESC signal-loss stop time and motor coast-down | Pending | Not run |
| Recovery requires explicit disarm, switch sequence, fresh command | Pending | Not run |
| UART and Wi-Fi console interoperability; disconnect behavior | Pending | Not run |

## Assessment

- Unresolved failures: Pending measurements.
- Repeatability / number of runs: Not run.
- Suitable for the next restrained test: **No; qualification pending.**
- Flight-qualified: **No.**

Attach unedited logs and captures. Measured thrust, current, mechanical limits,
vibration, tuned gains, restrained tests and controlled flight results need
separate reports before a flight-readiness claim.
