// Camera capture policy shared by the desktop scanner and its Node regression test.
// Keep enough source pixels for rqrr to detect a QR that occupies a modest part
// of the camera frame, while capping large cameras to bound IPC/decoder work.

export const MAX_CAMERA_CAPTURE_WIDTH = 1280;

// HTMLMediaElement.HAVE_CURRENT_DATA: drawImage() needs only the current frame.
// Never wait for HAVE_ENOUGH_DATA; WebKitGTK reports a playing live camera as
// HAVE_FUTURE_DATA indefinitely, so Linux would skip every frame.
const HAVE_CURRENT_DATA = 2;

export function hasCameraFrame({ readyState, videoWidth, videoHeight }) {
  return readyState >= HAVE_CURRENT_DATA && videoWidth > 0 && videoHeight > 0;
}

export function cameraCaptureDimensions(
  videoWidth,
  videoHeight,
  maxWidth = MAX_CAMERA_CAPTURE_WIDTH,
) {
  if (
    !Number.isFinite(videoWidth) ||
    !Number.isFinite(videoHeight) ||
    videoWidth <= 0 ||
    videoHeight <= 0
  ) {
    throw new Error("Invalid camera dimensions");
  }

  const width = Math.min(Math.round(videoWidth), maxWidth);
  const height = Math.max(1, Math.round(videoHeight * (width / videoWidth)));
  return { width, height };
}

export function cameraVideoConstraints() {
  return {
    facingMode: { ideal: "environment" },
    width: { ideal: 1280 },
    height: { ideal: 720 },
  };
}

// getUserMedia may stay pending without rejecting (for example while a portal
// waits for permission). A stream arriving after timeout still needs closing.
export function waitForCameraStartup(pending, stage, {
  timeoutMs = 20_000,
  onLateResolve = () => {},
} = {}) {
  let expired = false;
  let timer;
  const operation = Promise.resolve(pending).then((value) => {
    if (expired) onLateResolve(value);
    return value;
  });
  const timeout = new Promise((_, reject) => {
    timer = setTimeout(() => {
      expired = true;
      const error = new Error(`${stage} timed out. Check for a pending camera permission prompt, then retry.`);
      error.name = "CameraStartupTimeoutError";
      reject(error);
    }, timeoutMs);
  });
  return Promise.race([operation, timeout]).finally(() => clearTimeout(timer));
}

// A capture failure can mean a busy camera, a driver failure, or a missing
// GStreamer backend on Linux. Do not report every NotReadableError as "busy".
export function cameraErrorMessage(error, { userAgent = "", hasCameraApi = true } = {}) {
  const name = error?.name ?? "";
  const message = error?.message ?? String(error);
  const linux = userAgent.includes("Linux");
  if (!hasCameraApi) {
    return "Camera API unavailable in this webview.";
  }
  if (name === "CameraStartupTimeoutError") {
    return linux
      ? `${message} On Linux, also check camera drivers and the GStreamer/PipeWire camera setup.`
      : message;
  }
  if (name === "NotAllowedError" || name === "SecurityError") {
    if (userAgent.includes("Windows")) {
      return "Camera permission denied. Enable desktop-app camera access in Windows Settings > Privacy and security > Camera.";
    }
    if (linux) {
      return "Camera permission denied. Check your desktop camera permissions, then restart QR Wi-Fi RS.";
    }
    return "Camera permission denied. Allow camera access for QR Wi-Fi RS in macOS Privacy settings.";
  }
  if (name === "NotFoundError" || name === "OverconstrainedError") {
    return linux
      ? "No usable camera found. On Arch/Omarchy, check the camera driver and GStreamer camera plugins."
      : "No usable camera found.";
  }
  if (name === "NotReadableError") {
    return linux
      ? "Camera could not start. Close other camera apps and check the camera driver and GStreamer camera plugins."
      : "Camera could not start. It may be in use by another app or unavailable to the system.";
  }
  return `Camera access failed: ${message}`;
}
