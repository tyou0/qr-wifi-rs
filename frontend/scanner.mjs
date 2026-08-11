// Camera capture policy shared by the desktop scanner and its Node regression test.
// Keep enough source pixels for rqrr to detect a QR that occupies a modest part
// of the camera frame, while capping large cameras to bound IPC/decoder work.

export const MAX_CAMERA_CAPTURE_WIDTH = 1280;

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
