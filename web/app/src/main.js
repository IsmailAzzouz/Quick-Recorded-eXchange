import init, {
  packets_from_file,
  QrSymbol,
  QrxReceiver,
} from "../../crate/pkg/qrx_web_core.js";
import { registerSW } from "virtual:pwa-register";
import {
  delay,
  downloadBlob,
  waitForVideoFrame,
  recordQrVideo,
} from "./media.js";
import { FrameScanner } from "./scanner.js";
import { KeepAwake } from "./keep-awake.js";
import { createWakeVideo } from "./wake-video.js";
import { setupWorkspace } from "./workspace.js";
import "./style.css";

const $ = (id) => document.getElementById(id);
const video = $("incoming");
const outCanvas = $("qr");
const outContext = outCanvas.getContext("2d");
const scanCanvas = document.createElement("canvas");
const scanContext = scanCanvas.getContext("2d", { willReadFrequently: true });
const scanner = new FrameScanner(
  new Worker(new URL("./scanner.worker.js", import.meta.url), {
    type: "module",
  }),
);
let packets = [];
let packetIndex = 0;
let timer = null;
let receiver;
let stream = null;
let videoUrl = null;
let source = new AbortController();
let sendGeneration = 0;
let exporting = null;
let exportedVideoUrl = null;
setupWorkspace(document);

function clearExport() {
  if (exportedVideoUrl) URL.revokeObjectURL(exportedVideoUrl);
  exportedVideoUrl = null;
  $("video-ready").hidden = true;
  $("video-download").removeAttribute("href");
}

function message(target, content, isError = false) {
  $(target).textContent = content;
  $(target).classList.toggle("error", isError);
}

const wakeVideo = createWakeVideo(document, () => keepAwake.interrupted());
const keepAwake = new KeepAwake({
  page: document,
  requestNative: navigator.wakeLock?.request
    ? () => navigator.wakeLock.request("screen")
    : null,
  fallback: wakeVideo,
  report(state) {
    const statuses = {
      off: "Screen follows your device settings.",
      requesting: "Requesting screen protection…",
      native: "Screen wake lock active · keep this tab visible.",
      fallback:
        "Video fallback playing · keep this tab visible. Device sleep prevention varies by browser; HTTPS gives a native screen lock.",
      waiting:
        "Keep screen on requested · protection will return when this tab is visible.",
      retrying: "Screen protection was interrupted · retrying automatically…",
      blocked:
        "Screen protection blocked by the browser or device · retry, or use a trusted HTTPS address.",
    };
    message("wake-status", statuses[state], state === "blocked");
    $("wake-retry").hidden = !["blocked", "retrying"].includes(state);
  },
});
$("keep-awake").addEventListener("change", (event) => {
  try {
    localStorage.setItem("qrx.keepScreenOn", event.target.checked ? "1" : "0");
  } catch {
    /* Storage may be disabled. */
  }
  void keepAwake.setEnabled(event.target.checked);
});
$("wake-retry").addEventListener("click", () => {
  void keepAwake.ensure();
});
try {
  $("keep-awake").checked = localStorage.getItem("qrx.keepScreenOn") === "1";
} catch {
  /* Storage may be disabled. */
}
if ($("keep-awake").checked) void keepAwake.setEnabled(true);

function currentFps() {
  return Math.max(1, Math.min(30, Number($("fps").value) || 10));
}

function senderControls() {
  $("tx-preview").dataset.ready = String(packets.length > 0);
  for (const id of ["play", "prev", "next", "export"])
    $(id).disabled = !packets.length || !!exporting;
  for (const id of ["send-file", "chunk-size", "ecc", "fps"])
    $(id).disabled = !!exporting;
  $("cancel-export").hidden = !exporting;
  $("export-estimate").textContent = packets.length
    ? `${packets.length} QR ${packets.length === 1 ? "frame" : "frames"} · ${(packets.length * Math.max(1000 / currentFps(), 100) / 1000 + 0.3).toFixed(1)} seconds of video`
    : "Select a file to create its video.";
}

function stopPlayback() {
  clearInterval(timer);
  timer = null;
  $("play").textContent = "Play";
}

function renderPacket() {
  if (!packets.length) return;
  const symbol = new QrSymbol(packets[packetIndex], Number($("ecc").value));
  try {
    const size = symbol.size;
    const modules = symbol.modules();
    const scale = Math.floor(
      Math.min(outCanvas.width, outCanvas.height) / size,
    );
    if (scale < 1) throw new Error("Canvas cannot fit this QR symbol");
    const marginX = Math.floor((outCanvas.width - size * scale) / 2);
    const marginY = Math.floor((outCanvas.height - size * scale) / 2);
    outContext.fillStyle = "#fff";
    outContext.fillRect(0, 0, outCanvas.width, outCanvas.height);
    outContext.fillStyle = "#000";
    for (let y = 0; y < size; y++) {
      for (let x = 0; x < size; x++) {
        if (modules[y * size + x])
          outContext.fillRect(
            marginX + x * scale,
            marginY + y * scale,
            scale,
            scale,
          );
      }
    }
    message(
      "tx-status",
      `QR ${packetIndex + 1} / ${packets.length} · FQ1 · ${currentFps()} FPS`,
    );
  } finally {
    symbol.free();
  }
}

function advance(amount) {
  if (!packets.length) return;
  packetIndex = (packetIndex + amount + packets.length) % packets.length;
  try {
    renderPacket();
  } catch (err) {
    stopPlayback();
    message("tx-status", String(err), true);
  }
}

$("send-file").addEventListener("change", async (event) => {
  const token = ++sendGeneration;
  clearExport();
  stopPlayback();
  packets = [];
  senderControls();
  outContext.fillStyle = "#fff";
  outContext.fillRect(0, 0, outCanvas.width, outCanvas.height);
  const file = event.target.files?.[0];
  if (!file) return;
  if (file.size > 16 * 1024 * 1024) {
    message("tx-status", "Browser MVP limit: 16 MiB per file.", true);
    return;
  }
  message("tx-status", "Encoding file locally…");
  try {
    const bytes = new Uint8Array(await file.arrayBuffer());
    if (token !== sendGeneration) return;
    packets = Array.from(
      packets_from_file(file.name, bytes, Number($("chunk-size").value)),
    );
    packetIndex = 0;
    renderPacket();
    senderControls();
  } catch (err) {
    packets = [];
    senderControls();
    message("tx-status", String(err), true);
  }
});
$("chunk-size").addEventListener("change", () => {
  if ($("send-file").files?.length)
    $("send-file").dispatchEvent(new Event("change"));
});
$("ecc").addEventListener("change", () => {
  stopPlayback();
  if (!packets.length && $("send-file").files?.length)
    $("send-file").dispatchEvent(new Event("change"));
  else advance(0);
});
$("fps").addEventListener("change", () => {
  senderControls();
  if (timer) {
    stopPlayback();
    $("play").click();
  } else if (packets.length) advance(0);
});
$("play").addEventListener("click", () => {
  if (timer) {
    stopPlayback();
    return;
  }
  if (!packets.length) return;
  timer = setInterval(() => advance(1), 1000 / currentFps());
  $("play").textContent = "Pause";
});
$("prev").addEventListener("click", () => {
  stopPlayback();
  advance(-1);
});
$("next").addEventListener("click", () => {
  stopPlayback();
  advance(1);
});
$("cancel-export").addEventListener("click", () => exporting?.abort());
$("export").addEventListener("click", async () => {
  stopPlayback();
  clearExport();
  exporting = new AbortController();
  senderControls();
  try {
    const { blob, extension } = await recordQrVideo(
      outCanvas,
      packets.length,
      currentFps(),
      (index) => {
        packetIndex = index;
        renderPacket();
        message(
          "tx-status",
          `Creating video · QR ${index + 1} / ${packets.length}`,
        );
      },
      exporting.signal,
    );
    exporting.signal.throwIfAborted();
    exportedVideoUrl = URL.createObjectURL(blob);
    $("video-download").href = exportedVideoUrl;
    $("video-download").download = `${safeFilename($("send-file").files[0].name)}.qrx.${extension}`;
    $("video-ready").hidden = false;
    $("video-download").click();
    message(
      "tx-status",
      `QR video ready · ${packets.length} packets · ${(blob.size / 1024).toFixed(0)} KiB`,
    );
  } catch (err) {
    message(
      "tx-status",
      err.name === "AbortError" ? "Export cancelled." : String(err),
      err.name !== "AbortError",
    );
  } finally {
    exporting = null;
    senderControls();
  }
});

function safeFilename(name) {
  const clean = name
    .replace(/[\\/\x00-\x1f<>:"|?*]+/g, "_")
    .replace(/^\.+/, "")
    .trim();
  return clean.slice(0, 180) || "recovered.bin";
}

function refreshReceived() {
  const list = $("received");
  list.replaceChildren();
  for (let i = 0; i < receiver.file_count(); i++) {
    const card = document.createElement("article");
    card.className = "received-card";
    const title = document.createElement("strong");
    title.textContent = receiver.filename(i);
    const stats = document.createElement("p");
    const current = receiver.received_parts(i);
    const total = receiver.total_parts(i);
    stats.textContent = `${current} / ${total} packets · ${receiver.is_complete(i) ? "SHA-256 verified" : "Waiting for frames"}`;
    const progress = document.createElement("progress");
    progress.max = total;
    progress.value = current;
    card.append(title, stats, progress);
    const verificationError = receiver.verification_error(i);
    if (verificationError) {
      const detail = document.createElement("p");
      detail.className = "error";
      detail.textContent = verificationError;
      card.append(detail);
    }
    if (receiver.is_complete(i)) {
      const button = document.createElement("button");
      button.textContent = "Download verified file";
      button.addEventListener("click", () => {
        downloadBlob(
          new Blob([receiver.verified_bytes(i)], {
            type: "application/octet-stream",
          }),
          safeFilename(receiver.filename(i)),
        );
      });
      card.append(button);
    }
    list.append(card);
  }
}

async function scanFrame(signal) {
  if (
    video.readyState < HTMLMediaElement.HAVE_CURRENT_DATA ||
    !video.videoWidth
  )
    return;
  // Bound pixel buffers on 4K inputs. Scanning runs in a worker to keep Stop responsive.
  const scale = Math.min(
    1,
    1920 / Math.max(video.videoWidth, video.videoHeight),
  );
  const width = Math.max(1, Math.round(video.videoWidth * scale));
  const height = Math.max(1, Math.round(video.videoHeight * scale));
  if (scanCanvas.width !== width || scanCanvas.height !== height) {
    scanCanvas.width = width;
    scanCanvas.height = height;
  }
  scanContext.drawImage(video, 0, 0, width, height);
  const { data } = scanContext.getImageData(0, 0, width, height);
  const text = await scanner.scan(data.buffer, width, height, signal);
  if (signal.aborted || !text.startsWith("FQ1|")) return;
  try {
    if (receiver.ingest(text)) refreshReceived();
  } catch (err) {
    message("rx-status", String(err), true);
  }
}

function stopScan() {
  source.abort();
  source = new AbortController();
  video.pause();
  video.srcObject = null;
  video.removeAttribute("src");
  video.load();
  $("rx-preview").dataset.ready = "false";
  stream?.getTracks().forEach((track) => track.stop());
  stream = null;
  if (videoUrl) URL.revokeObjectURL(videoUrl);
  videoUrl = null;
  $("stop").disabled = true;
}
$("stop").addEventListener("click", () => {
  stopScan();
  message("rx-status", "Capture stopped.");
});
$("clear").addEventListener("click", () => {
  stopScan();
  receiver.free();
  receiver = new QrxReceiver();
  refreshReceived();
  message("rx-status", "Received transfers cleared.");
});

$("camera").addEventListener("click", async () => {
  stopScan();
  const signal = source.signal;
  if (!navigator.mediaDevices?.getUserMedia) {
    message(
      "rx-status",
      "Camera requires HTTPS or localhost and a compatible browser.",
      true,
    );
    return;
  }
  $("stop").disabled = false;
  try {
    const acquired = await navigator.mediaDevices.getUserMedia({
      video: { facingMode: { ideal: "environment" } },
      audio: false,
    });
    if (signal.aborted) {
      acquired.getTracks().forEach((track) => track.stop());
      return;
    }
    stream = acquired;
    video.srcObject = stream;
    video.removeAttribute("controls");
    await video.play();
    while (!signal.aborted) {
      await scanFrame(signal);
      if (!signal.aborted)
        message(
          "rx-status",
          `Scanning camera locally · ${receiver.file_count()} transfer(s)`,
        );
      await delay(30, signal);
    }
  } catch (err) {
    if (!signal.aborted) {
      stopScan();
      message("rx-status", `Camera stopped: ${err}`, true);
    }
  }
});

$("receive-file").addEventListener("change", async (event) => {
  stopScan();
  const file = event.target.files?.[0];
  event.target.value = ""; // Allow retrying the same recording.
  if (!file) return;
  const signal = source.signal;
  $("stop").disabled = false;
  videoUrl = URL.createObjectURL(file);
  video.src = videoUrl;
  video.removeAttribute("controls");
  message("rx-status", "Opening local recording…");
  try {
    await waitForVideoFrame(video, signal);
    // Some external MediaRecorder WebMs omit duration. Seek to the end to discover it.
    if (!Number.isFinite(video.duration)) {
      video.currentTime = 1e10;
      await waitForVideoFrame(video, signal);
    }
    const duration = video.duration;
    if (!Number.isFinite(duration) || duration <= 0)
      throw new Error("Cannot determine recording duration");
    // Decode by seeking rather than playing: slow scanners never skip time samples.
    // At 60 samples/sec, every frame in a typical <=30 FPS recording is visited.
    const samples = Math.max(1, Math.ceil(duration * 60));
    for (let index = 0; index < samples; index++) {
      signal.throwIfAborted();
      const time = Math.min(index / 60, Math.max(0, duration - 0.001));
      if (Math.abs(video.currentTime - time) > 0.0001) {
        video.currentTime = time;
        await waitForVideoFrame(video, signal);
      }
      await scanFrame(signal);
      if (!signal.aborted)
        message(
          "rx-status",
          `Scanning local video · ${Math.round(((index + 1) / samples) * 100)}% · ${receiver.file_count()} transfer(s)`,
        );
    }
    if (!signal.aborted) {
      const complete = Array.from({ length: receiver.file_count() }, (_, i) =>
        receiver.is_complete(i),
      ).filter(Boolean).length;
      message(
        "rx-status",
        `Scan complete · ${complete} verified / ${receiver.file_count()} transfer(s).${complete < receiver.file_count() ? " Missing frames: try another recording." : complete === 0 ? " No FQ1 QR codes found." : ""}`,
      );
    }
  } catch (err) {
    if (!signal.aborted)
      message("rx-status", `Cannot scan video: ${err}`, true);
  } finally {
    if (!signal.aborted) $("stop").disabled = true;
  }
});

video.addEventListener("loadeddata", () => {
  $("rx-preview").dataset.ready = "true";
});

async function start() {
  try {
    await init();
    receiver = new QrxReceiver();
    outContext.fillStyle = "#fff";
    outContext.fillRect(0, 0, outCanvas.width, outCanvas.height);
    $("send-file").disabled =
      $("receive-file").disabled =
      $("clear").disabled =
        false;
    $("camera").disabled = !navigator.mediaDevices?.getUserMedia;
    $("camera-hint").textContent = window.isSecureContext
      ? ""
      : "On a LAN HTTP address, file encoding and recorded-video decoding work. Camera and offline reload require a trusted HTTPS address.";
    message("tx-status", "Ready. Select a file to generate QR frames.");
    if (
      import.meta.env.PROD &&
      "serviceWorker" in navigator &&
      window.isSecureContext
    ) {
      message("cache-status", "Preparing this app for offline use…");
      registerSW({
        onOfflineReady() {
          message(
            "cache-status",
            "Ready offline · app and WASM cached on this device",
          );
        },
        onNeedRefresh() {
          message(
            "cache-status",
            "Update cached · close all QRX tabs and reopen to apply it",
          );
        },
        onRegisterError(error) {
          message("cache-status", `Offline setup failed: ${error}`, true);
        },
      });
      // Existing installations may already be active when the app starts.
      navigator.serviceWorker.ready.then(() =>
        message(
          "cache-status",
          "Ready offline · app and WASM cached on this device",
        ),
      );
    } else {
      message(
        "cache-status",
        import.meta.env.DEV
          ? "Development build · use npm run build and npm run serve for caching"
          : "Runs locally · WASM cached by your browser · HTTPS enables offline reload",
      );
    }
  } catch (err) {
    message("tx-status", `Cannot load WebAssembly: ${err}`, true);
    message("rx-status", `Cannot load WebAssembly: ${err}`, true);
    message("cache-status", "App could not start. Reload to retry.", true);
  }
}
start();
