# QRX in the browser (WASM MVP)

A fully client-side proof of concept built on the **desktop project's actual
`src/payload.rs` FQ1 packet format**, Rust + WebAssembly for packet encoding,
SHA-256 verification and QR generation, and browser-native camera/video/file
APIs for I/O. The QR scanner (`jsqr`) is bundled by Vite: nothing is
uploaded to a server.

## Run locally

Prerequisites: Rust 1.85+, `rustup target add wasm32-unknown-unknown`,
[wasm-pack](https://rustwasm.github.io/wasm-pack/), Node.js 20+.

```sh
cd web/app
npm install
npm run build:wasm
npm run dev
```

Open the URL printed by Vite (localhost is a secure context for camera access).

Production static site:

```sh
cd web/app
npm run build
# Upload web/app/dist/ to any HTTPS static host.
```

No API keys, servers, or external CDN dependencies at runtime. The optional
service worker caches assets after the first visit, enabling offline revisits
when installed assets have been cached.

## Included

- Browser file picker -> FQ1 packets -> animated QR canvas (128–1024 bytes per
  frame, error-correction selector, configurable playback speed).
- Camera QR reception (HTTPS or localhost) and locally selected browser-
  supported video files, scanned frame-by-frame in the page.
- Out-of-order FQ1 reassembly, duplicate suppression, SHA-256 verification,
  and safe manual download of completed files.
- Protocol interop with the existing desktop encoder/extractor.
- 16 MiB per-file limit, four simultaneous received transfers, 65,536 parts
  maximum; the browser holds packet data in memory.

## Not yet included

- H.264/MP4 export, MKV/AVI decoding, FFmpeg/ZBar native performance,
  guaranteed 4K/60 FPS scanning, or the desktop two-pass recovery algorithm.
- Progressive disk streaming for large files.
- Exact timing guarantees: camera/video decode throughput depends on the
  browser and device. If scanning misses frames, replay the recording.

This MVP intentionally leaves the desktop Rust crates and native workflows
unchanged. It is not an end-to-end replacement for the desktop application yet.
