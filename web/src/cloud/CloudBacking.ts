import type { Core } from "../core/Core";
import type { FileStorage } from "../storage/FileStorage";
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
 * the next visit. `fetch` is a dependency, so tests replace the network.
 */
export class CloudBacking {
  private current: Credentials | undefined;
  private readonly listeners = new Set<(connection: Connection | undefined) => void>();

  private constructor(
    private readonly core: Pick<Core, "webDav">,
    private readonly storage: FileStorage,
    private readonly send: Fetch,
  ) {}

  /** The module, Connected if the storage holds a connection. */
  static async open(
    core: Pick<Core, "webDav">,
    storage: FileStorage,
    send: Fetch,
  ): Promise<CloudBacking> {
    const cloud = new CloudBacking(core, storage, send);
    const saved = await storage.read(credentialsFile);
    if (saved) cloud.current = JSON.parse(new TextDecoder().decode(saved));
    return cloud;
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
    this.current = { url: url.trim().replace(/\/+$/, ""), username, password };
    await this.storage.write(
      credentialsFile,
      new TextEncoder().encode(JSON.stringify(this.current)),
    );
    this.changed();
  }

  /** Forgets the connection and its credentials. Every Drawing stays. */
  async disconnect(): Promise<void> {
    this.current = undefined;
    await this.storage.remove("Cloud");
    this.changed();
  }

  private changed(): void {
    for (const listener of this.listeners) listener(this.connection);
  }
}
