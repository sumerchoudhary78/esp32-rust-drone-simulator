const $ = (id) => document.getElementById(id),
  NS = "http://www.w3.org/2000/svg";
const labels = {
  esp32: "ESP32 DevKitC-32E",
  mpu6050: "MPU6050",
  esc: "30 A ESC",
  motor: "A2212 · 1000 KV",
  battery: "3S LiPo · 2200 mAh",
  regulator: "5 V regulator",
  divider: "100k / 27k divider",
};
const colors = {
  esp32: "#304e4b",
  mpu6050: "#467c90",
  esc: "#464d57",
  motor: "#7b6960",
  battery: "#bd784e",
  regulator: "#7a8653",
  divider: "#9c8256",
};
const clone = (v) => JSON.parse(JSON.stringify(v));
function element(name, attrs = {}, text) {
  const e = document.createElementNS(NS, name);
  for (const [key, value] of Object.entries(attrs)) e.setAttribute(key, value);
  if (text !== undefined) e.textContent = text;
  return e;
}
function shape(d, pins) {
  const w = d.kind === "esp32" ? 180 : d.kind === "motor" ? 135 : 170;
  const h = d.kind === "esp32" ? 500 : d.kind === "mpu6050" ? 145 : 115,
    points = {};
  pins.forEach((pin, i) => {
    let right, y;
    if (d.kind === "esp32") {
      right = i >= 19;
      y = 40 + (i % 19) * 23;
    } else if (d.kind === "esc") {
      right = i >= 3;
      y = 42 + (i % 3) * 26;
    } else if (d.kind === "mpu6050") {
      right = i >= 3;
      y = 45 + (i % 3) * 30;
    } else if (d.kind === "motor") {
      right = false;
      y = 42 + i * 26;
    } else {
      right = ["+", "-", "OUT5V", "OUT"].includes(pin);
      y = 50 + (i === 1 ? 32 : 0);
    }
    points[pin] = { x: right ? w : 0, y, right };
  });
  return { w, h, points };
}
export function createWorkbench({ send, notice }) {
  let catalog = {},
    preset,
    circuit,
    selected,
    armedPin,
    dragging,
    latest;
  let dirty = false,
    revision = -1,
    history = [],
    validation,
    image,
    validationTimer,
    validationVersion = 0,
    pendingApply = null;
  const svg = $("circuit-canvas"),
    wiresGroup = $("circuit-wires"),
    devicesGroup = $("circuit-devices");
  const key = (p) => `${p.device}.${p.pin}`;
  function remember() {
    history.push(clone(circuit));
    if (history.length > 30) history.shift();
  }
  function edited() {
    dirty = true;
    $("circuit-dirty").textContent =
      "Draft circuit · Apply to connect these devices";
    try {
      localStorage.setItem("aerolab-circuit-v1", JSON.stringify(circuit));
      $("restore-circuit").hidden = false;
    } catch {}
    draw();
    clearTimeout(validationTimer);
    const version = ++validationVersion;
    validationTimer = setTimeout(() => validate(version), 120);
  }
  async function validate(version = ++validationVersion) {
    try {
      const response = await fetch("/api/circuit/validate", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(circuit),
      });
      if (!response.ok) throw new Error("Invalid circuit document");
      const result = await response.json();
      if (version !== validationVersion) return;
      validation = result;
      showValidation(result);
    } catch (e) {
      if (version === validationVersion) {
        validation = null;
        $("circuit-validation").textContent = e.message;
      }
    }
  }
  function showValidation(v) {
    $("circuit-result").textContent = v.flight_connected
      ? "All 13 devices connected"
      : v.valid
        ? "Circuit valid · connections incomplete"
        : "Wiring error · cannot apply";
    $("circuit-result").className = v.flight_connected
      ? "connection-pill connected"
      : "connection-pill";
    const lines = [...v.errors, ...v.warnings];
    $("circuit-validation").replaceChildren(
      ...(lines.length
        ? lines
        : [
            "Power, ground, I²C, four ESC signals, motor phases and battery ADC connected.",
          ]
      ).map((text) => {
        const li = document.createElement("li");
        li.textContent = text;
        return li;
      }),
    );
    $("apply-circuit").disabled = !v.valid;
  }
  function connectPin(pin) {
    if (!armedPin) {
      armedPin = pin;
      selected = pin.device;
      draw();
      return;
    }
    if (key(pin) !== key(armedPin)) {
      const signature = [armedPin, pin].map(key).sort().join("|");
      const duplicate = circuit.wires.some(
        (w) => [w.from, w.to].map(key).sort().join("|") === signature,
      );
      if (!duplicate) {
        remember();
        circuit.wires.push({ from: armedPin, to: pin });
        armedPin = null;
        edited();
        return;
      }
    }
    armedPin = null;
    draw();
  }
  function endpoint(pin) {
    const d = circuit.devices.find((d) => d.id === pin.device);
    if (!d) return;
    const p = shape(d, catalog[d.kind]).points[pin.pin];
    if (!p) return;
    return { x: d.x + p.x, y: d.y + p.y, right: p.right };
  }
  function wireColor(w) {
    const p = [w.from.pin, w.to.pin];
    if (p.some((p) => p === "-" || p.startsWith("GND"))) return "#73817e";
    if (p.some((p) => ["+", "BAT+", "IN"].includes(p))) return "#c06b50";
    if (p.some((p) => ["5V", "OUT5V"].includes(p))) return "#d3973c";
    if (p.some((p) => ["VCC", "3V3"].includes(p))) return "#9b75ad";
    if (p.includes("SDA")) return "#4991b2";
    if (p.includes("SCL")) return "#76a06b";
    return p.includes("SIGNAL") ? "#d69a51" : "#8a959f";
  }
  function draw() {
    if (!circuit) return;
    wiresGroup.replaceChildren();
    devicesGroup.replaceChildren();
    circuit.wires.forEach((wire, i) => {
      const a = endpoint(wire.from),
        b = endpoint(wire.to);
      if (!a || !b) return;
      const bend = Math.max(38, Math.min(130, Math.abs(a.x - b.x) * 0.45));
      const path = element("path", {
        d: `M${a.x} ${a.y} C${a.x + (a.right ? bend : -bend)} ${a.y},${b.x + (b.right ? bend : -bend)} ${b.y},${b.x} ${b.y}`,
        stroke: wireColor(wire),
        fill: "none",
        "stroke-width": 2.2,
        class: "circuit-wire",
        tabindex: 0,
        role: "button",
        "aria-label": `Delete wire ${key(wire.from)} to ${key(wire.to)}`,
      });
      const remove = () => {
        remember();
        circuit.wires.splice(i, 1);
        edited();
      };
      path.addEventListener("click", remove);
      path.addEventListener("keydown", (e) => {
        if (["Enter", "Delete", "Backspace"].includes(e.key)) {
          e.preventDefault();
          remove();
        }
      });
      wiresGroup.append(path);
    });
    for (const d of circuit.devices) {
      const { w, h, points } = shape(d, catalog[d.kind]);
      const group = element("g", {
        transform: `translate(${d.x},${d.y})`,
        class: `circuit-device ${selected === d.id ? "selected" : ""}`,
        "data-device": d.id,
      });
      const body = element("rect", {
        x: 8,
        y: 0,
        width: w - 16,
        height: h,
        rx: 10,
        fill: colors[d.kind],
        class: "device-body",
      });
      group.append(
        body,
        element(
          "text",
          { x: 20, y: 22, class: "device-title" },
          d.kind === "esc"
            ? `ESC ${d.slot}`
            : d.kind === "motor"
              ? `M${d.slot} · ${[1, 3].includes(d.slot) ? "CW" : "CCW"}`
              : d.kind === "esp32"
                ? "ESP32"
                : labels[d.kind],
        ),
      );
      if (d.kind === "esp32")
        group.append(
          element("rect", {
            x: 52,
            y: 95,
            width: 76,
            height: 155,
            rx: 4,
            fill: "#a5aeab",
          }),
          element(
            "text",
            { x: 90, y: 160, "text-anchor": "middle", class: "chip-label" },
            "ESP32",
          ),
          element(
            "text",
            { x: 90, y: 179, "text-anchor": "middle", class: "chip-label" },
            "WROOM-32E",
          ),
          element("rect", {
            x: 70,
            y: 455,
            width: 40,
            height: 35,
            rx: 4,
            fill: "#b8bfbc",
          }),
          element("path", {
            d: "M58 80v-38h12v28h12v-28h12v28h12v-28h14v38",
            stroke: "#b5ad71",
            fill: "none",
            "stroke-width": 3,
          }),
        );
      if (d.kind === "motor")
        group.append(
          element("circle", { cx: 88, cy: 68, r: 27, fill: "#b8b0a8" }),
          element("circle", { cx: 88, cy: 68, r: 9, fill: "#474943" }),
        );
      for (const [pin, p] of Object.entries(points)) {
        const active = armedPin?.device === d.id && armedPin?.pin === pin;
        const node = element("g", {
          transform: `translate(${p.x},${p.y})`,
          class: `circuit-pin ${active ? "active" : ""}`,
          tabindex: 0,
          role: "button",
          "aria-label": `${d.id} ${pin}`,
          "data-pin": `${d.id}.${pin}`,
        });
        node.append(
          element("circle", { r: 11, fill: "transparent" }),
          element("circle", {
            r: 4.5,
            fill: active ? "#ffc66e" : "#e1d6aa",
            stroke: "white",
            "stroke-width": 1,
          }),
          element(
            "text",
            {
              x: p.right ? -10 : 10,
              y: 3,
              "text-anchor": p.right ? "end" : "start",
              class: "pin-label",
            },
            pin,
          ),
        );
        node.addEventListener("pointerdown", (e) => e.stopPropagation());
        node.addEventListener("click", (e) => {
          e.stopPropagation();
          connectPin({ device: d.id, pin });
        });
        node.addEventListener("keydown", (e) => {
          if (e.key === "Enter") {
            e.preventDefault();
            connectPin({ device: d.id, pin });
          }
        });
        group.append(node);
      }
      body.addEventListener("pointerdown", (e) => {
        if (e.button !== 0) return;
        e.preventDefault();
        remember();
        selected = d.id;
        const p = canvasPoint(e);
        dragging = { id: d.id, dx: p.x - d.x, dy: p.y - d.y };
        svg.setPointerCapture(e.pointerId);
        draw();
      });
      group.addEventListener("click", () => {
        selected = d.id;
        drawInspector();
      });
      devicesGroup.append(group);
    }
    $("wire-count").textContent =
      `${circuit.devices.length} devices · ${circuit.wires.length} wires`;
    $("pin-hint").textContent = armedPin
      ? `Connect ${key(armedPin)} → select another pin. Esc cancels.`
      : "Click two pins to wire. Drag a device to move it. Click a wire to remove it.";
    $("undo-circuit").disabled = !history.length;
    drawInspector();
  }
  function canvasPoint(e) {
    const p = svg.createSVGPoint();
    p.x = e.clientX;
    p.y = e.clientY;
    return p.matrixTransform(svg.getScreenCTM().inverse());
  }
  svg.addEventListener("pointermove", (e) => {
    if (!dragging) return;
    const p = canvasPoint(e),
      d = circuit.devices.find((d) => d.id === dragging.id),
      s = shape(d, catalog[d.kind]);
    d.x = Math.max(16, Math.min(1210 - s.w, p.x - dragging.dx));
    d.y = Math.max(12, Math.min(870 - s.h, p.y - dragging.dy));
    draw();
  });
  for (const name of ["pointerup", "pointercancel"])
    svg.addEventListener(name, () => {
      if (dragging) {
        dragging = null;
        edited();
      }
    });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      armedPin = null;
      draw();
    }
  });
  function drawInspector() {
    const d = circuit?.devices.find((d) => d.id === selected);
    $("device-inspector-title").textContent = d
      ? labels[d.kind] + (d.slot ? ` · M${d.slot}` : "")
      : "Select a device";
    const details = {
      esp32:
        "Classic Xtensa LX6, ECO3, 4 MiB. All 38 header pins are shown; flash pins and input-only pins cannot drive ESCs.",
      mpu6050:
        "I²C 0x68 / 0x69. WHO_AM_I, sleep/reset, register pointer, 14-byte samples and selectable accel/gyro ranges. Pull-ups are modeled at 3.3 V.",
      esc: "40–500 Hz PWM, 1000–2000 µs. Requires power, common ground, signal and three motor phases. DShot and ESC calibration are not modeled.",
      motor:
        "Fixed X-frame position and propeller direction. Match A/B/C. Quadratic thrust, 45 ms response; winding currents are not simulated.",
      battery:
        "3S, 12.6 V full. Models charge, load sag and discharge; the drawing does not qualify a physical battery.",
      regulator:
        "Ideal 5 V regulator plus the board's 3.3 V rail. Supply from battery + and share ground; no transient or thermal model.",
      divider:
        "100 kΩ / 27 kΩ. Ideal 12-bit ADC1 with a 3.3 V reference. Real ADC calibration is still required.",
    };
    $("device-inspector-description").textContent = d
      ? details[d.kind]
      : "Choose a component, connect its pins, then apply the circuit to the running simulation.";
    $("remove-device").disabled = !d;
  }
  $("remove-device").addEventListener("click", () => {
    if (!selected) return;
    remember();
    circuit.devices = circuit.devices.filter((d) => d.id !== selected);
    circuit.wires = circuit.wires.filter(
      (w) => w.from.device !== selected && w.to.device !== selected,
    );
    selected = null;
    armedPin = null;
    edited();
  });
  $("undo-circuit").addEventListener("click", () => {
    if (history.length) {
      circuit = history.pop();
      armedPin = null;
      edited();
    }
  });
  $("preset-circuit").addEventListener("click", () => {
    if (!preset) return;
    remember();
    circuit = clone(preset);
    selected = null;
    armedPin = null;
    edited();
  });
  $("new-circuit").addEventListener("click", () => {
    if (!circuit) return;
    remember();
    circuit = { version: 1, devices: [], wires: [] };
    selected = null;
    armedPin = null;
    edited();
  });
  try {
    $("restore-circuit").hidden = !localStorage.getItem("aerolab-circuit-v1");
  } catch {}
  $("restore-circuit").addEventListener("click", async () => {
    try {
      await loadDocument(
        JSON.parse(localStorage.getItem("aerolab-circuit-v1")),
      );
    } catch (e) {
      notice(e.message);
    }
  });
  async function loadDocument(document) {
    const response = await fetch("/api/circuit/validate", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(document),
    });
    if (!response.ok) throw new Error("Unsupported circuit JSON");
    const result = await response.json();
    if (!result.valid) throw new Error(result.errors.join(" · "));
    remember();
    circuit = {
      version: document.version,
      devices: document.devices.map((d) => ({
        id: d.id,
        kind: d.kind,
        x: d.x,
        y: d.y,
        slot: d.slot ?? null,
      })),
      wires: document.wires.map((w) => ({
        from: { device: w.from.device, pin: w.from.pin },
        to: { device: w.to.device, pin: w.to.pin },
      })),
    };
    selected = null;
    armedPin = null;
    edited();
  }
  $("export-circuit").addEventListener("click", () => {
    const url = URL.createObjectURL(
      new Blob([JSON.stringify(circuit, null, 2) + "\n"], {
        type: "application/json",
      }),
    );
    const a = document.createElement("a");
    a.href = url;
    a.download = "aerolab-circuit.json";
    a.click();
    setTimeout(() => URL.revokeObjectURL(url), 1000);
  });
  $("import-circuit").addEventListener("change", async (e) => {
    try {
      const f = e.target.files[0];
      if (!f) return;
      if (f.size > 32768) throw new Error("Circuit JSON exceeds 32 KiB");
      await loadDocument(JSON.parse(await f.text()));
    } catch (error) {
      notice(error.message, true);
    }
    e.target.value = "";
  });
  $("apply-circuit").addEventListener("click", () => {
    if (!validation?.valid) {
      notice("Correct the wiring errors before applying", true);
      return;
    }
    if (send({ type: "circuit", circuit })) {
      pendingApply = {
        json: JSON.stringify(circuit),
        version: validationVersion,
      };
      $("circuit-dirty").textContent = "Applying circuit · motors stopped";
    }
  });
  $("zoom-circuit").addEventListener("click", () => {
    const detailed =
      $("circuit-canvas").parentElement.classList.toggle("detailed");
    $("zoom-circuit").textContent = detailed ? "Fit circuit" : "View full size";
    $("zoom-circuit").setAttribute("aria-pressed", String(detailed));
  });
  document.querySelectorAll("[data-add-device]").forEach((button) =>
    button.addEventListener("click", () => {
      if (!circuit) return;
      const kind = button.dataset.addDevice;
      let slot = null;
      if (["esc", "motor"].includes(kind)) {
        slot = [1, 2, 3, 4].find(
          (s) => !circuit.devices.some((d) => d.kind === kind && d.slot === s),
        );
        if (!slot) {
          notice("All four positions already have this device");
          return;
        }
      } else if (circuit.devices.some((d) => d.kind === kind)) {
        notice("This assembly supports one of this device");
        return;
      }
      remember();
      let id = kind + (slot || "");
      while (circuit.devices.some((d) => d.id === id)) id += "_";
      circuit.devices.push({
        id,
        kind,
        x: 120 + (circuit.devices.length % 5) * 175,
        y: 80 + (circuit.devices.length % 3) * 130,
        slot,
      });
      selected = id;
      edited();
    }),
  );
  $("firmware-file").addEventListener("change", async (e) => {
    const file = e.target.files[0];
    if (!file) return;
    image = null;
    $("start-firmware").disabled = true;
    $("firmware-upload-status").textContent = "Preparing firmware…";
    try {
      if (file.size > 16 * 1024 * 1024)
        throw new Error("Firmware exceeds 16 MiB");
      const response = await fetch(
        `/api/firmware?name=${encodeURIComponent(file.name)}`,
        {
          method: "POST",
          headers: { "Content-Type": "application/octet-stream" },
          body: file,
        },
      );
      const result = await response.json();
      if (!response.ok) throw new Error(result.error || "Upload rejected");
      image = result;
      $("firmware-upload-status").textContent =
        `${result.name} · ready to start`;
      $("start-firmware").disabled = false;
    } catch (error) {
      $("firmware-upload-status").textContent = error.message;
      notice(error.message, true);
    }
  });
  $("start-firmware").addEventListener("click", () => {
    if (!image) return;
    if (dirty) {
      notice("Apply the circuit before starting firmware");
      return;
    }
    send({ type: "firmware_start", id: image.id });
  });
  $("stop-firmware").addEventListener("click", () =>
    send({ type: "action", action: "firmware_stop" }),
  );
  $("builtin-firmware").addEventListener("click", () =>
    send({ type: "action", action: "builtin_controller" }),
  );
  $("uart-form").addEventListener("submit", (e) => {
    e.preventDefault();
    send({ type: "firmware_uart", text: $("uart-input").value + "\n" });
    $("uart-input").value = "";
  });
  for (const b of document.querySelectorAll("[data-uart]"))
    b.addEventListener("click", () =>
      send({ type: "firmware_uart", text: b.dataset.uart + "\n" }),
    );
  fetch("/api/workbench")
    .then((r) => r.json())
    .then((data) => {
      catalog = Object.fromEntries(data.catalog.map((c) => [c.kind, c.pins]));
      preset = data.preset;
      circuit = clone(latest?.workbench?.circuit || preset);
      draw();
      validate();
    })
    .catch((e) => notice(`Workbench unavailable: ${e.message}`));
  return {
    update(t) {
      latest = t;
      const w = t.workbench;
      if (!w) return;
      if (circuit && revision !== w.revision) {
        const server = JSON.stringify(w.circuit);
        const keepDraft =
          pendingApply?.json === server &&
          pendingApply.version !== validationVersion;
        if (!keepDraft) {
          if (JSON.stringify(circuit) !== server) {
            circuit = clone(w.circuit);
            armedPin = null;
            draw();
          }
          dirty = false;
          $("circuit-dirty").textContent =
            "Applied circuit · changes stop motors";
          validation = w.validation;
          showValidation(validation);
        }
        pendingApply = null;
        revision = w.revision;
      }
      const firmware = w.driver === "firmware",
        f = w.firmware;
      $("driver-status").textContent = firmware
        ? f.running
          ? "ESP32 CPU running"
          : "ESP32 CPU stopped"
        : "Built-in Rust controller";
      $("emulator-availability").textContent = w.backend_available
        ? "CPU backend installed · classic ESP32"
        : "CPU backend optional · run python3 scripts/setup_emulator.py";
      $("firmware-state").textContent = firmware
        ? f.message
        : "Choose an image to execute your own firmware on the virtual ESP32.";
      $("cpu-cycles").textContent = Number(f.cycles).toLocaleString();
      $("cpu-time").textContent = `${f.guest_time_s.toFixed(3)} s`;
      $("i2c-count").textContent =
        `${f.i2c_transactions} transfers · ${f.imu_reads} samples`;
      $("uart-output").textContent =
        f.uart.join("\n") || "UART0 output will appear here.";
      $("stop-firmware").disabled = !f.running;
      $("builtin-firmware").disabled = !firmware;
      $("start-firmware").disabled =
        !image || !w.backend_available || t.state === "armed";
      $("device-live").replaceChildren(
        ...t.motor_actual.map((speed, i) => {
          const row = document.createElement("div");
          row.className = "device-live-row";
          const a = document.createElement("b");
          a.textContent = `M${i + 1}`;
          const b = document.createElement("span");
          b.textContent = w.validation.wiring.motors_connected[i]
            ? `${t.motors.pwm_us[i]} µs · ~${Math.round(speed * t.battery_voltage * 1000 * 0.85)} rpm`
            : "Disconnected · no drive";
          row.append(a, b);
          return row;
        }),
      );
    },
  };
}
