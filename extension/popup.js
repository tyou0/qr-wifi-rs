// QR Wi-Fi RS popup.
//
// Talks to the Native Messaging host (`com.thetomyou.qrwifi`) which is the
// `qr-wifi-host` binary. Each request is one JSON object; the host replies with
// the `{ ok, data?, error? }` envelope defined in qr-wifi-core's IPC module.

const HOST = "com.thetomyou.qrwifi";
const api = typeof browser !== "undefined" ? browser : chrome;

const status = document.getElementById("status");
const qrBox = document.getElementById("qr");
const qrImage = document.getElementById("qr-image");
const payload = document.getElementById("payload");
const payloadInput = document.getElementById("payload-input");

function setStatus(message) {
  status.textContent = message;
}

function send(request) {
  return new Promise((resolve, reject) => {
    let settled = false;
    const port = api.runtime.connectNative(HOST);

    port.onMessage.addListener((message) => {
      settled = true;
      resolve(message);
      port.disconnect();
    });
    port.onDisconnect.addListener(() => {
      if (settled) return;
      const lastError = api.runtime.lastError?.message ?? "native host disconnected";
      reject(new Error(lastError));
    });

    port.postMessage(request);
  });
}

function showQr(data) {
  qrImage.src = `data:image/png;base64,${data.png_base64}`;
  payload.textContent = data.payload;
  qrBox.hidden = false;
}

document.getElementById("share").addEventListener("click", async () => {
  setStatus("Asking host for current Wi-Fi...");
  try {
    const response = await send({ command: "share_current" });
    if (response.ok) {
      showQr(response.data);
      setStatus("QR ready");
    } else {
      setStatus(`Error: ${response.error}`);
    }
  } catch (error) {
    setStatus(`Host error: ${error.message}`);
  }
});

document.getElementById("connect").addEventListener("click", async () => {
  const text = payloadInput.value.trim();
  if (!text) {
    setStatus("Paste a WIFI: payload first");
    return;
  }
  setStatus("Connecting via host...");
  try {
    const response = await send({ command: "connect_payload", payload: text });
    if (response.ok) {
      setStatus("Connected.");
    } else {
      setStatus(`Error: ${response.error}`);
    }
  } catch (error) {
    setStatus(`Host error: ${error.message}`);
  }
});
