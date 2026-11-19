import { readFileSync } from "node:fs";
import { beforeAll, describe, expect, it } from "vitest";
import { type Core, loadCore } from "../core/Core";
import { MemoryWebDav } from "./MemoryWebDav";
import { runExchange } from "./WebDavSender";

const generated = new URL("../../generated/", import.meta.url);
let core: Core;
beforeAll(async () => {
  core = await loadCore(readFileSync(new URL("pkm_web_bg.wasm", generated)));
});

const url = "https://dav.example/dav";

describe("nested paths over WebDAV", () => {
  it("puts, gets and scans blobs in directories, and lists empty ones", async () => {
    const server = new MemoryWebDav(url);
    const protocol = core.webDav(url, "user", "secret");
    const send = server.fetch;
    const bytes = new TextEncoder().encode("hello");
    expect((await runExchange(protocol.connect(), send)).error).toBeUndefined();

    expect(
      (await runExchange(protocol.put("Work/Plans/a.md", "t1", bytes, false), send)).error,
    ).toBeUndefined();
    await runExchange(protocol.put("top.md", "t2", bytes, false), send);
    await runExchange(protocol.createDir("Ideas/Later"), send);

    const found = await runExchange(protocol.scan(), send);
    expect(found.blobs.map((b) => b.name).sort()).toEqual(["Work/Plans/a.md", "top.md"]);
    expect(found.emptyDirs).toEqual(["Ideas/Later"]);
    const got = await runExchange(protocol.get("Work/Plans/a.md"), send);
    expect(new TextDecoder().decode(got.data)).toBe("hello");

    await runExchange(protocol.delete("Work/Plans/a.md"), send);
    const after = await runExchange(protocol.scan(), send);
    expect(after.emptyDirs.sort()).toEqual(["Ideas/Later", "Work/Plans"]);
  });
});
