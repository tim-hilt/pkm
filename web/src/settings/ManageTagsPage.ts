import { button, make } from "../Dom";
import { tagLabel } from "../tasks/TagLabel";
import { deleteTagDialog } from "../tasks/TaskDialogs";
import type { TasksModel } from "../tasks/TasksModel";

/** What the color input shows for a Tag of Default: the grey of `.tag`. */
const defaultGrey = "#808080";

/**
 * Settings' "Manage Tags" subpage: every Tag, alphabetically, in its Tag
 * Color, each with a color input, a Default and a Delete button; "No Tags"
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
    const heading = make("h2", "", ["Manage Tags"]);
    this.element.append(button("tasks-back", "Back", back), heading, this.empty, this.list);
  }

  /** Lists the Tags as they are now. */
  show(): void {
    const tags = this.tasks.allTags;
    this.empty.hidden = tags.length > 0;
    this.list.replaceChildren(
      ...tags.map(({ name, color }) => {
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
        const remove = button("manage-tags-button", "Delete", () => {
          deleteTagDialog(this.tasks, this.element, name, () => this.show());
        });
        remove.setAttribute("aria-label", `Delete ${name}`);
        return make("li", "", [tagLabel(name, color), input, reset, remove]);
      }),
    );
  }
}
