import { test, before, after } from "node:test";
import assert from "node:assert/strict";
import { spawn } from "node:child_process";
import { setTimeout as delay } from "node:timers/promises";
import http from "node:http";
const base = "http://127.0.0.1:8791";
let server,
  client,
  latest,
  interval,
  sequence = 0,
  id,
  errors = [],
  command = { throttle: 0, roll: 0, pitch: 0, yaw: 0, arm: false };
async function until(predicate, timeout = 4000) {
  const end = Date.now() + timeout;
  while (Date.now() < end) {
    if (await predicate()) return;
    await delay(20);
  }
  throw new Error(`Timed out waiting for ${predicate.toString()}`);
}
const send = (value) => client.send(JSON.stringify(value));
const action = (name) => send({ type: "action", action: name });
async function reset() {
  command.arm = false;
  command.throttle = 0;
  const tick = latest.tick;
  action("reset");
  await until(() => latest.tick < tick);
  await until(() => latest.state === "disarmed" && latest.command_age_ms < 100);
}
before(async () => {
  server = spawn("target/debug/drone-sim", ["--bind", "127.0.0.1:8791"], {
    stdio: "pipe",
  });
  await until(async () => {
    try {
      return (await fetch(`${base}/api/health`)).ok;
    } catch {
      return false;
    }
  });
  client = new WebSocket(`${base.replace("http", "ws")}/ws`);
  client.addEventListener("message", (e) => {
    const v = JSON.parse(e.data);
    if (v.type === "welcome") id = v.client_id;
    if (v.type === "telemetry") latest = v;
    if (v.type === "error") errors.push(v.message);
  });
  await until(() => client.readyState === WebSocket.OPEN && id);
  interval = setInterval(
    () => send({ type: "command", sequence: ++sequence, ...command }),
    20,
  );
  await until(
    () => latest?.state === "disarmed" && latest?.controller_id === id,
  );
});
after(async () => {
  clearInterval(interval);
  client?.close();
  server?.kill("SIGINT");
  if (server && server.exitCode === null)
    await new Promise((resolve) => server.once("exit", resolve));
});

test("HTTP identifies simulation, exposes constraints and returns fresh telemetry", async () => {
  const health = await (await fetch(`${base}/api/health`)).json();
  assert.equal(health.mode, "simulation");
  assert.equal(health.healthy, true);
  assert.equal(health.nominal_loop_hz, 500);
  const config = await (await fetch(`${base}/api/config`)).json();
  assert.equal(config.command_timeout_ms, 500);
  assert.equal(config.motors.length, 4);
  assert.ok(config.limitations.length >= 5);
  const t = await (await fetch(`${base}/api/telemetry`)).json();
  assert.equal(t.type, "telemetry");
  assert.deepEqual(t.motors.pwm_us, [1000, 1000, 1000, 1000]);
  assert.equal(
    (await (await fetch(`${base}/api/openapi.json`)).json()).openapi,
    "3.1.0",
  );
});
test("cross-origin browser control is forbidden", async () => {
  const status = await new Promise((resolve, reject) => {
    const request = http.get(
      `${base}/ws`,
      {
        headers: {
          Connection: "Upgrade",
          Upgrade: "websocket",
          "Sec-WebSocket-Version": "13",
          "Sec-WebSocket-Key": "dGhlIHNhbXBsZSBub25jZQ==",
          Origin: "http://untrusted.invalid",
        },
      },
      (response) => {
        response.resume();
        resolve(response.statusCode);
      },
    );
    request.on("error", reject);
  });
  assert.equal(status, 403);
});
test("malformed messages, range violations, replays and unsafe tuning are rejected", async () => {
  for (const value of [
    { type: "command", sequence: 999999, ...command, throttle: 1.1 },
    { type: "command", sequence: 0, ...command },
    { type: "command", sequence: 999999, ...command, surprise: true },
    { type: "tune", kp: 99, ki: 0, kd: 0 },
    {
      type: "settings",
      altitude_assist: true,
      target_altitude: 1.5,
      latency_ms: 9000,
      packet_loss: 0,
      wind: 0,
    },
  ]) {
    const count = errors.length;
    send(value);
    await until(() => errors.length > count);
  }
  assert.equal(latest.state, "disarmed");
});
test("flight over real WebSocket reaches height, then link-loss failsafe latches", async () => {
  await reset();
  command.arm = true;
  await until(() => latest.state === "armed");
  action("takeoff");
  await until(() => latest.position_m.z > 1.4, 10000);
  assert.ok(Math.abs(latest.attitude_deg.x) < 2);
  const count = errors.length;
  send({ type: "tune", kp: 0.1, ki: 0.01, kd: 0.001 });
  await until(() => errors.length > count);
  assert.match(errors.at(-1), /Disarm/);
  send({ type: "fault", fault: "link_loss", enabled: true });
  await until(() => latest.reason === "link_timeout", 1500);
  assert.deepEqual(latest.motors.pwm_us, [1000, 1000, 1000, 1000]);
  send({ type: "fault", fault: "link_loss", enabled: false });
  await delay(600);
  assert.equal(latest.state, "failsafe");
  command.arm = false;
  await until(() => latest.state === "disarmed");
});
test("observer writes are rejected and claiming control always disarms", async () => {
  await reset();
  command.arm = true;
  await until(() => latest.state === "armed");
  const observer = new WebSocket(`${base.replace("http", "ws")}/ws`);
  let observerError;
  observer.addEventListener("message", (e) => {
    const v = JSON.parse(e.data);
    if (v.type === "error") observerError = v.message;
  });
  await until(() => observer.readyState === WebSocket.OPEN);
  observer.send(JSON.stringify({ type: "action", action: "reset" }));
  await until(() => observerError);
  assert.match(observerError, /Observer/);
  observer.send(JSON.stringify({ type: "action", action: "claim_control" }));
  await until(() => latest.controller_id !== id);
  assert.equal(latest.state, "disarmed");
  clearInterval(interval);
  observer.close();
});
