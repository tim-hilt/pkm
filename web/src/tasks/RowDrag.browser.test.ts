import { afterEach, describe, expect, it } from "vitest";
import { RowDrag } from "./RowDrag";

describe("RowDrag", () => {
  const area = document.createElement("div");
  const moves: [string, string | undefined][] = [];
  const drag = (id: string, onto = false) =>
    new RowDrag<{ id: string }>(
      (item, before) => moves.push([`${id}:${item.id}`, before?.id]),
      onto ? (item, parent) => moves.push([`${id}:${item.id} onto`, parent.id]) : undefined,
    );
  const row = (owner: RowDrag<{ id: string }>, id: string) => {
    const element = document.createElement("li");
    element.style.height = "40px";
    area.append(element);
    owner.attachRow(element, { id });
    return element;
  };
  /** Dispatches a drag event on `target` at `fraction` of its height; whether a drop is allowed there. */
  const fire = (target: HTMLElement, type: string, fraction = 0.1) => {
    const box = target.getBoundingClientRect();
    const event = new DragEvent(type, {
      bubbles: true,
      cancelable: true,
      clientY: box.top + box.height * fraction,
      dataTransfer: new DataTransfer(),
    });
    target.dispatchEvent(event);
    return event.defaultPrevented;
  };
  /** Drags `from` over `to` at `fraction` of its height and drops it there if allowed; whether it was. */
  const dropOn = (from: HTMLElement, to: HTMLElement, fraction?: number) => {
    fire(from, "dragstart");
    const allowed = fire(to, "dragover", fraction);
    if (allowed) fire(to, "drop", fraction);
    else fire(from, "dragend");
    return allowed;
  };

  afterEach(() => {
    area.replaceChildren();
    area.remove();
    moves.length = 0;
  });

  it("moves before a row of its own, or to the end below the rows", () => {
    document.body.append(area);
    const a = drag("a");
    a.attachEnd(area);
    const [a1, a2] = [row(a, "1"), row(a, "2")];
    expect(dropOn(a2, a1)).toBe(true);
    expect(dropOn(a1, area)).toBe(true);
    expect(moves).toEqual([
      ["a:2", "1"],
      ["a:1", undefined],
    ]);
  });

  it("drops onto the middle of a row, before its top and after its bottom", () => {
    document.body.append(area);
    const a = drag("a", true);
    const [a1, a2, a3] = [row(a, "1"), row(a, "2"), row(a, "3")];
    fire(a1, "dragstart");
    fire(a2, "dragover", 0.5);
    expect(a2.classList.contains("drop-onto")).toBe(true);
    fire(a1, "dragend");
    expect(a2.classList.contains("drop-onto")).toBe(false);
    expect(dropOn(a1, a2, 0.5)).toBe(true);
    expect(dropOn(a3, a2, 0.1)).toBe(true);
    expect(dropOn(a1, a2, 0.9)).toBe(true);
    expect(dropOn(a1, a3, 0.9)).toBe(true);
    expect(moves).toEqual([
      ["a:1 onto", "2"],
      ["a:3", "2"],
      ["a:1", "3"],
      ["a:1", undefined],
    ]);
  });

  it("only moves where it can't drop onto a row", () => {
    document.body.append(area);
    const a = drag("a");
    const [a1, a2] = [row(a, "1"), row(a, "2")];
    expect(dropOn(a2, a1, 0.5)).toBe(true);
    expect(moves).toEqual([["a:2", "1"]]);
  });

  it("refuses a drop on the dragged row and on another instance's row, whose end area is shared", () => {
    document.body.append(area);
    const [a, b] = [drag("a"), drag("b")];
    a.attachEnd(area);
    b.attachEnd(area);
    const a1 = row(a, "1");
    const b1 = row(b, "1");
    expect(dropOn(a1, a1)).toBe(false);
    expect(dropOn(a1, b1)).toBe(false);
    expect(moves).toEqual([]);
  });
});
