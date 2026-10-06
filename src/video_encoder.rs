use std::{
    io::Write,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Instant,
};

use crossbeam_channel::{Receiver, Sender, unbounded};
use thiserror::Error;

use crate::{
    payload::FilePacket,
    qr::{self, QrEcc, QrError},
};

#[derive(Debug, Clone)]
pub struct VideoEncoderSettings {
    pub width: usize,
    pub height: usize,
    pub fps: u32,
    pub hold_frames: u32,
    pub crf: u32,
    pub ecc: QrEcc,
}

impl Default for VideoEncoderSettings {
    fn default() -> Self {
        Self {
            width: 1920,
            height: 1080,
            fps: 15,
            hold_frames: 1,
            crf: 18,
            ecc: QrEcc::Medium,
        }
    }
}

#[derive(Debug)]
pub enum VideoEncoderEvent {
    Started {
        total_packets: usize,
        total_frames: usize,
    },
    Progress {
        packet_index: usize,
        total_packets: usize,
    },
    Finished {
        output_path: PathBuf,
        total_frames: usize,
        elapsed_seconds: f64,
    },
}

pub struct VideoEncoderHandle {
    pub events: Receiver<VideoEncoderEvent>,
    stop: Arc<AtomicBool>,
}

impl VideoEncoderHandle {
    pub fn cancel(&self) {
        self.stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Debug, Error)]
pub enum VideoEncoderError {
    #[error("could not start ffmpeg; install FFmpeg and verify it is on PATH: {0}")]
    FfmpegStart(#[source] std::io::Error),
    #[error("ffmpeg failed during encoding: {0}")]
    FfmpegFailed(String),
    #[error("QR encoding error: {0}")]
    Qr(#[from] QrError),
    #[error("I/O error during video streaming: {0}")]
    Io(#[from] std::io::Error),
}

/// Spawns a background worker that streams QR frames into `FFmpeg`.
pub fn spawn_video_export(
    packets: Vec<FilePacket>,
    output_path: PathBuf,
    settings: VideoEncoderSettings,
) -> Result<VideoEncoderHandle, VideoEncoderError> {
    let mut child = spawn_ffmpeg_child(&output_path, &settings)?;
    let stdin = child.stdin.take().expect("stdin was piped");
    let stop = Arc::new(AtomicBool::new(false));
    let (event_tx, event_rx) = unbounded();

    let worker_stop = Arc::clone(&stop);
    thread::spawn(move || {
        encode_worker_loop(
            packets,
            &settings,
            stdin,
            event_tx,
            worker_stop,
            output_path,
            child,
        );
    });

    Ok(VideoEncoderHandle {
        events: event_rx,
        stop,
    })
}

fn spawn_ffmpeg_child(
    output_path: &Path,
    settings: &VideoEncoderSettings,
) -> Result<Child, VideoEncoderError> {
    Command::new("ffmpeg")
        .args([
            "-y",
            "-hide_banner",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "gray",
            "-s",
            &format!("{}x{}", settings.width, settings.height),
            "-r",
            &format!("{}", settings.fps),
            "-i",
            "pipe:0",
            "-c:v",
            "libx264",
            "-pix_fmt",
            "yuv420p",
            "-preset",
            "ultrafast",
            "-crf",
            &format!("{}", settings.crf),
        ])
        .arg(output_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(VideoEncoderError::FfmpegStart)
}

#[allow(clippy::needless_pass_by_value)]
fn encode_worker_loop(
    packets: Vec<FilePacket>,
    settings: &VideoEncoderSettings,
    mut stdin: std::process::ChildStdin,
    tx: Sender<VideoEncoderEvent>,
    stop: Arc<AtomicBool>,
    output_path: PathBuf,
    mut child: Child,
) {
    let started = Instant::now();
    let total_packets = packets.len();
    let total_frames = total_packets * settings.hold_frames.max(1) as usize;
    let _ = tx.send(VideoEncoderEvent::Started {
        total_packets,
        total_frames,
    });

    let mut frames_written = 0;
    for (index, packet) in packets.iter().enumerate() {
        if stop.load(Ordering::Relaxed) {
            let _ = child.kill();
            return;
        }
        let serialized = packet.serialize();
        let Ok(matrix) = qr::encode_text(&serialized, settings.ecc) else {
            let _ = child.kill();
            return;
        };
        let frame = qr::render_to_gray_frame(&matrix, settings.width, settings.height);
        for _ in 0..settings.hold_frames.max(1) {
            if stdin.write_all(&frame).is_err() {
                let _ = child.kill();
                return;
            }
            frames_written += 1;
        }
        let _ = tx.send(VideoEncoderEvent::Progress {
            packet_index: index + 1,
            total_packets,
        });
    }

    drop(stdin);
    let _ = child.wait();
    let _ = tx.send(VideoEncoderEvent::Finished {
        output_path,
        total_frames: frames_written,
        elapsed_seconds: started.elapsed().as_secs_f64(),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::payload::FileAssembly;

    #[test]
    fn full_encode_and_decode_cycle() {
        if Command::new("ffmpeg").arg("-version").output().is_err() {
            return;
        }
        let test_data = b"Round-trip video encoding and extraction pipeline verification data!";
        let packets = FilePacket::create_packets("sample.txt", test_data, 32);
        let temp_dir = std::env::temp_dir().join(format!("qr-test-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let video_path = temp_dir.join("encoded_test.mp4");

        let settings = VideoEncoderSettings {
            width: 640,
            height: 480,
            fps: 10,
            hold_frames: 2,
            crf: 18,
            ecc: QrEcc::Medium,
        };
        let handle = spawn_video_export(packets.clone(), video_path.clone(), settings)
            .expect("spawn export");
        let mut finished = false;
        for event in handle.events {
            if let VideoEncoderEvent::Finished { .. } = event {
                finished = true;
                break;
            }
        }
        assert!(finished, "video export must complete");
        assert!(video_path.is_file(), "video file must exist");

        let scan = crate::decoder::start(
            video_path.clone(),
            crate::decoder::ScanSettings::exhaustive(),
        )
        .expect("start decoder");

        let mut assembly = FileAssembly::new(&packets[0]);
        loop {
            match scan
                .events
                .recv_timeout(std::time::Duration::from_secs(15))
                .expect("scan event")
            {
                crate::decoder::ScanEvent::QrDecoded(text) => {
                    if let Ok(pkt) = FilePacket::parse(&text) {
                        assembly.insert(pkt);
                    }
                }
                crate::decoder::ScanEvent::Finished { .. } => break,
                _ => {}
            }
        }

        let _ = std::fs::remove_file(video_path);
        let _ = std::fs::remove_dir(temp_dir);

        assert!(
            assembly.is_complete(),
            "assembly must recover all parts from encoded video"
        );
        assert_eq!(assembly.recovered.as_deref(), Some(&test_data[..]));
    }
}
