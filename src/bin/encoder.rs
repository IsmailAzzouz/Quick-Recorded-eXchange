#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{io::Write, path::PathBuf, process};

use qr_video_extractor::{
    encoder_app::QrEncoderApp,
    payload::FilePacket,
    qr::QrEcc,
    video_encoder::{self, VideoEncoderEvent, VideoEncoderSettings},
};

fn main() -> eframe::Result {
    let args: Vec<String> = std::env::args().collect();
    if args.len() > 1 && (args[1] == "--help" || args[1] == "-h") {
        print_usage();
        return Ok(());
    }

    if let Some(pos) = args.iter().position(|a| a == "--input" || a == "-i") {
        if let Some(input_str) = args.get(pos + 1) {
            let output_str = args
                .iter()
                .position(|a| a == "--output" || a == "-o")
                .and_then(|p| args.get(p + 1))
                .cloned()
                .unwrap_or_else(|| "output.mp4".to_owned());
            let chunk_size = args
                .iter()
                .position(|a| a == "--chunk-size" || a == "-c")
                .and_then(|p| args.get(p + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(512);
            let fps = args
                .iter()
                .position(|a| a == "--fps")
                .and_then(|p| args.get(p + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(15);
            let hold = args
                .iter()
                .position(|a| a == "--hold")
                .and_then(|p| args.get(p + 1))
                .and_then(|v| v.parse().ok())
                .unwrap_or(1);

            if let Err(err) = run_cli(input_str, &output_str, chunk_size, fps, hold) {
                eprintln!("Error: {err}");
                process::exit(1);
            }
            return Ok(());
        }
    }

    let options = eframe::NativeOptions {
        viewport: eframe::egui::ViewportBuilder::default()
            .with_inner_size([1_150.0, 780.0])
            .with_min_inner_size([700.0, 500.0]),
        ..Default::default()
    };
    eframe::run_native(
        "QR Video Encoder",
        options,
        Box::new(|cc| Ok(Box::new(QrEncoderApp::new(cc)))),
    )
}

fn print_usage() {
    println!("QR Video Encoder - Quick Recorded eXchange");
    println!("Usage (GUI):");
    println!("  cargo run --bin qr-video-encoder");
    println!();
    println!("Usage (CLI):");
    println!(
        "  cargo run --bin qr-video-encoder -- -i <input_file> [-o <output.mp4>] [-c <chunk_bytes>] [--fps <fps>] [--hold <repeat>]"
    );
}

fn run_cli(
    input_path: &str,
    output_path: &str,
    chunk_size: usize,
    fps: u32,
    hold: u32,
) -> Result<(), Box<dyn std::error::Error>> {
    let input = PathBuf::from(input_path);
    let output = PathBuf::from(output_path);
    let filename = input
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("file.bin");
    let bytes = std::fs::read(&input)?;
    println!("Read {} bytes from {}", bytes.len(), input.display());
    let packets = FilePacket::create_packets(filename, &bytes, chunk_size);
    println!(
        "Created {} FQ1 packets (chunk size: {chunk_size} bytes)",
        packets.len()
    );
    let settings = VideoEncoderSettings {
        width: 1920,
        height: 1080,
        fps,
        hold_frames: hold,
        crf: 18,
        ecc: QrEcc::Medium,
    };
    println!("Streaming frames to FFmpeg at {fps} FPS (hold: {hold})...");
    let handle = video_encoder::spawn_video_export(packets, output, settings)?;
    for event in handle.events {
        match event {
            VideoEncoderEvent::Started {
                total_packets,
                total_frames,
            } => {
                println!("Export started: {total_packets} packets ({total_frames} total frames)");
            }
            VideoEncoderEvent::Progress {
                packet_index,
                total_packets,
            } => {
                if packet_index % 10 == 0 || packet_index == total_packets {
                    print!("\rProgress: {packet_index}/{total_packets} packets");
                    let _ = std::io::stdout().flush();
                }
            }
            VideoEncoderEvent::Finished {
                output_path,
                total_frames,
                elapsed_seconds,
            } => {
                println!("\nExport finished successfully!");
                println!(
                    "Wrote {total_frames} frames to {} in {elapsed_seconds:.2}s",
                    output_path.display()
                );
            }
        }
    }
    Ok(())
}
