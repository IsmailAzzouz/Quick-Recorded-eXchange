# Quick Recorded eXchange (QR Video Extractor & Encoder)

Fast, native desktop suite in Rust for transferring large files through high-density QR code video streams (1080p, 4K, 60fps+).

Includes both a high-throughput **Decoder / Extractor** and a versatile **Encoder / Broadcaster** supporting screen playback and MP4 video generation.

---

## Features

### Extractor (Decoder)
- **High-Throughput Parallel QR Detection**: Distributes frames across CPU worker threads utilizing direct C-bindings to ZBar.
- **Low Memory Streaming Pipeline**: Streams grayscale frames directly from FFmpeg pipes into a bounded queue with automatic back-pressure, avoiding storing raw frames in memory.
- **Adaptive Frame Sampling**: Automatically adapts scan stride to video frame rate while skipping visually identical static frames.
- **Two-Pass Complete Recovery**: Runs an adaptive high-speed pass first; if any chunks remain missing, automatically initiates an exhaustive frame-by-frame pass.
- **Automatic Reassembly & Verification**: Chunks are assembled strictly according to protocol metadata and verified against the declared SHA-256 digest before writing to disk.
- **Safe Output Handling**: Automatically detects file types and writes recovered files into a `Recovered from QR` folder beside the source video, appending incrementing indices if files already exist.
- **Responsive GUI**: Clean desktop interface supporting drag-and-drop, real-time recovery progress, live file cards, and one-click cancellation.

### Encoder (Broadcaster & Video Generator)
- **Direct Screen Broadcast**: Cycles through generated QR codes in real-time in an interactive GUI player with Play/Pause, single-step navigation, scrub slider, and dynamic FPS control.
- **FFmpeg Video Export**: Streams crisp QR frames directly into FFmpeg to generate standard H.264 MP4 videos (1080p, 720p, 4K) with configurable frame holds for resilient camera capture.
- **Configurable FQ1 Parameters**: Customize chunk size (128 to 1024 bytes), error correction levels (Low, Medium, Quartile, High), and frame repetition rates.
- **Dual GUI / Headless CLI Mode**: Run interactively with egui or script automated video exports directly from the command line.

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

### 2. ZBar Shared Library (for Extractor)
The extractor dynamically links to ZBar at runtime:

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

Ensure you have the Rust toolchain installed (Rust 1.85+ recommended):

```bash
# Clone the repository
git clone https://github.com/IsmailAzzouz/Quick-Recorded-eXchange.git
cd Quick-Recorded-eXchange
```

### 1. Running the Extractor (Decoder)

Launch the decoder GUI application:

```bash
cargo run --release
# Or explicitly:
cargo run --bin qr-video-extractor --release
```

- Drag and drop any video file into the window, or click **Choose video**.
- Recovery starts automatically. Files are verified against their SHA-256 digest and saved to `Recovered from QR/` next to the video.

### 2. Running the Encoder (Broadcaster & Video Generator)

#### Graphical User Interface (GUI):

```bash
cargo run --bin qr-video-encoder --release
```

- Drag and drop or select any file to encode.
- **Screen Broadcast**: Play the QR stream directly on your monitor with playback controls (Space/Play/Pause, Prev/Next, scrubber, FPS slider).
- **Video Export**: Click **Export to Video** to render a high-quality H.264 MP4 file.

#### Command-Line Interface (CLI):

```bash
cargo run --bin qr-video-encoder --release -- -i <input_file> [-o <output.mp4>] [-c <chunk_bytes>] [--fps <fps>] [--hold <repeat>]
```

Example:
```bash
cargo run --bin qr-video-encoder --release -- -i archive.zip -o transfer.mp4 -c 512 --fps 15 --hold 2
```

---

## Protocol Specification: FQ1 Format

The suite uses the `FQ1` transfer wire standard:

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
cargo check --all-targets

# Run linter
cargo clippy --all-targets

# Check code formatting
cargo fmt --check

# Run all unit and integration tests
cargo test
```

---

## License

This project is licensed under the [MIT License](LICENSE).
