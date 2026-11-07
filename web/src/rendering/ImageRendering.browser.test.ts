import { beforeAll, describe, expect, it } from "vitest";
import type { Editor, Geometry } from "../core/Core";
import { expectMatchesReference } from "../testing/BrowserReferences";
import { type RenderHarness, makeRenderHarness } from "../testing/OffscreenRender";
import { image } from "../testing/TestScenes";
import { quadrants, rgb, png, blue } from "../testing/TestImages";
import type { Viewport } from "./CanvasRenderer";

/**
 * Renders images offscreen and probes pixels, a port of the Swift
 * `ImageRenderingTests`. The test picture has red at the top left, green at
 * the top right, blue at the bottom left and yellow at the bottom right; paper
 * is white. Also the reference images of images (`images`, `images-zoomed-*`).
 */

const red = [255, 0, 0];
const green = [0, 255, 0];
const yellow = [255, 255, 0];
const paper = [255, 255, 255];
const placeholderGrey = [0xe7, 0xe7, 0xe7];
const blueRgb = [0, 0, 255];

let harness: RenderHarness;
let files: Record<string, Uint8Array>;
beforeAll(async () => {
  harness = await makeRenderHarness();
  files = {
    q: await quadrants(),
    r: await quadrants(),
    junk: new TextEncoder().encode("not an image"),
  };
});

type Scene = { editor: Editor; geometry: Geometry };

/** A Drawing with the test picture as file "q" and the given Elements. */
const scene = (elements: string[], withFiles: Record<string, Uint8Array> = { q: files.q }): Scene =>
  harness.scenes.withImages(withFiles, elements);

const render = (
  { editor, geometry }: Scene,
  size = 200,
  viewport: Viewport = { originX: 0, originY: 0, zoom: 1 },
) => harness.render(geometry, viewport, { size, files: editor });

/** A 100 x 100 image at (50, 50), the quadrants 50 points wide. */
const box = (options: { scale?: string; crop?: string; extra?: string } = {}) =>
  scene([image("i", 50, 50, [100, 100], options)]);

describe("images", () => {
  it("fill their box with the picture", async () => {
    const frame = await render(box());

    expect(rgb(frame, 75, 75)).toEqual(red);
    expect(rgb(frame, 125, 75)).toEqual(green);
    expect(rgb(frame, 75, 125)).toEqual(blueRgb);
    expect(rgb(frame, 125, 125)).toEqual(yellow);
    expect(rgb(frame, 40, 100), "left of the box").toEqual(paper);
    expect(rgb(frame, 100, 160), "below the box").toEqual(paper);
  });

  it("are scaled and placed with the viewport", async () => {
    const frame = await render(box(), 400, { originX: 25, originY: 25, zoom: 2 });

    expect(rgb(frame, 60, 60), "the box starts at (50, 50) Scene, (50, 50) device").toEqual(red);
    expect(rgb(frame, 200, 200)).toEqual(yellow);
    expect(rgb(frame, 260, 260), "past the box's end at (250, 250)").toEqual(paper);
    expect(rgb(frame, 40, 40)).toEqual(paper);
  });

  it("show their picture mirrored when mirrored", async () => {
    const sideways = await render(box({ scale: "[-1,1]" }));
    expect(rgb(sideways, 75, 75)).toEqual(green);
    expect(rgb(sideways, 125, 75)).toEqual(red);
    expect(rgb(sideways, 75, 125)).toEqual(yellow);

    const upsideDown = await render(box({ scale: "[1,-1]" }));
    expect(rgb(upsideDown, 75, 75)).toEqual(blueRgb);
    expect(rgb(upsideDown, 75, 125)).toEqual(red);
  });

  it("turn about their centre", async () => {
    // A quarter turn clockwise takes the top left to the top right.
    const frame = await render(box({ extra: `,"angle":${Math.PI / 2}` }));

    expect(rgb(frame, 125, 75)).toEqual(red);
    expect(rgb(frame, 125, 125)).toEqual(green);
    expect(rgb(frame, 75, 125)).toEqual(yellow);
    expect(rgb(frame, 75, 75)).toEqual(blueRgb);
  });

  it("are mirrored before they are turned", async () => {
    // Excalidraw applies the scale after the rotation to the context, so the
    // picture is mirrored in the box and then the box turned.
    const frame = await render(box({ scale: "[-1,1]", extra: `,"angle":${Math.PI / 2}` }));

    expect(rgb(frame, 125, 75)).toEqual(green);
    expect(rgb(frame, 125, 125)).toEqual(red);
  });

  it("show only the cropped part of the picture", async () => {
    const corner = await render(
      box({ crop: `{"x":20,"y":0,"width":20,"height":20,"naturalWidth":40,"naturalHeight":40}` }),
    );
    expect(rgb(corner, 60, 60)).toEqual(green);
    expect(rgb(corner, 140, 140)).toEqual(green);

    // The middle of the file shows a quarter of every colour.
    const middle = await render(
      box({ crop: `{"x":10,"y":10,"width":20,"height":20,"naturalWidth":40,"naturalHeight":40}` }),
    );
    expect(rgb(middle, 60, 60)).toEqual(red);
    expect(rgb(middle, 140, 60)).toEqual(green);
    expect(rgb(middle, 60, 140)).toEqual(blueRgb);
    expect(rgb(middle, 140, 140)).toEqual(yellow);
  });

  it("blend with what is behind them at their opacity", async () => {
    const [r, g, b] = rgb(await render(box({ extra: `,"opacity":50` })), 75, 75);

    expect([r, Math.abs(g - 128) <= 1, Math.abs(b - 128) <= 1]).toEqual([255, true, true]);
  });

  it("are clipped at their corners when rounded", async () => {
    const rounded = await render(box({ extra: `,"roundness":{"type":3}` }));
    // The radius is a quarter of the side: 25 points.
    expect(rgb(rounded, 52, 52), "cut off at the corner").toEqual(paper);
    expect(rgb(rounded, 75, 52), "straight edges stay").toEqual(red);
    expect(rgb(rounded, 60, 60)).toEqual(red);

    expect(rgb(await render(box()), 52, 52), "sharp without roundness").toEqual(red);
  });

  it("are drawn in z-order with the shapes around them", async () => {
    const cover = `{"id":"c","type":"rectangle","x":60,"y":60,"width":40,"height":40,"roughness":0,"backgroundColor":"#a5d8ff","fillStyle":"solid","strokeColor":"transparent"}`;
    const picture = image("i", 50, 50, [100, 100]);

    const under = await render(scene([picture, cover]));
    expect(rgb(under, 80, 80), "a later shape covers the image").toEqual([0xa5, 0xd8, 0xff]);
    expect(rgb(under, 55, 55)).toEqual(red);

    const over = await render(scene([cover, picture]));
    expect(rgb(over, 80, 80), "a later image covers the shape").toEqual(red);
  });
});

describe("placeholders", () => {
  const missing = (extra = "", size: [number, number] = [100, 100], at = [50, 50]) =>
    scene([image("i", at[0], at[1], size, { file: "gone", extra })]);

  it("fill the box of an image whose file is missing, with the icon in the middle", async () => {
    const frame = await render(missing());

    expect(rgb(frame, 52, 52)).toEqual(placeholderGrey);
    expect(rgb(frame, 148, 52)).toEqual(placeholderGrey);
    expect(rgb(frame, 40, 40)).toEqual(paper);
    let icon = 0;
    for (let x = 50; x < 150; x++) {
      for (let y = 50; y < 150; y++) {
        if (rgb(frame, x, y).every((c) => c === 0x88)) icon++;
      }
    }
    expect(icon, "the picture icon in the middle").toBeGreaterThan(500);
  });

  it("use Excalidraw's dark colours on the dark Canvas", async () => {
    const { editor, geometry } = missing();
    const renderer = harness.makeRenderer();
    renderer.files = editor;
    renderer.isDark = true;
    renderer.setGeometry(geometry);

    expect(rgb(await harness.draw(renderer), 52, 52)).toEqual([0x2e, 0x2e, 0x2e]);
    renderer.isDark = false;
    expect(rgb(await harness.draw(renderer), 52, 52), "and back").toEqual(placeholderGrey);
  });

  it("are also what an image without a file and one that is no picture get", async () => {
    const noFile = await render(
      scene([`{"id":"i","type":"image","x":50,"y":50,"width":100,"height":100,"fileId":null}`]),
    );
    expect(rgb(noFile, 52, 52)).toEqual(placeholderGrey);

    const broken = await render(
      scene([image("i", 50, 50, [100, 100], { file: "junk" })], { junk: files.junk }),
    );
    expect(rgb(broken, 52, 52)).toEqual(placeholderGrey);
  });

  it("have the crossed-out icon instead of the picture icon when failed", async () => {
    const plain = await render(missing());
    const failed = await render(missing(`,"status":"error"`));

    expect(plain.rgba).not.toEqual(failed.rgba);
    expect(rgb(failed, 52, 52)).toEqual(placeholderGrey);
  });

  it("are turned and mirrored with their box", async () => {
    // A quarter turn about the centre of a wide box makes it tall.
    const frame = await render(missing(`,"angle":${Math.PI / 2}`, [120, 60], [40, 70]));

    expect(rgb(frame, 100, 45), "top of the turned box").toEqual(placeholderGrey);
    expect(rgb(frame, 100, 155), "bottom of the turned box").toEqual(placeholderGrey);
    expect(rgb(frame, 55, 100), "where the unturned box was").toEqual(paper);
  });
});

describe("textures", () => {
  it("are one per file however many images show it", async () => {
    const { editor, geometry } = scene([
      image("a", 10, 10, [50, 50]),
      image("b", 100, 10, [50, 50]),
      image("c", 10, 100, [50, 50], { scale: "[-1,1]" }),
    ]);
    const renderer = harness.makeRenderer();
    renderer.files = editor;
    renderer.setGeometry(geometry);

    await harness.draw(renderer);

    expect(renderer.textures.count).toBe(1);
    expect(renderer.textures.decodeCount).toBe(1);
  });

  it("are released when no image shows their file any more", async () => {
    const both = scene(
      [image("a", 10, 10, [50, 50]), image("b", 100, 10, [50, 50], { file: "r" })],
      {
        q: files.q,
        r: files.r,
      },
    );
    const renderer = harness.makeRenderer();
    renderer.files = both.editor;
    renderer.setGeometry(both.geometry);
    await harness.draw(renderer);
    expect(renderer.textures.count).toBe(2);

    const one = scene([image("a", 10, 10, [50, 50])]);
    renderer.files = one.editor;
    renderer.setGeometry(one.geometry);
    await harness.draw(renderer);
    expect(renderer.textures.count, "r is gone").toBe(1);

    const none = scene([`{"id":"r","type":"rectangle","width":5,"height":5}`]);
    renderer.setGeometry(none.geometry);
    await harness.draw(renderer);
    expect(renderer.textures.count).toBe(0);
  });

  it("are released with the renderer", async () => {
    const { editor, geometry } = scene([image("a", 10, 10, [50, 50])]);
    const renderer = harness.makeRenderer();
    renderer.files = editor;
    renderer.setGeometry(geometry);
    await harness.draw(renderer);
    expect(renderer.textures.count).toBe(1);

    renderer.dispose();

    expect(renderer.textures.count).toBe(0);
  });

  it("are kept for a moved image: its file is not decoded again", async () => {
    const first = scene([image("a", 10, 10, [50, 50], { extra: `,"version":1` })]);
    const renderer = harness.makeRenderer();
    renderer.files = first.editor;
    renderer.setGeometry(first.geometry);
    await harness.draw(renderer);

    const moved = scene([image("a", 90, 10, [50, 50], { extra: `,"version":2` })]);
    renderer.files = first.editor;
    renderer.setGeometry(moved.geometry);
    const frame = await harness.draw(renderer);

    expect(rgb(frame, 100, 20)).toEqual(red);
    expect(renderer.textures.decodeCount).toBe(1);
  });

  it("are of the file of the Drawing the image is in: ids are only unique within one", async () => {
    const quadrantsScene = scene([image("a", 50, 50, [100, 100])]);
    const renderer = harness.makeRenderer();
    renderer.files = quadrantsScene.editor;
    renderer.setGeometry(quadrantsScene.geometry);
    await harness.draw(renderer);

    const allBlue = await png(4, 4, () => blue);
    const other = scene([image("a", 50, 50, [100, 100])], { q: allBlue });
    renderer.files = other.editor;
    renderer.setGeometry(other.geometry);

    expect(rgb(await harness.draw(renderer), 75, 75)).toEqual(blueRgb);
  });

  it("show a file that was missing once the files come from where it is", async () => {
    const picture = image("a", 50, 50, [100, 100]);
    const without = scene([picture], {});
    const renderer = harness.makeRenderer();
    renderer.files = without.editor;
    renderer.setGeometry(without.geometry);
    expect(rgb(await harness.draw(renderer), 60, 60)).toEqual(placeholderGrey);

    renderer.files = scene([picture]).editor;

    expect(rgb(await harness.draw(renderer), 75, 75)).toEqual(red);
  });
});

describe("a live Canvas", () => {
  it("asks for another frame when the picture of an image has arrived", async () => {
    const { editor, geometry } = box();
    const renderer = harness.makeRenderer();
    renderer.files = editor;
    renderer.setGeometry(geometry);
    let requests = 0;
    renderer.onNeedsDisplay = () => requests++;

    // The first frame has what there is: no picture yet.
    renderer.textures.texture("q", editor);
    await renderer.textures.settled();

    expect(requests).toBeGreaterThan(0);
  });
});

describe("reference images", () => {
  it("shows every look an image can have, and the placeholders", async () => {
    const { editor, geometry } = harness.scenes.images(files);
    await expectMatchesReference(await render({ editor, geometry }, 440), "images");
  });

  it("stays smooth zoomed out and sharp zoomed in", async () => {
    const scenes = harness.scenes.images(files);
    await expectMatchesReference(
      await render(scenes, 220, { originX: 0, originY: 0, zoom: 0.5 }),
      "images-zoomed-out",
    );
    await expectMatchesReference(
      await render(scenes, 440, { originX: 20, originY: 20, zoom: 3.5 }),
      "images-zoomed-in",
    );
  });
});
