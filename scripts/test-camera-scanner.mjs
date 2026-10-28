#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { createContext, runInContext } from "node:vm";
import {
  cameraCaptureDimensions,
  cameraErrorMessage,
  cameraVideoConstraints,
  hasCameraFrame,
  waitForCameraStartup,
} from "../frontend/scanner.mjs";
import { ScanSession } from "../frontend/scanner-state.mjs";
import { appendConsoleText, formatScannedQr } from "../frontend/console.mjs";

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

const videoAt = (readyState, videoWidth = 1280, videoHeight = 720) => ({
  readyState,
  videoWidth,
  videoHeight,
});
assert.equal(
  hasCameraFrame(videoAt(3)),
  true,
  "WebKitGTK keeps a playing camera at HAVE_FUTURE_DATA; its frames must still be scanned",
);
assert.equal(hasCameraFrame(videoAt(2)), true);
assert.equal(hasCameraFrame(videoAt(4)), true);
assert.equal(hasCameraFrame(videoAt(1)), false, "metadata alone has no frame to draw");
assert.equal(hasCameraFrame(videoAt(4, 0, 0)), false);

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

const main = await readFile(
  process.env.QR_WIFI_FRONTEND_SOURCE ?? new URL("../frontend/main.js", import.meta.url),
  "utf8",
);
assert.match(
  main,
  /cameraCaptureDimensions\s*\(\s*video\.videoWidth,\s*video\.videoHeight,?\s*\)/,
);
assert.doesNotMatch(main, /const w = 360/);
assert.match(main, /hasCameraFrame\(video\)/);
assert.doesNotMatch(main, /HAVE_ENOUGH_DATA/, "Linux cameras never report HAVE_ENOUGH_DATA while playing");
assert.match(main, /toDataURL\("image\/jpeg", 0\.92\)/);
const linuxCamera = { userAgent: "Mozilla/5.0 (X11; Linux x86_64)" };
assert.match(cameraErrorMessage({ name: "NotAllowedError" }, linuxCamera), /permission denied/i);
assert.doesNotMatch(cameraErrorMessage({ name: "NotAllowedError" }, linuxCamera), /macOS|Windows|install/i);
assert.match(cameraErrorMessage({ name: "NotFoundError" }, linuxCamera), /Arch\/Omarchy.*GStreamer/);
assert.match(cameraErrorMessage({ name: "NotReadableError" }, linuxCamera), /driver.*GStreamer/);
assert.match(cameraErrorMessage({ name: "SecurityError" }, { userAgent: "Windows NT" }), /Windows Settings/);
assert.match(cameraErrorMessage({ name: "NotAllowedError" }, { userAgent: "Macintosh" }), /macOS Privacy/);
assert.match(cameraErrorMessage(new Error("test"), { hasCameraApi: false }), /API unavailable/);
assert.equal(cameraErrorMessage(new Error("capture failed")), "Camera access failed: capture failed");

const stream = { getTracks: () => [{ stop: () => { stoppedLateStream = true; } }] };
let stoppedLateStream = false;
assert.equal(await waitForCameraStartup(Promise.resolve(stream), "Camera access"), stream);
const denied = new Error("denied");
await assert.rejects(waitForCameraStartup(Promise.reject(denied), "Camera access"), (error) => error === denied);
let finishLateAccess;
const lateAccess = new Promise((resolve) => { finishLateAccess = resolve; });
await assert.rejects(waitForCameraStartup(lateAccess, "Camera access", {
  timeoutMs: 1,
  onLateResolve: (lateStream) => lateStream.getTracks().forEach((track) => track.stop()),
}), (error) => {
  assert.equal(error.name, "CameraStartupTimeoutError");
  assert.match(cameraErrorMessage(error, linuxCamera), /Camera access timed out.*GStreamer/);
  return true;
});
finishLateAccess(stream);
await Promise.resolve();
assert.equal(stoppedLateStream, true, "a camera arriving after timeout must be released");
await assert.rejects(
  waitForCameraStartup(new Promise(() => {}), "Camera preview", { timeoutMs: 1 }),
  /Camera preview timed out/,
);

// macOS camera access crosses a bundle/OS boundary. Keep the required metadata
// and the local loopback transport contract under test so a future packaging
// cleanup cannot silently turn a working scanner into a permanent permission
// denial on physical Macs.
const [infoPlist, entitlements, tauriConfig, rustMain] = await Promise.all([
  readFile(new URL("../src-tauri/Info.plist", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/entitlements.plist", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
  readFile(new URL("../src-tauri/src/main.rs", import.meta.url), "utf8"),
]);
assert.match(infoPlist, /<key>NSCameraUsageDescription<\/key>/);
assert.match(infoPlist, /<key>NSLocationWhenInUseUsageDescription<\/key>/);
assert.match(infoPlist, /<key>NSLocationUsageDescription<\/key>/);
assert.match(infoPlist, /<key>NSAllowsLocalNetworking<\/key>\s*<true\/>/);
assert.match(entitlements, /<key>com\.apple\.security\.device\.camera<\/key>\s*<true\/>/);
assert.ok(
  /<key>com\.apple\.security\.personal-information\.location<\/key>\s*<true\/>/.test(entitlements),
  "The hardened macOS app must include the Location Services entitlement",
);
assert.equal(JSON.parse(tauriConfig).bundle.macOS.entitlements, "entitlements.plist");
assert.match(rustMain, /TcpListener::bind\(\("127\.0\.0\.1", 0\)\)/);
assert.match(rustMain, /WebviewUrl::External\(url\.clone\(\)\)/);

// Sync Tauri commands run on the main thread, which also drives the webview.
// Decoding each scan frame there freezes the camera preview and Stop button.
for (const command of ["decode_qr", "connect_network"]) {
  assert.match(
    rustMain,
    new RegExp(`#\\[tauri::command\\(async\\)\\]\\s*fn ${command}\\(`),
    `${command} must run off the main thread`,
  );
}

// Exercise the real startup handler: on WebKitGTK, a hidden video can leave
// play() pending forever, even with a live camera track and granted permission.
// Reject that unsupported ordering here so a regression fails without hanging.
const elements = new Map();
function element(selector) {
  if (!elements.has(selector)) {
    const classes = new Set(selector === "#scanner-container" ? ["hidden"] : []);
    elements.set(selector, {
      textContent: "",
      value: "",
      dataset: {},
      classList: {
        add: (name) => classes.add(name),
        remove: (name) => classes.delete(name),
        contains: (name) => classes.has(name),
      },
      addEventListener() {},
      getContext: () => ({}),
    });
  }
  return elements.get(selector);
}
let visibleWhenPlaybackRequested = null;
let stoppedTracks = 0;
element("#scan-video").play = () => {
  visibleWhenPlaybackRequested = !element("#scanner-container").classList.contains("hidden");
  return visibleWhenPlaybackRequested
    ? Promise.resolve()
    : Promise.reject(new Error("Hidden video cannot start playback in WebKitGTK"));
};
const startupStream = { getTracks: () => [{ stop: () => { stoppedTracks += 1; } }] };
const context = createContext({
  cameraCaptureDimensions,
  cameraErrorMessage,
  cameraVideoConstraints,
  hasCameraFrame,
  waitForCameraStartup,
  ScanSession,
  appendConsoleText,
  formatScannedQr,
  document: {
    querySelector: element,
    querySelectorAll: () => [],
    createElement: element,
  },
  navigator: {
    userAgent: "Linux",
    mediaDevices: { getUserMedia: async () => startupStream },
  },
  window: {
    __TAURI__: { core: { invoke: async (command) => {
      assert.equal(command, "list_networks", "startup must not connect to Wi-Fi");
      return [];
    } } },
    addEventListener() {},
  },
  setTimeout: () => 1, // Do not start a capture loop in the startup test.
  clearTimeout() {},
});
runInContext(main.replace(/^import[\s\S]*?from "[^"]+";\n/gm, ""), context);
await runInContext("startScanning()", context);
assert.equal(visibleWhenPlaybackRequested, true, "show the camera before requesting playback");
assert.equal(element("#status").textContent, "Scanning for QR code");
runInContext("stopScanning()", context);
assert.equal(stoppedTracks, 1, "Stop must release the camera after successful startup");

console.log("camera scanner contract: PASS");
