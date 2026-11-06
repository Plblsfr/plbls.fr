import { defineConfig } from "@playwright/test";
import { backend, front, gateway } from "./e2e/ports";

export default defineConfig({
  testDir: "./e2e",
  testMatch: gateway ? "gateway.spec.ts" : ["ui.spec.ts", "errors.spec.ts"],
  globalSetup: "./e2e/global-setup.ts",
  timeout: 60_000,
  workers: 1,
  reporter: [["list"]],
  use: { baseURL: `http://localhost:${front}`, trace: "retain-on-failure", locale: "fr-FR" },
  webServer: {
    command: `npm run dev -- --host 127.0.0.1 --port ${front} --strictPort`,
    url: `http://localhost:${front}`,
    env: gateway ? { DEV_VIA_GATEWAY: "1" } : { BACKEND_URL: `http://127.0.0.1:${backend}` },
    reuseExistingServer: false,
    timeout: 120_000,
  },
});
