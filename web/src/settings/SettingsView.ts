import { CloudBackingSection } from "./CloudBackingSection";
import type { CloudBacking } from "../cloud/CloudBacking";
import { ManageTagsPage } from "./ManageTagsPage";
import type { TasksModel } from "../tasks/TasksModel";
import { type Appearance, appearanceChoices, appearanceTitle } from "./Appearance";

/**
 * The Settings, opened from the shell's gear: the Appearance (System, Light,
 * Dark), below the Cloud Backing (WebDAV). A dialog, as the native app's
 * sheet, with a Tags section whose "Manage Tags" opens a subpage; a port of the Swift `SettingsView`.
 */
export class SettingsView {
  readonly element = document.createElement("dialog");
  /** Settings itself and its subpages; one is shown. */
  private readonly main = document.createElement("div");
  private readonly pages: HTMLElement[];

  constructor(
    private readonly appearance: Appearance,
    cloud: CloudBacking,
    tasks: TasksModel,
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

    const manageTags = new ManageTagsPage(tasks, () => this.showPage(this.main));
    const open = document.createElement("button");
    open.type = "button";
    open.className = "settings-link";
    open.textContent = "Manage Tags";
    open.addEventListener("click", () => {
      manageTags.show();
      this.showPage(manageTags.element);
    });
    const tagsLegend = document.createElement("legend");
    tagsLegend.textContent = "Tags";
    const tagsSection = document.createElement("fieldset");
    tagsSection.append(tagsLegend, open);

    this.main.append(title, new CloudBackingSection(cloud).element, group, tagsSection, buttons);
    this.pages = [this.main, manageTags.element];
    this.showPage(this.main);
    this.element.append(...this.pages);
  }

  private showPage(page: HTMLElement): void {
    for (const each of this.pages) each.hidden = each !== page;
  }

  /** Shows the Settings with the Appearance as it is now. */
  show(): void {
    const current = this.appearance.choice;
    for (const input of this.element.querySelectorAll<HTMLInputElement>("input[name=appearance]"))
      input.checked = input.value === current;
    this.showPage(this.main);
    this.element.showModal();
  }
}
