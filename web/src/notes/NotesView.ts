import { button, make, onNewKey } from "../Dom";
import { DescriptionImages } from "../tasks/DescriptionImages";
import { DescriptionField } from "../tasks/DescriptionField";
import { RowMenu } from "../tasks/RowMenu";
import { confirmButton, modal } from "../tasks/TaskDialogs";
import { tagLabel } from "../tasks/TagLabel";
import { tagsRow } from "../tasks/TagsRow";
import { titleNodes } from "../tasks/TitleText";
import { type ListedNote, type NotesModel, type SearchHit } from "./NotesModel";
import { TitleField } from "./TitleField";

const plusGlyph = `<svg viewBox="0 0 24 24" width="24" height="24" fill="none" stroke="currentColor" stroke-width="2.2" stroke-linecap="round" aria-hidden="true"><path d="M12 5v14M5 12h14"/></svg>`;

/**
 * The Notes Context: the flat list of Notes (newest changed first) and the
 * open Note's editor, side by side; where the window is narrow the editor
 * covers the list (the CSS decides, from `has-open`). A Note is Deleted from
 * its row's menu (right click, long press), with ⌫ on a row or from the
 * editor, asking first when its Body has text. A port of the Swift
 * `NotesView`.
 */
export class NotesView {
  readonly element = make("main", "notes");
  private readonly list = make("ul", "tasks-list");
  private readonly empty = make("div", "tasks-empty", [
    make("strong", "", ["No Notes"]),
    make("span", "", ["Tap + to write one."]),
  ]);
  private readonly noResults = make("div", "tasks-empty");
  private readonly search = make("input", "notes-search");
  private readonly detail = make("div", "notes-detail");
  private readonly failure = make("p", "tasks-failure");
  private readonly menu: RowMenu;
  /** The Note the editor shows; the editor is built again for another. */
  private shown: string | undefined;
  /** The Images of the Note the editor shows. */
  private images: DescriptionImages | undefined;
  /** The editor's fields, and the model's `syncs` when they last took the Note as saved. */
  private fields:
    | { title: TitleField; body: DescriptionField; tags: HTMLElement; syncs: number }
    | undefined;

  constructor(private readonly model: NotesModel) {
    this.failure.setAttribute("role", "alert");
    this.list.setAttribute("aria-label", "Notes");
    this.menu = new RowMenu(this.element);
    this.search.type = "search";
    this.search.placeholder = "Search";
    this.search.ariaLabel = "Search Notes";
    this.search.addEventListener("input", () => model.search(this.search.value));
    const create = button("tasks-new", "", () => model.add());
    create.innerHTML = plusGlyph;
    create.title = create.ariaLabel = "New Note";
    const scroll = make("div", "tasks-scroll", [this.list, this.empty, this.noResults]);
    const side = make("div", "notes-side", [
      make("div", "notes-bar", [this.search]),
      scroll,
      this.failure,
    ]);
    this.element.append(side, this.detail, create, this.menu.element);
    onNewKey(this.element, () => model.add());
    model.onChange = () => this.update();
    this.update();
  }

  /** Shows the list and the open Note as the model has them. */
  update(): void {
    const { model } = this;
    this.menu.hide();
    this.element.classList.toggle("has-open", model.openId !== undefined);
    const searching = model.searchText.trim() !== "";
    this.list.replaceChildren(
      ...(searching
        ? model.results.map((hit) => this.result(hit))
        : model.notes.map((row) => this.row(row))),
    );
    this.empty.hidden = searching || model.notes.length > 0;
    this.noResults.hidden = !searching || model.results.length > 0;
    this.noResults.replaceChildren(
      make("strong", "", [`No Results for “${model.searchText.trim()}”`]),
    );
    this.failure.hidden = model.failure === undefined;
    this.failure.textContent = model.failure ?? "";
    this.showEditor();
  }

  private row(row: ListedNote): HTMLElement {
    const open = button("task-open note-open", "", () => this.model.open(row.id));
    open.append(...this.rowText(row), ...this.pills(row.tags));
    const item = make("li", "task-row note-row", [open]);
    if (row.id === this.model.openId) open.setAttribute("aria-current", "true");
    this.menu.attach(item, () => [
      { label: "Copy Note", run: () => this.copy(row.id) },
      { label: "Delete", danger: true, run: () => this.ask(row.id) },
    ]);
    item.addEventListener("keydown", (event) => {
      if (event.key === "Backspace" || event.key === "Delete") this.ask(row.id);
    });
    return item;
  }

  /** A Search result: the Title, and the snippet with the match in bold. */
  private result(hit: SearchHit): HTMLElement {
    const open = button("task-open note-open", "", () => this.model.open(hit.id));
    const { snippet, matchStart, matchEnd } = hit;
    const text = make("span", hit.title === "" ? "task-title" : "task-description", [
      snippet.slice(0, matchStart),
      make("b", "", [snippet.slice(matchStart, matchEnd)]),
      snippet.slice(matchEnd),
    ]);
    open.append(
      ...(hit.title === "" ? [] : [make("span", "task-title", titleNodes(hit.title))]),
      text,
    );
    if (hit.id === this.model.openId) open.setAttribute("aria-current", "true");
    return make("li", "task-row note-row", [open]);
  }

  /** The Tags as pills in their Tag Colors; none for a Note without Tags. */
  private pills(tags: string[]): HTMLElement[] {
    if (tags.length === 0) return [];
    const colors = new Map(this.model.knownTags().map((tag) => [tag.name, tag.color]));
    return [
      make(
        "span",
        "task-tags",
        tags.map((tag) => tagLabel(tag, colors.get(tag))),
      ),
    ];
  }

  /** The Tags row of the editor: the Note's Tags and the button that picks them. */
  private tagsRow(id: string, tags: string[]): HTMLElement {
    return tagsRow(this.model, tags, (picked) => void this.model.setTags(id, picked));
  }

  /** The Title formatted, or the Body's first line, or "New Note". */
  private rowText({ title, preview }: ListedNote): HTMLElement[] {
    const line = (text: string, className: string) => make("span", className, titleNodes(text));
    if (title !== "") {
      return [
        line(title, "task-title"),
        ...(preview !== "" ? [line(preview, "task-description")] : []),
      ];
    }
    return [preview !== "" ? line(preview, "task-title") : make("span", "note-none", ["New Note"])];
  }

  /**
   * The editor of the open Note; built again only when another Note opens.
   * After Sync changed the files it takes the Note as Sync left it, so that
   * the next keystroke doesn't save over what another Device wrote.
   */
  private showEditor(): void {
    const id = this.model.openId;
    if (id === this.shown) {
      const { fields } = this;
      if (id === undefined || !fields) return;
      const note = this.model.note(id);
      // Rebuilt on any change, as Tag Colors may have changed too.
      if (note) fields.tags.replaceChildren(this.tagsRow(id, note.tags));
      if (fields.syncs === this.model.syncs) return;
      fields.syncs = this.model.syncs;
      if (note && note.title !== fields.title.value) fields.title.value = note.title;
      if (note && note.body !== fields.body.value) fields.body.value = note.body;
      return;
    }
    this.shown = id;
    this.fields = undefined;
    this.images?.dispose();
    this.images = undefined;
    const note = id === undefined ? undefined : this.model.note(id);
    if (id === undefined || !note) {
      this.detail.replaceChildren(
        make("div", "tasks-empty", [make("strong", "", ["No Note Selected"])]),
      );
      return;
    }
    const save = () => this.model.save(id, title.value, body.value);
    const images = (this.images = new DescriptionImages({
      addImage: (bytes) => this.model.addImage(id, bytes),
      readImage: (reference) => this.model.readImage(id, reference),
    }));
    const body = new DescriptionField(note.body, images);
    body.element.setAttribute("aria-label", "Note");
    body.element.dataset.placeholder = "Note";
    const title = new TitleField(note.title, { change: save, submit: () => body.focus() });
    body.element.addEventListener("input", save);
    const more = button("tasks-back", "⋯", () => {
      const { right, bottom } = more.getBoundingClientRect();
      this.menu.show([{ label: "Copy Note", run: () => this.copy(id) }], right, bottom, more);
    });
    more.ariaLabel = "More";
    more.setAttribute("aria-haspopup", "menu");
    const bar = make("div", "notes-bar", [
      button("tasks-back", "Notes", () => this.model.open(undefined)),
      make("div", "notes-bar-end", [button("tasks-back", "Delete", () => this.ask(id)), more]),
    ]);
    const tags = make("div", "notes-tags", [this.tagsRow(id, note.tags)]);
    this.detail.replaceChildren(
      bar,
      make("div", "notes-editor", [title.element, tags, body.element]),
    );
    this.fields = { title, body, tags, syncs: this.model.syncs };
    if (this.model.newId === id) title.focus();
  }

  /** Puts the Note's text on the clipboard. */
  private copy(id: string): void {
    void navigator.clipboard.writeText(this.model.copyText(id));
  }

  /** Deletes the Note, or asks first if its Body has text. */
  private ask(id: string): void {
    const { model } = this;
    if (!model.confirmsDelete(id)) return model.delete(id);
    const confirm = confirmButton("Delete");
    const { dialog, form } = modal(
      this.element,
      "Delete this Note?",
      [],
      [button("", "Cancel", () => dialog.close()), confirm],
    );
    form.addEventListener("submit", (event) => {
      event.preventDefault();
      model.delete(id);
      dialog.close();
    });
  }
}
