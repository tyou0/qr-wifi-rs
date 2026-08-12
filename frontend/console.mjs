// The scanner console is an explicit local inspection surface. A Wi-Fi QR
// already contains these credentials, so studying/debugging the decoder requires
// the parsed fields and original payload to remain visible and comparable.
// Callers must render this string with textContent, never innerHTML.
export function appendConsoleText(existing, text, maxChars) {
  const combined = `${existing ? `${existing}\n` : ""}${text}`;
  if (combined.length <= maxChars) return combined;
  const marker = "[older console output truncated]\n";
  return `${marker}${combined.slice(-(maxChars - marker.length))}`;
}

export function formatScannedQr({ credentials, payload }) {
  return [
    "QR code decoded:",
    `SSID: ${credentials.ssid}`,
    `Security: ${credentials.security}`,
    `Password: ${credentials.password ?? "(none)"}`,
    `Hidden: ${credentials.hidden ? "yes" : "no"}`,
    `Raw payload: ${payload}`,
  ].join("\n");
}
