// QR Wi-Fi RS popup.
//
// Thin browser UI over the Native Messaging host (`qr-wifi-host`). All Wi-Fi,
// QR, and parsing logic runs in qr-wifi-core via the host IPC protocol.

const HOST = "com.thetomyou.qrwifi";
const api = typeof browser !== "undefined" ? browser : chrome;

const status = document.getElementById("status");
const qrBox = document.getElementById("qr");
const qrImage = document.getElementById("qr-image");
const payload = document.getElementById("payload");
const ssidList = document.getElementById("ssid-list");
const payloadInput = document.getElementById("payload-input");
const qrFile = document.getElementById("qr-file");

function setStatus(message, error = false) {
  status.textContent = message;
  status.classList.toggle("error", error);
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

async function request(command) {
  const response = await send(command);
  if (!response.ok) throw new Error(response.error ?? "native host error");
  return response.data;
}

function showQr(data) {
  qrImage.src = `data:image/png;base64,${data.png_base64}`;
  payload.textContent = data.payload;
  payloadInput.value = data.payload;
  qrBox.hidden = false;
}

function credentialsFromForm() {
  const ssid = document.getElementById("custom-ssid").value.trim();
  if (!ssid) throw new Error("SSID is required");

  const security = document.getElementById("custom-security").value;
  const rawPassword = document.getElementById("custom-password").value;
  return {
    ssid,
    security,
    password: security === "nopass" || rawPassword === "" ? null : rawPassword,
    hidden: document.getElementById("custom-hidden").checked,
  };
}

function readFileBase64(file) {
  return new Promise((resolve, reject) => {
    const reader = new FileReader();
    reader.onerror = () => reject(new Error("Could not read QR image"));
    reader.onload = () => resolve(String(reader.result).split(",", 2)[1] ?? "");
    reader.readAsDataURL(file);
  });
}

async function run(message, action) {
  setStatus(message);
  try {
    await action();
  } catch (error) {
    setStatus(`Error: ${error.message}`, true);
  }
}

document.querySelectorAll(".tab").forEach((button) => {
  button.addEventListener("click", () => {
    document.querySelectorAll(".tab").forEach((tab) => tab.classList.remove("active"));
    document.querySelectorAll(".panel").forEach((panel) => panel.classList.remove("active"));
    button.classList.add("active");
    document.getElementById(button.dataset.tab).classList.add("active");
  });
});

document.getElementById("share-current").addEventListener("click", () =>
  run("Sharing current Wi-Fi...", async () => {
    showQr(await request({ command: "share_current" }));
    setStatus("Current Wi-Fi QR ready");
  }),
);

document.getElementById("load-networks").addEventListener("click", () =>
  run("Loading SSIDs...", async () => {
    const data = await request({ command: "list_networks" });
    ssidList.textContent = "";
    for (const network of data.networks) {
      const option = document.createElement("option");
      option.value = network.ssid;
      option.textContent = `${network.active ? "● " : ""}${network.ssid}`;
      ssidList.append(option);
    }
    if (data.networks.length === 0) {
      ssidList.append(new Option("No networks found", ""));
    }
    setStatus(`Loaded ${data.networks.length} SSID(s)`);
  }),
);

document.getElementById("share-selected").addEventListener("click", () =>
  run("Sharing selected SSID...", async () => {
    const ssid = ssidList.value;
    if (!ssid) throw new Error("Load and choose an SSID first");
    const data = await request({ command: "get_credentials", ssid });
    showQr(await request({ command: "share_custom", credentials: data.credentials }));
    setStatus(`QR ready for ${ssid}`);
  }),
);

document.getElementById("share-custom").addEventListener("click", () =>
  run("Creating custom QR...", async () => {
    showQr(await request({ command: "share_custom", credentials: credentialsFromForm() }));
    setStatus("Custom QR ready");
  }),
);

document.getElementById("connect-payload").addEventListener("click", () =>
  run("Connecting from payload...", async () => {
    const text = payloadInput.value.trim();
    if (!text) throw new Error("Paste a WIFI: payload first");
    await request({ command: "connect_payload", payload: text });
    setStatus("Connected from payload");
  }),
);

document.getElementById("connect-image").addEventListener("click", () =>
  run("Decoding QR image...", async () => {
    const file = qrFile.files?.[0];
    if (!file) throw new Error("Choose a QR image first");
    const image_base64 = await readFileBase64(file);
    const decoded = await request({ command: "decode_qr", image_base64 });
    await request({ command: "connect", credentials: decoded.credentials });
    setStatus(`Connected to ${decoded.credentials.ssid}`);
  }),
);
