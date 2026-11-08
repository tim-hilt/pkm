import type { SyncSchedule } from "../../generated/excalidraw_native_web.js";
import type { Core } from "../core/Core";
import type { DrawingStore } from "../gallery/DrawingStore";
import type { FileStorage } from "../storage/FileStorage";
import { runSyncStep } from "./SyncRunner";
import { SyncState } from "./SyncState";
import { type TabMessage, type Tabs, browserTabs } from "./Tabs";
import { type Fetch, runExchange } from "./WebDavSender";

/** Why connecting failed. */
export type ConnectReason = "wrongCredentials" | "unreachable" | "notHttps" | "notWebDav";

/** Connecting failed; `reason` says why and `connectMessage` words it. */
export class ConnectFailed extends Error {
  constructor(readonly reason: ConnectReason) {
    super(connectMessage(reason));
  }
}

/** What Settings says next to the form: the native app's wording, with CORS named for an unreachable server. */
export function connectMessage(reason: ConnectReason): string {
  switch (reason) {
    case "wrongCredentials":
      return "Wrong username or password";
    case "unreachable":
      return "Server not reachable, its certificate is not trusted, or the server doesn't allow this site (CORS)";
    case "notHttps":
      return "URL must start with https://, so your password never travels in plain text";
    case "notWebDav":
      return "Not a WebDAV server";
  }
}

/** Where the Drawings go, as Settings shows it. */
export interface Connection {
  provider: "WebDAV";
  /** The Server URL. */
  server: string;
}

/** What is kept in the site's storage: all of it needed to resume. */
interface Credentials {
  url: string;
  username: string;
  password: string;
}

// Outside the Drawings folder, so the Drawing store never lists it. The
// password sits beside the Drawings: browsers have no keychain (ADR-0016).
const credentialsFile = "Cloud/connection.json";

/**
 * Cloud Backing of the web app, with WebDAV as the only Cloud Provider: Connect
 * and Disconnect, the connection kept in the site's storage and restored on
 * the next visit, and, while Connected, the Sync loop. The loop runs the core's
 * schedule with timers and Sync steps against the Drawing store, the Sync
 * state and `fetch`; it is told what happens around it (`localChange`,
 * `setVisible`, `setOnline`) and tells what a step changed (`onSynced`).
 * `fetch` is a dependency, so tests replace the network.
 *
 * Several tabs of one browser share the storage: a step runs only while its
 * tab holds the Sync lock, and a tab that changed Drawings, Connected or
 * Disconnected tells the others, which follow (`Tabs`).
 */
export class CloudBacking {
  private current: Credentials | undefined;
  private readonly listeners = new Set<(connection: Connection | undefined) => void>();
  private readonly syncedListeners = new Set<(names: string[]) => void>();
  private readonly state: SyncState;
  private visible = true;
  private online = true;
  /** Present while Connected. */
  private schedule: SyncSchedule | undefined;
  private timer: ReturnType<typeof setTimeout> | undefined;
  /** The step under way. */
  private step: Promise<void> | undefined;
  private readonly incoming = { known: false };

  private constructor(
    private readonly core: Core,
    private readonly storage: FileStorage,
    private readonly send: Fetch,
    private readonly drawings: DrawingStore,
    private readonly tabs: Tabs,
  ) {
    this.state = new SyncState(storage);
    tabs.listen((message) => void this.heard(message));
  }

  /** The module, Connected (and Syncing) if the storage holds a connection. */
  static async open(
    core: Core,
    storage: FileStorage,
    send: Fetch,
    drawings: DrawingStore,
    tabs: Tabs = browserTabs(),
  ): Promise<CloudBacking> {
    const cloud = new CloudBacking(core, storage, send, drawings, tabs);
    cloud.current = await cloud.saved();
    if (cloud.current) cloud.startLoop();
    return cloud;
  }

  private async saved(): Promise<Credentials | undefined> {
    const data = await this.storage.read(credentialsFile);
    return data && JSON.parse(new TextDecoder().decode(data));
  }

  /** Where the Drawings go; none while Disconnected. */
  get connection(): Connection | undefined {
    return this.current && { provider: "WebDAV", server: this.current.url };
  }

  /** Calls `listener` whenever the connection changes. Returns a function that stops it. */
  onConnectionChange(listener: (connection: Connection | undefined) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * Checks the server and makes its `ExcalidrawNative` folder, then keeps the
   * connection. Throws `ConnectFailed` and changes nothing if that fails.
   */
  async connect(url: string, username: string, password: string): Promise<void> {
    let protocol;
    try {
      protocol = this.core.webDav(url, username, password);
    } catch {
      throw new ConnectFailed("notHttps");
    }
    const result = await runExchange(protocol.connect(), this.send);
    protocol.free();
    if (result.error) throw new ConnectFailed(result.error as ConnectReason);
    await this.stopLoop();
    this.current = { url: url.trim().replace(/\/+$/, ""), username, password };
    await this.storage.write(
      credentialsFile,
      new TextEncoder().encode(JSON.stringify(this.current)),
    );
    this.startLoop();
    this.changed();
    this.tabs.post({ kind: "connected" });
  }

  /** Stops Syncing, and forgets the connection, its credentials and the Sync state. Every Drawing stays. */
  async disconnect(): Promise<void> {
    this.current = undefined;
    await this.stopLoop();
    // Under the lock: no step of another tab writes Sync state after it is gone.
    await this.tabs.exclusive(() => this.storage.remove("Cloud"));
    this.incoming.known = false;
    this.changed();
    this.tabs.post({ kind: "disconnected" });
  }

  /** A Drawing was created, edited, renamed, duplicated, imported or deleted here: Sync follows after a moment. */
  localChange(): void {
    this.schedule?.localChange(Date.now());
    this.arm();
  }

  /** The tab was shown or hidden. Nothing Syncs while hidden; shown again, a step runs at once. */
  setVisible(visible: boolean): void {
    this.visible = visible;
    if (visible) this.schedule?.cameToFront(Date.now());
    else this.schedule?.wentToBackground();
    this.arm();
  }

  /** The browser went online or offline. Nothing Syncs while offline; back online, a step runs at once. */
  setOnline(online: boolean): void {
    this.online = online;
    this.schedule?.setOnline(online, Date.now());
    this.arm();
  }

  /** Calls `listener` with the names of the Drawings a step changed here (a rename as the old name and the new). Returns a function that stops it. */
  onSynced(listener: (names: string[]) => void): () => void {
    this.syncedListeners.add(listener);
    return () => this.syncedListeners.delete(listener);
  }

  /** Follows what another tab did: the storage already holds the new connection (or none). */
  private async heard(message: TabMessage): Promise<void> {
    if (message.kind === "changed") {
      for (const listener of this.syncedListeners) listener(message.names);
      return;
    }
    const saved = message.kind === "connected" ? await this.saved() : undefined;
    await this.stopLoop();
    this.current = saved;
    this.incoming.known = false;
    if (saved) this.startLoop();
    this.changed();
  }

  private startLoop(): void {
    const now = Date.now();
    this.schedule = this.core.syncSchedule(now);
    if (!this.visible) this.schedule.wentToBackground();
    if (!this.online) this.schedule.setOnline(false, now);
    this.arm();
  }

  /** Stops the loop and waits for the step under way to give up: it makes no write after this. */
  private async stopLoop(): Promise<void> {
    clearTimeout(this.timer);
    this.schedule?.free();
    this.schedule = undefined;
    await this.step;
  }

  /** Sets the timer for the next step; none while one is running (it sets it when it ends). */
  private arm(): void {
    clearTimeout(this.timer);
    if (!this.schedule || this.step) return;
    const wait = this.schedule.wait(Date.now());
    if (wait !== undefined) this.timer = setTimeout(() => this.tick(), wait);
  }

  private tick(): void {
    if (!this.schedule?.takeDue(Date.now())) return this.arm();
    this.step = this.tabs
      .exclusive(() => this.runStep())
      .finally(() => {
        this.step = undefined;
        this.arm();
      });
  }

  private async runStep(): Promise<void> {
    const schedule = this.schedule;
    const credentials = this.current;
    if (!schedule || !credentials) return;
    // Another tab may have Disconnected or Connected elsewhere while this one waited for the lock.
    const saved = await this.saved();
    if (!saved || saved.url !== credentials.url || saved.password !== credentials.password) return;
    const protocol = this.core.webDav(credentials.url, credentials.username, credentials.password);
    let result;
    try {
      result = await runSyncStep({
        core: this.core,
        protocol,
        send: this.send,
        drawings: this.drawings,
        state: this.state,
        incoming: this.incoming,
        cancelled: () => this.schedule !== schedule,
      });
    } catch {
      // Whatever broke is no reason to stop Syncing: the step counts as failed.
      result = { changed: [], error: "failed", accessLost: false, storageFull: false };
    } finally {
      protocol.free();
    }
    if (!result || this.schedule !== schedule) return;
    schedule.stepEnded(Date.now(), result.error === undefined);
    if (result.accessLost) schedule.accessLost();
    if (result.changed.length > 0) {
      for (const listener of this.syncedListeners) listener(result.changed);
      this.tabs.post({ kind: "changed", names: result.changed });
    }
  }

  private changed(): void {
    for (const listener of this.listeners) listener(this.connection);
  }
}
