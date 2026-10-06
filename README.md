# QR Video Extractor

Fast, native desktop application for recovering large files transferred through high-density QR code video streams (1080p, 4K, 60fps+).

Powered by Rust, `eframe` (`egui`), multi-threaded ZBar scanning, and FFmpeg streaming.

---

## Features

- **High-Throughput Parallel QR Detection**: Distributes frames across CPU worker threads utilizing direct C-bindings to ZBar.
- **Low Memory Streaming Pipeline**: Streams grayscale frames directly from FFmpeg pipes into a bounded queue with automatic back-pressure, avoiding storing raw frames in memory.
- **Adaptive Frame Sampling**: Automatically adapts scan stride to video frame rate while skipping visually identical static frames.
- **Two-Pass Complete Recovery**: Runs an adaptive high-speed pass first; if any chunks remain missing, automatically initiates an exhaustive frame-by-frame pass.
- **Automatic Reassembly & Verification**: Chunks are assembled strictly according to protocol metadata and verified against the declared SHA-256 digest before writing to disk.
- **Safe Output Handling**: Automatically detects file types and writes recovered files into a `Recovered from QR` folder beside the source video, appending incrementing indices if files already exist.
- **Responsive GUI**: Clean desktop interface supporting drag-and-drop, real-time recovery progress, live file cards, and one-click cancellation.

---

## Architecture

```text
┌─────────────────┐       ┌─────────────────┐       ┌────────────────────────┐
│  Source Video   │ ───►  │  FFmpeg Stream  │ ───►  │ Bounded Memory Channel │
│ (MP4/MKV/etc.)  │       │ (Grayscale Y8)  │       │   (Back-pressure Queue)│
└─────────────────┘       └─────────────────┘       └───────────┬────────────┘
                                                                │
                            ┌───────────────────────────────────┴────────────────────────┐
                            ▼                                   ▼                        ▼
                  ┌───────────────────┐               ┌───────────────────┐    ┌───────────────────┐
                  │ Worker Thread 1   │               │ Worker Thread 2   │    │ Worker Thread N   │
                  │ (ZBar QR Scanner) │               │ (ZBar QR Scanner) │    │ (ZBar QR Scanner) │
                  └─────────┬─────────┘               └─────────┬─────────┘    └─────────┬─────────┘
                            │                                   │                        │
                            └───────────────────────────────────┬────────────────────────┘
                                                                ▼
                                                    ┌────────────────────────┐
                                                    │  FQ1 Assembly Engine   │
                                                    │  - Missing Part Track  │
                                                    │  - SHA-256 Validation  │
                                                    └───────────┬────────────┘
                                                                ▼
                                                    ┌────────────────────────┐
                                                    │ Recovered Output File  │
                                                    │  (Verified on disk)    │
                                                    └────────────────────────┘
```

---

## Prerequisites

### 1. FFmpeg & FFprobe
FFmpeg must be installed and accessible on your system `PATH`.

- **Windows**: Install via `winget install Gyan.FFmpeg` or `choco install ffmpeg`, or download from [ffmpeg.org](https://ffmpeg.org/).
- **Linux (Ubuntu/Debian)**: `sudo apt install ffmpeg`
- **macOS**: `brew install ffmpeg`

Verify your installation:
```bash
ffmpeg -version
ffprobe -version
```

### 2. ZBar Shared Library
The application dynamically links to ZBar at runtime:

- **Windows**:
  - Place `libzbar-64.dll` (and `libiconv.dll` if needed) in the same directory as the executable or project root, **OR**
  - If Python is installed with `pyzbar` (`pip install pyzbar`), the application automatically discovers its bundled libraries, **OR**
  - Set the `QR_VIDEO_ZBAR_DLL` environment variable pointing to the DLL file.
- **Linux (Ubuntu/Debian)**:
  ```bash
  sudo apt install libzbar0
  ```
- **macOS**:
  ```bash
  brew install zbar
  ```

---

## Installation & Running

### From Source

Ensure you have the Rust toolchain installed (Rust 1.85+ recommended):

```bash
# Clone the repository
git clone https://github.com/IsmailAzzouz/Quick-Recorded-eXchange.git
cd Quick-Recorded-eXchange

# Run the optimized application
cargo run --release
```

---

## Usage

1. Launch the application.
2. Drag and drop any video file into the window, or click **Choose video**.
3. Recovery starts automatically:
   - File cards will appear as soon as the first chunk is detected.
   - Progress bars show chunk collection status and verification state.
4. Once all parts of a file are recovered and validated against its SHA-256 checksum, it is automatically written to `Recovered from QR/` next to the source video.

---

## Protocol Specification: FQ1 Format

The application parses QR symbols formatted with the `FQ1` transfer wire standard:

```text
FQ1|<base64_filename>|<sha256>|<part>|<parts>|<part_bytes>|<base64_data>
```

| Field | Type | Description |
|---|---|---|
| `FQ1` | Magic string | Protocol header identifier |
| `base64_filename` | Base64 string | Original filename encoded in Base64 (UTF-8) |
| `sha256` | Hex string | 64-character lowercase hexadecimal SHA-256 of the complete file |
| `part` | Integer (>= 1) | 1-based index of the current chunk |
| `parts` | Integer (>= 1) | Total number of chunks needed to reconstruct the file |
| `part_bytes` | Integer | Byte count of the unencoded data payload in this chunk |
| `base64_data` | Base64 string | Chunk payload encoded in Base64 |

---

## Configuration & Environment Variables

| Variable | Description |
|---|---|
| `QR_VIDEO_ZBAR_DLL` | Explicit path to the `libzbar-64.dll` (or `libzbar.so` / `libzbar.dylib`) file. |
| `QR_TEST_DENSE_VIDEO` | Optional path to a test video file for running the integration benchmark test. |

---

## Development

```bash
# Check code without building
cargo check

# Run linter
cargo clippy --all-targets

# Check code formatting
cargo fmt --check

# Run unit tests
cargo test
```

---

## License

This project is licensed under the [MIT License](LICENSE).
