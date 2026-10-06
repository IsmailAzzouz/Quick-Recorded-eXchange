use std::{
    path::PathBuf,
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
    thread,
    time::Instant,
};

use crossbeam_channel::{Receiver, Sender, bounded, unbounded};
use thiserror::Error;

use crate::{
    video::{self, Frame, VideoError, VideoInfo},
    zbar::{ZbarError, ZbarScanner},
};

#[derive(Debug, Clone, Copy)]
pub struct ScanSettings {
    pub workers: usize,
    exhaustive: bool,
}

impl Default for ScanSettings {
    fn default() -> Self {
        Self {
            workers: num_cpus::get().saturating_sub(1).max(1),
            exhaustive: false,
        }
    }
}

impl ScanSettings {
    #[must_use]
    pub fn exhaustive() -> Self {
        Self {
            exhaustive: true,
            ..Self::default()
        }
    }
}

#[derive(Debug)]
pub enum ScanEvent {
    Started(VideoInfo),
    FrameDecoded { frame: u64, timestamp_seconds: f64 },
    QrDecoded(String),
    Finished { elapsed_seconds: f64, frames: u64 },
}

pub struct ScanHandle {
    pub events: Receiver<ScanEvent>,
    stop: Arc<AtomicBool>,
}

impl ScanHandle {
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug, Error)]
pub enum ScanError {
    #[error(transparent)]
    Video(#[from] VideoError),
    #[error(transparent)]
    Zbar(#[from] ZbarError),
}

pub fn start(video: PathBuf, settings: ScanSettings) -> Result<ScanHandle, ScanError> {
    let info = video::probe(&video)?;
    // Verify the bundled native engine before FFmpeg starts producing frames.
    let _ = ZbarScanner::new()?;
    let stop = Arc::new(AtomicBool::new(false));
    let (event_tx, event_rx) = unbounded();
    let (frame_tx, frame_rx) = bounded(settings.workers.max(1) * 2);
    let reader_stop = Arc::clone(&stop);
    let frames_done = Arc::new(AtomicU64::new(0));
    let reader = video::spawn_frame_reader(
        video,
        info.clone(),
        info.width,
        if settings.exhaustive {
            1
        } else {
            automatic_frame_step(info.fps)
        },
        false,
        frame_tx,
        reader_stop,
    )?;
    let _ = event_tx.send(ScanEvent::Started(info));

    let mut workers = Vec::with_capacity(settings.workers.max(1));
    for _ in 0..settings.workers.max(1) {
        let rx = frame_rx.clone();
        let tx = event_tx.clone();
        let worker_stop = Arc::clone(&stop);
        let worker_frames = Arc::clone(&frames_done);
        workers.push(thread::spawn(move || {
            worker_loop(rx, tx, worker_stop, worker_frames);
        }));
    }
    drop(frame_rx);

    thread::spawn(move || {
        let started = Instant::now();
        let _ = reader.join();
        for worker in workers {
            let _ = worker.join();
        }
        let _ = event_tx.send(ScanEvent::Finished {
            elapsed_seconds: started.elapsed().as_secs_f64(),
            frames: frames_done.load(Ordering::Relaxed),
        });
    });

    Ok(ScanHandle {
        events: event_rx,
        stop,
    })
}

#[allow(clippy::needless_pass_by_value)] // Values are deliberately moved into each worker thread.
fn worker_loop(
    rx: Receiver<Frame>,
    tx: Sender<ScanEvent>,
    stop: Arc<AtomicBool>,
    frames_done: Arc<AtomicU64>,
) {
    let mut scanner = ZbarScanner::new().expect("ZBar was verified before workers started");
    while !stop.load(Ordering::Relaxed) {
        let Ok(frame) = rx.recv() else {
            break;
        };
        let frame_number = frame.index;
        let timestamp = frame.timestamp_seconds;
        let grids = detect(&mut scanner, &frame);
        frames_done.fetch_add(1, Ordering::Relaxed);
        let _ = tx.send(ScanEvent::FrameDecoded {
            frame: frame_number,
            timestamp_seconds: timestamp,
        });
        for text in grids {
            if tx.send(ScanEvent::QrDecoded(text)).is_err() {
                return;
            }
        }
    }
}

fn detect(scanner: &mut ZbarScanner, frame: &Frame) -> Vec<String> {
    scanner.scan(&frame.pixels, frame.width, frame.height)
}

fn automatic_frame_step(fps: f64) -> usize {
    if !fps.is_finite() || fps <= 15.0 {
        return 1;
    }
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    // Fifteen native-resolution samples per second captures the short on-screen FQ1 holds in
    // high-FPS videos while avoiding redundant decoding of the same QR on every source frame.
    let step = (fps / 15.0).ceil() as usize;
    step.max(1)
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        process::Command,
        sync::atomic::{AtomicU64, Ordering},
        time::Duration,
    };

    use qrcodegen::{QrCode, QrCodeEcc};

    use super::*;

    static TEST_ID: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn decodes_a_real_qr_symbol() {
        let text = "FQ1|c21va2UuYmlu|0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef|1|1|5|aGVsbG8=";
        let frame = qr_frame(text, 8);
        let mut scanner = ZbarScanner::new().unwrap();
        assert_eq!(detect(&mut scanner, &frame), vec![text.to_owned()]);
    }

    #[test]
    fn scans_a_real_ffmpeg_video() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return; // FFmpeg is intentionally an external runtime dependency.
        }
        let text = "QR video pipeline smoke test";
        let frame = qr_frame(text, 10);
        let id = TEST_ID.fetch_add(1, Ordering::Relaxed);
        let base =
            std::env::temp_dir().join(format!("qr-video-extractor-{}-{id}", std::process::id()));
        let image_path = base.with_extension("pgm");
        let video_path = base.with_extension("mkv");
        let mut pgm = format!("P5\n{} {}\n255\n", frame.width, frame.height).into_bytes();
        pgm.extend_from_slice(&frame.pixels);
        fs::write(&image_path, pgm).expect("write QR test image");
        let encoded = Command::new("ffmpeg")
            .args([
                "-hide_banner",
                "-loglevel",
                "error",
                "-y",
                "-loop",
                "1",
                "-i",
            ])
            .arg(&image_path)
            .args(["-frames:v", "1", "-c:v", "ffv1"])
            .arg(&video_path)
            .status()
            .expect("run FFmpeg");
        assert!(
            encoded.success(),
            "FFmpeg should encode the temporary video"
        );

        let scan = start(
            video_path.clone(),
            ScanSettings {
                workers: 1,
                exhaustive: false,
            },
        )
        .expect("start video scan");
        let mut found = false;
        loop {
            match scan
                .events
                .recv_timeout(Duration::from_secs(10))
                .expect("scan event")
            {
                ScanEvent::QrDecoded(value) if value == text => found = true,
                ScanEvent::Finished { .. } => break,
                _ => {}
            }
        }
        let _ = fs::remove_file(image_path);
        let _ = fs::remove_file(video_path);
        assert!(found, "the QR payload should survive FFmpeg and be decoded");
    }

    #[test]
    #[ignore = "optional integration test requiring QR_TEST_DENSE_VIDEO environment variable"]
    fn recovers_every_block_from_the_dense_transfer_video() {
        let Some(path) = std::env::var_os("QR_TEST_DENSE_VIDEO") else {
            return;
        };
        let video = PathBuf::from(path);
        if !video.is_file() {
            return;
        }
        let scan = start(video, ScanSettings::default()).expect("start dense transfer scan");
        let mut parts = std::collections::BTreeSet::new();
        let mut total = None;
        loop {
            match scan
                .events
                .recv_timeout(Duration::from_secs(120))
                .expect("dense transfer scan event")
            {
                ScanEvent::QrDecoded(text) => {
                    if let Ok(packet) = crate::payload::FilePacket::parse(&text) {
                        parts.insert(packet.part);
                        total = Some(packet.parts);
                    }
                }
                ScanEvent::Finished { .. } => break,
                _ => {}
            }
        }
        assert!(total.is_some(), "at least one packet should be decoded");
        assert_eq!(
            parts.len(),
            total.unwrap(),
            "all FQ1 blocks must be recovered"
        );
    }

    fn qr_frame(text: &str, scale: usize) -> Frame {
        let code = QrCode::encode_text(text, QrCodeEcc::Medium).expect("encode QR");
        let border = 4_i32;
        let modules = code.size() + border * 2;
        let width = usize::try_from(modules).expect("positive QR width") * scale;
        let mut pixels = vec![255_u8; width * width];
        for module_y in -border..code.size() + border {
            for module_x in -border..code.size() + border {
                if module_x >= 0
                    && module_y >= 0
                    && module_x < code.size()
                    && module_y < code.size()
                    && code.get_module(module_x, module_y)
                {
                    let start_x = usize::try_from(module_x + border).expect("non-negative") * scale;
                    let start_y = usize::try_from(module_y + border).expect("non-negative") * scale;
                    for y in start_y..start_y + scale {
                        pixels[y * width + start_x..y * width + start_x + scale].fill(0);
                    }
                }
            }
        }
        Frame {
            index: 1,
            timestamp_seconds: 0.0,
            width,
            height: width,
            pixels,
        }
    }
}
