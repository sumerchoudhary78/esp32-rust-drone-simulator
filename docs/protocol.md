# Virtual flight protocol · v1

The local simulator serves HTTP and WebSocket on `127.0.0.1:8787`. It has no
physical outputs. All wire units are SI except angles explicitly suffixed `_deg`,
`pwm_us`, voltage, and normalized commands. Static files are bundled; no CDN is
required at runtime.

## HTTP

- `GET /api/health`: service, version, mode, nominal loop rate, snapshot age.
- `GET /api/telemetry`: latest immutable telemetry snapshot.
- `GET /api/config`: plant constants, command deadline, motor map, limitations.
- `GET /api/openapi.json`: the HTTP schema.
- `GET /ws`: WebSocket upgrade; telemetry at 25 Hz, commands at up to 50 Hz.

## WebSocket

The server sends `{"type":"welcome","client_id":1}` and telemetry snapshots
with `"type":"telemetry"`. The first connection owns the controller. Other
connections observe; `{"type":"action","action":"claim_control"}` takes
ownership and stops the motors. Ownership is visible as `controller_id`.

Send commands every 20 ms, with strictly increasing sequence numbers per owner:

```json
{"type":"command","sequence":1,"throttle":0,"roll":0,"pitch":0,"yaw":0,"arm":false}
```

Throttle is `[0,1]`; the other axes are `[-1,1]`. Roll and pitch request ±20°,
yaw requests ±90°/s. Invalid numbers, unknown fields, stale sequences and observer
mutations are rejected. Invalid/replayed packets never refresh the watchdog.
Arming requires completed calibration, fresh input, throttle ≤3%, near-level
attitude and ≥10.8 V. A rejected arm requires releasing arm before retrying.

Actions: `reset`, `emergency_stop`, `takeoff`, `land`, `disturb`, `claim_control`.
Faults: `{"type":"fault","fault":"link_loss","enabled":true}`; also
`imu_failure`, `low_battery`, `motor_failure` (front-left motor).

```json
{"type":"settings","altitude_assist":true,"target_altitude":1.5,"latency_ms":20,"packet_loss":0,"wind":0}
```

All settings fields are required. Height: 0.5–5 m; latency: 0–800 ms; loss:
0–100 percent; wind: 0–5 m/s. Altitude assistance uses ground-truth simulated
height and vertical speed. It is not an MPU6050 capability. Disable assistance
for direct manual throttle. `takeoff` requires already armed idle motors.

```json
{"type":"tune","kp":0.07,"ki":0.025,"kd":0.0015}
```

PID changes require disarmed motors. Ranges: P 0–0.3, I 0–0.2, D 0–0.02.
Rejections are `{"type":"error","message":"..."}`. Successful actions
produce flight-log events in telemetry. Settings and fault injection operate
outside the emulated link; emergency stop also bypasses it.

At 500 ms without fresh input, the core latches a failsafe and commands 1000 µs
to every ESC. IMU loss, ≤10.2 V, >70° tilt, emergency stop and severe host timing
overrun also cut motors. This is a motor-cut policy, not a guaranteed safe
landing. Recover by explicitly disarming, correcting the fault and rearming.
No automatic rearm occurs after a fault. Delayed commands are purged on resets,
ownership changes and emergency stop. A WebSocket is limited to 16 KiB messages,
200 incoming messages/s, and a bounded server queue.

The server checks browser Origin against Host. This is a local simulator, not
an authenticated network flight protocol. Binding to a LAN exposes control to
other people on that LAN; use only a trusted development network.
