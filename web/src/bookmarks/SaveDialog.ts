import { button, make } from "../Dom";
import { confirmButton, field, modal, textInput } from "../tasks/TaskDialogs";
import { pickTagsDialog } from "../tasks/TagPicker";
import { tagLabel } from "../tasks/TagLabel";
import type { BookmarkRow, BookmarksModel, Draft } from "./BookmarksModel";

/** The Tags of a Bookmark as labels, with the button that picks them. */
export function tagsRow(
  model: Pick<BookmarksModel, "knownTags">,
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
  return make("div", "bookmark-tags", [...tags.map((tag) => tagLabel(tag, colors.get(tag))), pick]);
}

/**
 * The Save dialog: the Address to paste, then the page downloaded through the server, with its
 * Title to rename, its Tags, whether to keep its Article (the app's decision, which the user can
 * flip), the progress of the downloads, and the reason if the page couldn't be read. An Address
 * Saved already closes the dialog and `found` shows that Bookmark. A port of the Swift `SaveSheet`.
 */
export function saveDialog(
  parent: HTMLElement,
  model: BookmarksModel,
  found: (bookmark: BookmarkRow) => void,
): void {
  const address = textInput("");
  address.placeholder = address.ariaLabel = "Address";
  address.autocapitalize = "off";
  address.autocomplete = "off";
  address.spellcheck = false;
  address.inputMode = "url";
  const note = make("p", "tasks-hint");
  note.setAttribute("role", "status");
  const step = make("div", "tasks-form");
  step.append(field("Address", address));
  const next = confirmButton("Next");
  const { dialog, form } = modal(
    parent,
    "Save Bookmark",
    [step, note],
    [button("", "Cancel", () => dialog.close()), next],
  );

  let draft: Draft | undefined;
  let working = false;
  const title = textInput("");
  title.ariaLabel = "Title";
  const keep = make("input", "");
  keep.type = "checkbox";
  let tags: string[] = [];
  const tagsField = make("div", "");
  const showTags = () =>
    tagsField.replaceChildren(
      tagsRow(model, tags, (picked) => {
        tags = picked;
        showTags();
      }),
    );

  const prepare = async () => {
    working = next.disabled = true;
    note.textContent = "";
    const progress = make("progress", "");
    note.replaceChildren(progress);
    const prepared = await model.prepare(address.value, (done, total) => {
      progress.max = total;
      progress.value = done;
    });
    working = next.disabled = false;
    note.replaceChildren();
    if (!dialog.isConnected) {
      if (prepared.kind === "draft") model.discard(prepared.draft);
      return;
    }
    if (prepared.kind === "invalid") {
      note.textContent = "This is not a web address.";
    } else if (prepared.kind === "existing") {
      dialog.close();
      found(prepared.bookmark);
    } else {
      draft = prepared.draft;
      address.readOnly = true;
      title.value = draft.title;
      keep.checked = draft.readsLikeArticle;
      step.append(field("Title", title), tagsField);
      if (draft.markdown !== "") {
        step.append(
          make("label", "tasks-field bookmark-keep", [keep, make("span", "", ["Keep Article"])]),
        );
      }
      if (draft.failure !== undefined) {
        note.textContent = `Saved with its address only: ${draft.failure}`;
      }
      showTags();
      next.textContent = "Save";
      title.focus();
    }
  };

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (working) return;
    if (!draft) return void prepare();
    const saving = draft;
    draft = undefined;
    dialog.close();
    void model.save(saving, title.value, tags, keep.checked && saving.markdown !== "");
  });
  // Cancel or Esc: what was downloaded is dropped.
  dialog.addEventListener("close", () => {
    if (draft) model.discard(draft);
    draft = undefined;
  });
  address.focus();
}
