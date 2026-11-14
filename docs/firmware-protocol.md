# ESP32 physical command protocol · AERO v2

This contract belongs to `aerolab-flight-bench`, independently of the simulator's
JSON/WebSocket interface. It is implemented in
[`commands.rs`](../firmware/esp32-devkitc/src/commands.rs) and the standard-library
[`bench_console.py`](../scripts/bench_console.py). The same fixed packet is sent
as a UDP datagram or UART hex frame. Neither transport runs on the flight core.

## Link and session

The Wi-Fi build hosts a WPA2 AP with per-device credentials from the private
build profile. DHCP is enabled, ESP32 address `192.168.4.1`, UDP port `14550`.
An `AERO2 HELLO` datagram returns bounded JSON status at most ten times per second,
including `protocol:2`, a 16-hex-digit `session`, and `next_sequence`. Discovery
is read-only and unauthenticated within this AP. It reveals no secret. Telemetry
is diagnostic, not a cryptographically authenticated flight-state indication.

UART0 is 115200 baud. Logs contain `bench session=... next=...` every 100 ms.
Transmit `@` + exactly 120 hex characters + newline. CR is ignored. Malformed,
partial and overlong frames cannot publish a command; `@` starts a new frame.

Physical builds acquire a fresh 64-bit boot session from the ESP32 true-random
source while ADC entropy is enabled, then release ADC1 for battery sensing.
The emulated build uses the public nonce `1122334455667788` and a public test key;
it is never a physical device profile. Receivers reject another boot's session.

## Fixed 60-byte binary packet

All multibyte values are little-endian; axes use two's-complement `i16`.

| Offset | Size | Field |
| --- | --- | --- |
| 0 | 4 | ASCII `AERO` |
| 4 | 1 | Version `2` |
| 5 | 1 | Flags: bit 0 ARM, bit 1 emergency stop; reserved bits zero; ARM+stop invalid |
| 6 | 2 | Packet length, `60` |
| 8 | 8 | Boot session, `u64` |
| 16 | 4 | Monotonic sequence, `u32` |
| 20 | 2 | Throttle `u16`, 0–1000 → 0–1 |
| 22 | 2 | Roll `i16`, −1000…1000 → −1…1 |
| 24 | 2 | Pitch `i16`, same range |
| 26 | 2 | Yaw `i16`, same range |
| 28 | 32 | HMAC-SHA256 of bytes 0…27 using the profile's 32-byte command key |

The receiver checks length/header/session, verifies the full MAC using the
constant-time HMAC verifier, then checks numeric ranges and sequence. A sequence
is newer when its wrapping distance from the previous accepted value lies in
`1..2^31`. The first valid packet may use any sequence; discovery reports the
next expected value. Rejected packets never advance state or refresh expiry.
Clients share one counter across UART and UDP, so run one command producer per
device. Knowledge of the PSK gives control; there is no separate multi-user
ownership service. HMAC authenticates packets; it does not encrypt them.

The network core timestamps accepted commands with the ESP32 clock. A bounded
atomic mailbox supplies only a consistent newest command, without a blocking
lock. Emergency stop additionally latches an atomic flag so an overwritten
mailbox cannot lose it. Commands expire after **250 ms from local receipt**.
This does not authenticate the sender's clock or prove upstream network latency.

## Arming and recovery

Calibration, a measured valid battery voltage, a level attitude, an observed
GPIO13 LOW, and an authenticated disarm while GPIO13 is LOW are prerequisites.
Set GPIO13 HIGH and send a fresh ARM packet at zero throttle. Arming while
calibrating is rejected, not queued. Do not increase throttle until arming has
been accepted. Continue commands at 50 Hz. Zero-throttle armed idle is 1060 µs.

GPIO13 LOW, emergency, IMU error, invalid/critical voltage, missed deadline,
excessive tilt and expired commands inhibit outputs. A fresh ARM alone cannot
recover a fault. Return the switch LOW, send authenticated disarm, restore
healthy inputs/calibration and explicitly arm again. A timer-watchdog trip
latches pad inhibition until reboot. Sensor reinitialization happens only while
the gate is LOW and restarts calibration.

The physical guide defines the required [hardware gate and bench procedures](physical-bench.md).
Current gains and airborne motor-cut policy are unqualified for free flight.

## Interoperability vector

Key: 32 bytes `42` (hex), session `1122334455667788`, sequence 5, ARM,
throttle .2, roll −.4, pitch .3, yaw 1:

```text
4145524f02013c00887766554433221105000000c80070fe2c01e80325d96b6961c07ed0131f2a6fd92374196111bd58d876103d06627c6b32958c64
```

Rust/Python tests verify this vector, malformed lengths/flags/ranges, modified
MACs, sequence wrap, duplicates and previous-session packets. Guest CPU tests
send the Python encoding through the actual firmware's UART/HMAC path.
