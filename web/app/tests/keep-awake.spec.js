import { test, expect } from "@playwright/test";
import { networkInterfaces } from "node:os";

async function openApp(page) {
  await page.goto("/");
  await expect(page.locator("#send-file")).toBeEnabled();
}

test("screen toggle acquires, reacquires, remembers preference, and releases the native lock", async ({
  page,
}) => {
  await page.addInitScript(() => {
    window.testLocks = [];
    class Lock extends EventTarget {
      released = false;
      async release() {
        this.released = true;
        this.dispatchEvent(new Event("release"));
      }
    }
    Object.defineProperty(navigator, "wakeLock", {
      value: {
        request: async () => {
          const lock = new Lock();
          window.testLocks.push(lock);
          return lock;
        },
      },
    });
  });
  await openApp(page);
  await page.getByRole("switch", { name: "Keep screen on" }).check();
  await expect(page.locator("#wake-status")).toContainText(
    "Screen wake lock active",
  );
  await page.evaluate(() => window.testLocks[0].release());
  await expect.poll(() => page.evaluate(() => window.testLocks.length)).toBe(2);
  await expect(page.locator("#wake-status")).toContainText(
    "Screen wake lock active",
  );
  await page.reload();
  await expect(
    page.getByRole("switch", { name: "Keep screen on" }),
  ).toBeChecked();
  await expect(page.locator("#wake-status")).toContainText(
    "Screen wake lock active",
  );
  await page.getByRole("switch", { name: "Keep screen on" }).uncheck();
  await expect(page.locator("#wake-status")).toContainText("device settings");
  expect(
    await page.evaluate(() => window.testLocks.every((lock) => lock.released)),
  ).toBe(true);
});

test("LAN HTTP uses the bundled video fallback and restarts interrupted playback", async ({
  page,
}) => {
  const address = Object.values(networkInterfaces())
    .flat()
    .find((entry) => entry.family === "IPv4" && !entry.internal)?.address;
  test.skip(!address, "No LAN interface available");
  await page.goto(`http://${address}:8788`);
  await expect(page.locator("#send-file")).toBeEnabled();
  await page.getByRole("switch", { name: "Keep screen on" }).check();
  await expect(page.locator("#wake-status")).toContainText(
    "Video fallback playing",
  );
  await expect
    .poll(() =>
      page
        .locator(".wake-video")
        .evaluate((video) => !video.paused && video.readyState >= 2),
    )
    .toBe(true);
  await page.locator(".wake-video").evaluate((video) => video.pause());
  await expect
    .poll(() => page.locator(".wake-video").evaluate((video) => !video.paused))
    .toBe(true);
  await expect(page.locator("#wake-status")).toContainText(
    "Video fallback playing",
  );
  await page.getByRole("switch", { name: "Keep screen on" }).uncheck();
  expect(
    await page.locator(".wake-video").evaluate((video) => video.paused),
  ).toBe(true);
  await expect(page.locator("#wake-status")).toContainText("device settings");
});

test("native refusal falls back without falsely reporting a native lock", async ({
  page,
}) => {
  await page.addInitScript(() =>
    Object.defineProperty(navigator, "wakeLock", {
      value: {
        request: async () => {
          throw new DOMException("System power policy", "NotAllowedError");
        },
      },
    }),
  );
  await openApp(page);
  await page.getByRole("switch", { name: "Keep screen on" }).check();
  await expect(page.locator("#wake-status")).toContainText(
    "Video fallback playing",
  );
  await expect(page.locator("#wake-status")).not.toContainText(
    "Screen wake lock active",
  );
});

test("direct export works hidden without canvas capture and survives an hour of delay", async ({ page }) => {
  await openApp(page);
  await page.locator("#send-file").setInputFiles({
    name: "suspended.bin", mimeType: "application/octet-stream",
    buffer: Buffer.alloc(4097, 23),
  });
  await expect(page.locator("#export")).toBeEnabled();
  await page.clock.install();
  await page.evaluate(() => {
    // Reproduce an unavailable/black live capture pipeline. Export must use pixels.
    HTMLCanvasElement.prototype.captureStream = () => { throw new Error("Live capture is broken"); };
    Object.defineProperty(document, "visibilityState", { get: () => "hidden" });
    Object.defineProperty(document, "hidden", { get: () => true });
    document.dispatchEvent(new Event("visibilitychange"));
  });
  const downloaded = page.waitForEvent("download");
  await page.locator("#export").click();
  await page.clock.fastForward(3_600_000);
  await page.clock.resume();
  const recording = await downloaded;
  await page.getByRole("tab", { name: "Receive", exact: true }).click();
  await page.locator("#receive-file").setInputFiles(await recording.path());
  await expect(page.locator("#rx-status")).toContainText("Scan complete");
  await expect(page.locator("#received")).toContainText("SHA-256 verified");
  expect(await page.locator("#incoming").evaluate(video => video.duration)).toBeLessThan(10);
});
