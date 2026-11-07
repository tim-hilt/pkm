import type { SettingsStorage } from "./DrawingSettings";

/** The user's choice: follow the system, or always Light or always Dark. */
export type AppearanceChoice = "system" | "light" | "dark";

export const appearanceChoices: readonly AppearanceChoice[] = ["system", "light", "dark"];

export const appearanceTitle: Record<AppearanceChoice, string> = {
  system: "System",
  light: "Light",
  dark: "Dark",
};

/** Where the choice is kept in the settings. */
const key = "appearance";

/** What tells the OS's appearance; a `matchMedia("(prefers-color-scheme: dark)")` query. */
export interface SystemAppearance {
  readonly matches: boolean;
  addEventListener(type: "change", listener: () => void): void;
  removeEventListener(type: "change", listener: () => void): void;
}

/**
 * The Appearance setting, kept per Device in the browser's local storage, and
 * whether the app is dark because of it: System follows the OS live. Like Dark
 * mode, it never changes a Drawing or an export (ADR-0010). A port of the
 * Swift `Appearance`.
 */
export class Appearance {
  private readonly listeners = new Set<() => void>();
  private shownDark: boolean;

  constructor(
    private readonly storage: SettingsStorage,
    private readonly system: SystemAppearance,
  ) {
    this.shownDark = this.isDark;
    system.addEventListener("change", () => this.notifyIfChanged());
  }

  /** The stored choice; System when none (or an unknown one) is stored. */
  get choice(): AppearanceChoice {
    const stored = this.storage.getItem(key);
    return appearanceChoices.find((choice) => choice === stored) ?? "system";
  }

  set choice(choice: AppearanceChoice) {
    this.storage.setItem(key, choice);
    this.notifyIfChanged();
  }

  /** Whether the app is dark now. */
  get isDark(): boolean {
    const choice = this.choice;
    return choice === "system" ? this.system.matches : choice === "dark";
  }

  /** Calls `listener` whenever `isDark` changes; returns what stops that. */
  subscribe(listener: () => void): () => void {
    this.listeners.add(listener);
    return () => void this.listeners.delete(listener);
  }

  private notifyIfChanged(): void {
    const dark = this.isDark;
    if (dark === this.shownDark) return;
    this.shownDark = dark;
    for (const listener of [...this.listeners]) listener();
  }
}
