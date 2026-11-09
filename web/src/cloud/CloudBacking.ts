import type { SyncSchedule } from "../../generated/pkm_web.js";
import type { Core } from "../core/Core";
import type { DrawingStore } from "../drawings/gallery/DrawingStore";
import type { FileStorage } from "../storage/FileStorage";
import { type SyncStatus, runSyncStep } from "./SyncRunner";
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

/** What Cloud Backing is doing, as Settings and the Gallery show it. */
export interface SyncReport {
  /** Undefined until the first step is through. */
  status?: SyncStatus;
  /** Drawings done of all Drawings a running step transfers. */
  progress?: { done: number; total: number };
  /** When a step last went through, in milliseconds since the epoch. */
  lastSync?: number;
}

/** What Settings says of `state`, as the native app does; `provider` names who is full. */
export function statusText({ status, progress }: SyncReport, provider: string): string {
  switch (status) {
    case undefined:
      return "Not synced yet";
    case "upToDate":
      return "Up to date";
    case "syncing":
      return progress ? `Syncing ${progress.done} of ${progress.total} Drawings` : "Syncing";
    case "offline":
      return "Offline";
    case "accessLost":
      return "Access lost";
    case "full":
      return `${provider} is full`;
  }
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
  private report: SyncReport = {};
  private readonly reportListeners = new Set<(report: SyncReport) => void>();
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

  /** Throws `ConnectFailed` unless the server takes `credentials` and has (or now has) its `pkm` folder. */
  private async check({ url, username, password }: Credentials): Promise<void> {
    let protocol;
    try {
      protocol = this.core.webDav(url, username, password);
    } catch {
      throw new ConnectFailed("notHttps");
    }
    const result = await runExchange(protocol.connect(), this.send);
    protocol.free();
    if (result.error) throw new ConnectFailed(result.error as ConnectReason);
  }

  /** Uses `credentials` from now on, here and on the next visit. */
  private async keep(credentials: Credentials): Promise<void> {
    this.current = credentials;
    await this.storage.write(
      credentialsFile,
      new TextEncoder().encode(JSON.stringify(credentials)),
    );
  }

  /** Where the Drawings go; none while Disconnected. */
  get connection(): Connection | undefined {
    return this.current && { provider: "WebDAV", server: this.current.url };
  }

  /** What Sync is doing; empty until the first step is through, and while Disconnected. */
  get syncReport(): SyncReport {
    return this.report;
  }

  /** Whether Sync is blocked until the user acts, which the Gallery shows with a warning: access lost or the server full. */
  get hasProblem(): boolean {
    return this.report.status === "accessLost" || this.report.status === "full";
  }

  /** Calls `listener` whenever the status, the progress or the time of the last Sync changes. Returns a function that stops it. */
  onSyncChange(listener: (report: SyncReport) => void): () => void {
    this.reportListeners.add(listener);
    return () => this.reportListeners.delete(listener);
  }

  /** Calls `listener` whenever the connection changes. Returns a function that stops it. */
  onConnectionChange(listener: (connection: Connection | undefined) => void): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /**
   * Checks the server and makes its `pkm` folder, then keeps the
   * connection. Throws `ConnectFailed` and changes nothing if that fails.
   */
  async connect(url: string, username: string, password: string): Promise<void> {
    await this.check({ url, username, password });
    await this.stopLoop();
    this.setReport({});
    await this.keep({ url: url.trim().replace(/\/+$/, ""), username, password });
    this.startLoop();
    this.changed();
    this.tabs.post({ kind: "connected" });
  }

  /**
   * Connects again with a new password after access was lost: checks it, keeps
   * it and resumes Sync where it stopped. Throws `ConnectFailed` and changes
   * nothing if the check fails.
   */
  async reconnect(password: string): Promise<void> {
    const credentials = this.current;
    if (!credentials) return;
    await this.check({ ...credentials, password });
    // A step under way, started with the old password, would block the schedule again once over.
    await this.step;
    await this.keep({ ...credentials, password });
    this.schedule?.reconnected(Date.now());
    this.arm();
    this.tabs.post({ kind: "connected" });
  }

  /** Stops Syncing, and forgets the connection, its credentials and the Sync state. Every Drawing stays. */
  async disconnect(): Promise<void> {
    this.current = undefined;
    await this.stopLoop();
    // Under the lock: no step of another tab writes Sync state after it is gone.
    await this.tabs.exclusive(() => this.storage.remove("Cloud"));
    this.incoming.known = false;
    this.setReport({});
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
    if (!online && this.schedule && !this.hasProblem && this.report.status !== undefined)
      this.setReport({ ...this.report, status: "offline", progress: undefined });
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
    this.setReport({});
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
        onProgress: (done, total) => {
          if (this.schedule === schedule && total > 0)
            this.setReport({ ...this.report, status: "syncing", progress: { done, total } });
        },
      });
    } catch {
      // Whatever broke is no reason to stop Syncing: the step counts as failed.
      result = {
        changed: [],
        error: "failed",
        accessLost: false,
        storageFull: false,
        status: "offline" as const,
      };
    } finally {
      protocol.free();
    }
    if (!result || this.schedule !== schedule) return;
    schedule.stepEnded(Date.now(), result.error === undefined);
    if (result.accessLost) schedule.accessLost();
    const wentThrough = result.status === "upToDate" || result.status === "full";
    this.setReport({
      status: result.status,
      progress: undefined,
      lastSync: wentThrough ? Date.now() : this.report.lastSync,
    });
    if (result.changed.length > 0) {
      for (const listener of this.syncedListeners) listener(result.changed);
      this.tabs.post({ kind: "changed", names: result.changed });
    }
  }

  private setReport(report: SyncReport): void {
    this.report = report;
    for (const listener of this.reportListeners) listener(report);
  }

  private changed(): void {
    for (const listener of this.listeners) listener(this.connection);
  }
}
