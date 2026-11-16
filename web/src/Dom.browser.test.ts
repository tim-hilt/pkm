import { afterEach, describe, expect, it } from "vitest";
import { make, onNewKey } from "./Dom";

describe("onNewKey", () => {
  const root = make("main", "");
  let stop = () => {};
  let calls = 0;
  const press = (target: EventTarget, init: KeyboardEventInit = {}) =>
    target.dispatchEvent(new KeyboardEvent("keydown", { key: "n", bubbles: true, ...init }));
  const listen = () => {
    document.body.append(root);
    stop = onNewKey(root, () => calls++);
  };

  afterEach(() => {
    stop();
    root.remove();
    root.hidden = false;
    calls = 0;
  });

  it("acts on n", () => {
    listen();
    press(document.body);
    expect(calls).toBe(1);
  });

  it("ignores n with a modifier, while typing, while hidden, or with a dialog open", () => {
    listen();
    press(document.body, { metaKey: true });
    const input = document.createElement("input");
    document.body.append(input);
    press(input);
    input.remove();
    root.hidden = true;
    press(document.body);
    root.hidden = false;
    const dialog = document.createElement("dialog");
    document.body.append(dialog);
    dialog.show();
    press(document.body);
    dialog.remove();
    expect(calls).toBe(0);
  });

  it("stops when told to", () => {
    listen();
    stop();
    press(document.body);
    expect(calls).toBe(0);
  });
});
