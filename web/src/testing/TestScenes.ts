import { type Core, type Geometry } from "../core/Core";
import { type GeometrySource } from "../rendering/GeometryBuffers";
import { type Point, fillPart } from "../rendering/HandBuiltGeometry";
import { handBuiltGeometry } from "../rendering/HandBuiltGeometry";

/**
 * Scenes for renderer tests, made the way the app makes its own: by loading a
 * Drawing into the core's `Editor`. A port of the Swift `TestScenes` (images follow with their ticket).
 */
export class TestScenes {
  constructor(private readonly core: Core) {}

  /** The geometry of `drawing`, a `.excalidraw` document. */
  geometryOf(drawing: string): Geometry {
    return this.geometryOfBytes(new TextEncoder().encode(drawing));
  }

  geometryOfBytes(bytes: Uint8Array): Geometry {
    const editor = this.core.makeEditor();
    editor.load(bytes);
    return editor.geometry();
  }

  /**
   * A Drawing with one rectangle Element at (50, 100), 280 x 180, drawn with
   * exact edges (roughness 0), so renderer tests can probe them. `extra` adds
   * JSON members.
   */
  rectangle(extra = ""): Geometry {
    return this.geometryOf(
      drawing(
        `{"id":"box","type":"rectangle","x":50,"y":100,"width":280,"height":180,"roughness":0${extra}}`,
      ),
    );
  }

  /**
   * Shapes in the styles hand-drawn shapes come in: dashed, dotted,
   * translucent and thick, rounded, every sloppiness. Seeds are fixed, so the
   * sketches are too.
   */
  handDrawnStyles(): Geometry {
    const shape = (type: string, x: number, y: number, seed: number, extra: string) =>
      `{"id":"${type}${seed}","type":"${type}","x":${x},"y":${y},"width":150,"height":100,"seed":${seed}${extra}}`;
    return this.geometryOf(
      drawing(
        shape("rectangle", 20, 20, 11, `,"strokeStyle":"dashed"`),
        shape("ellipse", 200, 20, 12, `,"strokeStyle":"dotted","strokeWidth":4`),
        shape("diamond", 20, 160, 13, `,"roundness":{"type":2},"opacity":50,"strokeWidth":4`),
        shape("rectangle", 200, 160, 14, `,"roundness":{"type":3},"roughness":2`),
        shape("ellipse", 20, 300, 15, `,"roughness":0`),
        shape(
          "diamond",
          200,
          300,
          16,
          `,"roughness":2,"strokeStyle":"dashed","strokeColor":"#e03131"`,
        ),
      ),
    );
  }

  /** Every fill style on a rectangle. */
  fillStyles(): Geometry {
    return this.geometryOf(
      drawing(
        filled("rectangle", 20, 20, 21, "hachure"),
        filled("rectangle", 200, 20, 22, "cross-hatch"),
        filled("rectangle", 20, 160, 23, "zigzag"),
        filled("rectangle", 200, 160, 24, "solid"),
        filled("rectangle", 20, 300, 25, "hachure", `,"roughness":0`),
        filled("rectangle", 200, 300, 26, "cross-hatch", `,"strokeWidth":4`),
      ),
    );
  }

  /** Fills on the other closed shapes, rounded, rotated and translucent. */
  filledShapes(): Geometry {
    return this.geometryOf(
      drawing(
        filled("ellipse", 20, 20, 31, "hachure"),
        filled("ellipse", 200, 20, 32, "solid"),
        filled("diamond", 20, 160, 33, "zigzag"),
        filled("diamond", 200, 160, 34, "solid", `,"roundness":{"type":2}`),
        filled("rectangle", 20, 300, 35, "cross-hatch", `,"roundness":{"type":3},"angle":0.3`),
        filled("ellipse", 200, 300, 36, "solid", `,"opacity":50,"strokeColor":"#e03131"`),
      ),
    );
  }

  /**
   * Straight, multi-point and curved lines and arrows, closed and filled
   * loops, rotated, dashed, and a selection of arrowheads.
   */
  linesAndArrows(): Geometry {
    const straight = "[[0,0],[150,40]]";
    const multi = "[[0,0],[50,50],[100,0],[150,50]]";
    const loop = "[[0,0],[100,0],[100,70],[0,70],[0,0]]";
    const round = `,"roundness":{"type":2}`;
    const head = (name: string, y: number, seed: number) =>
      linear("arrow", 230, y, seed, "[[0,0],[130,0]]", `,"endArrowhead":"${name}"`);
    return this.geometryOf(
      drawing(
        linear("line", 20, 20, 41, straight),
        linear("line", 20, 100, 42, multi),
        linear("line", 20, 180, 43, multi, round),
        linear("line", 20, 260, 44, loop, `,"backgroundColor":"#a5d8ff","fillStyle":"hachure"`),
        linear("arrow", 20, 360, 45, straight, `,"startArrowhead":"triangle"`),
        linear("arrow", 20, 430, 46, multi, round + `,"strokeStyle":"dashed"`),
        linear("arrow", 230, 380, 47, straight, `,"angle":0.5,"strokeWidth":4`),
        head("arrow", 20, 51),
        head("bar", 60, 52),
        head("circle", 100, 53),
        head("triangle_outline", 140, 54),
        head("diamond", 180, 55),
        head("cardinality_one_or_many", 220, 56),
        head("cardinality_zero_or_one", 260, 57),
        head("cardinality_exactly_one", 300, 58),
      ),
    );
  }

  /**
   * Strokes with simulated and with real pressure, thick, translucent,
   * rotated, and closed loops with hachure and solid fills.
   */
  freedraw(): Geometry {
    const wave: Point[] = range(24).map((i) => [i * 6, 30 + 25 * Math.sin(i / 3.5)]);
    const pressures = range(24).map((i) => 0.25 + 0.6 * Math.sin((i / 23) * Math.PI));
    const loop: Point[] = range(25).map((i) => [
      50 + 45 * Math.cos((i / 24) * 2 * Math.PI),
      50 + 45 * Math.sin((i / 24) * 2 * Math.PI),
    ]);
    const fill = `,"backgroundColor":"#ffc9c9"`;
    return this.geometryOf(
      drawing(
        freedraw(20, 20, 61, wave),
        freedraw(20, 100, 62, wave, pressures),
        freedraw(20, 180, 63, wave, undefined, `,"strokeWidth":4`),
        freedraw(20, 260, 64, wave, pressures, `,"strokeColor":"#e03131","opacity":50`),
        freedraw(200, 20, 65, wave, undefined, `,"angle":0.5`),
        freedraw(200, 120, 66, loop, undefined, fill + `,"fillStyle":"hachure"`),
        freedraw(
          200,
          240,
          67,
          loop,
          loop.map(() => 0.5),
          fill + `,"fillStyle":"solid"`,
        ),
      ),
    );
  }

  /**
   * Text in every alignment, size and font family, rotated, translucent, and
   * labels wrapped to a rectangle, an ellipse and a diamond.
   */
  text(): Geometry {
    const label = "A label that wraps to its container";
    return this.geometryOf(
      drawing(
        textElement("left", 20, 20, [220, 25], "Left aligned", `,"textAlign":"left"`),
        textElement("centre", 20, 60, [220, 25], "Centred", `,"textAlign":"center"`),
        textElement("right", 20, 100, [220, 25], "Right aligned", `,"textAlign":"right"`),
        textElement("lines", 20, 140, [120, 60], "Two\\nlines", `,"fontSize":24,"lineHeight":1.25`),
        textElement(
          "family",
          20,
          220,
          [200, 23],
          "Another family",
          `,"fontFamily":2,"lineHeight":1.15`,
        ),
        textElement(
          "red",
          20,
          260,
          [220, 25],
          "Red, translucent",
          `,"strokeColor":"#e03131","opacity":50`,
        ),
        textElement("turned", 40, 340, [160, 25], "Turned about its middle", `,"angle":-0.3`),
        ...labelled("rectangle", 260, 20, [220, 110], label),
        ...labelled("ellipse", 260, 160, [220, 140], label),
        ...labelled("diamond", 260, 330, [220, 160], "A diamond label"),
      ),
    );
  }

  /** A concave outline and a pentagram, each by both fill rules. */
  fillRules(): GeometrySource {
    const ell: Point[] = [
      [20, 20],
      [80, 20],
      [80, 120],
      [180, 120],
      [180, 180],
      [20, 180],
    ];
    const star = pentagram(100, 80);
    const moved = (points: Point[], dx: number, dy: number): Point[] =>
      points.map(([x, y]) => [x + dx, y + dy]);
    const red = 0xe03131ff;
    return handBuiltGeometry(
      fillPart(ell),
      fillPart(moved(ell, 200, 0), { fillRule: 1 }),
      fillPart(moved(star, 0, 200), { color: red, fillRule: 0 }),
      fillPart(moved(star, 200, 200), { color: red, fillRule: 1 }),
    );
  }
}

/** A pentagram drawn as one self-intersecting path around (`centre`, `centre`): the middle pentagon is wound twice. */
export function pentagram(centre: number, radius: number): Point[] {
  return range(5).map((i) => {
    const angle = Math.fround((i * 4 * Math.PI) / 5 - Math.PI / 2);
    return [centre + radius * Math.cos(angle), centre + radius * Math.sin(angle)];
  });
}

const range = (count: number) => Array.from({ length: count }, (_, i) => i);

/** A `.excalidraw` document of `elements`, JSON objects. */
export function drawing(...elements: string[]): string {
  return `{"type":"excalidraw","version":2,"elements":[${elements.join(",")}]}`;
}

/** One 150 x 100 shape of `type` at (x, y) with a pastel background. */
function filled(
  type: string,
  x: number,
  y: number,
  seed: number,
  fill: string,
  extra = "",
): string {
  return `{"id":"${type}${seed}","type":"${type}","x":${x},"y":${y},"width":150,"height":100,"seed":${seed},"backgroundColor":"#a5d8ff","fillStyle":"${fill}"${extra}}`;
}

/** A linear Element at (x, y) through `points`, which start at the origin. */
function linear(
  type: string,
  x: number,
  y: number,
  seed: number,
  points: string,
  extra = "",
): string {
  return `{"id":"${type}${seed}","type":"${type}","x":${x},"y":${y},"width":150,"height":60,"seed":${seed},"points":${points}${extra}}`;
}

/** A freedraw stroke at (x, y) through `points`, which start at the origin. */
function freedraw(
  x: number,
  y: number,
  seed: number,
  points: Point[],
  pressures?: number[],
  extra = "",
): string {
  const number = (value: number) => value.toFixed(2);
  const path = points.map(([px, py]) => `[${number(px)},${number(py)}]`).join(",");
  const pressure = pressures
    ? `,"simulatePressure":false,"pressures":[${pressures.map(number).join(",")}]`
    : `,"simulatePressure":true,"pressures":[]`;
  return `{"id":"freedraw${seed}","type":"freedraw","x":${x},"y":${y},"width":150,"height":60,"seed":${seed},"points":[${path}]${pressure}${extra}}`;
}

/** A text Element at (x, y) with a box of `size`. */
export function textElement(
  id: string,
  x: number,
  y: number,
  size: [number, number],
  text: string,
  extra = "",
): string {
  return `{"id":"${id}","type":"text","x":${x},"y":${y},"width":${size[0]},"height":${size[1]},"text":"${text}","originalText":"${text}","fontSize":20,"fontFamily":5,"lineHeight":1.25${extra}}`;
}

/** A shape with the label `text` bound to it. */
function labelled(
  type: string,
  x: number,
  y: number,
  size: [number, number],
  text: string,
): string[] {
  return [
    `{"id":"${type}","type":"${type}","x":${x},"y":${y},"width":${size[0]},"height":${size[1]},"roughness":0,"boundElements":[{"type":"text","id":"${type}-label"}]}`,
    textElement(
      `${type}-label`,
      x,
      y,
      [10, 25],
      text,
      `,"containerId":"${type}","textAlign":"center","verticalAlign":"middle"`,
    ),
  ];
}
