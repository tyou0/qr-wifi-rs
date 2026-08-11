#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  cameraCaptureDimensions,
  cameraVideoConstraints,
} from "../frontend/scanner.mjs";

assert.deepEqual(
  cameraCaptureDimensions(640, 480),
  { width: 640, height: 480 },
  "640x480 camera frames must not be destructively downscaled",
);
assert.deepEqual(
  cameraCaptureDimensions(1920, 1080),
  { width: 1280, height: 720 },
  "large frames should be capped while preserving aspect ratio",
);
assert.throws(() => cameraCaptureDimensions(0, 480), /camera dimensions/i);

assert.deepEqual(cameraVideoConstraints(), {
  facingMode: { ideal: "environment" },
  width: { ideal: 1280 },
  height: { ideal: 720 },
});

const main = await readFile(new URL("../frontend/main.js", import.meta.url), "utf8");
assert.match(
  main,
  /cameraCaptureDimensions\s*\(\s*video\.videoWidth,\s*video\.videoHeight,?\s*\)/,
);
assert.doesNotMatch(main, /const w = 360/);
assert.match(main, /toDataURL\("image\/jpeg", 0\.92\)/);

console.log("camera scanner contract: PASS");
