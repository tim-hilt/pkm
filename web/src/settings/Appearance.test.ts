import { describe, expect, it, vi } from "vitest";
import { Appearance, type SystemAppearance, appearanceChoices } from "./Appearance";
import type { SettingsStorage } from "../drawings/chrome/DrawingSettings";

const memory = (): SettingsStorage & { values: Map<string, string> } => {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, value),
  };
};

/** The OS's appearance, which the test turns like a user at sunset. */
class FakeSystem implements SystemAppearance {
  private listeners = new Set<() => void>();
  constructor(public matches = false) {}
  addEventListener(_type: "change", listener: () => void): void {
    this.listeners.add(listener);
  }
  removeEventListener(_type: "change", listener: () => void): void {
    this.listeners.delete(listener);
  }
  turn(dark: boolean): void {
    this.matches = dark;
    for (const listener of this.listeners) listener();
  }
}

describe("the Appearance setting", () => {
  it("follows the system until the user picks one", () => {
    expect(new Appearance(memory(), new FakeSystem()).choice).toBe("system");
  });

  it("keeps a picked Appearance per Device", () => {
    const storage = memory();
    new Appearance(storage, new FakeSystem()).choice = "dark";
    expect(new Appearance(storage, new FakeSystem()).choice).toBe("dark");
  });

  it("follows the system when an unknown value is stored", () => {
    const storage = memory();
    storage.setItem("appearance", "sepia");
    expect(new Appearance(storage, new FakeSystem()).choice).toBe("system");
  });

  it("offers System, Light and Dark in that order", () => {
    expect(appearanceChoices).toEqual(["system", "light", "dark"]);
  });

  it("is dark when Dark, light when Light, whatever the system is", () => {
    const appearance = new Appearance(memory(), new FakeSystem(true));
    appearance.choice = "light";
    expect(appearance.isDark).toBe(false);
    appearance.choice = "dark";
    expect(appearance.isDark).toBe(true);
  });

  it("on System follows the system live", () => {
    const system = new FakeSystem(false);
    const appearance = new Appearance(memory(), system);
    const changed = vi.fn();
    appearance.subscribe(changed);
    expect(appearance.isDark).toBe(false);

    system.turn(true);
    expect(appearance.isDark).toBe(true);
    expect(changed).toHaveBeenCalledTimes(1);
  });

  it("does not tell of a system change it does not follow, nor of a choice that changes nothing", () => {
    const system = new FakeSystem(false);
    const appearance = new Appearance(memory(), system);
    appearance.choice = "light";
    const changed = vi.fn();
    appearance.subscribe(changed);

    system.turn(true);
    appearance.choice = "light";
    appearance.choice = "system"; // light -> dark, as the system is dark now
    expect(changed).toHaveBeenCalledTimes(1);
  });

  it("stops telling once unsubscribed", () => {
    const appearance = new Appearance(memory(), new FakeSystem());
    const changed = vi.fn();
    const stop = appearance.subscribe(changed);
    stop();
    appearance.choice = "dark";
    expect(changed).not.toHaveBeenCalled();
  });
});
