import { createScene } from "./scene.js";
import { createWorkbench } from "./workbench.js";
const $ = (id) => document.getElementById(id);
const pilot = { throttle: 0, roll: 0, pitch: 0, yaw: 0, arm: false };
let socket,
  clientId = null,
  sequence = 0,
  latest,
  owner = false,
  samples = [],
  lastSnapshotAt = 0,
  lastEvent = -1,
  page = "flight";
let suspended = false,
  resetAfterEvent = null;
let scene;
const workbench = createWorkbench({ send, notice });
try {
  scene = createScene($("viewport"));
} catch (error) {
  $("render-fallback").hidden = false;
  console.warn("3D display unavailable:", error.message);
}
const names = {
  flight: [
    "Flight deck",
    "Your first flight starts here. No propellers required.",
  ],
  hardware: [
    "Virtual hardware",
    "Assemble the electronics. Connect the pins. Execute your firmware.",
  ],
  tuning: [
    "Control tuning",
    "Change a gain. Introduce a fault. Understand the response.",
  ],
  log: ["Flight log", "A clear record of what your controller is doing."],
  guide: [
    "Getting started",
    "From a stationary sensor to your first virtual flight.",
  ],
};
function setPage(name) {
  page = name;
  for (const button of document.querySelectorAll("[data-page]")) {
    const active = button.dataset.page === name;
    button.classList.toggle("active", active);
    if (active) button.setAttribute("aria-current", "page");
    else button.removeAttribute("aria-current");
  }
  for (const section of document.querySelectorAll(".page"))
    section.classList.toggle("active", section.id === `page-${name}`);
  $("page-title").textContent = names[name][0];
  $("page-description").textContent = names[name][1];
  if (name === "flight") requestAnimationFrame(drawChart);
  location.hash = name;
}
document
  .querySelectorAll("[data-page]")
  .forEach((button) =>
    button.addEventListener("click", () => setPage(button.dataset.page)),
  );
if (names[location.hash.slice(1)]) setPage(location.hash.slice(1));
let noticeTimeout;
function notice(message, danger = false) {
  clearTimeout(noticeTimeout);
  $("notice").textContent = message;
  $("notice").hidden = false;
  $("notice").classList.toggle("danger", danger);
  noticeTimeout = setTimeout(() => {
    if (latest?.state !== "failsafe") $("notice").hidden = true;
  }, 6500);
}
function send(message) {
  if (socket?.readyState !== WebSocket.OPEN) {
    notice(
      "The simulator is disconnected. Start the Rust server to reconnect.",
    );
    return false;
  }
  if (!owner && message.action !== "claim_control") {
    notice("This browser is observing. Select Take control to fly.");
    return false;
  }
  socket.send(JSON.stringify(message));
  return true;
}
function command() {
  if (
    socket?.readyState === WebSocket.OPEN &&
    owner &&
    !document.hidden &&
    !suspended
  )
    socket.send(
      JSON.stringify({ type: "command", sequence: ++sequence, ...pilot }),
    );
}
function action(action) {
  return send({ type: "action", action });
}
function neutralize() {
  pilot.roll = 0;
  pilot.pitch = 0;
  pilot.yaw = 0;
  keys.clear();
  for (const id of ["roll-range", "pitch-range", "yaw-range"]) $(id).value = 0;
  for (const thumb of document.querySelectorAll(".stick-thumb"))
    thumb.style.transform = "translate(0px,0px)";
}
function emergency() {
  pilot.arm = false;
  pilot.throttle = 0;
  $("throttle").value = 0;
  $("throttle-output").textContent = "0%";
  neutralize();
  action("emergency_stop");
}
function connect() {
  socket = new WebSocket(
    `${location.protocol === "https:" ? "wss" : "ws"}://${location.host}/ws`,
  );
  socket.addEventListener("open", () => {
    $("connection").textContent = "Connected";
    $("connection").classList.remove("offline");
  });
  socket.addEventListener("message", (e) => {
    const message = JSON.parse(e.data);
    if (message.type === "welcome") {
      clientId = message.client_id;
      sequence = 0;
      pilot.arm = false;
      pilot.throttle = 0;
      return;
    }
    if (message.type === "error") {
      resetAfterEvent = null;
      notice(message.message);
      return;
    }
    if (message.type === "telemetry") {
      if (resetAfterEvent !== null) {
        if (
          !message.events.some(
            (e) =>
              e.id > resetAfterEvent && e.message.startsWith("Workspace reset"),
          )
        )
          return;
        resetAfterEvent = null;
      }
      const previous = latest;
      latest = message;
      owner = message.controller_id === clientId;
      lastSnapshotAt = performance.now();
      if (previous && message.tick < previous.tick) {
        samples = [];
        pilot.arm = false;
        pilot.throttle = 0;
        neutralize();
      }
      if (previous?.state === "armed" && message.state === "disarmed") {
        pilot.arm = false;
        pilot.throttle = 0;
        $("throttle").value = "0";
      }
      if (previous && previous.controller_id !== message.controller_id) {
        pilot.arm = false;
        pilot.throttle = 0;
        neutralize();
      }
      samples.push(message);
      if (samples.length > 6000) samples.shift();
      scene?.update(message);
      render(message);
      workbench.update(message);
      drawChart();
    }
  });
  socket.addEventListener("close", () => {
    owner = false;
    pilot.arm = false;
    pilot.throttle = 0;
    neutralize();
    $("connection").textContent = "Disconnected";
    $("connection").classList.add("offline");
    $("control-status").textContent = "OFFLINE";
    disableControls();
    setTimeout(connect, 1000);
  });
  socket.addEventListener("error", () => socket.close());
}
setInterval(command, 20);
setInterval(() => {
  if (lastSnapshotAt && performance.now() - lastSnapshotAt > 1000) {
    $("connection").textContent = "Telemetry stale";
    $("connection").classList.add("offline");
    disableControls();
  }
}, 250);
function disableControls() {
  for (const id of [
    "arm",
    "takeoff",
    "land",
    "disturb",
    "throttle",
    "altitude-assist",
    "target-height",
  ])
    $(id).disabled = true;
}
function formatTime(seconds) {
  const min = Math.floor(seconds / 60);
  return `${String(min).padStart(2, "0")}:${(seconds % 60).toFixed(1).padStart(4, "0")}`;
}
const number = (id, value, digits = 1) => {
  $(id).textContent = value.toFixed(digits);
};
for (let i = 1; i <= 4; i++) {
  const row = document.createElement("div");
  row.className = "motor-row";
  const name = document.createElement("span");
  name.textContent = `M${i}`;
  const track = document.createElement("div");
  track.className = "motor-track";
  const fill = document.createElement("span");
  fill.id = `motor-fill-${i}`;
  track.append(fill);
  const output = document.createElement("span");
  output.id = `motor-value-${i}`;
  output.textContent = "1000";
  row.append(name, track, output);
  $("motors").append(row);
}
function syncSetting(id, value) {
  if (document.activeElement !== $(id)) $(id).value = value;
}
function render(t) {
  const guest = t.workbench?.driver === "firmware";
  $("attitude-source").textContent = guest
    ? "Simulation truth · guest estimator is not exported"
    : "Estimated orientation · MPU6050 fusion";
  number("altitude", t.position_m.z, 2);
  number("battery", t.battery_voltage, 2);
  $("battery-pct").textContent = `${Math.round(t.battery_percent)}%`;
  $("battery-health").textContent =
    t.battery_voltage > 10.8
      ? "Healthy"
      : t.battery_voltage > 10.2
        ? "Low voltage"
        : "Critical";
  $("latency").textContent = t.command_age_ms ?? "—";
  $("loop-hz").textContent = Math.round(t.measured_loop_hz);
  $("loop-period").textContent = t.timing_overruns
    ? `${t.timing_overruns} host overrun${t.timing_overruns > 1 ? "s" : ""}`
    : "2.0 ms nominal";
  const linkOk = t.command_age_ms !== null && t.command_age_ms < 500;
  $("link-dot").style.background = linkOk ? "#4d8268" : "#cd7968";
  $("link-label").textContent = t.faults.link_loss
    ? "Virtual Wi-Fi interrupted"
    : linkOk
      ? `${t.settings.latency_ms} ms link · ${t.settings.packet_loss}% loss`
      : "No fresh controller input";
  number("roll", t.attitude_deg.x);
  number("pitch", t.attitude_deg.y);
  number("yaw", t.attitude_deg.z);
  $("sim-time").textContent = formatTime(t.sim_time_s);
  $("state-badge").textContent = guest
    ? t.workbench.firmware.running
      ? "FIRMWARE RUNNING"
      : "FIRMWARE STOPPED"
    : t.state === "armed"
      ? t.phase === "flying"
        ? "IN FLIGHT"
        : t.phase === "landing"
          ? "LANDING"
          : "ARMED · IDLE"
      : t.state.toUpperCase();
  $("state-badge").className = `state-badge ${t.state}`;
  $("calibration-label").textContent =
    t.calibration_progress === 1
      ? "READY"
      : `${Math.round(t.calibration_progress * 100)}%`;
  $("calibration-bar").style.width = `${t.calibration_progress * 100}%`;
  const checks = [
    t.calibration_progress === 1 && !t.faults.imu_failure,
    linkOk,
    t.battery_voltage >= 10.8,
  ];
  ["check-imu", "check-link", "check-battery"].forEach((id, i) =>
    $(id).classList.toggle("ok", checks[i]),
  );
  $("preflight-count").textContent = `${checks.filter(Boolean).length} / 3`;
  $("control-status").textContent = owner ? "YOUR CONTROL" : "OBSERVER";
  $("emergency").disabled = !owner;
  $("claim-control").hidden = owner;
  $("resume-controls").hidden = !owner || !suspended;
  $("arm").disabled =
    !owner ||
    t.state === "calibrating" ||
    (t.state === "disarmed" &&
      (!checks.every(Boolean) || pilot.throttle > 0.03));
  $("arm").querySelector("span").textContent =
    t.state === "failsafe"
      ? "Acknowledge & disarm"
      : t.state === "armed"
        ? "Disarm motors"
        : "Arm motors";
  $("arm").className =
    `button arm-button ${t.state === "armed" ? "is-armed" : t.state === "failsafe" ? "is-failsafe" : ""}`;
  $("takeoff").disabled =
    !owner ||
    t.state !== "armed" ||
    !t.settings.altitude_assist ||
    t.phase === "flying" ||
    t.phase === "landing";
  $("land").disabled = !owner || t.state !== "armed" || t.phase !== "flying";
  $("disturb").disabled = !owner || t.state !== "armed" || t.position_m.z < 0.2;
  $("altitude-assist").disabled = !owner || t.state === "armed";
  $("altitude-assist").checked = t.settings.altitude_assist;
  $("target-height").disabled = !owner || !t.settings.altitude_assist;
  $("throttle").disabled = !owner || t.settings.altitude_assist;
  $("apply-pid").disabled = !owner || t.state === "armed";
  $("default-pid").disabled = !owner || t.state === "armed";
  for (const id of ["yaw-range", "roll-range", "pitch-range", "reset"])
    $(id).disabled = !owner;
  for (const node of document.querySelectorAll("[data-fault]")) {
    node.checked = t.faults[node.dataset.fault];
    node.disabled = !owner;
  }
  for (let i = 0; i < 4; i++) {
    $(`motor-value-${i + 1}`).textContent = t.motors.pwm_us[i];
    $(`motor-fill-${i + 1}`).style.width = `${t.motors.normalized[i] * 100}%`;
  }
  number("imu-temp", t.imu.temperature_c);
  $("imu-temp").textContent += " °C";
  const vec = (v) => `${v.x.toFixed(3)}, ${v.y.toFixed(3)}, ${v.z.toFixed(3)}`;
  $("imu-accel").textContent = vec(t.imu.accel);
  $("imu-gyro").textContent = vec(t.imu.gyro);
  if (!t.workbench?.validation.wiring.imu_powered || t.faults.imu_failure) {
    for (const id of ["imu-temp", "imu-accel", "imu-gyro"])
      $(id).textContent = "Unavailable";
  }
  syncSetting("target-height", t.settings.target_altitude);
  $("target-height-output").textContent =
    `${Number($("target-height").value).toFixed(1)} m`;
  // Form edits remain local until Apply; incoming telemetry must not overwrite them.
  if (!render.initialized || t.tick < (render.previousTick || 0)) {
    for (const [id, value] of Object.entries({
      kp: t.gains.kp,
      ki: t.gains.ki,
      kd: t.gains.kd,
      "latency-setting": t.settings.latency_ms,
      "loss-setting": t.settings.packet_loss,
      "wind-setting": t.settings.wind,
    }))
      $(id).value = value;
    updateEnvironmentOutputs();
    render.initialized = true;
  }
  render.previousTick = t.tick;
  $("throttle-help").textContent = t.settings.altitude_assist
    ? "Disable altitude assist while disarmed to use manual throttle."
    : "Direct ESC throttle. Hover is approximately 67% in this model.";
  $("controller-hint").textContent = owner
    ? t.state === "armed"
      ? "Small inputs are enough. Space cuts all motors."
      : "Arm at zero throttle, then select Take off."
    : "Observer mode. Take control to use these sticks.";
  if (guest) {
    for (const id of [
      "arm",
      "takeoff",
      "land",
      "throttle",
      "altitude-assist",
      "target-height",
      "disturb",
      "apply-pid",
      "default-pid",
    ])
      $(id).disabled = true;
    $("calibration-label").textContent = "GUEST";
    $("controller-hint").textContent =
      "Firmware owns the motors. Use its UART protocol in Virtual hardware.";
  }
  if (t.state === "failsafe") {
    $("notice").hidden = false;
    $("notice").classList.add("danger");
    $("notice").textContent =
      `Failsafe: ${t.reason.replaceAll("_", " ")}. Motors cut. Clear the fault, then select Acknowledge & disarm.`;
  } else if (render.lastState === "failsafe") {
    $("notice").hidden = true;
  }
  render.lastState = t.state;
  if (t.events[0]?.id !== lastEvent) {
    lastEvent = t.events[0]?.id;
    $("event-count").textContent = t.events.length;
    $("event-log").replaceChildren(
      ...t.events.map((event) => {
        const row = document.createElement("div");
        row.className = `log-row ${event.level}`;
        const time = document.createElement("time");
        time.textContent = formatTime(event.time_s);
        const dot = document.createElement("span");
        dot.className = "event-dot";
        const text = document.createElement("span");
        text.textContent = event.message;
        row.append(time, dot, text);
        return row;
      }),
    );
  }
}
function settings(overrides = {}) {
  if (!latest) return;
  return send({ type: "settings", ...latest.settings, ...overrides });
}
$("arm").addEventListener("click", () => {
  if (!latest || !owner) return;
  if (latest.state === "armed" || latest.state === "failsafe") {
    pilot.arm = false;
    pilot.throttle = 0;
    $("throttle").value = 0;
    $("throttle-output").textContent = "0%";
    neutralize();
  } else pilot.arm = true;
  command();
});
$("takeoff").addEventListener("click", () => action("takeoff"));
$("land").addEventListener("click", () => action("land"));
$("emergency").addEventListener("click", emergency);
$("disturb").addEventListener("click", () => action("disturb"));
$("claim-control").addEventListener("click", () => {
  pilot.arm = false;
  pilot.throttle = 0;
  neutralize();
  action("claim_control");
});
$("reset").addEventListener("click", () => {
  pilot.arm = false;
  pilot.throttle = 0;
  suspended = false;
  neutralize();
  $("throttle").value = 0;
  $("throttle-output").textContent = "0%";
  if (action("reset")) {
    resetAfterEvent = latest?.events[0]?.id ?? 0;
    disableControls();
    $("state-badge").textContent = "CALIBRATING";
    $("preflight-count").textContent = "0 / 3";
  }
});
$("resume-controls").addEventListener("click", () => {
  suspended = false;
  pilot.arm = false;
  pilot.throttle = 0;
  neutralize();
  command();
});
$("altitude-assist").addEventListener("change", (e) => {
  pilot.throttle = 0;
  $("throttle").value = 0;
  $("throttle-output").textContent = "0%";
  settings({ altitude_assist: e.target.checked });
});
$("target-height").addEventListener("input", (e) => {
  $("target-height-output").textContent =
    `${Number(e.target.value).toFixed(1)} m`;
});
$("target-height").addEventListener("change", (e) =>
  settings({ target_altitude: Number(e.target.value) }),
);
$("throttle").addEventListener("input", (e) => {
  pilot.throttle = Number(e.target.value);
  $("throttle-output").textContent = `${Math.round(pilot.throttle * 100)}%`;
});
for (const axis of ["yaw", "roll", "pitch"])
  $(`${axis}-range`).addEventListener("input", (e) => {
    pilot[axis] = Number(e.target.value);
    updateStickPositions();
  });
$("center").addEventListener("click", neutralize);
const keys = new Set();
function updateKeys() {
  pilot.roll = (keys.has("d") ? 0.5 : 0) - (keys.has("a") ? 0.5 : 0);
  pilot.pitch = (keys.has("w") ? 0.5 : 0) - (keys.has("s") ? 0.5 : 0);
  pilot.yaw = (keys.has("q") ? 0.5 : 0) - (keys.has("e") ? 0.5 : 0);
  updateStickPositions();
}
function updateStickPositions() {
  $("tilt-stick").querySelector(".stick-thumb").style.transform =
    `translate(${pilot.roll * 32}px,${-pilot.pitch * 32}px)`;
  $("yaw-stick").querySelector(".stick-thumb").style.transform =
    `translate(${-pilot.yaw * 32}px,0px)`;
  for (const axis of ["yaw", "roll", "pitch"])
    $(`${axis}-range`).value = pilot[axis];
}
document.addEventListener("keydown", (e) => {
  if (["INPUT", "TEXTAREA", "SELECT"].includes(e.target.tagName)) return;
  if (e.code === "Space") {
    if (owner && !e.repeat) {
      e.preventDefault();
      emergency();
    }
    return;
  }
  if (
    !owner ||
    page !== "flight" ||
    !["w", "a", "s", "d", "q", "e"].includes(e.key.toLowerCase())
  )
    return;
  e.preventDefault();
  keys.add(e.key.toLowerCase());
  updateKeys();
});
document.addEventListener("keyup", (e) => {
  if (keys.delete(e.key.toLowerCase())) updateKeys();
});
window.addEventListener("blur", neutralize);
document.addEventListener("visibilitychange", () => {
  if (document.hidden && owner) {
    suspended = true;
    neutralize();
    pilot.throttle = 0;
  }
});
function stick(id, axes) {
  const surface = $(id);
  let activePointer = null;
  function move(e) {
    if (activePointer !== e.pointerId || !owner) return;
    const rect = surface.getBoundingClientRect(),
      range = rect.width * 0.32;
    let x = (e.clientX - rect.left - rect.width / 2) / range,
      y = (e.clientY - rect.top - rect.height / 2) / range;
    const length = Math.hypot(x, y);
    if (length > 1) {
      x /= length;
      y /= length;
    }
    if (axes === "tilt") {
      pilot.roll = x;
      pilot.pitch = -y;
    } else pilot.yaw = -x;
    updateStickPositions();
  }
  surface.addEventListener("pointerdown", (e) => {
    if (!owner) return;
    e.preventDefault();
    activePointer = e.pointerId;
    surface.setPointerCapture(e.pointerId);
    move(e);
  });
  surface.addEventListener("pointermove", move);
  const release = (e) => {
    if (e.pointerId !== activePointer) return;
    activePointer = null;
    if (axes === "tilt") {
      pilot.roll = 0;
      pilot.pitch = 0;
    } else pilot.yaw = 0;
    updateStickPositions();
  };
  for (const event of ["pointerup", "pointercancel", "lostpointercapture"])
    surface.addEventListener(event, release);
}
stick("tilt-stick", "tilt");
stick("yaw-stick", "yaw");
document.querySelectorAll("[data-camera]").forEach((button) =>
  button.addEventListener("click", () => {
    document.querySelectorAll("[data-camera]").forEach((b) => {
      const active = b === button;
      b.classList.toggle("active", active);
      b.setAttribute("aria-pressed", active);
    });
    scene?.setCamera(button.dataset.camera);
  }),
);
$("pid-form").addEventListener("submit", (e) => {
  e.preventDefault();
  send({
    type: "tune",
    kp: Number($("kp").value),
    ki: Number($("ki").value),
    kd: Number($("kd").value),
  });
});
$("default-pid").addEventListener("click", () => {
  $("kp").value = 0.07;
  $("ki").value = 0.025;
  $("kd").value = 0.0015;
  send({ type: "tune", kp: 0.07, ki: 0.025, kd: 0.0015 });
});
function updateEnvironmentOutputs() {
  $("latency-setting-output").textContent = `${$("latency-setting").value} ms`;
  $("loss-setting-output").textContent = `${$("loss-setting").value}%`;
  $("wind-setting-output").textContent =
    `${Number($("wind-setting").value).toFixed(1)} m/s`;
}
for (const id of ["latency-setting", "loss-setting", "wind-setting"])
  $(id).addEventListener("input", updateEnvironmentOutputs);
$("environment-form").addEventListener("submit", (e) => {
  e.preventDefault();
  settings({
    latency_ms: Number($("latency-setting").value),
    packet_loss: Number($("loss-setting").value),
    wind: Number($("wind-setting").value),
  });
});
document.querySelectorAll("[data-fault]").forEach((input) =>
  input.addEventListener("change", () =>
    send({
      type: "fault",
      fault: input.dataset.fault,
      enabled: input.checked,
    }),
  ),
);
$("export").addEventListener("click", () => {
  if (!samples.length) {
    notice("No telemetry captured yet. Connect to the simulator first.");
    return;
  }
  const fields = [
    "time_s",
    "state",
    "reason",
    "altitude_m",
    "x_m",
    "y_m",
    "roll_deg",
    "pitch_deg",
    "yaw_deg",
    "truth_roll_deg",
    "truth_pitch_deg",
    "truth_yaw_deg",
    "battery_v",
    "command_age_ms",
    "m1_us",
    "m2_us",
    "m3_us",
    "m4_us",
  ];
  const rows = samples.map((t) =>
    [
      t.sim_time_s,
      t.state,
      t.reason || "",
      t.position_m.z,
      t.position_m.x,
      t.position_m.y,
      t.attitude_deg.x,
      t.attitude_deg.y,
      t.attitude_deg.z,
      t.truth_attitude_deg.x,
      t.truth_attitude_deg.y,
      t.truth_attitude_deg.z,
      t.battery_voltage,
      t.command_age_ms ?? "",
      ...t.motors.pwm_us,
    ].join(","),
  );
  const url = URL.createObjectURL(
    new Blob([[fields.join(","), ...rows].join("\n")], {
      type: "text/csv;charset=utf-8",
    }),
  );
  const anchor = document.createElement("a");
  anchor.href = url;
  anchor.download = `aerolab-flight-${new Date().toISOString().slice(0, 19).replaceAll(":", "-")}.csv`;
  anchor.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
  notice(`Exported ${samples.length.toLocaleString()} telemetry snapshots.`);
});
function drawChart() {
  if (page !== "flight") return;
  const canvas = $("attitude-chart"),
    ctx = canvas.getContext("2d"),
    rect = canvas.getBoundingClientRect();
  if (!rect.width) return;
  const dpr = Math.min(devicePixelRatio, 2);
  canvas.width = rect.width * dpr;
  canvas.height = rect.height * dpr;
  ctx.scale(dpr, dpr);
  const w = rect.width,
    h = rect.height,
    left = 30,
    right = w - 10,
    top = 16,
    bottom = h - 19;
  ctx.font = "7px ui-monospace, monospace";
  ctx.fillStyle = "#a4ae99";
  ctx.strokeStyle = "#eef1e8";
  ctx.lineWidth = 1;
  const end = latest?.sim_time_s || 0,
    start = end - 12,
    view = samples.filter((t) => t.sim_time_s >= start);
  const maximum = Math.max(
    15,
    ...view.map((t) =>
      Math.max(
        Math.abs(t.attitude_deg.x),
        Math.abs(t.attitude_deg.y),
        Math.abs(t.attitude_deg.z),
      ),
    ),
  );
  const scale = Math.ceil(maximum / 15) * 15;
  for (const value of [-scale, 0, scale]) {
    const y = top + (1 - (value + scale) / (scale * 2)) * (bottom - top);
    ctx.beginPath();
    ctx.moveTo(left, y);
    ctx.lineTo(right, y);
    ctx.stroke();
    ctx.fillText(`${value}°`, 4, y + 2);
  }
  for (let n = 0; n <= 4; n++) {
    const x = left + ((right - left) * n) / 4;
    ctx.beginPath();
    ctx.moveTo(x, top);
    ctx.lineTo(x, bottom);
    ctx.stroke();
    ctx.fillText(n === 4 ? "now" : `${12 - n * 3}s`, x - 5, h - 5);
  }
  for (const [axis, color] of [
    ["x", "#df8b61"],
    ["y", "#799b8a"],
    ["z", "#92a8bb"],
  ]) {
    ctx.beginPath();
    ctx.strokeStyle = color;
    ctx.lineWidth = 1.4;
    let first = true;
    for (const t of view) {
      const x = left + ((t.sim_time_s - start) / 12) * (right - left),
        y =
          top +
          (1 - (t.attitude_deg[axis] + scale) / (2 * scale)) * (bottom - top);
      if (first) ctx.moveTo(x, y);
      else ctx.lineTo(x, y);
      first = false;
    }
    ctx.stroke();
  }
}
new ResizeObserver(drawChart).observe($("attitude-chart"));
connect();
