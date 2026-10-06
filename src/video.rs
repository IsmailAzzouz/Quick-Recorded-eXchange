use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use crossbeam_channel::Sender;
use serde::Deserialize;
use thiserror::Error;

#[derive(Debug, Clone)]
pub struct VideoInfo {
    pub width: usize,
    pub height: usize,
    pub fps: f64,
    pub duration_seconds: Option<f64>,
}

#[derive(Debug)]
pub struct Frame {
    pub index: u64,
    pub timestamp_seconds: f64,
    pub width: usize,
    pub height: usize,
    pub pixels: Vec<u8>,
}

#[derive(Debug, Error)]
pub enum VideoError {
    #[error("could not start ffprobe; install FFmpeg and ensure it is on PATH: {0}")]
    ProbeStart(#[source] std::io::Error),
    #[error("ffprobe failed: {0}")]
    ProbeFailed(String),
    #[error("could not parse ffprobe metadata: {0}")]
    ProbeJson(#[from] serde_json::Error),
    #[error("video does not contain a decodable video stream")]
    NoVideoStream,
    #[error("could not start ffmpeg; install FFmpeg and ensure it is on PATH: {0}")]
    FfmpegStart(#[source] std::io::Error),
}

#[derive(Deserialize)]
struct ProbeResponse {
    streams: Vec<ProbeStream>,
}

#[derive(Deserialize)]
struct ProbeStream {
    width: Option<usize>,
    height: Option<usize>,
    avg_frame_rate: Option<String>,
    duration: Option<String>,
}

pub fn probe(path: &Path) -> Result<VideoInfo, VideoError> {
    let output = Command::new("ffprobe")
        .args([
            "-v",
            "error",
            "-select_streams",
            "v:0",
            "-show_entries",
            "stream=width,height,avg_frame_rate,duration",
            "-of",
            "json",
        ])
        .arg(path)
        .output()
        .map_err(VideoError::ProbeStart)?;
    if !output.status.success() {
        return Err(VideoError::ProbeFailed(
            String::from_utf8_lossy(&output.stderr).trim().to_owned(),
        ));
    }
    let response: ProbeResponse = serde_json::from_slice(&output.stdout)?;
    let stream = response.streams.first().ok_or(VideoError::NoVideoStream)?;
    let (Some(width), Some(height)) = (stream.width, stream.height) else {
        return Err(VideoError::NoVideoStream);
    };
    Ok(VideoInfo {
        width,
        height,
        fps: parse_rate(stream.avg_frame_rate.as_deref()).unwrap_or(30.0),
        duration_seconds: stream
            .duration
            .as_deref()
            .and_then(|duration| duration.parse().ok()),
    })
}

#[allow(clippy::needless_pass_by_value)] // The metadata is moved into the reader thread.
pub fn spawn_frame_reader(
    path: PathBuf,
    source: VideoInfo,
    scan_width: usize,
    frame_step: usize,
    skip_identical_frames: bool,
    frame_tx: Sender<Frame>,
    stop: Arc<AtomicBool>,
) -> Result<thread::JoinHandle<()>, VideoError> {
    let scan_width = scan_width.min(source.width).max(64);
    let scan_height = if scan_width == source.width {
        source.height
    } else {
        scaled_height(source.width, source.height, scan_width)
    };
    let frame_bytes = scan_width * scan_height;
    let frame_step = frame_step.max(1);
    let temporal_filter = if frame_step == 1 {
        String::new()
    } else {
        // The escaped comma belongs to FFmpeg's filter expression, not the command shell.
        format!("select=not(mod(n\\,{frame_step})),")
    };
    let video_filter = format!(
        "{temporal_filter}scale={scan_width}:{scan_height}:flags=fast_bilinear,format=gray"
    );
    let mut child = Command::new("ffmpeg")
        .args(["-hide_banner", "-loglevel", "error", "-nostdin", "-i"])
        .arg(path)
        .args([
            "-an",
            "-sn",
            "-vf",
            &video_filter,
            "-pix_fmt",
            "gray",
            "-f",
            "rawvideo",
            "pipe:1",
        ])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .map_err(VideoError::FfmpegStart)?;
    let stdout = child.stdout.take().expect("ffmpeg stdout was piped");

    Ok(thread::spawn(move || {
        let mut reader = std::io::BufReader::with_capacity(frame_bytes * 2, stdout);
        let mut selected_index = 0_u64;
        let mut previous_signature = None;
        loop {
            if stop.load(Ordering::Relaxed) {
                break;
            }
            let mut pixels = vec![0_u8; frame_bytes];
            if reader.read_exact(&mut pixels).is_err() {
                break;
            }
            selected_index += 1;
            let source_index = 1 + (selected_index - 1) * frame_step as u64;
            if skip_identical_frames {
                let signature = sampled_signature(&pixels);
                if previous_signature == Some(signature) {
                    continue;
                }
                previous_signature = Some(signature);
            }
            let frame = Frame {
                index: source_index,
                #[allow(clippy::cast_precision_loss)] // Video frame counters cannot approach f64's exact-integer limit.
                timestamp_seconds: source_index as f64 / source.fps,
                width: scan_width,
                height: scan_height,
                pixels,
            };
            if frame_tx.send(frame).is_err() {
                break;
            }
        }
        let _ = child.kill();
        let _ = child.wait();
        drop(frame_tx);
    }))
}

fn scaled_height(source_width: usize, source_height: usize, target_width: usize) -> usize {
    let height = source_height.saturating_mul(target_width) / source_width;
    height.max(2) & !1
}

fn parse_rate(rate: Option<&str>) -> Option<f64> {
    let rate = rate?;
    let (numerator, denominator) = rate.split_once('/')?;
    let numerator: f64 = numerator.parse().ok()?;
    let denominator: f64 = denominator.parse().ok()?;
    (denominator > 0.0).then_some(numerator / denominator)
}

/// A cache-friendly frame signature used only to avoid rescanning exact visual holds.
///
/// Sampling at most 4,096 evenly distributed luma values keeps this below the QR detector's
/// cost by orders of magnitude while making unrelated frames overwhelmingly unlikely to match.
fn sampled_signature(pixels: &[u8]) -> u64 {
    const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
    const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
    let stride = (pixels.len() / 4_096).max(1);
    pixels
        .iter()
        .step_by(stride)
        .fold(FNV_OFFSET, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(FNV_PRIME)
        })
}

#[cfg(test)]
mod tests {
    use super::sampled_signature;

    #[test]
    fn signature_changes_when_sampled_content_changes() {
        let mut pixels = vec![255; 8_192];
        let original = sampled_signature(&pixels);
        pixels[4_096] = 0;
        assert_ne!(original, sampled_signature(&pixels));
    }
}
