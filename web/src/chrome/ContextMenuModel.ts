import { type Editor, type PointerKind, ZOrder } from "../core/Core";
import type { CanvasClipboard } from "../input/CanvasClipboard";

/** One entry of the Context menu: an action, or a submenu. */
export interface MenuEntry {
  label: string;
  disabled?: boolean;
  destructive?: boolean;
  run?(): void | Promise<void>;
  children?: MenuEntry[];
}

type Point = { x: number; y: number };

/**
 * What a hold or secondary click of `kind` at `at` (in the Canvas's
 * coordinates) opens, as the Swift `CanvasContextMenu` does: the core selects
 * what is pressed and says which menu it is, this gives its entries; `undefined`
 * if the press means nothing. A Paste from the menu lands around `at`.
 */
export function openContextMenu(
  editor: Editor,
  clipboard: Pick<CanvasClipboard, "copy" | "cut" | "paste">,
  at: Point,
  kind: PointerKind,
): MenuEntry[] | undefined {
  const menu = editor.longPress(at.x, at.y, kind);
  if (!menu) return undefined;
  const paste: MenuEntry = { label: "Paste", run: () => clipboard.paste(at) };
  if (!menu.element) {
    return [paste, { label: "Select All", run: () => editor.selectAll() }];
  }
  const move = (label: string, how: ZOrder): MenuEntry => ({
    label,
    run: () => editor.moveInZOrder(how),
  });
  return [
    { label: "Cut", run: () => clipboard.cut() },
    { label: "Copy", run: () => clipboard.copy() },
    paste,
    { label: "Duplicate", run: () => editor.duplicate() },
    { label: "Group", disabled: !menu.canGroup, run: () => editor.group() },
    { label: "Ungroup", disabled: !menu.canUngroup, run: () => editor.ungroup() },
    {
      label: "Arrange",
      children: [
        move("Bring to Front", ZOrder.ToFront),
        move("Bring Forward", ZOrder.Forward),
        move("Send Backward", ZOrder.Backward),
        move("Send to Back", ZOrder.ToBack),
      ],
    },
    { label: "Delete", destructive: true, run: () => editor.deleteSelection() },
  ];
}
