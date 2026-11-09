import type { Core } from "../core/Core";
import type { DrawingStore } from "../drawings/gallery/DrawingStore";
import type { SyncRequest, SyncRun, WebDavProtocol } from "../../generated/pkm_web.js";
import type { SyncState } from "./SyncState";
import { type Fetch, runExchange } from "./WebDavSender";

/** What Cloud Backing is doing, for the user. */
export type SyncStatus = "upToDate" | "syncing" | "offline" | "accessLost" | "full";

/** How a Sync step ended. */
export interface StepResult {
  /** The names of the Drawings whose local file the step wrote or deleted; a rename as the old and the new. */
  changed: string[];
  /** The first thing that went wrong, if anything did. */
  error?: string;
  accessLost: boolean;
  storageFull: boolean;
  /** What the user is told: `upToDate`, `offline`, `accessLost` or `full`. */
  status: SyncStatus;
}

/** What a step needs to answer its requests. */
export interface StepContext {
  core: Core;
  protocol: WebDavProtocol;
  send: Fetch;
  drawings: DrawingStore;
  state: SyncState;
  /** Set once a put has found the folder for uploads there; shared by the steps of one connection. */
  incoming: { known: boolean };
  /** Ask before every request: a step whose connection was dropped stops. */
  cancelled: () => boolean;
  /** Told how far the transfers are, once their number is known. */
  onProgress?: (done: number, total: number) => void;
}

/**
 * Runs one Sync step: answers the step machine's requests with the Drawing
 * store, the Sync state and the WebDAV protocol over `fetch`, until it is
 * done. The stamp of a local Drawing is its modification time and size.
 * Returns `undefined` if the step was cancelled.
 */
export async function runSyncStep(context: StepContext): Promise<StepResult | undefined> {
  const run = context.core.syncRun(Date.now());
  try {
    let request = run.start();
    while (request) {
      if (context.cancelled()) return undefined;
      const current = request;
      try {
        request = await answer(run, current, context).catch((error) =>
          run.fail("failed", String(error)),
        );
      } finally {
        current.free();
      }
      reportProgress(run, context);
    }
    const outcome = run.outcome();
    const result: StepResult = {
      changed: outcome.changed,
      error: outcome.error,
      accessLost: outcome.accessLost,
      storageFull: outcome.storageFull,
      status: outcome.status as SyncStatus,
    };
    outcome.free();
    return result;
  } finally {
    run.free();
  }
}

function reportProgress(run: SyncRun, { onProgress }: StepContext): void {
  if (!onProgress) return;
  const progress = run.progress();
  if (!progress) return;
  onProgress(progress.done, progress.total);
  progress.free();
}

/** Answers `request`; returns the step's next one. */
async function answer(
  run: SyncRun,
  request: SyncRequest,
  { protocol, send, drawings, state, incoming }: StepContext,
): Promise<SyncRequest | undefined> {
  const { kind, file } = request;
  switch (kind) {
    case "listCloud": {
      const result = await runExchange(protocol.list(), send);
      if (result.error) return run.fail(result.error, result.message);
      return run.answerCloud(
        result.blobs.map((b) => b.name),
        result.blobs.map((b) => b.marker),
      );
    }
    case "listLocal": {
      const files = await drawings.files();
      return run.answerLocal(
        files.map((f) => f.file),
        files.map((f) => `${f.modified} ${f.size}`),
        Float64Array.from(files, (f) => f.modified),
      );
    }
    case "readLocal":
      return run.answerBytes(await drawings.read(file));
    case "readMarks": {
      const { marker, stamp } = await state.marks(file);
      return run.answerMarks(marker, stamp);
    }
    case "readSynced": {
      const synced = await state.synced(file);
      return run.answerSynced(synced?.bytes, synced?.marker);
    }
    case "listSynced":
      return run.answerNames(await state.names());
    case "writeSynced":
      await state.remember(file, request.bytes, request.marker);
      return run.answerDone();
    case "writeStamp":
      await state.rememberStamp(file, request.stamp);
      return run.answerDone();
    case "forgetSynced":
      await state.forget(file);
      return run.answerDone();
    case "download": {
      const result = await runExchange(protocol.get(file), send);
      if (result.error) return run.fail(result.error, result.message);
      return run.answerBytes(result.data);
    }
    case "upload": {
      const result = await runExchange(
        protocol.put(file, crypto.randomUUID(), request.bytes, incoming.known),
        send,
      );
      incoming.known = !result.error;
      if (result.error) return run.fail(result.error, result.message);
      return run.answerDone();
    }
    case "deleteCloud": {
      const result = await runExchange(protocol.delete(file), send);
      if (result.error) return run.fail(result.error, result.message);
      return run.answerDone();
    }
    case "writeLocal":
      return run.answerHappened(
        await drawings.writeIfHolding(file, request.bytes, request.expected ?? undefined),
      );
    case "renameLocal":
      return run.answerHappened(
        await drawings.renameIfHolding(file, request.to, request.expected!),
      );
    case "deleteLocal":
      return run.answerHappened(await drawings.deleteIfHolding(file, request.expected!));
    default:
      return run.fail("failed", `unknown request ${kind}`);
  }
}
