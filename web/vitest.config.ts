import { defineConfig } from "vitest/config";

// Node tests against the real wasm (`make web-wasm` builds it first). The
// browser tests have their own config, see vitest.browser.config.ts.
export default defineConfig({
  test: {
    environment: "node",
    include: ["src/**/*.test.ts"],
    exclude: ["src/**/*.browser.test.ts", "node_modules/**"],
  },
});
