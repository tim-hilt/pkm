import { type GeometryImage, GeometryVerb } from "./GeometryBuffers";
import { type SvgPathData, parseSvgPath, transformedPath } from "./SvgPathData";

/**
 * What is drawn in an image's box while there are no pixels for its file:
 * Excalidraw's placeholder (`drawImagePlaceholder`), a light grey box with a
 * picture icon, or a crossed-out one if loading the file failed. Both are
 * described as filled paths, so they stay sharp at any zoom. A port of the
 * Swift `ImagePlaceholder`.
 */

/** A filled path in the box's own frame: origin at its top-left corner, unrotated and unmirrored. */
export interface PlaceholderFill {
  /** `0xRRGGBBAA`. */
  color: number;
  path: SvgPathData;
}

/** Excalidraw's light and dark placeholder backgrounds. The icon is a picture of its own there, the same in both. */
const backgroundColor = (dark: boolean) => (dark ? 0x2e2e2eff : 0xe7e7e7ff);
const iconColor = 0x888888ff;
/** The icon is at most this large, in Scene units. */
const largestIcon = 100;

/** A path of the artwork with the transform the SVG gives it. */
interface Piece {
  path: SvgPathData;
  scale: number;
  translation: [number, number];
}

// The two icons are Excalidraw's own (Font Awesome "image", and the same with
// a "ban" sign); the numbers are copied from its SVG sources.

const picture =
  "M464 448H48c-26.51 0-48-21.49-48-48V112c0-26.51 21.49-48 48-48h416c26.51 0 48 21.49 48 48v288" +
  "c0 26.51-21.49 48-48 48zM112 120c-30.928 0-56 25.072-56 56s25.072 56 56 56 56-25.072 56-56" +
  "-25.072-56-56-56zM64 384h384V272l-87.515-87.515c-4.686-4.686-12.284-4.686-16.971 0L208 320" +
  "l-55.515-55.515c-4.686-4.686-12.284-4.686-16.971 0L64 336v48z";

const icon: Piece[] = [{ path: parseSvgPath(picture), scale: 1, translation: [0, 0] }];

const errorIcon: Piece[] = [
  {
    path: parseSvgPath(picture.replaceAll("z", "Z")),
    scale: 0.81709,
    translation: [124.825, 145.825],
  },
  {
    path: parseSvgPath(
      "M256 8C119.034 8 8 119.033 8 256c0 136.967 111.034 248 248 248s248-111.034 248-248" +
        "S392.967 8 256 8Zm130.108 117.892c65.448 65.448 70 165.481 20.677 235.637L150.47 105.216" +
        "c70.204-49.356 170.226-44.735 235.638 20.676ZM125.892 386.108c-65.448-65.448-70-165.481" +
        "-20.677-235.637L361.53 406.784c-70.203 49.356-170.226 44.736-235.638-20.676Z",
    ),
    scale: 0.30366,
    translation: [506.822, 60.065],
  },
];

/** The fills for a `width` x `height` box, background first. */
export function placeholderFills(
  width: number,
  height: number,
  failed: boolean,
  dark = false,
): PlaceholderFill[] {
  const box: PlaceholderFill = {
    color: backgroundColor(dark),
    path: {
      verbs: [GeometryVerb.move, GeometryVerb.line, GeometryVerb.line, GeometryVerb.line],
      coords: [0, 0, width, 0, width, height, 0, height],
    },
  };
  const size = Math.min(Math.min(width, height) * 0.4, largestIcon);
  if (!(size > 0)) return [box];
  const [artwork, artworkSize] = failed ? [errorIcon, 668] : [icon, 512];
  const scale = size / artworkSize;
  const ox = (width - size) / 2;
  const oy = (height - size) / 2;
  return [
    box,
    ...artwork.map((piece) => ({
      color: iconColor,
      path: transformedPath(
        piece.path,
        scale * piece.scale,
        ox + scale * piece.translation[0],
        oy + scale * piece.translation[1],
      ),
    })),
  ];
}

/**
 * Where the point `(localX, localY)` of the image's box (origin at its
 * top-left corner) lies in the Scene: mirrored and turned about the box's
 * centre as the record says.
 */
export function scenePoint(image: GeometryImage, localX: number, localY: number): [number, number] {
  const cx = image.width / 2;
  const cy = image.height / 2;
  const ox = (localX - cx) * image.scaleX;
  const oy = (localY - cy) * image.scaleY;
  const sin = Math.sin(image.angle);
  const cos = Math.cos(image.angle);
  return [image.x + cx + ox * cos - oy * sin, image.y + cy + ox * sin + oy * cos];
}
