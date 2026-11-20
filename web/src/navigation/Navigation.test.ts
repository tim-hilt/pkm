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
  go(delta: number) {
    this.at += delta;
    this.listener();
  }
  back() {
    this.go(-1);
  }
  forward() {
    this.go(1);
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

  it("opens the Item of a path, keeping the path", () => {
    const { navigation, history } = open("/drawings/abc123abc123");
    expect(navigation.selected).toBe("drawings");
    expect(navigation.item).toBe("abc123abc123");
    expect(history.entries).toEqual(["/drawings/abc123abc123"]);
  });

  it("opens the Item of a Tasks path, keeping the path", () => {
    const { navigation, history } = open("/tasks/abc123abc123");
    expect(navigation.selected).toBe("tasks");
    expect(navigation.item).toBe("abc123abc123");
    expect(history.entries).toEqual(["/tasks/abc123abc123"]);
  });

  it("replaces a path below a Context without an Item list, or with a deeper path, by the Context's own", () => {
    expect(open("/books/abc").history.path).toBe("/books");
    expect(open("/tasks/abc/def").history.path).toBe("/tasks");
    expect(open("/drawings/abc/def").history.path).toBe("/drawings");
    expect(open("/drawings/").history.path).toBe("/drawings");
  });

  it("pushes an entry for an opened Item and tells its listener", () => {
    const { navigation, history } = open("/drawings");
    let changes = 0;
    navigation.onChange = () => changes++;
    navigation.openItem("abc123abc123");
    expect(navigation.item).toBe("abc123abc123");
    expect(history.entries).toEqual(["/drawings", "/drawings/abc123abc123"]);
    expect(changes).toBe(1);
    navigation.openItem("abc123abc123");
    expect(history.entries.length).toBe(2);
    expect(changes).toBe(1);
  });

  it("closes an Item it pushed by going back", () => {
    const { navigation, history } = open("/drawings");
    navigation.openItem("abc123abc123");
    navigation.closeItem();
    expect(navigation.item).toBeUndefined();
    expect(history.path).toBe("/drawings");
    expect(history.entries).toEqual(["/drawings", "/drawings/abc123abc123"]);
    expect(history.at).toBe(0);
  });

  it("leaves a pushed Item at once, and goes back only once, before the browser has gone back", () => {
    const { navigation, history } = open("/drawings");
    navigation.openItem("abc123abc123");
    let changes = 0;
    navigation.onChange = () => changes++;
    const goingBack: (() => void)[] = [];
    history.go = (delta) =>
      void goingBack.push(() => FakeHistory.prototype.go.call(history, delta));
    navigation.closeItem();
    navigation.closeItem();
    expect(navigation.item).toBeUndefined();
    expect(changes).toBe(1);
    expect(goingBack).toHaveLength(1);
    goingBack[0]();
    expect(history.path).toBe("/drawings");
    expect(navigation.item).toBeUndefined();
  });

  it("opens Items on top of each other, and closes them all by going back to the list", () => {
    const { navigation, history } = open("/tasks");
    navigation.openItem("aaaaaaaaaaaa");
    navigation.openItem("bbbbbbbbbbbb");
    expect(history.entries).toEqual(["/tasks", "/tasks/aaaaaaaaaaaa", "/tasks/bbbbbbbbbbbb"]);
    navigation.closeItem();
    expect(history.path).toBe("/tasks");
    expect(navigation.item).toBeUndefined();
  });

  it("goes back one Item, and then closes to the list", () => {
    const { navigation, history } = open("/tasks");
    navigation.openItem("aaaaaaaaaaaa");
    navigation.openItem("bbbbbbbbbbbb");
    navigation.back();
    expect(navigation.item).toBe("aaaaaaaaaaaa");
    navigation.closeItem();
    expect(history.path).toBe("/tasks");
  });

  it("closes Items on top of one opened by a link by replacing the current entry", () => {
    const { navigation, history } = open("/tasks/aaaaaaaaaaaa");
    navigation.openItem("bbbbbbbbbbbb");
    navigation.closeItem();
    expect(history.path).toBe("/tasks");
    expect(history.at).toBe(1);
  });

  it("closes an Item opened by a link by replacing its entry", () => {
    const { navigation, history } = open("/drawings/abc123abc123");
    navigation.closeItem();
    expect(navigation.item).toBeUndefined();
    expect(history.entries).toEqual(["/drawings"]);
  });

  it("replaces the entry of an unknown Item", () => {
    const { navigation, history } = open("/drawings/abc123abc123");
    let changes = 0;
    navigation.onChange = () => changes++;
    navigation.closeItem();
    expect(history.entries).toEqual(["/drawings"]);
    expect(changes).toBe(1);
  });

  it("opens and closes Items on back and forward", () => {
    const { navigation, history } = open("/drawings");
    navigation.openItem("abc123abc123");
    history.back();
    expect(navigation.item).toBeUndefined();
    history.forward();
    expect(navigation.item).toBe("abc123abc123");
    expect(navigation.selected).toBe("drawings");
  });

  it("leaves no Item when another Context is selected", () => {
    const { navigation, history } = open("/drawings/abc123abc123");
    navigation.select("books");
    expect(navigation.item).toBeUndefined();
    expect(history.path).toBe("/books");
  });

  it("selects a Context other than the one with the Item shown, also the same", () => {
    const { navigation, history } = open("/drawings/abc123abc123");
    navigation.select("drawings");
    expect(navigation.item).toBeUndefined();
    expect(history.path).toBe("/drawings");
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

  describe("Settings", () => {
    it("opens the Settings page of a path, keeping the path and the last Context", () => {
      const storage = new FakeStorage();
      open("/books", storage);
      for (const path of ["/settings", "/settings/tags", "/settings/cloud-backing"]) {
        const { navigation, history } = open(path, storage);
        expect(history.entries).toEqual([path]);
        expect(navigation.settings).toBe(path === "/settings" ? "main" : path.slice(10));
        expect(navigation.selected).toBe("books");
        expect(navigation.item).toBeUndefined();
      }
      expect(storage.getItem("lastContext")).toBe("books");
    });

    it("replaces an unknown Settings page and a deeper path by Settings", () => {
      expect(open("/settings/nowhere").history.path).toBe("/settings");
      expect(open("/settings/tags/x").history.path).toBe("/settings");
      expect(open("/settings/").history.path).toBe("/settings");
      expect(open("/settingsx").navigation.settings).toBeUndefined();
    });

    it("pushes an entry per page and tells its listener", () => {
      const { navigation, history } = open("/notes");
      let changes = 0;
      navigation.onChange = () => changes++;
      navigation.openSettings();
      navigation.openSettings("tags");
      navigation.openSettings("tags");
      expect(history.entries).toEqual(["/notes", "/settings", "/settings/tags"]);
      expect(navigation.settings).toBe("tags");
      expect(changes).toBe(2);
    });

    it("restores the pages on back and forward", () => {
      const { navigation, history } = open("/notes");
      navigation.openSettings();
      navigation.openSettings("cloud-backing");
      history.back();
      expect(navigation.settings).toBe("main");
      history.back();
      expect(navigation.settings).toBeUndefined();
      expect(navigation.selected).toBe("notes");
      history.forward();
      history.forward();
      expect(navigation.settings).toBe("cloud-backing");
    });

    it("leaves Settings by selecting a Context, also the one shown before", () => {
      const { navigation, history } = open("/notes");
      navigation.openSettings();
      navigation.select("notes");
      expect(navigation.settings).toBeUndefined();
      expect(history.path).toBe("/notes");
    });

    it("leaves an open Item for Settings", () => {
      const { navigation } = open("/tasks/abc123abc123");
      navigation.openSettings();
      expect(navigation.item).toBeUndefined();
    });
  });

  it("opens the Tasks list of a path, keeping the path", () => {
    const filters = open("/tasks/filters");
    expect(filters.navigation.list).toEqual({ kind: "filters" });
    const one = open("/tasks/filter/abc");
    expect(one.navigation.list).toEqual({ kind: "filter", id: "abc" });
    expect(one.navigation.item).toBeUndefined();
    expect(one.history.entries).toEqual(["/tasks/filter/abc"]);
  });

  it("pushes an entry per Tasks list, and restores it on back", () => {
    const { navigation, history } = open("/tasks/filters");
    navigation.showList({ kind: "filter", id: "abc" });
    navigation.showList({ kind: "filter", id: "abc" });
    expect(history.entries).toEqual(["/tasks/filters", "/tasks/filter/abc"]);
    history.back();
    expect(navigation.list).toEqual({ kind: "filters" });
  });

  it("settles the address of the list shown without an entry", () => {
    const { navigation, history } = open("/tasks");
    navigation.settleList({ kind: "filter", id: "abc" });
    expect(history.entries).toEqual(["/tasks/filter/abc"]);
  });
});
