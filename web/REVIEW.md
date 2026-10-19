# PR #1 design review

Reviewed base: `20a3834` on `feat/browser-wasm-mvp`.

The overall split is appropriate for an MVP: a small WASM crate reuses the exact
FQ1 protocol and SHA-256 implementation used by the desktop app; the browser owns
files, camera/video, canvas rendering, and downloads. jsQR is bundled locally.
Neither a backend codec nor a separate JavaScript implementation of FQ1 is needed.
The `#[path]` reuse is pragmatic for now; extracting a shared protocol crate would
be a useful later refactor if more clients or FQ1 versions appear.

## Issues corrected on this branch

| Priority | Original issue | Result |
| --- | --- | --- |
| P1 | The production Vite build fails because top-level `await` is outside the default build target. | Initialization runs inside an async entry function; production WASM/Vite build verified. |
| P1 | The server binds only to 127.0.0.1 and no production LAN serving command exists. | LAN dev/preview binding and a static-only GET/HEAD production server with WASM MIME and cache headers. |
| P1 | Service worker installation only caches HTML/manifest. First-load JS and WASM may load before worker control, preventing offline reload. | Build-generated precache explicitly includes content-hashed WASM, JS, CSS, HTML, and scanner worker. First-visit/offline-reload browser test. |
| P1 | Fixed cache-first names keep serving stale HTML/code across releases and activation deletes unrelated origin caches. | Revision-aware Workbox precache with QRX cache identity; app updates activate after existing tabs close. No broad cache deletion. |
| P1 | Video decoding runs at playback speed, skipping frames whenever scanning is slow. | Paused video seek sampling waits for scans; QR detection executes in a worker. |
| P2 | Only live QR display is provided, so users cannot encode a file into a downloadable recording in the browser. | Direct VP8 frame encoding and seekable WebM muxing, fixed frame durations, cancellation, and output bound. Actual file/video/file tests. |
| P2 | Duplicate packets at the 16 MiB byte limit incorrectly fail the size check. | Metadata and duplicate checks precede accounting; exact-byte-limit regression test. |
| P2 | Receiver clearing replaces a wasm-bindgen object without freeing it. | Explicit `free()` and cancellation before reset. |
| P2 | Asynchronous file reads/media changes can apply stale results, and early controls can run before WASM is ready. | Sender generation check, receiver abort signals, startup-disabled controls, and media/worker errors handled. |
| P2 | Packet and filename bounds are applied after potentially large parsing/allocation. Sender part count is checked after generating all packets. | Text/filename bounds and sender part-count preflight. |
| P2 | No npm or web Rust lockfile; CI builds but does not exercise a browser. | Both lockfiles added, npm ci, Rust limits/integrity tests, and Playwright production tests in CI. |

## Remaining design limits

The browser buffers packet strings and complete payloads; its limits make this
acceptable for a small-file MVP, not a large-file streaming architecture. Video
export also uses memory; direct frame encoding avoids live-capture black frames and real-time recording. Seek-based decode improves
reliability but depends on browser codecs and is slower than native decoding.
Main-thread QR generation can pause the UI for dense symbols; scanning is already
isolated in a worker. Camera reception still needs real hardware validation.

The local static server does no file processing. Its immutable cache headers cover
versioned assets; reliable offline app reload and camera access over the LAN require
trusted HTTPS due to browser policy. LAN HTTP supports the file/video workflow.
No server can promise that a browser will never evict an HTTP or service-worker cache.

Future work should follow measured needs: a shared protocol crate, incremental
packet generation/assembly, then better optical
recovery or erasure coding. None requires introducing payload uploads.

## Local validation results

- Production Rust/WASM + Vite build passed. WASM is approximately 106 kB; the
  generated precache includes all application assets and fallback-video data.
- Eight web-core Rust tests passed, including reordered/duplicate packets,
  corrupted payloads, metadata/resource limits, and duplicates at exactly 16 MiB.
  Web-core Clippy and formatting checks passed (desktop-only shared helper warnings).
- Eighteen Chromium browser tests passed: empty and binary file/video/file roundtrips,
  offline reload plus exchange, LAN HTTP with cached WASM reuse and static-only
  endpoints, export cancellation, and camera reception from a simulated canvas
  stream, and a simulated one-hour worker delay with browser freeze/resume.
  Workspace tests cover isolated modes, keyboard controls, retained file state,
  reversible themes, manual video saves, hidden-panel export, and 320–1440px layouts.
  Wake-lock tests cover native lock releases/preferences, actual local-video
  playback on LAN HTTP, fallback after native refusal, and a one-hour hidden export
  followed by verified decoding with live canvas capture deliberately broken.
  Eighteen JavaScript runtime tests cover hour-long loads/seeks/worker requests,
  readiness on resume, explicit cancellation, late replies, real failures, screen
  lock lifecycle/races, and retry backoff.
  Physical camera performance remains untested.
- Eight native Rust tests passed; one optional dense-video fixture test is ignored.
  Native tests used FFmpeg and a ZBar library extracted under `/tmp/qrx-native-deps`
  via `QR_VIDEO_ZBAR_DLL`, without installing system packages.
- A separate desktop Rust/FFmpeg encoding -> browser video decoding check recovered
  the exact original 1,025 bytes. The page fits a 375-pixel mobile viewport.
- FFmpeg independently decoded the exported VP8/WebM: eleven 720×720 QR frames,
  duration 1.4 seconds, with a visibly correct QR image rather than black pixels.
- npm audit reported zero vulnerabilities. Diff whitespace checks passed.
- Root `cargo fmt --check` flags pre-existing formatting in `src/decoder.rs` and
  `src/video_encoder.rs`; those unrelated files were left unchanged.
