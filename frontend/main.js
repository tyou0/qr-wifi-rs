import {
  cameraCaptureDimensions,
  cameraVideoConstraints,
} from "./scanner.mjs";
import { appendConsoleText, formatScannedQr } from "./console.mjs";
import { ScanSession } from "./scanner-state.mjs";

// QR Wi-Fi RS frontend. Vanilla JS — no framework, no bundler.
// All heavy lifting happens in Rust via Tauri commands (window.__TAURI__).

const tauri = window.__TAURI__ ?? {};
const invoke = (tauri.core?.invoke ?? tauri.invoke)?.bind(tauri.core ?? tauri);

if (!invoke) {
  document.body.innerHTML =
    '<p style="padding:2rem;font-family:sans-serif;color:#ff6b7a">' +
    "Tauri bridge unavailable. Run this UI through `cargo tauri dev`.</p>";
}

const $ = (selector) => {
  const el = document.querySelector(selector);
  if (!el) throw new Error(`Missing element: ${selector}`);
  return el;
};

const output = $("#output");
const outputViewport = $("#console-viewport");
const consoleSection = $("#console-section");
const toggleConsole = $("#toggle-console");
const status = $("#status");
const networkList = $("#network-list");
const networkSearch = $("#network-search");
const securityInput = $("#custom-security");
const passwordGroup = $("#password-group");

const qrModal = $("#qr-modal");
const qrImage = $("#qr-image");
const modalDesc = $("#modal-desc");
const modalPayload = $("#modal-payload");
const modalOverlay = document.querySelector(".modal-overlay");

let allNetworks = [];
let scanStream = null;
let scanTimer = null;
let isDecoding = false;
const scanSession = new ScanSession();
const scanCanvas = document.createElement("canvas");
const scanCtx = scanCanvas.getContext("2d", { willReadFrequently: true });
const MAX_CONSOLE_CHARS = 100_000;

function setStatus(message, kind = "idle") {
  status.textContent = message;
  status.dataset.kind = kind;
}
function print(text) {
  const placeholder = "Generated status/logs will appear here.";
  if (output.textContent === placeholder) output.textContent = "";
  // textContent keeps QR-controlled SSIDs/passwords inert while preserving a
  // scrollable chronological record for studying decode and connection stages.
  output.textContent = appendConsoleText(output.textContent, text, MAX_CONSOLE_CHARS);
  outputViewport.scrollTop = outputViewport.scrollHeight;
}

function cameraErrorMessage(error) {
  const name = error?.name ?? "";
  const message = error?.message ?? String(error);
  if (!navigator.mediaDevices?.getUserMedia) {
    return "Camera API unavailable in this webview.";
  }
  if (name === "NotAllowedError" || name === "SecurityError") {
    return "Camera permission denied. Allow camera access for QR Wi-Fi RS in macOS Privacy settings.";
  }
  if (name === "NotFoundError" || name === "OverconstrainedError") {
    return "No usable camera found.";
  }
  if (name === "NotReadableError") {
    return "Camera is already in use by another app.";
  }
  return `Camera access failed: ${message}`;
}

async function showQr(result, description) {
  qrImage.src = `data:image/png;base64,${result.png_base64}`;
  modalDesc.textContent = description ?? result.payload;
  // Show the raw WIFI: payload string at the bottom of the modal.
  modalPayload.textContent = result.payload;
  qrModal.classList.remove("hidden");
}
function hideQr() {
  qrModal.classList.add("hidden");
}

$("#close-modal").addEventListener("click", hideQr);
modalOverlay.addEventListener("click", hideQr);
$("#copy-payload").addEventListener("click", async () => {
  const text = modalPayload.textContent ?? "";
  if (!text) return;
  try {
    await navigator.clipboard.writeText(text);
    setStatus("Payload copied", "success");
  } catch {
    setStatus("Could not copy payload", "error");
  }
});
window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") hideQr();
});

$("#clear-output").addEventListener("click", () => {
  output.textContent = "Generated status/logs will appear here.";
  outputViewport.scrollTop = 0;
});
toggleConsole.addEventListener("click", () => {
  const collapsed = consoleSection.classList.toggle("collapsed");
  toggleConsole.setAttribute("aria-expanded", String(!collapsed));
  toggleConsole.textContent = collapsed ? "Show" : "Hide";
  if (!collapsed) outputViewport.scrollTop = outputViewport.scrollHeight;
});

// Tabs
document.querySelectorAll(".tab-btn").forEach((btn) => {
  btn.addEventListener("click", () => {
    const tab = btn.getAttribute("data-tab");
    document.querySelectorAll(".tab-btn").forEach((b) => b.classList.remove("active"));
    document.querySelectorAll(".tab-pane").forEach((p) => p.classList.remove("active"));
    btn.classList.add("active");
    $(`#tab-${tab}`).classList.add("active");
    if (tab !== "connect") stopScanning();
  });
});

securityInput.addEventListener("change", () => {
  const open = securityInput.value === "nopass";
  passwordGroup.classList.toggle("hidden", open);
});

function fuzzyMatch(text, query) {
  const t = text.toLowerCase();
  let idx = 0;
  for (let i = 0; i < t.length && idx < query.length; i += 1) {
    if (t[i] === query[idx].toLowerCase()) idx += 1;
  }
  return idx === query.length;
}
function renderNetworks(networks) {
  networkList.innerHTML = "";
  if (networks.length === 0) {
    networkList.textContent = "No networks found.";
    return;
  }
  for (const network of networks) {
    const button = document.createElement("button");
    button.type = "button";
    button.className = "network-card";
    button.textContent = network.active ? `* ${network.ssid}` : network.ssid;
    button.addEventListener("click", async () => {
      setStatus(`Building QR for ${network.ssid}...`, "loading");
      try {
        const credentials = await invoke("get_credentials", { ssid: network.ssid });
        const result = await invoke("share_custom", { credentials });
        await showQr(result, `SSID: ${credentials.ssid} · ${credentials.security}${credentials.hidden ? " (Hidden)" : ""}`);
        print(`QR for ${network.ssid}.`);
        setStatus("Done", "success");
      } catch (error) {
        handleError(error);
      }
    });
    networkList.append(button);
  }
}
networkSearch.addEventListener("input", () => {
  const query = networkSearch.value.trim();
  renderNetworks(query ? allNetworks.filter((n) => fuzzyMatch(n.ssid, query)) : allNetworks);
});

$("#share-current").addEventListener("click", async () => {
  setStatus("Detecting current Wi-Fi...", "loading");
  try {
    const result = await invoke("share_current");
    await showQr(result);
    print(`QR for current network.`);
    setStatus("Done", "success");
  } catch (error) {
    handleError(error);
  }
});

async function refreshNetworks() {
  setStatus("Loading networks...", "loading");
  try {
    networkSearch.value = "";
    allNetworks = await invoke("list_networks");
    renderNetworks(allNetworks);
    setStatus(`${allNetworks.length} network(s) loaded`, "success");
  } catch (error) {
    handleError(error, true);
  }
}
$("#refresh-networks").addEventListener("click", refreshNetworks);

$("#custom-form").addEventListener("submit", async (event) => {
  event.preventDefault();
  setStatus("Generating custom QR...", "loading");
  const ssid = $("#custom-ssid").value.trim();
  const security = securityInput.value;
  const password = $("#custom-password").value;
  const hidden = $("#custom-hidden").checked;
  if (!ssid) {
    setStatus("SSID is required", "error");
    return;
  }
  const credentials = { ssid, security, hidden };
  if (security !== "nopass" && password) credentials.password = password;
  try {
    const result = await invoke("share_custom", { credentials });
    await showQr(result, `SSID: ${ssid} · ${security}${hidden ? " (Hidden)" : ""}`);
    print(`Custom QR for ${ssid}.`);
    setStatus("Done", "success");
  } catch (error) {
    handleError(error);
  }
});

// Camera scanning: capture a frame, send the JPEG to Rust for QR decoding.
async function startScanning() {
  const token = scanSession.begin();
  if (token === null) return;

  const startButton = $("#scan-camera");
  startButton.disabled = true;
  startButton.classList.add("hidden");
  $("#stop-scan").classList.remove("hidden");
  print("Initializing camera...");
  setStatus("Accessing camera", "loading");
  let requestedStream = null;
  try {
    requestedStream = await navigator.mediaDevices.getUserMedia({
      audio: false,
      video: cameraVideoConstraints(),
    });
    // Stop may have been pressed while the permission prompt was open. Never
    // adopt a stream that belongs to an invalidated session.
    if (!scanSession.activate(token)) {
      requestedStream.getTracks().forEach((track) => track.stop());
      return;
    }

    scanStream = requestedStream;
    const video = $("#scan-video");
    video.srcObject = scanStream;
    await video.play();
    if (!scanSession.isScanning(token)) return;

    startButton.disabled = false;
    $("#scanner-container").classList.remove("hidden");
    setStatus("Scanning for QR code", "loading");
    print("Point camera at a Wi-Fi QR code...");
    scheduleNextScan();
  } catch (error) {
    if (requestedStream && requestedStream !== scanStream) {
      requestedStream.getTracks().forEach((track) => track.stop());
    }
    if (!scanSession.isCurrent(token)) return;
    const message = cameraErrorMessage(error);
    print(message);
    setStatus(message, "error");
    stopScanning();
  }
}
function releaseScanner(enableStart = true) {
  if (scanTimer) {
    clearTimeout(scanTimer);
    scanTimer = null;
  }
  isDecoding = false;
  if (scanStream) {
    scanStream.getTracks().forEach((track) => track.stop());
    scanStream = null;
  }
  const video = $("#scan-video");
  if (video) video.srcObject = null;
  const start = document.querySelector("#scan-camera");
  const stop = document.querySelector("#stop-scan");
  const container = document.querySelector("#scanner-container");
  if (start) {
    start.disabled = !enableStart;
    start.classList.remove("hidden");
  }
  if (stop) stop.classList.add("hidden");
  if (container) container.classList.add("hidden");
}
function stopScanning() {
  // Camera acquisition/decode can be invalidated. A connection already handed
  // to the OS cannot be cancelled, so cancel() keeps that phase locked and only
  // suppresses stale UI until the side effect settles.
  const canStart = scanSession.cancel();
  releaseScanner(canStart);
}
function scheduleNextScan() {
  // Self-rescheduling: only queue the next capture once the previous decode
  // round-trip finishes, so frames never pile up or race.
  scanTimer = setTimeout(captureAndDecode, 400);
}

async function captureAndDecode() {
  scanTimer = null;
  const token = scanSession.token;
  const video = $("#scan-video");
  if (!scanSession.isScanning(token)) return;
  if (!scanStream || video.paused || video.ended) {
    if (scanStream && scanSession.isScanning(token)) scheduleNextScan();
    return;
  }
  if (isDecoding || video.readyState < video.HAVE_ENOUGH_DATA) {
    scheduleNextScan();
    return;
  }
  isDecoding = true;
  // Preserve native detail for QR detection. The old unconditional 360px
  // downscale discarded enough modules that ordinary camera-distance codes
  // became undecodable. Cap only large sources to bound IPC/decoder work.
  const { width: w, height: h } = cameraCaptureDimensions(
    video.videoWidth,
    video.videoHeight,
  );
  scanCanvas.width = w;
  scanCanvas.height = h;
  scanCtx.drawImage(video, 0, 0, w, h);
  const dataUrl = scanCanvas.toDataURL("image/jpeg", 0.92);
  const base64 = dataUrl.slice("data:image/jpeg;base64,".length);
  let decoded;
  try {
    decoded = await invoke("decode_qr", { imageBase64: base64 });
  } catch {
    isDecoding = false;
    if (scanSession.isScanning(token)) scheduleNextScan();
    return;
  }

  isDecoding = false;
  if (!scanSession.beginConnecting(token)) return;
  releaseScanner(false);
  print(formatScannedQr(decoded));
  setStatus(`Connecting to ${decoded.credentials.ssid}...`, "loading");
  const startButton = $("#scan-camera");
  let connectionError = null;
  try {
    await invoke("connect_network", { credentials: decoded.credentials });
  } catch (error) {
    connectionError = error;
  }

  const shouldReport = scanSession.settleConnection(token);
  startButton.disabled = false;
  if (!shouldReport) {
    setStatus("Ready", "idle");
    return;
  }
  if (connectionError === null) {
    print(`Connected to ${decoded.credentials.ssid}.`);
    setStatus("Connected", "success");
  } else {
    handleError(connectionError);
  }
}
$("#scan-camera").addEventListener("click", startScanning);
$("#stop-scan").addEventListener("click", stopScanning);

function handleError(error, inList = false) {
  const message = typeof error === "string" ? error : error?.message ?? String(error);
  print(message);
  setStatus(message, "error");
  if (inList) networkList.textContent = message;
}

setStatus("Ready", "idle");
void refreshNetworks().catch(() => undefined);
