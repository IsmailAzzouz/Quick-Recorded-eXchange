import { test, expect } from "@playwright/test";
import { readFile, readdir } from "node:fs/promises";
import { createHash } from "node:crypto";
import { networkInterfaces } from "node:os";

async function openApp(page) {
  await page.goto("/");
  await expect(page.locator("#send-file")).toBeEnabled();
}

for (const size of [0, 4097]) {
  test(`file -> recorded QR video -> verified original (${size} bytes)`, async ({
    page,
  }, info) => {
    const errors = [];
    const requests = [];
    page.on("pageerror", (error) => errors.push(error.message));
    page.on("request", (request) =>
      requests.push({ url: request.url(), method: request.method() }),
    );
    await openApp(page);
    let seed = 0x12345678;
    const original = Buffer.from(
      Array.from({ length: size }, () => {
        seed ^= seed << 13;
        seed ^= seed >>> 17;
        seed ^= seed << 5;
        return seed & 255;
      }),
    );
    const filename = "résumé.bin";
    await page.locator(".encoding-settings summary").click();
    await page.locator("#fps").fill("30");
    await page.locator("#send-file").setInputFiles({
      name: filename,
      mimeType: "application/octet-stream",
      buffer: original,
    });
    await expect(page.locator("#export")).toBeEnabled();
    const videoDownloaded = page.waitForEvent("download");
    await page.locator("#export").click();
    const recording = await videoDownloaded;
    expect(recording.suggestedFilename()).toMatch(/\.qrx\.(webm|mp4)$/);
    const videoPath = info.outputPath(recording.suggestedFilename());
    await recording.saveAs(videoPath);
    await page.getByRole("tab", { name: "Receive", exact: true }).click();
    await page.locator("#receive-file").setInputFiles(videoPath);
    await expect(page.locator("#rx-status")).toContainText("Scan complete", {
      timeout: 100_000,
    });
    await expect(page.locator("#received")).toContainText("SHA-256 verified");
    const recoveredDownloaded = page.waitForEvent("download");
    await page.getByRole("button", { name: "Download verified file" }).click();
    const recovered = await recoveredDownloaded;
    expect(recovered.suggestedFilename()).toBe(filename);
    const actual = await readFile(await recovered.path());
    expect(actual).toEqual(original);
    expect(createHash("sha256").update(actual).digest("hex")).toBe(
      createHash("sha256").update(original).digest("hex"),
    );
    // Every network request is for static, same-origin code/assets. No payload upload.
    expect(
      requests
        .filter((request) => !request.url.startsWith("blob:"))
        .every(
          (request) =>
            request.method === "GET" &&
            new URL(request.url).origin === "http://127.0.0.1:8788",
        ),
    ).toBe(true);
    expect(errors).toEqual([]);
    await page.locator("#clear").click();
    await expect(page.locator(".received-card")).toHaveCount(0);
  });
}

test("first visit precaches WASM and scanner; reload and full exchange work offline", async ({
  page,
  context,
}) => {
  await page.addInitScript(() =>
    Object.defineProperty(navigator, "wakeLock", { value: undefined }),
  );
  await openApp(page);
  await expect(page.locator("#cache-status")).toContainText("Ready offline");
  await page.getByRole("switch", { name: "Keep screen on" }).check();
  await expect(page.locator("#wake-status")).toContainText(
    "Video fallback playing",
  );
  const cached = await page.evaluate(async () => {
    const names = await caches.keys();
    return (
      await Promise.all(
        names.map(async (name) =>
          (await (await caches.open(name)).keys()).map(
            (request) => request.url,
          ),
        ),
      )
    ).flat();
  });
  expect(cached.some((url) => url.endsWith(".wasm"))).toBe(true);
  expect(cached.some((url) => /scanner\.worker.*\.js$/.test(url))).toBe(true);
  await context.setOffline(true);
  await page.reload();
  await expect(page.locator("#send-file")).toBeEnabled();
  await expect(page.locator("#cache-status")).toContainText("Ready offline");
  await page.getByRole("switch", { name: "Keep screen on" }).uncheck();
  await page.getByRole("switch", { name: "Keep screen on" }).check();
  await expect(page.locator("#wake-status")).toContainText(
    "Video fallback playing",
  );
  await page.locator("#send-file").setInputFiles({
    name: "offline.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("offline file transfer"),
  });
  await expect(page.locator("#export")).toBeEnabled();
  const downloaded = page.waitForEvent("download");
  await page.locator("#export").click();
  const file = await downloaded;
  await page.getByRole("tab", { name: "Receive", exact: true }).click();
  await page.locator("#receive-file").setInputFiles(await file.path());
  await expect(page.locator("#received")).toContainText("SHA-256 verified");
});

test("LAN HTTP works with immutable WASM caching and no upload endpoints", async ({
  page,
  request,
}) => {
  const address = Object.values(networkInterfaces())
    .flat()
    .find((entry) => entry.family === "IPv4" && !entry.internal)?.address;
  test.skip(!address, "No LAN interface available");
  await page.goto(`http://${address}:8788`);
  await expect(page.locator("#send-file")).toBeEnabled();
  await page.getByRole("tab", { name: "Receive", exact: true }).click();
  await expect(page.locator("#camera-hint")).toContainText("trusted HTTPS");
  await expect(page.locator("#camera")).toBeDisabled();
  await page.getByRole("tab", { name: "Encode", exact: true }).click();
  await page.locator("#send-file").setInputFiles({
    name: "lan.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("LAN encoding"),
  });
  await expect(page.locator("#tx-status")).toContainText("QR 1 / 1");
  await page.reload();
  await expect(page.locator("#send-file")).toBeEnabled();
  const wasmTiming = await page.evaluate(() =>
    performance
      .getEntriesByType("resource")
      .find((entry) => entry.name.endsWith(".wasm"))
      ?.toJSON(),
  );
  expect(wasmTiming.decodedBodySize).toBeGreaterThan(0);
  expect(wasmTiming.transferSize).toBe(0);
  const wasm = (await readdir("dist/assets")).find((name) =>
    name.endsWith(".wasm"),
  );
  expect(wasm).toBeTruthy();
  const asset = await request.get(`/assets/${wasm}`);
  expect(asset.headers()["content-type"]).toBe("application/wasm");
  expect(asset.headers()["cache-control"]).toBe(
    "public, max-age=31536000, immutable",
  );
  expect(
    (
      await request.get(`/assets/${wasm}`, {
        headers: { "If-None-Match": asset.headers().etag },
      })
    ).status(),
  ).toBe(304);
  expect((await request.post("/", { data: "payload" })).status()).toBe(405);
  expect((await request.get("/src/main.js")).status()).toBe(404);
  expect((await request.get("/missing.wasm")).status()).toBe(404);
});

test("export can be cancelled and sender remains usable", async ({ page }) => {
  await openApp(page);
  await page.locator("#send-file").setInputFiles({
    name: "cancel.bin",
    mimeType: "application/octet-stream",
    buffer: Buffer.alloc(65536, 42),
  });
  await expect(page.locator("#export")).toBeEnabled();
  await page.locator("#export").click();
  await page.locator("#cancel-export").click();
  await expect(page.locator("#tx-status")).toContainText("Export cancelled");
  await expect(page.locator("#export")).toBeEnabled();
  await expect(page.locator("#send-file")).toBeEnabled();
});

test("camera capture pipeline reassembles a looping local canvas stream", async ({
  page,
}) => {
  await openApp(page);
  await page.locator(".encoding-settings summary").click();
    await page.locator("#fps").fill("3");
  await page.locator("#send-file").setInputFiles({
    name: "camera.bin",
    mimeType: "application/octet-stream",
    buffer: Buffer.alloc(100, 37),
  });
  await expect(page.locator("#play")).toBeEnabled();
  // A separate canvas models another device while Encode is hidden.
  await page.evaluate(() => {
    const canvas = document.createElement("canvas");
    canvas.width = canvas.height = 720;
    const context = canvas.getContext("2d");
    context.drawImage(document.getElementById("qr"), 0, 0);
    const stream = canvas.captureStream(0);
    window.cameraTimer = setInterval(() => stream.getVideoTracks()[0].requestFrame(), 50);
    navigator.mediaDevices.getUserMedia = async () => stream;
  });
  await page.getByRole("tab", { name: "Receive", exact: true }).click();
  await page.locator("#camera").click();
  await expect(page.locator("#received")).toContainText("SHA-256 verified");
  await page.locator("#stop").click();
  await page.evaluate(() => clearInterval(window.cameraTimer));
  await expect(page.locator("#rx-status")).toHaveText("Capture stopped.");
  expect(
    await page.locator("#incoming").evaluate((video) => video.srcObject),
  ).toBeNull();
});

test("video decoding survives a one-hour worker delay and browser freeze/resume", async ({
  page,
  context,
}) => {
  await page.addInitScript(() => {
    const RealWorker = window.Worker;
    window.Worker = class extends RealWorker {
      addEventListener(name, listener, options) {
        if (name !== "message")
          return super.addEventListener(name, listener, options);
        super.addEventListener(
          name,
          (event) => {
            if (window.holdScanReply)
              window.resumeScanReply = () => listener.call(this, event);
            else listener.call(this, event);
          },
          options,
        );
      }
    };
  });
  await openApp(page);
  await page.locator("#send-file").setInputFiles({
    name: "long-decode.txt",
    mimeType: "text/plain",
    buffer: Buffer.from("This decode survives suspension"),
  });
  await expect(page.locator("#export")).toBeEnabled();
  const downloaded = page.waitForEvent("download");
  await page.locator("#export").click();
  const recording = await downloaded;
  await page.clock.install();
  await page.evaluate(() => {
    window.holdScanReply = true;
  });
  await page.getByRole("tab", { name: "Receive", exact: true }).click();
  await page.locator("#receive-file").setInputFiles(await recording.path());
  await expect
    .poll(() => page.evaluate(() => typeof window.resumeScanReply))
    .toBe("function");
  await page.clock.fastForward(60 * 60 * 1000);
  await expect(page.locator("#stop")).toBeEnabled();
  await expect(page.locator("#rx-status")).not.toContainText(
    "Cannot scan video",
  );
  const cdp = await context.newCDPSession(page);
  await cdp.send("Page.setWebLifecycleState", { state: "frozen" });
  await cdp.send("Page.setWebLifecycleState", { state: "active" });
  await page.evaluate(() => {
    window.holdScanReply = false;
    window.resumeScanReply();
    delete window.resumeScanReply;
  });
  await expect(page.locator("#received")).toContainText("SHA-256 verified");
  await expect(page.locator("#rx-status")).toContainText("Scan complete");
});
