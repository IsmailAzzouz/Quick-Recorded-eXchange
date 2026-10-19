import { test } from "node:test";
import assert from "node:assert/strict";
import { waitForVideoFrame } from "../src/media.js";
import { FrameScanner } from "../src/scanner.js";

function videoState({ readyState = 0, seeking = false } = {}) {
  const video = new EventTarget();
  video.readyState = readyState;
  video.seeking = seeking;
  video.error = null;
  video.ownerDocument = new EventTarget();
  video.ownerDocument.defaultView = new EventTarget();
  return video;
}

async function remainsPendingForAnHour(t, pending) {
  let settled = false;
  pending.then(
    () => {
      settled = true;
    },
    () => {
      settled = true;
    },
  );
  t.mock.timers.tick(60 * 60 * 1000);
  await Promise.resolve();
  assert.equal(settled, false, "elapsed time must not end decoding");
}

for (const seeking of [false, true]) {
  test(`${seeking ? "seeking" : "loading"} waits past an hour, then completes`, async (t) => {
    t.mock.timers.enable({ apis: ["setTimeout"] });
    const video = videoState({ seeking, readyState: seeking ? 2 : 0 });
    const pending = waitForVideoFrame(video, new AbortController().signal);
    await remainsPendingForAnHour(t, pending);
    video.seeking = false;
    video.readyState = 2;
    video.dispatchEvent(new Event(seeking ? "seeked" : "loadeddata"));
    await pending;
  });
}

test("an already completed seek needs no further media event", async () => {
  await waitForVideoFrame(
    videoState({ readyState: 2 }),
    new AbortController().signal,
  );
});

for (const event of ["visibilitychange", "resume", "pageshow"]) {
  test(`${event} recovers readiness when a media event was missed during suspension`, async () => {
    const video = videoState({ readyState: 2, seeking: true });
    const pending = waitForVideoFrame(video, new AbortController().signal);
    video.seeking = false;
    const target =
      event === "pageshow"
        ? video.ownerDocument.defaultView
        : video.ownerDocument;
    target.dispatchEvent(new Event(event));
    await pending;
  });
}

test("Stop cancels a stalled video immediately, even after an hour", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const controller = new AbortController();
  const pending = waitForVideoFrame(videoState(), controller.signal);
  await remainsPendingForAnHour(t, pending);
  const rejected = assert.rejects(pending, { name: "AbortError" });
  controller.abort();
  await rejected;
});

test("a real decoder error still fails, including an already reported error", async () => {
  const video = videoState();
  const pending = waitForVideoFrame(video, new AbortController().signal);
  const rejected = assert.rejects(pending, /damaged recording/);
  video.error = { message: "damaged recording" };
  video.dispatchEvent(new Event("error"));
  await rejected;
  await assert.rejects(
    waitForVideoFrame(video, new AbortController().signal),
    /damaged recording/,
  );
});

class FakeWorker extends EventTarget {
  requests = [];
  postMessage(request) {
    this.requests.push(request);
  }
  reply(text, id = this.requests.at(-1).id) {
    this.dispatchEvent(new MessageEvent("message", { data: { id, text } }));
  }
}

test("a worker reply remains valid after an hour of suspension", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const worker = new FakeWorker();
  const scanner = new FrameScanner(worker);
  const pending = scanner.scan(
    new ArrayBuffer(4),
    1,
    1,
    new AbortController().signal,
  );
  await remainsPendingForAnHour(t, pending);
  worker.reply("FQ1|packet");
  assert.equal(await pending, "FQ1|packet");
});

test("Stop cancels a suspended worker request and a late reply cannot affect the next scan", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const worker = new FakeWorker();
  const scanner = new FrameScanner(worker);
  const controller = new AbortController();
  const pending = scanner.scan(new ArrayBuffer(4), 1, 1, controller.signal);
  await remainsPendingForAnHour(t, pending);
  const rejected = assert.rejects(pending, { name: "AbortError" });
  controller.abort();
  await rejected;
  const next = scanner.scan(
    new ArrayBuffer(4),
    1,
    1,
    new AbortController().signal,
  );
  worker.reply("stale packet", worker.requests[0].id);
  worker.reply("current packet");
  assert.equal(await next, "current packet");
});

test("worker failures reject pending and future scans instead of hanging", async () => {
  const worker = new FakeWorker();
  const scanner = new FrameScanner(worker);
  const pending = scanner.scan(
    new ArrayBuffer(4),
    1,
    1,
    new AbortController().signal,
  );
  const rejected = assert.rejects(pending, /QR scanner failed/);
  worker.dispatchEvent(new Event("error"));
  await rejected;
  await assert.rejects(
    scanner.scan(new ArrayBuffer(4), 1, 1, new AbortController().signal),
    /QR scanner failed/,
  );
});
