import { button, make } from "../Dom";
import { tagLabel } from "../tasks/TagLabel";
import { deleteTagDialog } from "../tasks/TaskDialogs";
import type { TasksModel } from "../tasks/TasksModel";

/** What the color input shows for a Tag of Default: the grey of `.tag`. */
const defaultGrey = "#808080";

/**
 * Settings' Tags page: every Tag by Relevance, in its Tag Color, each with its
 * Relevance, a color input, a Default, a Reset to 0 and a Delete button; "No Tags"
 * if there are none. `back` returns to Settings. Call `show` to list the Tags as they are.
 */
export class ManageTagsPage {
  readonly element = make("div", "manage-tags");
  private readonly list = make("ul", "manage-tags-list");
  private readonly empty = make("p", "manage-tags-empty", ["No Tags"]);

  constructor(
    private readonly tasks: TasksModel,
    back: () => void,
  ) {
    const heading = make("h2", "", ["Tags"]);
    this.element.append(button("tasks-back", "Back", back), heading, this.empty, this.list);
  }

  /** Lists the Tags as they are now. */
  show(): void {
    const tags = this.tasks.allTags;
    this.empty.hidden = tags.length > 0;
    this.list.replaceChildren(
      ...tags.map(({ name, color, relevance }) => {
        const input = make("input", "manage-tags-color");
        input.type = "color";
        input.value = color ?? defaultGrey;
        input.setAttribute("aria-label", `Color of ${name}`);
        // `change` fires when the picker closes, so each pick is saved once.
        input.addEventListener("change", () => {
          this.tasks.setTagColor(name, input.value);
          this.show();
        });
        const reset = button("manage-tags-button", "Default", () => {
          this.tasks.setTagColor(name, undefined);
          this.show();
        });
        reset.disabled = color === undefined;
        reset.setAttribute("aria-label", `Reset ${name} to Default`);
        const count = make("span", "manage-tags-relevance", [String(relevance)]);
        count.setAttribute("aria-label", `Relevance ${relevance}`);
        const resetRelevance = button("manage-tags-button", "Reset to 0", () => {
          this.tasks.resetTagRelevance(name);
          this.show();
        });
        resetRelevance.disabled = relevance === 0;
        resetRelevance.setAttribute("aria-label", `Reset the Relevance of ${name} to 0`);
        const remove = button("manage-tags-button", "Delete", () => {
          deleteTagDialog(this.tasks, this.element, name, () => this.show());
        });
        remove.setAttribute("aria-label", `Delete ${name}`);
        return make("li", "", [tagLabel(name, color), count, input, reset, resetRelevance, remove]);
      }),
    );
  }
}
