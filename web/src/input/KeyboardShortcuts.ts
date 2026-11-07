import type { Modifiers } from "../core/Core";

/** The parts of a `KeyboardEvent` a shortcut is made of. */
export type KeyLike = Pick<
  KeyboardEvent,
  "key" | "code" | "metaKey" | "ctrlKey" | "altKey" | "shiftKey"
>;

/** A key and the modifiers held, as the core's shortcut list has them (`Shortcut.key`). */
export interface KeyChord {
  key: string;
  modifiers: Modifiers;
}

/** Whether `platform` (`navigator.platform`) is one of Apple's, where ⌘ is the primary modifier. */
export const isApplePlatform = (platform: string): boolean =>
  /^(Mac|iPhone|iPad|iPod)/.test(platform);

const named: Record<string, string> = {
  Backspace: "backspace",
  Delete: "delete",
  Escape: "escape",
  Enter: "enter",
};

const punctuation: Record<string, string> = {
  Minus: "-",
  Equal: "=",
  BracketLeft: "[",
  BracketRight: "]",
};

/**
 * The character the key types without Shift and Alt, which change it (⇧1 types
 * "!", ⌥] types "‘"): from the physical key where they are held.
 */
function nameOf({ key, code, shiftKey, altKey }: KeyLike): string | undefined {
  if (key in named) return named[key];
  if (shiftKey || altKey) {
    if (/^Key[A-Z]$/.test(code)) return code.slice(3).toLowerCase();
    if (/^Digit[0-9]$/.test(code)) return code.slice(5);
    if (code in punctuation) return punctuation[code];
  }
  // A character, but not a named key such as "Shift" or "F5".
  return [...key].length === 1 ? key.toLowerCase() : undefined;
}

/**
 * The shortcut a key event is, in the core's terms, where `meta` is the
 * primary modifier: ⌘ on Apple platforms and Ctrl elsewhere (the core's table
 * is of ⌘ shortcuts; the Windows key is not used). `undefined` for a key that
 * is no shortcut by itself (a modifier, a function key).
 */
export function shortcutOf(event: KeyLike, apple: boolean): KeyChord | undefined {
  const key = nameOf(event);
  if (key === undefined) return undefined;
  return {
    key,
    modifiers: {
      shift: event.shiftKey,
      alt: event.altKey,
      ctrl: apple && event.ctrlKey,
      meta: apple ? event.metaKey : event.ctrlKey,
    },
  };
}
