import { type RenderedImage } from "../rendering/RenderedImage";
import { type ImageTolerance, mismatchBetween, standardTolerance } from "./ImageComparison";

/** Reads and writes files by path relative to the web app's folder. */
export interface ReferenceFiles {
  read(path: string): Promise<Uint8Array | undefined>;
  write(path: string, bytes: Uint8Array): Promise<void>;
  remove(path: string): Promise<void>;
}

export interface PngCodec {
  encode(image: RenderedImage): Promise<Uint8Array>;
  /** Undefined if `bytes` is not a decodable image. */
  decode(bytes: Uint8Array): Promise<RenderedImage | undefined>;
}

export type ReferenceOutcome =
  | { status: "matched" }
  /** Recording mode wrote a new or changed reference; review it before committing. */
  | { status: "recorded" }
  /** The message says what is wrong and what to do about it. */
  | { status: "failed"; message: string };

/** Where references (committed) and the images of failed checks (not committed) live. */
export const referencesDirectory = "references";
export const failuresDirectory = "reference-failures";

/**
 * Reference images (PNG files) that renders are checked against, a port of
 * the Swift `ReferenceStore`. Normal runs only compare. Recording is the one
 * explicit way to change references; see docs/web-renderer-reference-images.md.
 */
export class ReferenceStore {
  constructor(
    private readonly files: ReferenceFiles,
    private readonly codec: PngCodec,
    private readonly recording: boolean,
  ) {}

  /**
   * Compares `image` with the reference called `name`. When recording, a
   * missing or no longer matching reference is replaced by `image` instead of failing.
   */
  async check(
    image: RenderedImage,
    name: string,
    tolerance: ImageTolerance = standardTolerance,
  ): Promise<ReferenceOutcome> {
    const referencePath = `${referencesDirectory}/${name}.png`;
    const failure = (kind: string) => `${failuresDirectory}/${name}-${kind}.png`;

    const stored = await this.files.read(referencePath);
    const reference = stored && (await this.codec.decode(stored));
    const mismatch = reference ? mismatchBetween(image, reference, tolerance) : undefined;

    if (reference && !mismatch) {
      for (const kind of ["actual", "expected", "diff"]) await this.files.remove(failure(kind));
      return { status: "matched" };
    }
    if (this.recording) {
      await this.files.write(referencePath, await this.codec.encode(image));
      return { status: "recorded" };
    }
    if (!reference || !mismatch) {
      return {
        status: "failed",
        message: `no reference image ${referencePath}; record it with \`make web-record-references\``,
      };
    }

    await this.files.write(failure("actual"), await this.codec.encode(image));
    await this.files.write(failure("expected"), stored!);
    if (mismatch.diff)
      await this.files.write(failure("diff"), await this.codec.encode(mismatch.diff));
    return {
      status: "failed",
      message:
        `${name}: ${mismatch.summary}. Compare ${failure("actual")} with ${referencePath}, ` +
        `differences in ${failure("diff")}. If the change is intended, run \`make web-record-references\`.`,
    };
  }
}
