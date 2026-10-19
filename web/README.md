# QRX browser MVP

Encode a file into animated FQ1 QR codes or a downloadable QR video, then recover
it from a camera or a local recording. Rust/WASM uses the desktop app's actual
`src/payload.rs` for packet encoding, reassembly, and SHA-256 verification.
The QR scanner and video encoder also run in the browser. Files are never uploaded.

## Build and run on the LAN

Build prerequisites: Rust 1.85+, Node.js 22+, and wasm-pack. Install the build tools once:

```sh
rustup target add wasm32-unknown-unknown
cargo install wasm-pack --locked
```

From the repository root:

```sh
cd web/app
npm ci
npm run build
npm run serve
```

Open `http://localhost:8787` on this machine, or `http://<this-machine-LAN-IP>:8787`
on a device connected to the same local network. The command prints available
addresses. Port 8787 must be reachable through the host firewall. Change it with
`npm run serve -- --port 9000`. The server binds to `0.0.0.0`; pass
`--host 127.0.0.1` to restrict it to this machine.

After building, **only Node.js and `web/app/dist/` are needed to serve the app**
(with `scripts/serve.mjs`). Rust and native FFmpeg/ZBar are build/desktop tools,
not browser runtime dependencies. Any static host can also serve `dist/`.
The bundled Node server accepts only GET/HEAD and serves only files inside `dist/`.
There are no payload, encoding, decoding, or upload endpoints.

For development: `npm run build:wasm`, then `npm run dev`. The development server
is accessible on the LAN, but offline caching is enabled only in production builds.

## Use

1. Choose **Encode** and select a file. **Encoding settings** contains chunk size,
   QR error correction, and playback FPS. Start with small files: this is an optical
   channel, so a large file creates many thousands of QR symbols.
2. **Play** loops the sequence for another device's camera. Previous/Next inspect
   individual symbols. **Create QR video** encodes the actual canvas pixels into
   a seekable WebM, entirely locally. It does not record a live canvas stream or
   wait for real-time playback. Each symbol lasts at least 100 ms in the video,
   with another 150 ms on the first and last frames. Output is limited to 128 MiB.
3. Video creation yields between frames for progress and cancellation. You can
   switch modes during export; a hidden canvas still produces the same pixels.
   Suspension pauses the computation, with no deadline and no change to the video's
   duration. The video downloads automatically; **Save video** stays available to
   save again if the browser blocked automatic download. Creating another video
   or selecting another input releases the previous video from memory.
4. Choose **Receive** to open a recording or start your camera. Only the selected
   workspace is shown; switching keeps your in-tab work. Decoding seeks at 60 time
   samples per second and waits for each scan, so slower scanning does not skip
   time samples. A worker performs QR detection without blocking page controls.
   Stop cancels the current scan; select the same recording again to retry.
5. Download becomes available only after the Rust reassembler verifies SHA-256.
   **Clear received files** stops scanning and frees the buffered WASM receiver.

**Dark mode** reverses the neutral interface palette and remembers your preference.
The QR pixels remain black on white in either theme. Video creation requires canvas
WebP encoding (Chrome and Firefox); an unsupported browser reports this explicitly.

Recorded-video decoding has **no elapsed-time limit**, including individual load,
seek, and QR-worker operations. An hour-long scan or a long pause does not cause a
timeout. **Stop** cancels pending operations immediately; actual media/worker errors
are still reported. Readiness is checked again when a suspended page resumes.

With the screen off, the browser/OS may suspend the tab, so continuous background
execution cannot be guaranteed. Decoding remains pending and continues when the
tab can run again. Keep the tab open: reload, tab discard, or process termination
loses the in-memory job. See [browser page lifecycle](https://developer.chrome.com/docs/web-platform/page-lifecycle-api).

## Keep screen on

Enable the **Keep screen on** switch before a long transfer. The preference is
remembered locally; turn it off to release protection and restore normal screen
sleep. The switch means protection is requested; the status underneath reports
whether it is actually active.

- On trusted HTTPS or localhost, QRX requests a native screen wake lock. It handles
  system releases, tab visibility, page restoration, and retries with backoff.
- On LAN HTTP or when the native API is unavailable/refused, QRX tries a small,
  silent local-video fallback using the MIT-licensed NoSleep.js media assets.
  This fallback's effect on screen sleep varies by device/browser. It is labelled
  as a fallback, not a confirmed native lock. Interrupted playback is restarted;
  later retries can upgrade to a native lock.
- If both methods are blocked, the app reports that protection is unavailable and
  offers **Retry keep screen on**. It never claims protection based only on a
  checked checkbox. Returning to the tab restores the requested protection.

There are no external media requests, reload loops, fake input, or server-side
processing. The fallback is bundled with the app and works offline. Keeping the
screen awake consumes battery. Browsers and the OS can still override/revoke wake
locks, and no web page can prevent a physical lock, shutdown, browser termination,
or tab discard. See [Screen Wake Lock API](https://developer.mozilla.org/en-US/docs/Web/API/Screen_Wake_Lock_API).

## Caching and HTTPS

The WASM, JavaScript, scanner worker, and CSS have content-hashed filenames. The
bundled server serves them with `Cache-Control: public, max-age=31536000, immutable`.
The browser downloads a version once and normally reuses its HTTP cache on later
visits, including LAN HTTP. HTML and service-worker files are revalidated so a
rebuild can deliver updated code. Browsers can evict storage; caching is not a
permanent-storage guarantee.

For **offline reloads and camera access on other devices**, browsers require a
trusted HTTPS origin. `localhost` is also accepted as a secure context, but a
plain `http://192.168.x.x` address is not. File encoding, video export, and recorded
video decoding still work over LAN HTTP. The UI displays which mode is available.
See [browser secure contexts](https://developer.mozilla.org/en-US/docs/Web/Security/Defenses/Secure_Contexts).

To enable LAN HTTPS, supply a certificate trusted by each receiving device,
covering the host's LAN IP or hostname (for example, one issued by your local CA):

```sh
npm run serve -- --port 8787 --cert /path/to/lan-cert.pem --key /path/to/lan-key.pem
```

Then open `https://<certificate-host-or-IP>:8787`. Merely bypassing a self-signed
certificate warning is not a reliable substitute for installing a trusted CA on
the client. Certificates and private keys should stay outside the built site.

On a secure origin the generated service worker precaches the **entire versioned
application, including WASM and the QR scanner worker, during the first visit**.
Wait for **Ready offline** before disconnecting. It caches only bundled app assets;
selected files, recordings, and recovered payloads remain in tab memory. Close all
QRX tabs and reopen to activate an available update without interrupting a transfer.
Do not use a hard reload or clear site data when expecting offline availability.

## Validation

```sh
# From the repository root: shared protocol and receiver limits.
cargo test --manifest-path web/crate/Cargo.toml --locked
cargo clippy --manifest-path web/crate/Cargo.toml --all-targets --locked
cargo fmt --manifest-path web/crate/Cargo.toml --check

# From web/app: build, then actual browser exchange / offline / LAN tests.
npm ci
npm run build
npx playwright install --with-deps chromium
npm test
```

`npm test` includes deterministic one-hour media/worker-delay tests and a browser
freeze/resume regression, alongside the file/video exchange tests.
Tests launch the production static server on port 8788. To use an existing Chromium
binary, set `PLAYWRIGHT_CHROMIUM_EXECUTABLE_PATH=/path/to/chromium`. CI runs the same
Rust tests, WASM check, production build, and browser tests.

## Limits

- 16 MiB per file, four buffered received transfers, at most 65,536 parts per file,
  128–1024 bytes per generated chunk. Smaller chunk sizes can hit the part limit
  before the byte limit. Packet text and filenames are bounded before buffering.
- Recovery requires all packets. Duplicate and out-of-order packets are supported;
  a missing packet or SHA-256 mismatch prevents download. There is no erasure coding.
- Supported recording formats depend on browser codecs. Native MKV/AVI support,
  guaranteed 4K/60 FPS recovery, native FFmpeg throughput, and a desktop-style second
  scan pass are outside this MVP. Video frames are downscaled to at most 1920 pixels
  on their longest side; very dense or blurry recordings may require desktop QRX.
- Live cameras can miss frames; loop the sender and use a modest FPS. Uploaded video
  scanning can take longer than the recording, especially on phones. Seek sampling
  visits time points and is not a guarantee for arbitrarily high-framerate footage.
- Payloads and exported video are buffered in memory. No progressive disk streaming.
- Camera capture needs a real-device test on the intended phone/camera and lighting.
  Automated tests cover file/video exchange, not optical capture hardware.

See [REVIEW.md](./REVIEW.md) for the PR design assessment and fixes.
