import type { Sections } from "../core/Core";

/** One control of the phone Action row. `duplicate` and `delete` are on the row itself only when it is wide enough, else inside `actions`. */
export type ActionRowItem =
  | "stroke"
  | "background"
  | "properties"
  | "text"
  | "actions"
  | "duplicate"
  | "delete";

/** Row width from which Delete, then Duplicate, sit on the row (excalidraw.com's). */
export const deleteWidth = 374;
export const duplicateWidth = 412;

/** What the row depends on. */
export interface ActionRowInputs {
  /** What the core says applies; `undefined` when there is nothing to style. */
  sections: Sections | undefined;
  hasSelection: boolean;
  /** The row's width. */
  width: number;
}

/** Whether the item opens a popover (the others act at once). */
export const opensPopover = (item: ActionRowItem) => item !== "delete" && item !== "duplicate";

/** The items for a row of the inputs' width. */
export function actionRowItems({ sections: s, hasSelection, width }: ActionRowInputs) {
  const items: ActionRowItem[] = [];
  if (s) {
    if (s.strokeColor) items.push("stroke");
    if (s.backgroundColor) items.push("background");
    if (s.fillStyle || s.strokeWidth || s.strokeStyle || s.sloppiness || s.edges || s.opacity) {
      items.push("properties");
    }
    if (s.text) items.push("text");
  }
  if (hasSelection) {
    items.push("actions");
    // excalidraw.com's order, Duplicate before Delete; Delete is the first to fit.
    if (width >= duplicateWidth) items.push("duplicate");
    if (width >= deleteWidth) items.push("delete");
  }
  return items;
}

/**
 * The phone Action row as a pure value: which items it shows and which popover
 * is open. The view renders it and forwards events; it holds no logic of its
 * own. A port of the Swift `ActionRowModel`.
 */
export class ActionRowModel {
  items: readonly ActionRowItem[] = [];
  /** The open popover's trigger. */
  open: ActionRowItem | undefined;

  /** Whether the `…` popover still offers Duplicate / Delete: only while they are not on the row. */
  get popoverHasDuplicate(): boolean {
    return !this.items.includes("duplicate");
  }
  get popoverHasDelete(): boolean {
    return !this.items.includes("delete");
  }

  /** The row's inputs changed: a popover whose trigger is gone closes. */
  update(inputs: ActionRowInputs): void {
    this.items = actionRowItems(inputs);
    if (this.open && !this.items.includes(this.open)) this.open = undefined;
  }

  /** A tap on a popover trigger: opens it, switches to it, or closes it if it is the open one. */
  tap(item: ActionRowItem): void {
    if (!opensPopover(item) || !this.items.includes(item)) return;
    this.open = this.open === item ? undefined : item;
  }

  close(): void {
    this.open = undefined;
  }

  /** A touch on the Canvas: closes the open popover; true if it did, so that the touch must not reach the Canvas. */
  canvasTouch(): boolean {
    const wasOpen = this.open !== undefined;
    this.open = undefined;
    return wasOpen;
  }
}
