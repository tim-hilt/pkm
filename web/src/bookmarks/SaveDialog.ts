import { button, make } from "../Dom";
import { confirmButton, field, modal, textInput } from "../tasks/TaskDialogs";
import { tagsRow } from "../tasks/TagsRow";
import { type HandedPage, bookmarklet } from "./Bookmarklet";
import type { BookmarkRow, BookmarksModel, Draft } from "./BookmarksModel";

/**
 * The Save dialog: the Address to paste, then the page downloaded through the server, with its
 * Title to rename, its Tags, whether to keep its Article (the app's decision, which the user can
 * flip), the progress of the downloads, and the reason if the page couldn't be read. An Address
 * Saved already closes the dialog and `found` shows that Bookmark. A page `handed` over by the
 * bookmarklet is read as it is, without a download, and its step is shown at once; if its Bookmark
 * has no Article yet, the dialog offers Add Article from that page first. Until then, the dialog
 * offers the bookmarklet for copying. A port of the Swift `SaveSheet`.
 */
export function saveDialog(
  parent: HTMLElement,
  model: BookmarksModel,
  found: (bookmark: BookmarkRow) => void,
  handed?: HandedPage,
): void {
  const address = textInput("");
  address.placeholder = address.ariaLabel = "Address";
  address.autocapitalize = "off";
  address.autocomplete = "off";
  address.spellcheck = false;
  address.inputMode = "url";
  const note = make("p", "tasks-hint");
  note.setAttribute("role", "status");
  const copy = button("tasks-back", "Copy Bookmarklet", () => {
    void navigator.clipboard.writeText(bookmarklet(location.origin)).then(
      () => (copy.textContent = "Copied"),
      () => {},
    );
  });
  const tip = make("p", "tasks-hint", [
    "To Save the page of any browser tab as you see it, add the bookmarklet to your bookmarks: ",
    copy,
  ]);
  const step = make("div", "tasks-form");
  step.append(field("Address", address), ...(handed ? [] : [tip]));
  const next = confirmButton("Next");
  const { dialog, form } = modal(
    parent,
    "Save Bookmark",
    [step, note],
    [button("", "Cancel", () => dialog.close()), next],
  );

  let draft: Draft | undefined;
  /** The Bookmark of the handed page, Saved before without an Article, to Add it to. */
  let lacking: BookmarkRow | undefined;
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
    const prepared = await model.prepare(
      address.value,
      (done, total) => {
        progress.max = total;
        progress.value = done;
      },
      handed?.html,
    );
    working = next.disabled = false;
    note.replaceChildren();
    if (!dialog.isConnected) {
      if (prepared.kind === "draft") model.discard(prepared.draft);
      return;
    }
    if (prepared.kind === "invalid") {
      note.textContent = "This is not a web address.";
    } else if (prepared.kind === "existing" && handed && !prepared.bookmark.hasArticle) {
      lacking = prepared.bookmark;
      address.readOnly = true;
      note.textContent = "Saved already, without its Article.";
      next.textContent = "Add Article";
    } else if (prepared.kind === "existing") {
      dialog.close();
      found(prepared.bookmark);
    } else {
      draft = prepared.draft;
      tip.remove();
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

  const addArticle = async (bookmark: BookmarkRow, html: string) => {
    working = next.disabled = true;
    const added = await model.addArticle(bookmark.id, html);
    working = next.disabled = false;
    if (!added) {
      note.textContent = "The page has no text to keep.";
    } else if (dialog.isConnected) {
      dialog.close();
      found(bookmark);
    }
  };

  form.addEventListener("submit", (event) => {
    event.preventDefault();
    if (working) return;
    if (lacking && handed) return void addArticle(lacking, handed.html);
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
  if (handed) {
    address.value = handed.address;
    void prepare();
  } else {
    address.focus();
  }
}
