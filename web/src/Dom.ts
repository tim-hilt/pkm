type Tag = keyof HTMLElementTagNameMap;

/** An element of `tag` with a class and children (nodes or text). */
export function make<K extends Tag>(
  tag: K,
  className: string,
  children: (Node | string)[] = [],
): HTMLElementTagNameMap[K] {
  const element = document.createElement(tag);
  element.className = className;
  element.append(...children);
  return element;
}

/** A `<button type="button">` that calls `onClick`. */
export function button(className: string, label: string, onClick: () => void): HTMLButtonElement {
  const element = make("button", className, [label]);
  element.type = "button";
  element.addEventListener("click", onClick);
  return element;
}

/**
 * Calls `action` when "n" is pressed with `root` shown, no dialog open and
 * no text being typed. Returns what stops listening.
 */
export function onNewKey(root: HTMLElement, action: () => void): () => void {
  const listener = (event: KeyboardEvent) => {
    if (event.key !== "n" || event.ctrlKey || event.metaKey || event.altKey || event.shiftKey)
      return;
    if (event.defaultPrevented || event.isComposing || event.repeat) return;
    if (!root.isConnected || root.closest("[hidden]") || document.querySelector("dialog[open]"))
      return;
    const target = event.target;
    if (
      target instanceof HTMLElement &&
      target.closest("input, textarea, select, [contenteditable]")
    )
      return;
    event.preventDefault();
    action();
  };
  document.addEventListener("keydown", listener);
  return () => document.removeEventListener("keydown", listener);
}
