#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import {
  cameraCaptureDimensions,
  cameraVideoConstraints,
} from "../frontend/scanner.mjs";
import { ScanSession } from "../frontend/scanner-state.mjs";

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

const sessions = new ScanSession();
const first = sessions.begin();
assert.equal(typeof first, "number");
assert.equal(sessions.begin(), null, "repeated Start must be ignored while camera startup is pending");
sessions.cancel();
assert.equal(sessions.activate(first), false, "a stopped startup must not adopt its late camera stream");
const second = sessions.begin();
assert.equal(sessions.activate(second), true);
assert.equal(sessions.isScanning(second), true);
sessions.cancel();
assert.equal(sessions.isScanning(second), false, "Stop must invalidate an in-flight decode token");
const third = sessions.begin();
assert.equal(sessions.activate(third), true);
assert.equal(sessions.beginConnecting(third), true);
assert.equal(sessions.phase, "connecting");
assert.equal(sessions.begin(), null, "a second scan must not start during an OS connection side effect");
assert.equal(sessions.cancel(), false, "connecting cannot be cancelled after it reaches the OS");
assert.equal(sessions.phase, "connecting");
assert.equal(sessions.begin(), null, "tab changes must not unlock a pending connection");
assert.equal(sessions.settleConnection(third), false, "a cancelled view must suppress stale connection UI");
assert.equal(sessions.phase, "idle");
const fourth = sessions.begin();
assert.equal(sessions.activate(fourth), true);
assert.equal(sessions.beginConnecting(fourth), true);
assert.equal(sessions.settleConnection(fourth), true, "active connection completion should be reported");
assert.equal(sessions.phase, "idle");

const main = await readFile(new URL("../frontend/main.js", import.meta.url), "utf8");
assert.match(
  main,
  /cameraCaptureDimensions\s*\(\s*video\.videoWidth,\s*video\.videoHeight,?\s*\)/,
);
assert.doesNotMatch(main, /const w = 360/);
assert.match(main, /toDataURL\("image\/jpeg", 0\.92\)/);

console.log("camera scanner contract: PASS");
