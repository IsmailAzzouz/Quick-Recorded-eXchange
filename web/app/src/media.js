import WebMWriter from "webm-writer";

export function downloadBlob(blob, name) {
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = name;
  link.hidden = true;
  document.body.append(link);
  try { link.click(); } finally { link.remove(); }
  setTimeout(() => URL.revokeObjectURL(url), 30_000);
}

// Readiness, decoder errors, and explicit cancellation determine completion.
// Wall-clock time is irrelevant: background tabs and locked devices can freeze
// media decoding for an arbitrary period without invalidating the recording.
export function waitForVideoFrame(element, signal) {
  return new Promise((resolve, reject) => {
    const events = ["loadeddata", "canplay", "seeked", "progress"];
    const page = element.ownerDocument;
    const view = page?.defaultView;
    const cleanup = () => {
      for (const name of events) element.removeEventListener(name, check);
      element.removeEventListener("error", failed);
      page?.removeEventListener("visibilitychange", check);
      page?.removeEventListener("resume", check);
      view?.removeEventListener("pageshow", check);
      signal.removeEventListener("abort", aborted);
    };
    const check = () => {
      if (signal.aborted) {
        aborted();
        return;
      }
      if (element.error) {
        failed();
        return;
      }
      if (!element.seeking && element.readyState >= 2) {
        cleanup();
        resolve();
      }
    };
    const failed = () => {
      cleanup();
      reject(
        new Error(element.error?.message || "Unsupported or damaged video"),
      );
    };
    const aborted = () => {
      cleanup();
      reject(new DOMException("Cancelled", "AbortError"));
    };
    for (const name of events) element.addEventListener(name, check);
    element.addEventListener("error", failed, { once: true });
    page?.addEventListener("visibilitychange", check);
    page?.addEventListener("resume", check);
    view?.addEventListener("pageshow", check);
    signal.addEventListener("abort", aborted, { once: true });
    // Also handles a seek/load that finished before event listeners were added,
    // or whose event was suppressed while the page was frozen.
    check();
  });
}

export function delay(ms, signal) {
  return new Promise((resolve, reject) => {
    const aborted = () => {
      clearTimeout(timer);
      reject(new DOMException("Cancelled", "AbortError"));
    };
    const timer = setTimeout(() => {
      signal.removeEventListener("abort", aborted);
      resolve();
    }, ms);
    signal.addEventListener("abort", aborted, { once: true });
    if (signal.aborted) aborted();
  });
}

// Encode actual canvas pixels as VP8 keyframes. Live captureStream/MediaRecorder
// can return black frames on some browsers and requires rendering-time pacing.
// Fixed timestamps make this independent of display, visibility, and wall time.
export async function recordQrVideo(canvas, count, fps, draw, signal) {
  signal.throwIfAborted();
  if (!count) throw new Error("Select a file before creating a video");
  const probe = canvas.ownerDocument.createElement("canvas");
  probe.width = probe.height = 1;
  if (!probe.toDataURL("image/webp", 0.95).startsWith("data:image/webp;"))
    throw new Error("This browser cannot encode WebP frames. Create the video in Chrome or Firefox.");
  const writer = new WebMWriter({ quality: 0.95, frameDuration: Math.max(1000 / fps, 100) });
  const maxSize = 128 * 1024 * 1024;
  for (let index = 0; index < count; index++) {
    signal.throwIfAborted();
    draw(index);
    writer.addFrame(canvas, Math.max(1000 / fps, 100) +
      (index === 0 ? 150 : 0) + (index === count - 1 ? 150 : 0));
    if (writer.getWrittenSize() > maxSize)
      throw new Error("Video exceeds the 128 MiB export limit; use a smaller file");
    // Yield between frames for progress and cancellation. A suspended tab can
    // resume this work hours later without gaps or timeouts in the saved video.
    await delay(0, signal);
  }
  const blob = await writer.complete();
  signal.throwIfAborted();
  if (blob.size > maxSize)
    throw new Error("Video exceeds the 128 MiB export limit; use a smaller file");
  return { blob, extension: "webm" };
}
