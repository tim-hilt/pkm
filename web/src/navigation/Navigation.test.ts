import { describe, expect, it } from "vitest";
import { type BrowserHistory, contexts, Navigation } from "./Navigation";

/** A browser's History: a stack of paths and a position in it. */
class FakeHistory implements BrowserHistory {
  entries: string[];
  at = 0;
  private listener = () => {};
  constructor(path: string) {
    this.entries = [path];
  }
  get path() {
    return this.entries[this.at];
  }
  push(path: string) {
    this.entries = [...this.entries.slice(0, this.at + 1), path];
    this.at++;
  }
  replace(path: string) {
    this.entries[this.at] = path;
  }
  onTraverse(listener: () => void) {
    this.listener = listener;
  }
  back() {
    this.at--;
    this.listener();
  }
  forward() {
    this.at++;
    this.listener();
  }
}

/** The part of localStorage the navigation uses. */
class FakeStorage {
  values = new Map<string, string>();
  getItem = (key: string) => this.values.get(key) ?? null;
  setItem = (key: string, value: string) => void this.values.set(key, value);
}

function open(path: string, storage = new FakeStorage()) {
  const history = new FakeHistory(path);
  return { history, storage, navigation: new Navigation(history, storage) };
}

describe("Navigation", () => {
  it("has the five Contexts in their fixed order", () => {
    expect(contexts).toEqual(["tasks", "notes", "drawings", "bookmarks", "books"]);
  });

  it("opens the Context of the path", () => {
    const { navigation, history } = open("/books");
    expect(navigation.selected).toBe("books");
    expect(history.entries).toEqual(["/books"]);
  });

  it("redirects / to Tasks on the first visit, without a History entry", () => {
    const { navigation, history } = open("/");
    expect(navigation.selected).toBe("tasks");
    expect(history.entries).toEqual(["/tasks"]);
  });

  it("redirects / to the last Context", () => {
    const storage = new FakeStorage();
    open("/notes", storage);
    const { navigation, history } = open("/", storage);
    expect(navigation.selected).toBe("notes");
    expect(history.path).toBe("/notes");
  });

  it("replaces an unknown path with the last Context", () => {
    const { navigation, history } = open("/nowhere/at/all");
    expect(navigation.selected).toBe("tasks");
    expect(history.entries).toEqual(["/tasks"]);
  });

  it("shows the Context of a path to an Item it does not know, at the Context's own path", () => {
    const { navigation, history } = open("/drawings/abc123abc123");
    expect(navigation.selected).toBe("drawings");
    expect(history.entries).toEqual(["/drawings"]);
  });

  it("does not take a Context name for a prefix", () => {
    expect(open("/drawingsx").navigation.selected).toBe("tasks");
  });

  it("pushes a History entry per switch and tells its listener", () => {
    const { navigation, history } = open("/tasks");
    let changes = 0;
    navigation.onChange = () => changes++;
    navigation.select("books");
    navigation.select("drawings");
    expect(history.entries).toEqual(["/tasks", "/books", "/drawings"]);
    expect(navigation.selected).toBe("drawings");
    expect(changes).toBe(2);
  });

  it("pushes nothing when the Context is already shown", () => {
    const { navigation, history } = open("/tasks");
    let changes = 0;
    navigation.onChange = () => changes++;
    navigation.select("tasks");
    expect(history.entries).toEqual(["/tasks"]);
    expect(changes).toBe(0);
  });

  it("restores the Context on back and forward", () => {
    const { navigation, history } = open("/tasks");
    navigation.select("notes");
    navigation.select("books");
    history.back();
    expect(navigation.selected).toBe("notes");
    history.back();
    expect(navigation.selected).toBe("tasks");
    history.forward();
    expect(navigation.selected).toBe("notes");
  });

  it("tells its listener when back changes the Context", () => {
    const { navigation, history } = open("/tasks");
    navigation.select("notes");
    let changes = 0;
    navigation.onChange = () => changes++;
    history.back();
    expect(changes).toBe(1);
  });

  it("remembers the Context last shown, also after back", () => {
    const { navigation, history, storage } = open("/tasks");
    navigation.select("books");
    expect(open("/", storage).navigation.selected).toBe("books");
    history.back();
    expect(open("/", storage).navigation.selected).toBe("tasks");
  });

  it("ignores a remembered Context that no longer exists", () => {
    const storage = new FakeStorage();
    storage.setItem("lastContext", "calendar");
    expect(open("/", storage).navigation.selected).toBe("tasks");
  });
});
