import { defineConfig } from "@playwright/test";
export default defineConfig({
  testDir: "./tests/browser",
  timeout: 35000,
  workers: 1,
  fullyParallel: false,
  use: {
    baseURL: "http://127.0.0.1:8790",
    channel: "chrome",
    headless: true,
    viewport: { width: 1440, height: 1000 },
    screenshot: "only-on-failure",
    trace: "retain-on-failure",
  },
  webServer: {
    command: "cargo run -p drone-sim -- --bind 127.0.0.1:8790",
    url: "http://127.0.0.1:8790/api/health",
    timeout: 120000,
    reuseExistingServer: false,
  },
});
