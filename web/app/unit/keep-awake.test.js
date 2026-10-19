import { test } from "node:test";
import assert from "node:assert/strict";
import { KeepAwake } from "../src/keep-awake.js";

class Lock extends EventTarget {
  released = false;
  async release() {
    this.released = true;
    this.dispatchEvent(new Event("release"));
  }
}

function setup(t, requestNative) {
  const page = new EventTarget();
  page.visibilityState = "visible";
  page.defaultView = new EventTarget();
  const states = [];
  const locks = [];
  const fallback = {
    playing: false,
    async start() {
      this.playing = true;
    },
    stop() {
      this.playing = false;
    },
  };
  const controller = new KeepAwake({
    page,
    requestNative:
      requestNative === undefined
        ? async () => {
            const lock = new Lock();
            locks.push(lock);
            return lock;
          }
        : requestNative,
    fallback,
    report: (state) => states.push(state),
  });
  t.after(() => controller.setEnabled(false));
  return { controller, page, fallback, states, locks };
}

test("toggle acquires and releases the native screen lock without duplicate requests", async (t) => {
  const { controller, locks, states } = setup(t);
  await controller.setEnabled(true);
  await controller.ensure();
  assert.equal(locks.length, 1);
  assert.equal(states.at(-1), "native");
  await controller.setEnabled(false);
  assert.equal(locks[0].released, true);
  assert.equal(states.at(-1), "off");
});

test("a late wake-lock grant after Off is released, including rapid Off/On changes", async (t) => {
  let grant;
  const { controller, states } = setup(
    t,
    () =>
      new Promise((resolve) => {
        grant = resolve;
      }),
  );
  const pending = controller.setEnabled(true);
  await controller.setEnabled(false);
  const lock = new Lock();
  grant(lock);
  await pending;
  assert.equal(lock.released, true);
  assert.equal(states.at(-1), "off");
});

test("a device release retries automatically and Off stops all retry timers", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { controller, locks, states } = setup(t);
  await controller.setEnabled(true);
  await locks[0].release();
  assert.equal(states.at(-1), "retrying");
  t.mock.timers.tick(1000);
  await controller.ensure();
  assert.equal(locks.length, 2);
  assert.equal(states.at(-1), "native");
  await controller.setEnabled(false);
  t.mock.timers.tick(3_600_000);
  assert.equal(locks.length, 2);
});

test("hidden tabs release protection and reacquire it on return", async (t) => {
  const { controller, page, locks, states } = setup(t);
  await controller.setEnabled(true);
  page.visibilityState = "hidden";
  page.dispatchEvent(new Event("visibilitychange"));
  assert.equal(locks[0].released, true);
  assert.equal(states.at(-1), "waiting");
  page.visibilityState = "visible";
  page.dispatchEvent(new Event("visibilitychange"));
  await controller.ensure();
  assert.equal(locks.length, 2);
  assert.equal(states.at(-1), "native");
});

test("HTTP and native denials use local video, and a later retry upgrades to a native lock", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  let denied = true;
  const { controller, fallback, states } = setup(t, async () => {
    if (denied) throw new Error("Device policy denied wake lock");
    return new Lock();
  });
  await controller.setEnabled(true);
  assert.equal(states.at(-1), "fallback");
  assert.equal(fallback.playing, true);
  denied = false;
  t.mock.timers.tick(30_000);
  await controller.ensure();
  assert.equal(states.at(-1), "native");
  assert.equal(fallback.playing, false);
});

test("an interrupted fallback retries, while rejected playback is reported honestly", async (t) => {
  t.mock.timers.enable({ apis: ["setTimeout"] });
  const { controller, fallback, states } = setup(t, null);
  await controller.setEnabled(true);
  fallback.stop();
  controller.interrupted();
  t.mock.timers.tick(1000);
  await controller.ensure();
  assert.equal(fallback.playing, true);
  assert.equal(states.at(-1), "fallback");
  await controller.setEnabled(false);
  fallback.start = async () => {
    throw new Error("Autoplay blocked");
  };
  await controller.setEnabled(true);
  assert.equal(states.at(-1), "blocked");
});

test("pagehide releases resources and pageshow restores the requested lock", async (t) => {
  const { controller, page, locks } = setup(t);
  await controller.setEnabled(true);
  page.defaultView.dispatchEvent(new Event("pagehide"));
  assert.equal(locks[0].released, true);
  page.defaultView.dispatchEvent(new Event("pageshow"));
  await controller.ensure();
  assert.equal(locks.length, 2);
});

