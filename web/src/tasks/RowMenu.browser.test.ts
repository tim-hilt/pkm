import { userEvent } from "vitest/browser";
import { afterEach, describe, expect, it } from "vitest";
import { RowMenu } from "./RowMenu";

describe("RowMenu choices", () => {
  const picked: string[] = [];
  const menu = new RowMenu(document.body);
  const choice = (label: string, checked: boolean) => ({
    label,
    checked,
    className: `priority-${label.slice(1)}`,
    run: () => picked.push(label),
  });
  const open = () => {
    document.body.append(menu.element);
    const after = document.createElement("button");
    document.body.append(after);
    menu.show(
      [
        { choices: ["P1", "P2", "P3", "P4"].map((p) => choice(p, p === "P3")) },
        { label: "Reject", run: () => picked.push("Reject"), separated: true },
      ],
      10,
      10,
      after,
    );
  };
  const focused = () =>
    document.activeElement?.getAttribute("aria-label") ?? document.activeElement?.textContent;

  afterEach(() => {
    menu.hide();
    menu.element.remove();
    picked.length = 0;
  });

  it("labels each choice, marks the checked one and opens on it", () => {
    open();
    const circles = [...menu.element.querySelectorAll("[role=menuitemradio]")];
    expect(circles.map((c) => c.getAttribute("aria-label"))).toEqual(["P1", "P2", "P3", "P4"]);
    expect(circles.map((c) => c.getAttribute("aria-checked"))).toEqual([
      "false",
      "false",
      "true",
      "false",
    ]);
    expect(focused()).toBe("P3");
  });

  it("is reached by keyboard: arrows within the group, up and down between entries, Enter", async () => {
    open();
    await userEvent.keyboard("{ArrowRight}");
    expect(focused()).toBe("P4");
    await userEvent.keyboard("{ArrowRight}");
    expect(focused()).toBe("P1");
    await userEvent.keyboard("{ArrowDown}");
    expect(focused()).toBe("Reject");
    await userEvent.keyboard("{ArrowUp}{ArrowLeft}{Enter}");
    expect(picked).toEqual(["P2"]); // Up re-enters the group at the checked P3.
    expect(menu.element.hidden).toBe(true);
  });

  it("is reached by click", async () => {
    open();
    await userEvent.click(menu.element.querySelector("[aria-label=P2]")!);
    expect(picked).toEqual(["P2"]);
    expect(menu.element.hidden).toBe(true);
  });
});
