import { mkdir, readFile, rm, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

// Vitest browser commands: the browser tests cannot touch the file system, so
// the reference store reads and writes its files through these, in Node.

const webFolder = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

/** The file at `path` inside the web app's folder; refuses to leave it. */
function inWebFolder(path: string): string {
  const full = resolve(webFolder, path);
  if (!full.startsWith(webFolder + "/")) throw new Error(`${path} is outside the web app`);
  return full;
}

export const referenceCommands = {
  /** The file as base64, or null if there is none. */
  async readReferenceFile(_context: unknown, path: string): Promise<string | null> {
    try {
      return (await readFile(inWebFolder(path))).toString("base64");
    } catch (error) {
      if ((error as NodeJS.ErrnoException).code === "ENOENT") return null;
      throw error;
    }
  },
  async writeReferenceFile(_context: unknown, path: string, base64: string): Promise<void> {
    const file = inWebFolder(path);
    await mkdir(dirname(file), { recursive: true });
    await writeFile(file, Buffer.from(base64, "base64"));
  },
  async removeReferenceFile(_context: unknown, path: string): Promise<void> {
    await rm(inWebFolder(path), { force: true });
  },
  /** Whether `RECORD_REFERENCES` asks for references to be (re)recorded. */
  async isRecordingReferences(): Promise<boolean> {
    return process.env.RECORD_REFERENCES !== undefined;
  },
};
