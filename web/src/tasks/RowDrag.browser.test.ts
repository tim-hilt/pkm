import { afterEach, describe, expect, it } from "vitest";
import { RowDrag } from "./RowDrag";

describe("RowDrag", () => {
  const area = document.createElement("div");
  const moves: [string, string | undefined][] = [];
  const drag = (id: string) =>
    new RowDrag<{ id: string }>((item, before) => moves.push([`${id}:${item.id}`, before?.id]));
  const row = (owner: RowDrag<{ id: string }>, id: string) => {
    const element = document.createElement("li");
    area.append(element);
    owner.attachRow(element, { id });
    return element;
  };
  /** Dispatches a drag event on `target`; whether a drop is allowed there. */
  const fire = (target: HTMLElement, type: string) => {
    const event = new DragEvent(type, {
      bubbles: true,
      cancelable: true,
      dataTransfer: new DataTransfer(),
    });
    target.dispatchEvent(event);
    return event.defaultPrevented;
  };
  /** Drags `from` over `to` and drops it there if allowed; whether it was. */
  const dropOn = (from: HTMLElement, to: HTMLElement) => {
    fire(from, "dragstart");
    const allowed = fire(to, "dragover");
    if (allowed) fire(to, "drop");
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
