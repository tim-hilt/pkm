import { button, make } from "../Dom";
import type { CloudBacking } from "../cloud/CloudBacking";
import { statusText } from "../cloud/CloudBacking";
import type { Navigation, SettingsPage } from "../navigation/Navigation";
import type { TasksModel } from "../tasks/TasksModel";
import { type Appearance, appearanceChoices, appearanceTitle } from "./Appearance";
import { CloudBackingSection } from "./CloudBackingSection";
import { ManageTagsPage } from "./ManageTagsPage";

/**
 * Settings as pages in the content area, structured like the native root:
 * Appearance inline (System, Light, Dark), then the links to Tags and to Cloud
 * Backing, which shows its status. Which page is shown is the `Navigation`'s:
 * call `update` after it changed. A port of the Swift `SettingsView`.
 */
export class SettingsView {
  readonly element = make("section", "settings-page");
  private readonly pages: Record<SettingsPage, HTMLElement>;
  private readonly manageTags: ManageTagsPage;

  constructor(
    private readonly navigation: Navigation,
    private readonly appearance: Appearance,
    cloud: CloudBacking,
    tasks: TasksModel,
  ) {
    this.element.setAttribute("aria-label", "Settings");
    this.manageTags = new ManageTagsPage(tasks, () => navigation.openSettings());

    const cloudStatus = make("span", "settings-detail");
    const showCloudStatus = () => {
      cloudStatus.textContent = cloud.connection
        ? `WebDAV · ${statusText(cloud.syncReport, "WebDAV")}`
        : "Not connected";
      cloudStatus.classList.toggle("problem", cloud.hasProblem);
    };
    cloud.onConnectionChange(showCloudStatus);
    cloud.onSyncChange(showCloudStatus);
    showCloudStatus();

    const main = make("div", "settings-main", [
      make("h2", "", ["Settings"]),
      this.appearanceChoices(),
      make("div", "settings-links", [
        this.link("tags", "Tags"),
        this.link("cloud-backing", "Cloud Backing", cloudStatus),
      ]),
    ]);
    const cloudPage = make("div", "settings-subpage", [
      this.back(),
      new CloudBackingSection(cloud).element,
    ]);
    this.pages = { main, tags: this.manageTags.element, "cloud-backing": cloudPage };
    this.element.append(...Object.values(this.pages));
    this.update();
  }

  /** Shows the page of the `Navigation`, with the Appearance and the Tags as they are now. */
  update(): void {
    const shown = this.navigation.settings;
    this.element.hidden = shown === undefined;
    for (const [page, element] of Object.entries(this.pages)) element.hidden = page !== shown;
    if (shown === "main") this.showAppearance();
    if (shown === "tags") this.manageTags.show();
  }

  private showAppearance(): void {
    for (const input of this.element.querySelectorAll<HTMLInputElement>("input[name=appearance]"))
      input.checked = input.value === this.appearance.choice;
  }

  private back(): HTMLElement {
    return button("tasks-back", "Back", () => this.navigation.openSettings());
  }

  private appearanceChoices(): HTMLElement {
    const choices = make("div", "appearance-choices");
    for (const choice of appearanceChoices) {
      const input = make("input", "");
      input.type = "radio";
      input.name = "appearance";
      input.value = choice;
      input.addEventListener("change", () => (this.appearance.choice = choice));
      choices.append(make("label", "", [input, appearanceTitle[choice]]));
    }
    return make("fieldset", "settings-group", [make("legend", "", ["Appearance"]), choices]);
  }

  /** A row that opens `page`; a real link, so that a modified click opens it in a new tab. */
  private link(page: SettingsPage, title: string, detail?: HTMLElement): HTMLAnchorElement {
    const link = make("a", "settings-link", [
      make("span", "", [title]),
      ...(detail ? [detail] : []),
    ]);
    link.href = `/settings/${page}`;
    link.addEventListener("click", (event) => {
      if (event.metaKey || event.ctrlKey || event.shiftKey || event.button !== 0) return;
      event.preventDefault();
      this.navigation.openSettings(page);
    });
    return link;
  }
}
