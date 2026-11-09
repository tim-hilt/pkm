import { CloudBackingSection } from "./CloudBackingSection";
import type { CloudBacking } from "../cloud/CloudBacking";
import { type Appearance, appearanceChoices, appearanceTitle } from "./Appearance";

/**
 * The Settings, opened from the shell's gear: the Appearance (System, Light,
 * Dark), below the Cloud Backing (WebDAV). A dialog, as the native app's
 * sheet; a port of the Swift `SettingsView`.
 */
export class SettingsView {
  readonly element = document.createElement("dialog");

  constructor(
    private readonly appearance: Appearance,
    cloud: CloudBacking,
  ) {
    this.element.className = "gallery-dialog settings-dialog";
    this.element.setAttribute("aria-label", "Settings");

    const legend = document.createElement("legend");
    legend.textContent = "Appearance";
    const choices = document.createElement("div");
    choices.className = "appearance-choices";
    for (const choice of appearanceChoices) {
      const input = document.createElement("input");
      input.type = "radio";
      input.name = "appearance";
      input.value = choice;
      input.addEventListener("change", () => (appearance.choice = choice));
      const label = document.createElement("label");
      label.append(input, appearanceTitle[choice]);
      choices.append(label);
    }
    const group = document.createElement("fieldset");
    group.append(legend, choices);

    const done = document.createElement("button");
    done.type = "button";
    done.className = "primary";
    done.textContent = "Done";
    done.addEventListener("click", () => this.element.close());
    const buttons = document.createElement("div");
    buttons.className = "gallery-dialog-buttons";
    buttons.append(done);

    const title = document.createElement("h2");
    title.textContent = "Settings";
    this.element.append(title, new CloudBackingSection(cloud).element, group, buttons);
  }

  /** Shows the Settings with the Appearance as it is now. */
  show(): void {
    const current = this.appearance.choice;
    for (const input of this.element.querySelectorAll("input"))
      input.checked = input.value === current;
    this.element.showModal();
  }
}
