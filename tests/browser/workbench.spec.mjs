import { test, expect } from "@playwright/test";
import { existsSync } from "node:fs";
import { readFile } from "node:fs/promises";

async function openWorkbench(page) {
  await page.goto("/#hardware");
  await expect(page.locator("#connection")).toHaveText("Connected");
  await expect(page.locator("#control-status")).toHaveText(
    /YOUR CONTROL|OBSERVER/,
  );
  if (await page.locator("#claim-control").isVisible())
    await page.locator("#claim-control").click();
  await expect(page.locator("#control-status")).toHaveText("YOUR CONTROL");
  await page.locator("#preset-circuit").click();
  await expect(page.locator("#circuit-result")).toHaveText(
    "All 13 devices connected",
  );
  await page.locator("#apply-circuit").click();
  await expect(page.locator("#circuit-dirty")).toHaveText(
    "Applied circuit · changes stop motors",
  );
  await page.locator("#builtin-firmware").evaluate((button) => {
    if (!button.disabled) button.click();
  });
}
async function pin(page, name) {
  await page.locator(`[data-pin="${name}"]`).press("Enter");
}
test("wire edits affect the sensor, reconnect, and export the actual assembly", async ({
  page,
  request,
}) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await openWorkbench(page);
  await expect(page.locator(".circuit-device")).toHaveCount(13);
  expect(
    (await page.locator("#circuit-canvas").boundingBox()).height,
  ).toBeGreaterThan(400);
  await expect(page.locator(".circuit-pin")).toHaveCount(88);
  const board = page.locator('[data-device="esp"] .device-body');
  const before = await board.boundingBox();
  await page.mouse.move(before.x + before.width / 2, before.y + 8);
  await page.mouse.down();
  await page.mouse.move(before.x + before.width / 2 + 20, before.y + 18, {
    steps: 5,
  });
  await page.mouse.up();
  expect((await board.boundingBox()).x).toBeGreaterThan(before.x + 10);
  await page.locator("#undo-circuit").click();
  await page.locator("#zoom-circuit").click();
  expect(
    (await page.locator("#circuit-canvas").boundingBox()).width,
  ).toBeGreaterThan(1100);
  await page.locator("#zoom-circuit").click();
  await page
    .getByRole("button", {
      name: "Delete wire esp.3V3 to imu.VCC",
      exact: true,
    })
    .press("Enter");
  await expect(page.locator("#circuit-result")).toHaveText(
    "Circuit valid · connections incomplete",
  );
  await page.locator("#apply-circuit").click();
  await expect
    .poll(
      async () =>
        (await (await request.get("/api/telemetry")).json()).workbench
          .validation.wiring.imu_powered,
    )
    .toBe(false);
  await page.waitForTimeout(1200);
  expect(
    (await (await request.get("/api/telemetry")).json()).calibration_progress,
  ).toBe(0);
  await pin(page, "esp.3V3");
  await pin(page, "imu.VCC");
  await expect(page.locator("#circuit-result")).toHaveText(
    "All 13 devices connected",
  );
  await page.locator("#apply-circuit").click();
  await expect
    .poll(
      async () =>
        (await (await request.get("/api/telemetry")).json())
          .calibration_progress,
    )
    .toBe(1);
  const event = page.waitForEvent("download");
  await page.locator("#export-circuit").click();
  const download = await event;
  const circuit = JSON.parse(await readFile(await download.path(), "utf8"));
  expect(circuit.devices.length).toBe(13);
  expect(circuit.wires.length).toBe(36);
  // Import with a different key order and omitted optional slots.
  const imported = {
    wires: circuit.wires,
    devices: circuit.devices.map(({ slot, ...d }) =>
      slot ? { slot, ...d } : d,
    ),
    version: 1,
  };
  await page
    .locator("#import-circuit")
    .setInputFiles({
      name: "shared-circuit.json",
      mimeType: "application/json",
      buffer: Buffer.from(JSON.stringify(imported)),
    });
  await expect(page.locator("#circuit-dirty")).toContainText("Draft circuit");
  await page.locator("#apply-circuit").click();
  await expect(page.locator("#circuit-dirty")).toHaveText(
    "Applied circuit · changes stop motors",
  );
  expect(errors).toEqual([]);
  await page.screenshot({
    path: "output/device-workbench-desktop.png",
    fullPage: true,
  });
});

test("assemble all devices from an empty workspace and reject a power short", async ({
  page,
  request,
}) => {
  await openWorkbench(page);
  const preset = await (await request.get("/api/workbench")).json();
  await page.locator("#new-circuit").click();
  for (const kind of [
    "esp32",
    "mpu6050",
    "battery",
    "regulator",
    "divider",
    ...Array(4).fill("esc"),
    ...Array(4).fill("motor"),
  ])
    await page.locator(`[data-add-device="${kind}"]`).click();
  await expect(page.locator(".circuit-device")).toHaveCount(13);
  const name = (p) =>
    `${{ esp: "esp32", imu: "mpu6050" }[p.device] || p.device}.${p.pin}`;
  for (const w of preset.preset.wires) {
    await pin(page, name(w.from));
    await pin(page, name(w.to));
  }
  await expect(page.locator("#circuit-result")).toHaveText(
    "All 13 devices connected",
  );
  await page.locator("#apply-circuit").click();
  await expect
    .poll(
      async () =>
        (await (await request.get("/api/telemetry")).json()).workbench
          .validation.flight_connected,
    )
    .toBe(true);
  await pin(page, "battery.+");
  await pin(page, "battery.-");
  await expect(page.locator("#circuit-result")).toHaveText(
    "Wiring error · cannot apply",
  );
  await expect(page.locator("#apply-circuit")).toBeDisabled();
  await page.locator("#preset-circuit").click();
  await expect(page.locator("#circuit-result")).toHaveText(
    "All 13 devices connected",
  );
  await page.locator("#apply-circuit").click();
});

test("compiled firmware drives the assembled devices through CPU, I2C, ADC, UART and PWM", async ({
  page,
  request,
}) => {
  const available =
    existsSync("output/emulator-build/aerolab-esp32") &&
    existsSync("output/firmware/simulation-fixture/aerolab-sim-fixture.elf");
  if (process.env.AEROLAB_REQUIRE_EMULATOR) expect(available).toBe(true);
  test.skip(
    !available,
    "Optional CPU backend and compiled guest fixture are required",
  );
  await openWorkbench(page);
  await page
    .locator("#firmware-file")
    .setInputFiles(
      "output/firmware/simulation-fixture/aerolab-sim-fixture.elf",
    );
  await expect(page.locator("#firmware-upload-status")).toContainText(
    "ready to start",
  );
  await page.locator("#start-firmware").click();
  await expect(page.locator("#driver-status")).toHaveText("ESP32 CPU running");
  try {
    await expect(page.locator("#uart-output")).toContainText("WHO_AM_I=0x68");
  } catch (error) {
    console.error(
      "Guest startup diagnostics",
      (await (await request.get("/api/telemetry")).json()).workbench,
    );
    throw error;
  }
  await expect(page.locator("#uart-output")).toContainText("battery_adc=");
  await page.locator('[data-uart="5"]').click();
  await expect
    .poll(
      async () =>
        (await (await request.get("/api/telemetry")).json()).motors.pwm_us[0],
    )
    .toBeGreaterThan(1490);
  const t = await (await request.get("/api/telemetry")).json();
  expect(t.workbench.firmware.imu_reads).toBeGreaterThan(0);
  expect(t.motor_actual.every((v) => v > 0)).toBe(true);
  expect(t.workbench.firmware.unhandled).toBe(0);
  await page.getByRole("button", { name: "Flight deck", exact: true }).click();
  await expect(page.locator("#arm")).toBeDisabled();
  await expect(page.locator("#state-badge")).toHaveText("FIRMWARE RUNNING");
  await page.locator("#emergency").click();
  await expect(page.locator("#state-badge")).toHaveText("FIRMWARE STOPPED");
  expect(
    (await (await request.get("/api/telemetry")).json()).motors.pwm_us,
  ).toEqual([1000, 1000, 1000, 1000]);
  await page
    .getByRole("button", { name: "Virtual hardware", exact: true })
    .click();
  await page.locator("#start-firmware").click();
  await expect(page.locator("#driver-status")).toHaveText("ESP32 CPU running");
  await page.evaluate(() => {
    Object.defineProperty(document, "hidden", {
      configurable: true,
      get: () => true,
    });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await expect(page.locator("#driver-status")).toHaveText("ESP32 CPU stopped");
  await expect(page.locator("#firmware-state")).toContainText(
    "heartbeat expired",
  );
  await page.evaluate(() => {
    delete document.hidden;
    document.dispatchEvent(new Event("visibilitychange"));
  });
  await page.getByRole("button", { name: "Flight deck", exact: true }).click();
  await page.locator("#resume-controls").click();
  await expect(page.locator("#state-badge")).toHaveText("FIRMWARE STOPPED");
  await page
    .getByRole("button", { name: "Virtual hardware", exact: true })
    .click();
  await page.locator("#builtin-firmware").click();
  await expect(page.locator("#driver-status")).toHaveText(
    "Built-in Rust controller",
  );
});

test("workbench stays within the phone viewport", async ({ page }) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openWorkbench(page);
  expect(
    await page.evaluate(() => document.documentElement.scrollWidth),
  ).toBeLessThanOrEqual(390);
  await page.screenshot({
    path: "output/device-workbench-mobile.png",
    fullPage: true,
  });
});
