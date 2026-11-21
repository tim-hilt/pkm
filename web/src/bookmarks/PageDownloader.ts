/** Why a download came to nothing, in words for the user. */
export class DownloadFailure extends Error {}

/** Where the web app's server downloads pages and images from (ADR-0033). */
const script = "/cgi-bin/fetch";

/** The most a page or image may take; the script caps it too, but the answer is checked all the same. */
const limit = 10_000_000;

/**
 * Downloads pages and images through the server's CGI script, as the browser can't fetch other
 * sites itself. The core decides what the bytes are worth; this only fetches them.
 */
export class PageDownloader {
  constructor(private readonly send: (url: string) => Promise<Response> = (url) => fetch(url)) {}

  /** The HTML of the page at `address`; throws a `DownloadFailure` for a refusal, an error or anything but a page. */
  async page(address: string): Promise<string> {
    const { bytes, type } = await this.get(address);
    const [mime = "", ...parameters] = type.split(";").map((part) => part.trim().toLowerCase());
    if (mime !== "text/html" && mime !== "application/xhtml+xml") {
      throw new DownloadFailure("The address is not a web page.");
    }
    const charset = parameters.find((p) => p.startsWith("charset="))?.slice(8);
    return decodeAs(bytes, charset);
  }

  /** The bytes at `address`, `undefined` where they can't be downloaded: a broken image is left out. */
  async image(address: string): Promise<Uint8Array | undefined> {
    try {
      return (await this.get(address)).bytes;
    } catch {
      return undefined;
    }
  }

  private async get(address: string): Promise<{ bytes: Uint8Array; type: string }> {
    let response: Response;
    try {
      response = await this.send(`${script}?url=${encodeURIComponent(address)}`);
    } catch {
      throw new DownloadFailure("The page could not be downloaded.");
    }
    if (!response.ok) {
      // A refusal of the script carries its reason as one line of text.
      const reason = (await response.text().catch(() => "")).trim();
      throw new DownloadFailure(
        reason === "" || reason.length > 200 || reason.startsWith("<")
          ? "The page could not be downloaded."
          : `${reason.charAt(0).toUpperCase()}${reason.slice(1)}.`,
      );
    }
    const bytes = new Uint8Array(await response.arrayBuffer());
    if (bytes.length > limit) throw new DownloadFailure("The page is too large.");
    return { bytes, type: response.headers.get("Content-Type") ?? "" };
  }
}

/** `bytes` as text in the encoding the server named; UTF-8 if it named none or one that is unknown. */
function decodeAs(bytes: Uint8Array, charset: string | undefined): string {
  try {
    return new TextDecoder(charset?.replace(/["']/g, "")).decode(bytes);
  } catch {
    return new TextDecoder().decode(bytes);
  }
}
