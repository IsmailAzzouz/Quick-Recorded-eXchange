// A worker request has no elapsed-time deadline. It stays pending through tab
// suspension, and ends only on a reply, an actual worker failure, or cancellation.
export class FrameScanner {
  #worker;
  #nextId = 0;
  #pending = new Map();
  #failure;

  constructor(worker) {
    this.#worker = worker;
    worker.addEventListener("message", ({ data }) => {
      const finish = this.#pending.get(data.id);
      if (finish) finish(data.error ? new Error(data.error) : null, data.text);
    });
    const failed = (event) => {
      this.#failure = new Error(event.message || "QR scanner failed");
      for (const finish of this.#pending.values()) finish(this.#failure);
    };
    worker.addEventListener("error", failed);
    worker.addEventListener("messageerror", failed);
  }

  scan(pixels, width, height, signal) {
    signal.throwIfAborted();
    if (this.#failure) return Promise.reject(this.#failure);
    const id = ++this.#nextId;
    return new Promise((resolve, reject) => {
      const finish = (error, text) => {
        this.#pending.delete(id);
        signal.removeEventListener("abort", aborted);
        if (error) reject(error);
        else resolve(text);
      };
      const aborted = () => finish(new DOMException("Cancelled", "AbortError"));
      this.#pending.set(id, finish);
      signal.addEventListener("abort", aborted, { once: true });
      try {
        this.#worker.postMessage({ id, pixels, width, height }, [pixels]);
      } catch (error) {
        finish(error);
      }
    });
  }
}
