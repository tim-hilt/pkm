import { describe, expect, it } from "vitest";
import { acquireDevice } from "./WebGPUSupport";

const device = { label: "device" } as unknown as GPUDevice;
const adapter = { requestDevice: async () => device } as unknown as GPUAdapter;

describe("acquireDevice", () => {
  it("is unsupported without navigator.gpu", async () => {
    expect(await acquireDevice(undefined)).toBeUndefined();
  });

  it("is unsupported without an adapter", async () => {
    const gpu = { requestAdapter: async () => null } as unknown as GPU;
    expect(await acquireDevice(gpu)).toBeUndefined();
  });

  it("is unsupported when asking for the adapter fails", async () => {
    const gpu = {
      requestAdapter: async () => {
        throw new Error("blocked");
      },
    } as unknown as GPU;
    expect(await acquireDevice(gpu)).toBeUndefined();
  });

  it("gives the device of the adapter", async () => {
    const gpu = { requestAdapter: async () => adapter } as unknown as GPU;
    expect(await acquireDevice(gpu)).toBe(device);
  });
});
