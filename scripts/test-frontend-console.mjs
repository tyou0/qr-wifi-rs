#!/usr/bin/env node
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";

import { appendConsoleText, formatScannedQr } from "../frontend/console.mjs";

const root = new URL("../", import.meta.url);
const [html, js, css, rust, consoleModule] = await Promise.all([
  readFile(new URL("frontend/index.html", root), "utf8"),
  readFile(new URL("frontend/main.js", root), "utf8"),
  readFile(new URL("frontend/styles.css", root), "utf8"),
  readFile(new URL("src-tauri/src/main.rs", root), "utf8"),
  readFile(new URL("frontend/console.mjs", root), "utf8"),
]);

assert.match(html, /id="toggle-console"[^>]*aria-expanded="true"[^>]*aria-controls="console-viewport"/);
assert.match(html, /id="console-viewport"[^>]*class="output-viewport"/);
assert.match(js, /appendConsoleText\(output\.textContent, text, MAX_CONSOLE_CHARS\)/);
assert.match(js, /outputViewport\.scrollTop\s*=\s*outputViewport\.scrollHeight/);
assert.match(js, /consoleSection\.classList\.toggle\("collapsed"/);
assert.match(consoleModule, /Raw payload:/);
assert.match(consoleModule, /Password:/);
assert.match(js, /formatScannedQr\(decoded\)/);
assert.match(js, /decoded\.credentials/);
assert.match(css, /\.output-viewport\s*\{[^}]*overflow:\s*auto/s);
assert.match(css, /\.output-section\.collapsed\s+\.output-viewport\s*\{[^}]*display:\s*none/s);
assert.match(rust, /struct DecodedQr/);
assert.match(rust, /payload:\s*String/);
assert.match(rust, /credentials:\s*WifiCredentials/);

const formatted = formatScannedQr({
  payload: "WIFI:S:Fixture;T:WPA;P:test-password;H:true;;",
  credentials: {
    ssid: "Fixture",
    security: "WPA",
    password: "test-password",
    hidden: true,
  },
});
assert.equal(
  formatted,
  [
    "QR code decoded:",
    "SSID: Fixture",
    "Security: WPA",
    "Password: test-password",
    "Hidden: yes",
    "Raw payload: WIFI:S:Fixture;T:WPA;P:test-password;H:true;;",
  ].join("\n"),
);

assert.equal(appendConsoleText("first", "second", 100), "first\nsecond");
const capped = appendConsoleText("a".repeat(120_000), "<b>QR-controlled</b>", 100_000);
assert.equal(capped.length, 100_000, "console history must honor its exact bound");
assert.match(capped, /^\[older console output truncated\]\n/);
assert.match(capped, /<b>QR-controlled<\/b>$/, "HTML-like QR text must remain plain text data");

console.log("frontend console contract: PASS");
