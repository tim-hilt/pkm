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
