# ESP32-DevKitC-32E flight-controller bench

The real controller is implemented in Rust: MPU6050 acquisition, sensor fusion,
PID/mixing, ADC battery sensing, gated ESC PWM and authenticated commands. It
runs on the two cores of **ESP32-DevKitC-32E / WROOM-32E, ECO3, 4 MiB**. This is
unqualified development firmware. No physical board, ESC or motor has been
measured for this revision. The default output cap is **1150 µs at 50 Hz**;
that can spin a motor and is intended for a **propellers-removed bench**.

## Build without hardware

From the repository root, after [toolchain setup](../firmware/esp32-devkitc/README.md):

```sh
python3 scripts/esp32_firmware.py lint
python3 scripts/esp32_firmware.py test
python3 scripts/esp32_firmware.py build
python3 scripts/esp32_firmware.py build-fixture
python3 scripts/esp32_firmware.py init-profile
python3 scripts/esp32_firmware.py build-bench --profile output/firmware-config/device.json
python3 scripts/esp32_firmware.py build-bench --profile output/firmware-config/device.json --wifi
python3 scripts/esp32_firmware.py build-bench --emulated --fault-injection
python3 scripts/setup_emulator.py
AEROLAB_REQUIRE_EMULATOR=1 python3 -m unittest discover -s tests -p '*_test.py' -v
```

`init-profile` creates unique command and WPA2 secrets with file mode 0600. It
refuses to overwrite a profile. Keep `output/firmware-config/device.json` private;
Git ignores `output/`. Device secrets are embedded in its ELF/bin, so treat
physical build artifacts as private too. The tracked emulated profile has an
intentionally public key/nonce and is refused by `flash-bench`.

Each build produces `aerolab-flight-bench.elf`, an application-only `.bin`, a
`-merged.bin` at flash offset **0x0**, a manifest and `SHA256SUMS` in:

| Build | Output folder |
| --- | --- |
| Physical UART | `output/firmware/flight-bench/` |
| Physical Wi-Fi + UART | `output/firmware/flight-bench-wifi/` |
| Emulated, never flash | `output/firmware/flight-bench-emulated/` |

Manifests record PWM cap/rate, transport, calibration presence, profile digest,
artifact hashes and `physical_bench_validated=false`, `flight_qualified=false`.
Packaging verifies the actual watchdog ISR is linked into IRAM and records its
address. Image integrity does not establish actuator behavior. CPU tests execute the
actual dual-core UART bench binary; the Wi-Fi image is cross-built, with RF,
DHCP and timing under radio load still requiring hardware.

## Wiring reference — measurement pending

All GPIO numbers below are **ESP32 signal names**, not header-position numbers.
Use the exact [Espressif DevKitC-32E pin/power reference](https://docs.espressif.com/projects/esp-dev-kits/en/latest/esp32/esp32-devkitc/user_guide.html).
The board must use a single power input: USB **or** its appropriate regulated
header supply. Do not feed the 5 V header while an ordinary powered USB cable
is connected. Never connect the 3S battery directly to a DevKit power pin.

| Signal | Connection |
| --- | --- |
| MPU6050 SDA / SCL | GPIO21 / GPIO22; pull-ups to 3.3 V, short wires |
| MPU6050 power / ground | Verified module 3.3 V rail / common GND; regulators on breakout modules vary |
| MPU6050 AD0 | GND, address `0x68`; INT is unused, fresh data is polled |
| Battery measurement | GPIO34, ADC1 channel 6, through the divider/filter below |
| Physical arm input | GPIO13, external 10 kΩ pull-down; switch connects it to **3.3 V** |
| Motor enable | GPIO32 to all four AND-gate B inputs; external 10 kΩ pull-down |
| Loop timing probe | GPIO33, HIGH during each control cycle; do not connect an actuator |
| Common reference | ESP32, IMU, logic gate, divider and ESC signal grounds connected |

Add a **3.3 V quad AND gate**, for example SN74LVC08APW on a suitable breakout,
between the four ESP32 PWM pins and ESC signals. This firmware depends on that
external inhibit for reset/boot behavior. The pin table is for the **14-pin PW
package**, per the [TI datasheet](https://www.ti.com/lit/ds/symlink/sn74lvc08a.pdf);
verify the actual package before assembly.

| Motor, viewed from above | ESP32 → gate A | Gate B | Gate Y → ESC signal |
| --- | --- | --- | --- |
| M1 front left, CW | GPIO25 → pin 1 | pin 2 | pin 3 |
| M2 front right, CCW | GPIO26 → pin 4 | pin 5 | pin 6 |
| M3 rear right, CW | GPIO27 → pin 9 | pin 10 | pin 8 |
| M4 rear left, CCW | GPIO14 → pin 12 | pin 13 | pin 11 |

Gate VCC pin 14 → 3.3 V, GND pin 7 → GND, with 100 nF decoupling at the IC.
Each A input also needs a 10 kΩ pull-down so boot-time PWM inputs cannot float.
Tie B inputs to GPIO32; its pull-down keeps all four outputs LOW during reset.
Use a 220 Ω series resistor and 10 kΩ pull-down on each ESC signal output. The
specific ESC must accept 3.3 V logic; this has not been checked for a generic
"30 A ESC". Signal loss must stop that ESC within a measured acceptable time.
A LOW signal gate removes pulses; it cannot force an unknown ESC to stop
immediately or remove motor inertia.

Proposed 3S sensing network (separate from the virtual workbench's divider):

```text
PACK+ ── 33 kΩ, 1% ──┬── GPIO34 (ADC1_CH6)
                      ├── 100 nF ── GND
                      └── 6.8 kΩ, 1% ── GND
PACK− ──────────────────────────── GND
```

The nominal divider output at 12.6 V is about 2.15 V. Check actual voltage,
resistors, grounding and rail behavior before connecting GPIO34. Firmware uses
11 dB ADC attenuation and **measured two-point pack-voltage calibration**, not
a guessed ADC reference. A missing calibration, clipped rail or failed read
inhibits arming. Validate interpolation at intermediate voltages as well.

Battery → rated distribution → four ESC power inputs → motors. Power the
DevKit through a suitable regulated supply with adequate radio-load margin.
Do not parallel unknown ESC BEC outputs. A2212/1000KV, generic 30 A ESC, F450,
10-inch props and a 3S pack remain a starting BOM, not a verified combination.

## First USB and sensor check

1. Leave ESC power disconnected and remove all propellers. Keep GPIO13 LOW.
   Check the divider and gate wiring with power removed.
2. Build the physical UART image with the newly generated, **uncalibrated**
   profile. Flash only the identified board port:

   ```sh
   python3 scripts/esp32_firmware.py flash-bench --port /dev/ttyUSB0 --props-removed
   ```

   Replace the port with the board's actual `/dev/cu.usbserial-*`,
   `/dev/cu.SLAB_USBtoUART` or Linux port. Only explicit flash/console commands
   open hardware. Builds and emulator tests do not.
3. Inspect the 115200-baud log. After about four seconds stationary and level,
   expect `state=1 calibration=2000`, `gate=false`. Missing battery calibration
   reports `voltage=0.000`; arming remains blocked. State codes are 0 calibrating,
   1 disarmed, 2 armed, 3 failsafe. The gyro uses ±2000°/s, acceleration ±8 g,
   a 98 Hz gyro DLPF and 1 kHz sensor updates. The controller polls at 500 Hz.
4. Check actual MPU axes against body **X forward, Y left, Z up**. With the
   board level, specific force is +G on Z. Positive roll is right-wing-down;
   positive pitch is nose-down; positive yaw is counterclockwise viewed from
   above. Verify telemetry and all four corrective motor signs before applying
   load. A different physical sensor mounting requires an explicit transform.
5. Capture GPIO32, all four gate outputs and supply voltage through power-up,
   reset and panic. They must remain inhibited. Firmware logs alone cannot
   prove this. Record the observations in the [report template](bench-report-template.md).

## Measure battery calibration

With ESC power disconnected, use a current-limited adjustable source at the
pack-divider input and a multimeter. The board can remain on USB; the test
source supplies the divider only and shares GND. Record stable `adc_raw` plus
actual voltage at two points spanning at least 1 V (for example around 9 and
12.6 V). Replace all four capitalized placeholders below with measurements:

```sh
python3 scripts/bench_console.py --profile output/firmware-config/device.json calibrate \
  --low-raw LOW_RAW --low-volts LOW_VOLTS --high-raw HIGH_RAW --high-volts HIGH_VOLTS
python3 scripts/esp32_firmware.py build-bench --profile output/firmware-config/device.json
```

Flash again. Verify telemetry at multiple intermediate voltages, including the
**10.8 V arm** and **10.2 V critical-cut** thresholds. Ten valid ADC reads are
required after a bad reading. Downward changes act immediately in software;
an exponential filter smooths upward changes. The external RC/sensor/ESC delays
remain part of the physical stop-latency measurement.

## Finite propellers-removed motor check

Keep the vehicle secured. First measure PWM and the gate with ESC power off.
Use an independent way to remove ESC power during powered checks. Start with
one ESC/motor, then repeat for the verified motor order; this console drives
the normal mixer, not an automatic ESC calibration or high-throttle routine.

Close espflash's monitor before using the serial console. Install `pyserial`
for a serial connection; UDP needs only standard Python. While GPIO13 is LOW:

```sh
python3 scripts/bench_console.py --profile output/firmware-config/device.json --serial /dev/ttyUSB0 status
python3 scripts/bench_console.py --profile output/firmware-config/device.json --serial /dev/ttyUSB0 disarm
```

After calibration/battery checks pass, set GPIO13 HIGH and request a finite run:

```sh
python3 scripts/bench_console.py --profile output/firmware-config/device.json --serial /dev/ttyUSB0 arm --props-removed --seconds 3 --throttle 0.08
```

The console starts with 400 ms at zero throttle, sends 50 authenticated commands
per second, ramps the requested throttle, and sends disarm on exit or Ctrl+C.
Zero-throttle arm uses the shared controller's **1060 µs idle**, which may spin
motors. The bench firmware caps all channels at **1150 µs**. Check real pulse
width/rate, all four motors, direction and corrective mixing. Return GPIO13 LOW
and send disarm before each new arming attempt; a held arm during calibration,
expired command or fault cannot re-arm automatically.

## Wi-Fi bench connection

Build with `--wifi` and flash using
`--artifacts output/firmware/flight-bench-wifi`. The local profile contains
`wifi_ssid` and its unique WPA2 password. Join that AP, retain the connection
when the phone/computer reports no internet, and use DHCP. The board is
**192.168.4.1**, UDP **14550**. Replace `--serial PORT` with `--udp 192.168.4.1`
in the console examples. UART and UDP share one authenticated receiver and
sequence counter. Use one command client at a time. Existing browser/Slint
simulator controllers use a different protocol; they do not control this board.

The flight loop is pinned to Core 1. Wi-Fi, sockets, parsing, HMAC and serial
telemetry run on Core 0. The bounded atomic mailbox never waits on a mutex;
a sticky emergency flag survives a later command replacing the mailbox. There
is no network operation or allocation in the flight loop. This separation does
not establish worst-case timing under real radio traffic; measure it.

## Failure tests and timing gates

Use the [report template](bench-report-template.md) to record raw serial logs,
scope captures, firmware hashes, equipment and observed limits:

| Test | Required observation; physical result pending |
| --- | --- |
| Unplug IMU / wrong address / SDA stuck | No stale sample reuse; gate LOW on failure, sensor reinit only while inhibited; full recalibration before fresh arming |
| ADC rail / disconnected divider / low voltage | Gate LOW; ten good samples plus explicit recovery required |
| Stop sending / kill console / disable Wi-Fi | Gate LOW after 250 ms command expiry, plus at most the next completed 2 ms loop |
| Duplicate, modified or previous-boot packet | Rejected; never refreshes expiry; see [packet contract](firmware-protocol.md) |
| GPIO13 LOW / authenticated emergency | Gate LOW at the next loop; verify downstream ESC stop response |
| Missed deadline / slow I²C | Arrival >500 µs late or execution ≥1500 µs inhibits outputs; I²C transaction timeout 750 µs, ADC budget 150 µs |
| Loop progress lost with interrupts active | TIMG1 priority-3 ISR latches inhibit after five stale 2 ms checks (nominal ≤12 ms plus ISR latency); reboot required |
| All controller interrupts blocked | TIMG1 MWDT is fed only after completed cycles; nominal 40 ms reset, followed by external pull-down inhibition |
| Wi-Fi flood / weak signal / brownout / panic / reset | Record GPIO33 period/high time, gate/output voltage, resets and supply minimum; no unintended pulses |

Watchdog fault injection exists **only** when explicitly built with
`--fault-injection`; it is forbidden in a full-range profile. A physical test
image additionally requires `flash-bench --allow-fault-injection`. With a scope
connected, these serial commands deliberately stall the controller:

```sh
python3 scripts/bench_console.py --profile output/firmware-config/device.json --serial /dev/ttyUSB0 fault --props-removed
# Separate test: masks Core 1 interrupts to exercise the hardware reset fallback.
python3 scripts/bench_console.py --profile output/firmware-config/device.json --serial /dev/ttyUSB0 fault --props-removed --mask-interrupts
```

GPIO33 stays HIGH at the stall; measure until GPIO32 and downstream signals
fall. The two watchdog paths use separate TIMG1 timer/WD counters, independent
of foreground loop progress, but share the ESP32 clock/power domain. They are
not an external safety processor. Classic-ESP32 MWDT reset behavior is not
established by the current functional emulator. Do not infer real-time latency
from its instruction counts.

## Beyond the bench

An `experimental_flight` profile can select up to 2000 µs and a supported
50/100/200/250/400 Hz ESC rate. Build-time validation rejects other values;
`flash-bench --experimental-flight` is required for that unqualified mode.
Do not remove the cap merely to make a flight claim. Shared gains are illustrative,
not tuned to an F450. Before restrained trials, measure motor/prop/ESC/battery
compatibility, thrust/current, inertia, vibration, mounting signs, loop jitter,
all stop/reset paths and gains. MPU6050 alone supplies no absolute heading,
height or position hold. The current airborne failure policy cuts motors; it
cannot promise a controlled landing. Record controlled flights before labeling
any release flight-ready.
