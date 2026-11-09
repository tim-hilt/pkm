import { readFileSync } from "node:fs";
import { beforeAll, beforeEach, describe, expect, it } from "vitest";
import { type Core, loadCore } from "../core/Core";
import { DrawingStore } from "../drawings/gallery/DrawingStore";
import { MemoryStorage } from "../storage/MemoryStorage";
import { CloudBacking, ConnectFailed, connectMessage } from "./CloudBacking";
import { MemoryWebDav } from "./MemoryWebDav";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const url = "https://dav.example/dav";
let server: MemoryWebDav;
let storage: MemoryStorage;
beforeEach(() => {
  server = new MemoryWebDav(url);
  storage = new MemoryStorage();
});

const open = () => CloudBacking.open(core, storage, server.fetch, new DrawingStore(storage));

const failure = async (promise: Promise<unknown>) => {
  const error = await promise.then(
    () => undefined,
    (e) => e,
  );
  expect(error).toBeInstanceOf(ConnectFailed);
  return (error as ConnectFailed).reason;
};

describe("Connect", () => {
  it("connects and makes the pkm folder", async () => {
    const cloud = await open();
    await cloud.connect(url, "user", "secret");
    expect(cloud.connection).toEqual({ provider: "WebDAV", server: url });
    expect(server.hasFolder("pkm")).toBe(true);
  });

  it("keeps an existing folder and its Drawings", async () => {
    const cloud = await open();
    await cloud.connect(url, "user", "secret");
    const again = await open();
    await again.disconnect();
    await again.connect(`${url}/`, "user", "secret");
    expect(again.connection?.server).toBe(url);
  });

  it("refuses an http URL without sending anything", async () => {
    const cloud = await open();
    expect(await failure(cloud.connect("http://dav.example/dav", "user", "secret"))).toBe(
      "notHttps",
    );
    expect(server.requests).toEqual([]);
    expect(cloud.connection).toBeUndefined();
  });

  it("fails on wrong credentials", async () => {
    const cloud = await open();
    expect(await failure(cloud.connect(url, "user", "wrong"))).toBe("wrongCredentials");
    expect(cloud.connection).toBeUndefined();
  });

  it("fails on a server that is not WebDAV", async () => {
    server.notWebDav = true;
    const cloud = await open();
    expect(await failure(cloud.connect(url, "user", "secret"))).toBe("notWebDav");
  });

  it("reports a throwing fetch as unreachable", async () => {
    server.offline = true;
    const cloud = await open();
    expect(await failure(cloud.connect(url, "user", "secret"))).toBe("unreachable");
    expect(cloud.connection).toBeUndefined();
  });

  it("words the failures as the native app does, the unreachable one naming CORS", () => {
    expect(connectMessage("wrongCredentials")).toBe("Wrong username or password");
    expect(connectMessage("notWebDav")).toBe("Not a WebDAV server");
    expect(connectMessage("notHttps")).toBe(
      "URL must start with https://, so your password never travels in plain text",
    );
    expect(connectMessage("unreachable")).toBe(
      "Server not reachable, its certificate is not trusted, or the server doesn't allow this site (CORS)",
    );
  });

  it("sends Authorization: Basic with every request", async () => {
    const cloud = await open();
    await cloud.connect(url, "user", "secret");
    expect(server.authorizations.length).toBeGreaterThan(0);
    expect(server.authorizations.every((a) => a === `Basic ${btoa("user:secret")}`)).toBe(true);
  });
});

describe("the connection", () => {
  it("survives a restart of the module", async () => {
    const cloud = await open();
    await cloud.connect(url, "user", "secret");
    const restarted = await open();
    expect(restarted.connection).toEqual({ provider: "WebDAV", server: url });
  });

  it("is not there before Connect", async () => {
    expect((await open()).connection).toBeUndefined();
  });

  it("is forgotten by Disconnect, also after a restart", async () => {
    const cloud = await open();
    await cloud.connect(url, "user", "secret");
    await cloud.disconnect();
    expect(cloud.connection).toBeUndefined();
    expect((await open()).connection).toBeUndefined();
  });

  it("tells listeners about Connect and Disconnect", async () => {
    const cloud = await open();
    const seen: unknown[] = [];
    cloud.onConnectionChange((connection) => seen.push(connection?.server));
    await cloud.connect(url, "user", "secret");
    await cloud.disconnect();
    expect(seen).toEqual([url, undefined]);
  });
});
