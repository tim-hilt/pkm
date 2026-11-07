import { GeometryVerb } from "./GeometryBuffers";

/**
 * The path of an SVG `d` attribute as ops in the format of the core's
 * geometry (`GeometryVerb`), so the renderer can fill it like any other path.
 * A port of the Swift `SVGPathData`.
 */
export interface SvgPathData {
  verbs: number[];
  coords: number[];
}

/** The path with every point scaled by `scale` and then moved by `(dx, dy)`. */
export function transformedPath(
  path: SvgPathData,
  scale: number,
  dx: number,
  dy: number,
): SvgPathData {
  return {
    verbs: path.verbs,
    coords: path.coords.map((value, index) => value * scale + (index % 2 === 0 ? dx : dy)),
  };
}

const isLetter = (c: string) => /[A-Za-z]/.test(c);

/**
 * Parses the value of a `d` attribute. Understands what icon paths are made
 * of: move, line, horizontal and vertical line, cubic Bézier (also the smooth
 * form) and close, absolute or relative. Quadratic curves and arcs are not
 * supported: a path using them parses as far as it can.
 */
export function parseSvgPath(data: string): SvgPathData {
  const verbs: number[] = [];
  const coords: number[] = [];
  let position = 0;

  const skipSeparators = () => {
    while (position < data.length && (data[position] === "," || data[position] <= " ")) position++;
  };
  /** `-?digits[.digits]`, where a sign or a second dot starts the next number. */
  const number = (): number | undefined => {
    skipSeparators();
    const start = position;
    if (data[position] === "-" || data[position] === "+") position++;
    let seenDot = false;
    while (position < data.length) {
      const c = data[position];
      if (c >= "0" && c <= "9") position++;
      else if (c === "." && !seenDot) {
        seenDot = true;
        position++;
      } else break;
    }
    const value = Number.parseFloat(data.slice(start, position));
    return Number.isNaN(value) ? undefined : value;
  };
  const point = (ox: number, oy: number): [number, number] | undefined => {
    const x = number();
    const y = number();
    return x === undefined || y === undefined ? undefined : [ox + x, oy + y];
  };

  let [cx, cy] = [0, 0];
  let [startX, startY] = [0, 0];
  /** Second control point of the previous cubic, for the smooth form. */
  let previousControl: [number, number] | undefined;
  let command = "M";
  /** After a close the next subpath begins at the old one's start. */
  let needsMove = false;
  const moveTo = (x: number, y: number) => {
    verbs.push(GeometryVerb.move);
    coords.push(x, y);
  };
  const lineTo = (x: number, y: number) => {
    verbs.push(GeometryVerb.line);
    coords.push(x, y);
  };

  while (true) {
    skipSeparators();
    if (position < data.length && isLetter(data[position])) {
      command = data[position++];
      if (command === "Z" || command === "z") {
        [cx, cy] = [startX, startY];
        previousControl = undefined;
        needsMove = true;
        continue;
      }
    } else if (position >= data.length) {
      break;
    }
    // A command letter may be followed by several argument sets; a move's
    // further sets are lines.
    const relative = command === command.toLowerCase();
    const [ox, oy] = relative ? [cx, cy] : [0, 0];
    const upper = command.toUpperCase();
    if (needsMove && upper !== "M") moveTo(cx, cy);
    needsMove = false;

    if (upper === "M" || upper === "L") {
      const p = point(ox, oy);
      if (!p) return { verbs, coords };
      if (upper === "M") {
        moveTo(...p);
        [startX, startY] = p;
        command = relative ? "l" : "L";
      } else {
        lineTo(...p);
      }
      [cx, cy] = p;
      previousControl = undefined;
    } else if (upper === "H" || upper === "V") {
      const value = number();
      if (value === undefined) return { verbs, coords };
      if (upper === "H") cx = relative ? cx + value : value;
      else cy = relative ? cy + value : value;
      lineTo(cx, cy);
      previousControl = undefined;
    } else if (upper === "C" || upper === "S") {
      let first: [number, number] | undefined;
      if (upper === "S") {
        // The reflection of the previous control point.
        first = previousControl
          ? [2 * cx - previousControl[0], 2 * cy - previousControl[1]]
          : [cx, cy];
      } else {
        first = point(ox, oy);
      }
      const second = first && point(ox, oy);
      const end = second && point(ox, oy);
      if (!first || !second || !end) return { verbs, coords };
      verbs.push(GeometryVerb.cubic);
      coords.push(...first, ...second, ...end);
      [cx, cy] = end;
      previousControl = second;
    } else {
      return { verbs, coords };
    }
  }
  return { verbs, coords };
}
