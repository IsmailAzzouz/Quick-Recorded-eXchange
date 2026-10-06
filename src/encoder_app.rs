use std::{fs, path::PathBuf, time::Instant};

use eframe::egui::{self, Align, Color32, Layout, RichText, TextureHandle};

use crate::{
    payload::FilePacket,
    qr::{self, QrEcc},
    video_encoder::{self, VideoEncoderEvent, VideoEncoderHandle, VideoEncoderSettings},
};

const RESOLUTIONS: [(&str, usize, usize); 3] = [
    ("1080p (1920 × 1080)", 1920, 1080),
    ("720p (1280 × 720)", 1280, 720),
    ("4K (3840 × 2160)", 3840, 2160),
];

pub struct QrEncoderApp {
    file_path: Option<PathBuf>,
    file_bytes: Vec<u8>,
    packets: Vec<FilePacket>,
    current_index: usize,
    chunk_size: usize,
    ecc: QrEcc,
    fps: u32,
    hold_frames: u32,
    resolution_index: usize,
    is_playing: bool,
    loop_playback: bool,
    last_frame_instant: Instant,
    cached_texture: Option<(usize, TextureHandle)>,
    export_handle: Option<VideoEncoderHandle>,
    export_progress: (usize, usize),
    status: String,
    error: Option<String>,
}

impl QrEncoderApp {
    #[must_use]
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            file_path: None,
            file_bytes: Vec::new(),
            packets: Vec::new(),
            current_index: 0,
            chunk_size: 512,
            ecc: QrEcc::Medium,
            fps: 15,
            hold_frames: 1,
            resolution_index: 0,
            is_playing: false,
            loop_playback: true,
            last_frame_instant: Instant::now(),
            cached_texture: None,
            export_handle: None,
            export_progress: (0, 0),
            status: String::from("Select a file to encode and transmit via QR."),
            error: None,
        }
    }

    fn choose_file(&mut self) {
        if let Some(path) = rfd::FileDialog::new().pick_file() {
            self.load_file(path);
        }
    }

    fn load_file(&mut self, path: PathBuf) {
        match fs::read(&path) {
            Ok(bytes) => {
                let filename = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("file.bin")
                    .to_owned();
                self.file_path = Some(path);
                self.file_bytes = bytes;
                self.error = None;
                self.rebuild_packets(&filename);
                self.status = format!("Loaded {} with {} packet(s).", filename, self.packets.len());
            }
            Err(err) => {
                self.error = Some(format!("Could not read file: {err}"));
            }
        }
    }

    fn rebuild_packets(&mut self, filename: &str) {
        self.packets = FilePacket::create_packets(filename, &self.file_bytes, self.chunk_size);
        self.current_index = 0;
        self.cached_texture = None;
        self.is_playing = false;
    }

    fn poll_export(&mut self) {
        let events = self
            .export_handle
            .as_ref()
            .map_or_else(Vec::new, |h| h.events.try_iter().collect());

        for event in events {
            match event {
                VideoEncoderEvent::Started { total_packets, .. } => {
                    self.export_progress = (0, total_packets);
                    self.status = String::from("Starting video encoding with FFmpeg…");
                }
                VideoEncoderEvent::Progress {
                    packet_index,
                    total_packets,
                } => {
                    self.export_progress = (packet_index, total_packets);
                    self.status = format!("Encoding frame {packet_index} of {total_packets}…");
                }
                VideoEncoderEvent::Finished {
                    output_path,
                    total_frames,
                    elapsed_seconds,
                } => {
                    self.export_handle = None;
                    self.status = format!(
                        "Export complete: {total_frames} frames saved to {} in {elapsed_seconds:.1}s.",
                        output_path.display()
                    );
                }
            }
        }
    }

    fn advance_playback(&mut self) {
        if !self.is_playing || self.packets.is_empty() {
            return;
        }
        let frame_interval = 1.0 / f64::from(self.fps.max(1));
        if self.last_frame_instant.elapsed().as_secs_f64() >= frame_interval {
            self.last_frame_instant = Instant::now();
            if self.current_index + 1 < self.packets.len() {
                self.current_index += 1;
            } else if self.loop_playback {
                self.current_index = 0;
            } else {
                self.is_playing = false;
            }
        }
    }

    fn start_export(&mut self) {
        if self.packets.is_empty() {
            return;
        }
        let default_name = self
            .file_path
            .as_ref()
            .and_then(|p| p.file_stem())
            .and_then(|stem| stem.to_str())
            .unwrap_or("qr_stream");
        let output_name = format!("{default_name}.mp4");

        if let Some(target) = rfd::FileDialog::new()
            .set_file_name(&output_name)
            .add_filter("Video MP4", &["mp4"])
            .save_file()
        {
            let (_, width, height) = RESOLUTIONS[self.resolution_index];
            let settings = VideoEncoderSettings {
                width,
                height,
                fps: self.fps,
                hold_frames: self.hold_frames,
                crf: 18,
                ecc: self.ecc,
            };
            match video_encoder::spawn_video_export(self.packets.clone(), target, settings) {
                Ok(handle) => {
                    self.export_handle = Some(handle);
                    self.is_playing = false;
                }
                Err(err) => {
                    self.error = Some(err.to_string());
                }
            }
        }
    }
}

impl eframe::App for QrEncoderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_export();
        self.advance_playback();

        if let Some(path) = ctx.input(|i| i.raw.dropped_files.iter().find_map(|f| f.path.clone())) {
            self.load_file(path);
        }

        if self.is_playing || self.export_handle.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(16));
        }

        egui::TopBottomPanel::top("encoder_header")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 14)))
            .show(ctx, |ui| self.draw_header(ui));

        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(32, 24)))
            .show(ctx, |ui| {
                if let Some(err) = &self.error {
                    ui.colored_label(Color32::LIGHT_RED, format!("Error: {err}"));
                    ui.add_space(10.0);
                }
                if self.export_handle.is_some() {
                    self.draw_export_loading(ui);
                } else if self.packets.is_empty() {
                    self.draw_empty_state(ui);
                } else {
                    self.draw_player(ctx, ui);
                }
            });

        egui::TopBottomPanel::bottom("encoder_footer")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(24, 10)))
            .show(ctx, |ui| {
                ui.label(&self.status);
            });
    }
}

impl QrEncoderApp {
    fn draw_header(&mut self, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.label(RichText::new("QR Video Encoder").size(19.0).strong());
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if let Some(handle) = &self.export_handle {
                    if ui.button("Cancel Export").clicked() {
                        handle.cancel();
                        self.status = String::from("Cancelling export…");
                    }
                } else {
                    if ui.button("Choose File").clicked() {
                        self.choose_file();
                    }
                    if !self.packets.is_empty() && ui.button("Export to Video").clicked() {
                        self.start_export();
                    }
                }
            });
        });
    }

    fn draw_empty_state(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(100.0);
            ui.label(
                RichText::new("Encode and transmit files through QR video")
                    .size(24.0)
                    .strong(),
            );
            ui.add_space(8.0);
            ui.label("Drop any file here or select one to begin.");
            ui.add_space(20.0);
            if ui
                .add_sized([180.0, 40.0], egui::Button::new("Choose File"))
                .clicked()
            {
                self.choose_file();
            }
            ui.add_space(30.0);
            self.draw_settings_grid(ui);
        });
    }

    fn draw_export_loading(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(120.0);
            ui.label(
                RichText::new("Generating Video with FFmpeg")
                    .size(22.0)
                    .strong(),
            );
            ui.add_space(12.0);
            let (done, total) = self.export_progress;
            #[allow(clippy::cast_precision_loss)]
            let progress = if total > 0 {
                done as f32 / total as f32
            } else {
                0.0
            };
            ui.add(egui::ProgressBar::new(progress).text(format!(
                "{done} / {total} packets ({:.0}%)",
                progress * 100.0
            )));
            ui.add_space(12.0);
            ui.small("Encoding frames directly into standard H.264 MP4 container.");
        });
    }

    fn draw_player(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        ui.horizontal(|ui| {
            ui.vertical(|ui| {
                self.draw_qr_display(ctx, ui);
                ui.add_space(12.0);
                self.draw_player_controls(ui);
            });
            ui.add_space(24.0);
            ui.vertical(|ui| {
                self.draw_info_card(ui);
                ui.add_space(16.0);
                self.draw_settings_grid(ui);
            });
        });
    }

    fn draw_qr_display(&mut self, ctx: &egui::Context, ui: &mut egui::Ui) {
        if self.packets.is_empty() {
            return;
        }
        let needs_texture = self
            .cached_texture
            .as_ref()
            .is_none_or(|(idx, _)| *idx != self.current_index);
        if needs_texture {
            let serialized = self.packets[self.current_index].serialize();
            if let Ok(matrix) = qr::encode_text(&serialized, self.ecc) {
                let img = qr::render_to_color_image(&matrix, 8);
                let texture = ctx.load_texture("qr_preview", img, egui::TextureOptions::NEAREST);
                self.cached_texture = Some((self.current_index, texture));
            }
        }

        if let Some((_, texture)) = &self.cached_texture {
            let display_size = egui::Vec2::splat(420.0);
            ui.image((texture.id(), display_size));
        }
    }

    fn draw_player_controls(&mut self, ui: &mut egui::Ui) {
        let total = self.packets.len();
        ui.horizontal(|ui| {
            let play_label = if self.is_playing { "Pause" } else { "Play" };
            if ui.button(play_label).clicked() {
                self.is_playing = !self.is_playing;
            }
            if ui.button("Prev").clicked() && self.current_index > 0 {
                self.current_index -= 1;
            }
            if ui.button("Next").clicked() && self.current_index + 1 < total {
                self.current_index += 1;
            }
            ui.checkbox(&mut self.loop_playback, "Loop");
            ui.add(egui::Slider::new(&mut self.fps, 1..=60).text("FPS"));
        });

        ui.add_space(6.0);
        #[allow(clippy::cast_precision_loss)]
        let mut scrub_index = self.current_index;
        if ui
            .add(
                egui::Slider::new(&mut scrub_index, 0..=total.saturating_sub(1))
                    .text(format!("Part {} / {total}", self.current_index + 1)),
            )
            .changed()
        {
            self.current_index = scrub_index;
        }
    }

    fn draw_info_card(&self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("File Metadata").strong());
            ui.add_space(4.0);
            if let Some(path) = &self.file_path {
                ui.small(format!("Path: {}", path.display()));
            }
            ui.small(format!("Total size: {} bytes", self.file_bytes.len()));
            ui.small(format!("Total chunks: {}", self.packets.len()));
            if let Some(first) = self.packets.first() {
                ui.small("SHA-256:");
                ui.monospace(&first.sha256);
            }
        });
    }

    fn draw_settings_grid(&mut self, ui: &mut egui::Ui) {
        ui.group(|ui| {
            ui.label(RichText::new("Encoder Settings").strong());
            ui.add_space(6.0);
            let mut chunk = self.chunk_size;
            if ui
                .add(egui::Slider::new(&mut chunk, 128..=1024).text("Chunk bytes"))
                .changed()
            {
                self.chunk_size = chunk;
                if let Some(path) = &self.file_path {
                    let filename = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("file.bin")
                        .to_owned();
                    self.rebuild_packets(&filename);
                }
            }
            ui.add(egui::Slider::new(&mut self.hold_frames, 1..=5).text("Frame repeat"));
            egui::ComboBox::from_label("Resolution")
                .selected_text(RESOLUTIONS[self.resolution_index].0)
                .show_ui(ui, |ui| {
                    for (i, (label, _, _)) in RESOLUTIONS.iter().enumerate() {
                        ui.selectable_value(&mut self.resolution_index, i, *label);
                    }
                });
        });
    }
}
