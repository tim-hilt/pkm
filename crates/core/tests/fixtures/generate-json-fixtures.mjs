// Regenerates drawing.min.excalidraw / drawing.pretty.excalidraw.
//
// The scene mimics what excalidraw.com exports (shape of `serializeAsJSON` at
// the reference commit). The bytes are produced by JavaScript's own
// JSON.stringify, so they are the ground truth for number formatting, string
// escaping and key order. Excalidraw itself saves with indent 2 (pretty);
// the minified variant is what `JSON.stringify(data)` yields.
//
// Run: node crates/core/tests/fixtures/generate-json-fixtures.mjs
import { writeFileSync } from "node:fs";
import { dirname, join } from "node:path";
import { fileURLToPath } from "node:url";

const base = {
  angle: 0,
  strokeColor: "#1e1e1e",
  backgroundColor: "transparent",
  fillStyle: "solid",
  strokeWidth: 2,
  strokeStyle: "solid",
  roughness: 1,
  opacity: 100,
  groupIds: [],
  frameId: null,
  roundness: null,
  isDeleted: false,
  boundElements: null,
  updated: 1730380000000,
  link: null,
  locked: false,
};

const scene = {
  type: "excalidraw",
  version: 2,
  source: "https://excalidraw.com",
  elements: [
    {
      id: "Wq3pL0xYk2",
      type: "rectangle",
      x: 123.45000000000002,
      y: -50.5,
      width: 200,
      height: 1e21,
      ...base,
      roundness: { type: 3 },
      seed: 1968410350,
      version: 12,
      versionNonce: 1150084233,
      index: "a0",
      boundElements: [{ id: "Zk9", type: "arrow" }],
    },
    {
      id: "Zk9",
      type: "arrow",
      x: 323.45,
      y: 49.5,
      width: 140.00000000000003,
      height: 0.0000001,
      ...base,
      seed: 42,
      version: 3,
      versionNonce: 7,
      index: "a1",
      points: [
        [0, 0],
        [140.00000000000003, 1.5e-7],
        [-0, 123456789012345680000],
      ],
      lastCommittedPoint: null,
      startBinding: { elementId: "Wq3pL0xYk2", focus: -0.1, gap: 8 },
      endBinding: null,
      startArrowhead: null,
      endArrowhead: "arrow",
    },
    {
      id: "T1",
      type: "text",
      x: 10,
      y: 20,
      width: 180.5,
      height: 25,
      ...base,
      seed: 5,
      version: 2,
      versionNonce: 6,
      index: "a2",
      text: 'Héllo "wörld" \\ / \n\t\u0001 \u2028 日本語 😀 \u{1d11e}',
      fontSize: 20,
      fontFamily: 5,
      textAlign: "left",
      verticalAlign: "top",
      containerId: null,
      originalText: "Héllo",
      autoResize: true,
      lineHeight: 1.25,
    },
    {
      id: "F1",
      type: "freedraw",
      x: 0,
      y: 0,
      width: 3.3000000000000003,
      height: 4.35,
      ...base,
      seed: 9,
      version: 1,
      versionNonce: 10,
      index: "a3",
      points: [[0, 0], [1.1, 1.45], [3.3000000000000003, 4.35]],
      pressures: [0.5, 0.6000000000000001, 0.7],
      simulatePressure: false,
      lastCommittedPoint: [3.3000000000000003, 4.35],
    },
    {
      id: "X1",
      type: "futuristic-widget",
      x: 1,
      y: 2,
      customProp: { nested: [1, 2.5, null, true, false, "s"], empty: {}, none: [] },
    },
  ],
  appState: { gridSize: null, viewBackgroundColor: "#ffffff" },
  files: {
    abc123: {
      mimeType: "image/png",
      id: "abc123",
      dataURL: "data:image/png;base64,iVBORw0KGgo=",
      created: 1730380000000,
      lastRetrieved: 1730380000000,
    },
  },
};

const dir = dirname(fileURLToPath(import.meta.url));
writeFileSync(join(dir, "drawing.min.excalidraw"), JSON.stringify(scene));
writeFileSync(join(dir, "drawing.pretty.excalidraw"), JSON.stringify(scene, null, 2));
