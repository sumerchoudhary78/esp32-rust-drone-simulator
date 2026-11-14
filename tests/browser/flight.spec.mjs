import { test, expect } from "@playwright/test";
import { readFile } from "node:fs/promises";

async function telemetry(request) {
  return (await request.get("/api/telemetry")).json();
}
async function connect(page) {
  await page.goto("/");
  await expect(page.locator("#connection")).toHaveText("Connected");
  await expect(page.locator("#control-status")).toHaveText(
    /YOUR CONTROL|OBSERVER/,
  );
  if (await page.locator("#claim-control").isVisible())
    await page.locator("#claim-control").click();
  await expect(page.locator("#control-status")).toHaveText("YOUR CONTROL");
  await page.locator("#reset").click();
  await expect(page.locator("#state-badge")).toHaveText("CALIBRATING");
  await expect(page.locator("#preflight-count")).toHaveText("3 / 3");
}
test("complete flight: arm, takeoff, keyboard, disturbance, landing and CSV", async ({
  page,
  request,
}) => {
  const errors = [];
  page.on("pageerror", (e) => errors.push(e.message));
  await connect(page);
  await page.getByRole("button", { name: "Arm motors", exact: true }).click();
  await expect(page.locator("#state-badge")).toHaveText("ARMED · IDLE");
  await page.getByRole("button", { name: "Take off", exact: true }).click();
  await expect
    .poll(async () => (await telemetry(request)).position_m.z, {
      timeout: 10000,
    })
    .toBeGreaterThan(1.4);
  await page.keyboard.down("d");
  await expect
    .poll(async () => (await telemetry(request)).attitude_deg.x)
    .toBeGreaterThan(3);
  await page.keyboard.up("d");
  await expect
    .poll(async () => Math.abs((await telemetry(request)).attitude_deg.x))
    .toBeLessThan(1.5);
  await page.locator("#disturb").click();
  await expect
    .poll(async () =>
      (await telemetry(request)).events.some((e) =>
        e.message.includes("Angular impulse"),
      ),
    )
    .toBeTruthy();
  await expect
    .poll(async () => Math.abs((await telemetry(request)).rates_deg_s.x))
    .toBeLessThan(2);
  await page.getByRole("button", { name: "Land", exact: true }).click();
  await expect(page.locator("#state-badge")).toHaveText("DISARMED", {
    timeout: 15000,
  });
  expect((await telemetry(request)).motors.pwm_us).toEqual([
    1000, 1000, 1000, 1000,
  ]);
  const downloadEvent = page.waitForEvent("download");
  await page.locator("#export").click();
  const download = await downloadEvent;
  const csv = await readFile(await download.path(), "utf8");
  expect(csv).toContain("truth_roll_deg");
  expect(csv.split("\n").length).toBeGreaterThan(100);
  expect(errors).toEqual([]);
});
test("link fault latches motor cut and requires an explicit acknowledgment", async ({
  page,
  request,
}) => {
  await connect(page);
  await page.locator("#arm").click();
  await expect(page.locator("#state-badge")).toHaveText("ARMED · IDLE");
  await page
    .getByRole("button", { name: "Control tuning", exact: true })
    .click();
  await page.locator('[data-fault="link_loss"]').check();
  await expect
    .poll(async () => (await telemetry(request)).reason)
    .toBe("link_timeout");
  expect((await telemetry(request)).motors.pwm_us).toEqual([
    1000, 1000, 1000, 1000,
  ]);
  await page.locator('[data-fault="link_loss"]').uncheck();
  await page.getByRole("button", { name: "Flight deck", exact: true }).click();
  await expect(page.locator("#state-badge")).toHaveText("FAILSAFE");
  await page
    .getByRole("button", { name: "Acknowledge & disarm", exact: true })
    .click();
  await expect(page.locator("#state-badge")).toHaveText("DISARMED");
});
test("space emergency stop and control handoff", async ({
  page,
  context,
  request,
}) => {
  await connect(page);
  await page.locator("#arm").click();
  await expect(page.locator("#state-badge")).toHaveText("ARMED · IDLE");
  const observer = await context.newPage();
  await observer.goto("/");
  await expect(observer.locator("#control-status")).toHaveText("OBSERVER");
  await observer.locator("#claim-control").click();
  await expect(observer.locator("#control-status")).toHaveText("YOUR CONTROL");
  await expect(observer.locator("#state-badge")).toHaveText("DISARMED");
  await expect(page.locator("#control-status")).toHaveText("OBSERVER");
  await observer.locator("#arm").click();
  await expect(observer.locator("#state-badge")).toHaveText("ARMED · IDLE");
  await observer.keyboard.press("Space");
  await expect
    .poll(async () => (await telemetry(request)).motors.pwm_us)
    .toEqual([1000, 1000, 1000, 1000]);
  expect((await telemetry(request)).state).not.toBe("armed");
  await observer.close();
});
for (const width of [320, 390, 768, 1440])
  test(`all panels fit at ${width}px and touch controls work`, async ({
    page,
  }) => {
    await page.setViewportSize({ width, height: 900 });
    await connect(page);
    for (const name of [
      "Virtual hardware",
      "Control tuning",
      "Flight log",
      "Getting started",
      "Flight deck",
    ]) {
      await page.getByRole("button", { name: new RegExp(`^${name}`) }).click();
      expect(
        await page.evaluate(() => document.documentElement.scrollWidth),
      ).toBeLessThanOrEqual(width);
    }
    await page.locator("#tilt-stick").scrollIntoViewIfNeeded();
    const box = await page.locator("#tilt-stick").boundingBox();
    await page.mouse.move(box.x + box.width / 2, box.y + box.height / 2);
    await page.mouse.down();
    await page.mouse.move(box.x + box.width * 0.7, box.y + box.height / 2);
    await expect(page.locator("#roll-range")).not.toHaveValue("0");
    await page.mouse.up();
    await expect(page.locator("#roll-range")).toHaveValue("0");
    await page.screenshot({
      path: `output/flight-deck-${width}.png`,
      fullPage: true,
    });
  });
