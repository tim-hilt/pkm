import type { FileStorage } from "../storage/FileStorage";

// One folder per Context folder (ADR-0020); never the `Cloud/state` that held
// the state of the whole `pkm` folder before, whose names would delete Drawings.
const text = new TextDecoder();
const encode = (value: string) => new TextEncoder().encode(value);

/**
 * What this Device remembers of each Drawing's last Sync (ADR-0011): the
 * bytes then, the cloud blob's change marker and the stamp the local file had
 * when it was found equal to those bytes. It lives in a private folder of the
 * site's storage that the Drawing store never lists, and goes with the
 * credentials on Disconnect. Names are file names.
 */
export class SyncState {
  /** The state for the Context folder `name` ("Drawings", "BooksState"). */
  constructor(
    private readonly storage: FileStorage,
    name = "Drawings",
  ) {
    this.folder = `Cloud/state/${name}`;
  }

  private readonly folder: string;

  async marks(file: string): Promise<{ marker?: string; stamp?: string }> {
    const [marker, stamp] = await Promise.all([
      this.storage.read(`${this.folder}/markers/${file}`),
      this.storage.read(`${this.folder}/stamps/${file}`),
    ]);
    return {
      marker: marker && text.decode(marker),
      stamp: stamp && text.decode(stamp),
    };
  }

  /** The remembered bytes with their marker; none unless both are there. */
  async synced(file: string): Promise<{ bytes: Uint8Array; marker: string } | undefined> {
    const [bytes, marker] = await Promise.all([
      this.storage.read(`${this.folder}/bytes/${file}`),
      this.storage.read(`${this.folder}/markers/${file}`),
    ]);
    return bytes && marker ? { bytes, marker: text.decode(marker) } : undefined;
  }

  /** The names with a remembered marker. */
  async names(): Promise<string[]> {
    return (await this.storage.files(`${this.folder}/markers`)).map(({ name }) => name);
  }

  /** The marker goes last: without it the bytes count for nothing. The stamp goes first, as it says nothing about the new bytes. */
  async remember(file: string, bytes: Uint8Array, marker: string): Promise<void> {
    await this.storage.remove(`${this.folder}/stamps/${file}`);
    await this.storage.write(`${this.folder}/bytes/${file}`, bytes);
    await this.storage.write(`${this.folder}/markers/${file}`, encode(marker));
  }

  async rememberStamp(file: string, stamp: string): Promise<void> {
    await this.storage.write(`${this.folder}/stamps/${file}`, encode(stamp));
  }

  async forget(file: string): Promise<void> {
    for (const kind of ["stamps", "markers", "bytes"]) {
      await this.storage.remove(`${this.folder}/${kind}/${file}`);
    }
  }
}
