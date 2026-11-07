import { defineConfig } from "vitest/config";

// Node tests against the real wasm (`make web-wasm` builds it first).
export default defineConfig({
  test: { environment: "node", include: ["src/**/*.test.ts"] },
});
