import { test, expect } from "@playwright/test";

async function openApp(page) {
  await page.goto("/");
  await expect(page.locator("#send-file")).toBeEnabled();
}

test("one workspace at a time, keyboard selection, and retained file state", async ({ page }) => {
  await openApp(page);
  await expect(page.locator('[role="tabpanel"]:visible')).toHaveCount(1);
  await page.locator("#send-file").setInputFiles({ name: "kept.txt", mimeType: "text/plain", buffer: Buffer.from("keep my file") });
  await expect(page.locator("#play")).toBeEnabled();
  await page.getByRole("tab", { name: "Encode", exact: true }).focus();
  await page.keyboard.press("ArrowRight");
  await expect(page.getByRole("tab", { name: "Receive", exact: true })).toBeFocused();
  await expect(page.locator("#receive-panel")).toBeVisible();
  await expect(page.locator("#encode-panel")).toBeHidden();
  await expect(page.locator('[role="tabpanel"]:visible')).toHaveCount(1);
  await page.keyboard.press("Home");
  await expect(page.locator("#encode-panel")).toBeVisible();
  await expect(page.locator("#tx-status")).toContainText("QR 1 / 1");
  await expect(page.locator("#play")).toBeEnabled();
});

test("black and white theme reverses, remembers preference, and preserves QR pixels", async ({ page }) => {
  await page.emulateMedia({ colorScheme: "light" });
  await openApp(page);
  const background = () => page.locator("html").evaluate(el => getComputedStyle(el).backgroundColor);
  expect(await background()).toBe("rgb(255, 255, 255)");
  await page.locator("#send-file").setInputFiles({ name: "theme.txt", mimeType: "text/plain", buffer: Buffer.from("same QR pixels") });
  await expect(page.locator("#play")).toBeEnabled();
  const pixels = () => page.locator("#qr").evaluate(canvas => canvas.toDataURL());
  const original = await pixels();
  await page.getByRole("switch", { name: "Dark mode", exact: true }).check();
  expect(await background()).toBe("rgb(17, 17, 17)");
  expect(await pixels()).toBe(original);
  await page.reload();
  await expect(page.getByRole("switch", { name: "Dark mode", exact: true })).toBeChecked();
  expect(await background()).toBe("rgb(17, 17, 17)");
  await page.getByRole("switch", { name: "Dark mode", exact: true }).uncheck();
  expect(await background()).toBe("rgb(255, 255, 255)");
});

test("manual Save video works when automatic download is blocked, including export in hidden Encode", async ({ page }) => {
  await page.addInitScript(() => { HTMLAnchorElement.prototype.click = () => {}; });
  await openApp(page);
  await page.locator("#send-file").setInputFiles({ name: "hidden.bin", mimeType: "application/octet-stream", buffer: Buffer.alloc(4097, 73) });
  await expect(page.locator("#export")).toBeEnabled();
  await page.evaluate(() => {
    document.getElementById("export").click();
    document.getElementById("mode-receive").click();
  });
  await expect(page.locator("#encode-panel")).toBeHidden();
  await page.getByRole("tab", { name: "Encode", exact: true }).click();
  await expect(page.locator("#video-ready")).toBeVisible();
  const downloaded = page.waitForEvent("download");
  await page.getByRole("link", { name: "Save video", exact: true }).click();
  const recording = await downloaded;
  expect(recording.suggestedFilename()).toBe("hidden.bin.qrx.webm");
  await page.getByRole("tab", { name: "Receive", exact: true }).click();
  await page.locator("#receive-file").setInputFiles(await recording.path());
  await expect(page.locator("#received")).toContainText("SHA-256 verified");
  await expect(page.locator("#rx-status")).toContainText("Scan complete");
});

for (const width of [320, 375, 768, 1440]) {
  test(`workspace fits ${width}px without horizontal scrolling`, async ({ page }) => {
    await page.setViewportSize({ width, height: 900 });
    await openApp(page);
    for (const name of ["Encode", "Receive"]) {
      await page.getByRole("tab", { name, exact: true }).click();
      expect(await page.evaluate(() => document.documentElement.scrollWidth <= innerWidth)).toBe(true);
    }
  });
}
