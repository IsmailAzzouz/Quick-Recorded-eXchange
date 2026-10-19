// Screen wake locks are revocable. Keep the user's intent separate from the
// currently held lock, serialize requests, and release late grants after Off.
export class KeepAwake {
  #page;
  #native;
  #fallback;
  #report;
  #enabled = false;
  #generation = 0;
  #lock = null;
  #pending = null;
  #retry = null;
  #retryDelay = 1000;
  #fallbackActive = false;

  constructor({ page, requestNative, fallback, report }) {
    this.#page = page;
    this.#native = requestNative;
    this.#fallback = fallback;
    this.#report = report;
    const resume = () => {
      if (this.#page.visibilityState === "visible") void this.ensure();
      else if (this.#enabled) {
        this.#clearRetry();
        this.#release();
        this.#report("waiting");
      }
    };
    page.addEventListener("visibilitychange", resume);
    page.addEventListener("resume", resume);
    page.defaultView?.addEventListener("pageshow", resume);
    page.defaultView?.addEventListener("focus", resume);
    page.defaultView?.addEventListener("pagehide", () => {
      this.#generation++;
      this.#clearRetry();
      this.#release();
    });
  }

  setEnabled(enabled) {
    this.#enabled = enabled;
    this.#generation++;
    this.#retryDelay = 1000;
    this.#clearRetry();
    if (!enabled) {
      this.#release();
      this.#report("off");
      return Promise.resolve();
    }
    return this.ensure();
  }

  ensure() {
    if (!this.#enabled) return Promise.resolve();
    if (this.#page.visibilityState !== "visible") {
      this.#report("waiting");
      return Promise.resolve();
    }
    if (this.#lock && !this.#lock.released) return Promise.resolve();
    if (this.#pending) return this.#pending;
    this.#clearRetry();
    const generation = this.#generation;
    const pending = this.#acquire(generation).finally(() => {
      this.#pending = null;
      if (this.#enabled && !this.#lock) this.#scheduleRetry();
    });
    this.#pending = pending;
    return pending;
  }

  interrupted() {
    if (!this.#enabled || this.#lock) return;
    this.#clearRetry();
    this.#retryDelay = 1000;
    this.#fallbackActive = false;
    this.#report(
      this.#page.visibilityState === "visible" ? "retrying" : "waiting",
    );
    this.#scheduleRetry();
  }

  async #acquire(generation) {
    const current = () =>
      this.#enabled &&
      generation === this.#generation &&
      this.#page.visibilityState === "visible";
    if (!this.#fallbackActive) this.#report("requesting");
    if (this.#native) {
      try {
        const lock = await this.#native();
        if (!current()) {
          await lock.release();
          return;
        }
        if (lock.released)
          throw new Error("The device released the screen lock");
        this.#lock = lock;
        lock.addEventListener(
          "release",
          () => {
            if (this.#lock !== lock) return;
            this.#lock = null;
            if (!this.#enabled) return;
            this.#report(
              this.#page.visibilityState === "visible" ? "retrying" : "waiting",
            );
            this.#scheduleRetry();
          },
          { once: true },
        );
        this.#fallbackActive = false;
        this.#fallback.stop();
        this.#retryDelay = 1000;
        this.#report("native");
        return;
      } catch {
        /* Try the bundled local-video fallback if the platform refuses. */
      }
    }
    if (!current()) return;
    try {
      await this.#fallback.start();
      if (!current()) {
        this.#fallback.stop();
        return;
      }
      this.#fallbackActive = true;
      this.#report("fallback");
    } catch (error) {
      if (!current()) return;
      this.#fallbackActive = false;
      this.#report("blocked", error);
    }
  }

  #scheduleRetry() {
    if (
      this.#retry !== null ||
      !this.#enabled ||
      this.#page.visibilityState !== "visible"
    )
      return;
    // Fallback playback is monitored too. Back off after repeated system denials.
    this.#retry = setTimeout(
      () => {
        this.#retry = null;
        void this.ensure();
      },
      this.#fallbackActive ? 30_000 : this.#retryDelay,
    );
    this.#retryDelay = Math.min(30_000, this.#retryDelay * 2);
  }

  #clearRetry() {
    clearTimeout(this.#retry);
    this.#retry = null;
  }

  #release() {
    const lock = this.#lock;
    this.#lock = null;
    if (lock) void lock.release().catch(() => {});
    this.#fallbackActive = false;
    this.#fallback.stop();
  }
}
