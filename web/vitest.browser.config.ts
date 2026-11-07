import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";
import { referenceCommands } from "./src/testing/referenceCommands.ts";

// Renderer tests in Chromium with WebGPU (`make web-test-browser`), outside
// `make check`: they need a GPU. They render offscreen with the production
// renderer and compare with the reference images in references/.
export default defineConfig({
  test: {
    include: ["src/**/*.browser.test.ts"],
    testTimeout: 30_000,
    browser: {
      enabled: true,
      headless: true,
      provider: playwright({
        launchOptions: {
          args: ["--enable-unsafe-webgpu", "--enable-features=Vulkan", "--use-angle=metal"],
        },
      }),
      instances: [{ browser: "chromium" }],
      commands: referenceCommands,
    },
  },
});
