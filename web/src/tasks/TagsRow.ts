import { button, make } from "../Dom";
import { pickTagsDialog } from "./TagPicker";
import { tagLabel } from "./TagLabel";
import type { Tag } from "./TasksModel";

/** The Tags of an Item as labels in their Tag Colors, with the button that picks them; `change` gets the Tags picked. */
export function tagsRow(
  model: { knownTags: () => Tag[] },
  tags: string[],
  change: (tags: string[]) => void,
): HTMLElement {
  const colors = new Map(model.knownTags().map((tag) => [tag.name, tag.color]));
  const pick = button("tasks-back", "Tags", () =>
    pickTagsDialog(
      document.body,
      {
        get allTags() {
          return model.knownTags();
        },
      },
      tags,
      { kind: "many", done: change },
    ),
  );
  pick.ariaLabel = "Pick Tags";
  return make("div", "item-tags", [...tags.map((tag) => tagLabel(tag, colors.get(tag))), pick]);
}
