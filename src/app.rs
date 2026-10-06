use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Instant,
};

use eframe::egui::{self, Align, Color32, Layout, RichText};

use crate::{
    decoder::{self, ScanEvent, ScanHandle, ScanSettings},
    payload::{FileAssembly, FilePacket},
    video::VideoInfo,
};

pub struct QrExtractorApp {
    video_path: Option<PathBuf>,
    scan: Option<ScanHandle>,
    video_info: Option<VideoInfo>,
    started_at: Option<Instant>,
    last_frame: u64,
    last_timestamp_seconds: f64,
    finished_frames: Option<u64>,
    exhaustive_pass: bool,
    assemblies: BTreeMap<String, FileAssembly>,
    seen_payloads: HashSet<String>,
    status: String,
}

impl QrExtractorApp {
    #[must_use]
    pub fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        Self {
            video_path: None,
            scan: None,
            video_info: None,
            started_at: None,
            last_frame: 0,
            last_timestamp_seconds: 0.0,
            finished_frames: None,
            exhaustive_pass: false,
            assemblies: BTreeMap::new(),
            seen_payloads: HashSet::new(),
            status: "Choose a video to start recovery.".to_owned(),
        }
    }

    fn choose_video(&mut self) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Video", &["mp4", "mkv", "mov", "avi", "webm", "m4v"])
            .pick_file()
        {
            self.open_video(&path);
        }
    }

    fn open_video(&mut self, video: &Path) {
        self.video_path = Some(video.to_path_buf());
        self.video_info = None;
        self.last_frame = 0;
        self.last_timestamp_seconds = 0.0;
        self.finished_frames = None;
        self.assemblies.clear();
        self.seen_payloads.clear();
        self.exhaustive_pass = false;
        self.begin_scan(ScanSettings::default());
    }

    fn begin_scan(&mut self, settings: ScanSettings) {
        let Some(video) = self.video_path.clone() else {
            return;
        };
        match decoder::start(video, settings) {
            Ok(scan) => {
                self.scan = Some(scan);
                self.started_at = Some(Instant::now());
                self.status = String::from("Preparing automatic recovery…");
            }
            Err(error) => self.status = error.to_string(),
        }
    }

    fn poll_scan(&mut self) {
        let events = self
            .scan
            .as_ref()
            .map_or_else(Vec::new, |scan| scan.events.try_iter().collect());
        for event in events {
            match event {
                ScanEvent::Started(info) => {
                    self.status = String::from("Recovering QR transfers automatically…");
                    self.video_info = Some(info);
                }
                ScanEvent::FrameDecoded {
                    frame,
                    timestamp_seconds,
                } => {
                    self.last_frame = self.last_frame.max(frame);
                    self.last_timestamp_seconds =
                        self.last_timestamp_seconds.max(timestamp_seconds);
                }
                ScanEvent::QrDecoded(text) => self.record_payload(&text),
                ScanEvent::Finished {
                    elapsed_seconds,
                    frames,
                } => {
                    self.save_recovered_files();
                    self.finished_frames = Some(frames);
                    self.scan = None;
                    if self.has_incomplete_files() && !self.exhaustive_pass {
                        self.exhaustive_pass = true;
                        self.status =
                            String::from("Verifying every frame to recover the remaining blocks…");
                        self.begin_scan(ScanSettings::exhaustive());
                    } else if self.has_incomplete_files() {
                        self.status = format!(
                            "Recovery incomplete — {} block(s) are still missing. No incomplete file was written.",
                            self.missing_blocks()
                        );
                    } else {
                        self.status = format!(
                            "Recovery complete — {frames} frames checked in {elapsed_seconds:.1}s."
                        );
                    }
                }
            }
        }
    }

    fn record_payload(&mut self, text: &str) {
        if !self.seen_payloads.insert(text.to_owned()) {
            return;
        }
        let Ok(packet) = FilePacket::parse(text) else {
            return;
        };
        let key = format!("{}:{}", packet.filename, packet.sha256);
        let assembly = self
            .assemblies
            .entry(key)
            .or_insert_with(|| FileAssembly::new(&packet));
        let completed = assembly.insert(packet) && assembly.is_complete();
        if completed {
            self.save_recovered_files();
        }
    }

    fn recovery_folder(&self) -> Option<PathBuf> {
        self.video_path
            .as_deref()
            .and_then(Path::parent)
            .map(|folder| folder.join("Recovered from QR"))
    }

    fn save_recovered_files(&mut self) {
        let Some(folder) = self.recovery_folder() else {
            return;
        };
        if let Err(error) = fs::create_dir_all(&folder) {
            self.status = format!("Could not create recovery folder: {error}");
            return;
        }
        for assembly in self.assemblies.values_mut() {
            if assembly.recovered.is_none() || assembly.saved_path.is_some() {
                continue;
            }
            let safe_name = Path::new(&assembly.filename)
                .file_name()
                .filter(|name| !name.is_empty())
                .unwrap_or_else(|| std::ffi::OsStr::new("recovered.bin"));
            let path = unique_path(&folder.join(safe_name));
            if let Some(data) = &assembly.recovered {
                match fs::write(&path, data) {
                    Ok(()) => {
                        assembly.saved_path = Some(path);
                        self.status = String::from("Verified file recovered automatically.");
                    }
                    Err(error) => self.status = format!("Could not save recovered file: {error}"),
                }
            }
        }
    }

    fn completed_files(&self) -> usize {
        self.assemblies
            .values()
            .filter(|assembly| assembly.is_complete())
            .count()
    }

    fn has_incomplete_files(&self) -> bool {
        self.assemblies
            .values()
            .any(|assembly| !assembly.is_complete())
    }

    fn missing_blocks(&self) -> usize {
        self.assemblies
            .values()
            .map(|assembly| assembly.parts.saturating_sub(assembly.received_parts()))
            .sum()
    }
}

impl eframe::App for QrExtractorApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        self.poll_scan();
        if self.scan.is_none()
            && let Some(path) = ctx.input(|input| {
                input
                    .raw
                    .dropped_files
                    .iter()
                    .find_map(|file| file.path.clone())
            })
        {
            self.open_video(&path);
        }
        if self.scan.is_some() {
            ctx.request_repaint_after(std::time::Duration::from_millis(33));
        }

        egui::TopBottomPanel::top("header")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 15)))
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("QR Video Extractor").size(19.0).strong());
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if self.scan.is_some() {
                            if ui.button("Cancel").clicked()
                                && let Some(scan) = &self.scan
                            {
                                scan.cancel();
                                self.status = String::from("Finishing the current recovery…");
                            }
                        } else if ui.button("Choose video").clicked() {
                            self.choose_video();
                        }
                    });
                });
            });

        egui::CentralPanel::default()
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(42, 32)))
            .show(ctx, |ui| {
                if self.video_path.is_none() {
                    self.draw_welcome(ui);
                } else {
                    self.draw_recovery(ui);
                }
            });

        egui::TopBottomPanel::bottom("footer")
            .frame(egui::Frame::new().inner_margin(egui::Margin::symmetric(28, 12)))
            .show(ctx, |ui| {
                ui.label(&self.status);
            });
    }
}

impl QrExtractorApp {
    fn draw_welcome(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space(150.0);
            ui.label(
                RichText::new("Recover files from a QR video")
                    .size(30.0)
                    .strong(),
            );
            ui.add_space(10.0);
            ui.label("Drop a video here, or choose one. Everything else is automatic.");
            ui.add_space(24.0);
            if ui
                .add_sized([180.0, 42.0], egui::Button::new("Choose video"))
                .clicked()
            {
                self.choose_video();
            }
            ui.add_space(18.0);
            ui.small(
                "We detect the file name, type, blocks, checksum, and save verified files for you.",
            );
        });
    }

    #[allow(clippy::too_many_lines)]
    fn draw_recovery(&self, ui: &mut egui::Ui) {
        let source_name = self
            .video_path
            .as_deref()
            .and_then(Path::file_name)
            .map_or_else(
                || "Selected video".to_owned(),
                |name| name.to_string_lossy().into_owned(),
            );
        ui.label(RichText::new(source_name).size(26.0).strong());
        ui.add_space(5.0);
        if let Some(info) = &self.video_info {
            ui.small(format!(
                "{} × {} · {:.0} FPS",
                info.width, info.height, info.fps
            ));
        }
        ui.add_space(26.0);
        self.draw_progress(ui);
        ui.add_space(28.0);

        if self.assemblies.is_empty() {
            ui.vertical_centered(|ui| {
                ui.add_space(55.0);
                ui.label(RichText::new("Looking for QR transfers…").size(21.0));
                ui.add_space(6.0);
                ui.small("Files will appear here as soon as their first block is detected.");
            });
            return;
        }

        ui.label(
            RichText::new(format!(
                "Recovered files · {} verified",
                self.completed_files()
            ))
            .size(20.0)
            .strong(),
        );
        ui.add_space(10.0);
        egui::ScrollArea::vertical().show(ui, |ui| {
            for assembly in self.assemblies.values() {
                Self::draw_file_card(ui, assembly);
                ui.add_space(10.0);
            }
        });
    }

    fn draw_progress(&self, ui: &mut egui::Ui) {
        let label = if let Some(info) = &self.video_info {
            if let Some(duration) = info.duration_seconds.filter(|duration| *duration > 0.0) {
                #[allow(clippy::cast_possible_truncation)]
                let progress = (self.last_timestamp_seconds / duration).clamp(0.0, 1.0) as f32;
                ui.add(
                    egui::ProgressBar::new(progress)
                        .text(format!("Checking video · {:.0}%", progress * 100.0)),
                );
                return;
            }
            format!("Checking frame {}", self.last_frame)
        } else {
            "Reading video…".to_owned()
        };
        ui.add(
            egui::ProgressBar::new(0.0)
                .animate(self.scan.is_some())
                .text(label),
        );
    }

    fn draw_file_card(ui: &mut egui::Ui, assembly: &FileAssembly) {
        #[allow(clippy::cast_precision_loss)]
        let progress = assembly.received_parts() as f32 / assembly.parts as f32;
        ui.group(|ui| {
            ui.horizontal(|ui| {
                ui.label(RichText::new(&assembly.filename).size(17.0).strong());
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    let (label, color) = if assembly.is_complete() {
                        ("Verified", Color32::from_rgb(90, 190, 125))
                    } else {
                        ("Recovering", Color32::from_rgb(220, 175, 75))
                    };
                    ui.colored_label(color, RichText::new(label).strong());
                });
            });
            ui.small(assembly.file_type());
            ui.add_space(8.0);
            ui.add(egui::ProgressBar::new(progress).text(format!(
                "Blocks recovered: {} of {}",
                assembly.received_parts(),
                assembly.parts
            )));
            ui.add_space(6.0);
            ui.horizontal_wrapped(|ui| {
                ui.small("SHA-256");
                ui.monospace(&assembly.sha256);
            });
            if let Some(path) = &assembly.saved_path {
                ui.add_space(5.0);
                ui.colored_label(
                    Color32::from_rgb(90, 190, 125),
                    format!("Saved automatically to {}", path.display()),
                );
            } else if let Some(error) = &assembly.verification_error {
                ui.add_space(5.0);
                ui.colored_label(Color32::LIGHT_RED, error);
            }
        });
    }
}

fn unique_path(path: &Path) -> PathBuf {
    if !path.exists() {
        return path.to_owned();
    }
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .unwrap_or("recovered");
    let extension = path
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or("");
    for index in 1.. {
        let candidate = if extension.is_empty() {
            path.with_file_name(format!("{stem} ({index})"))
        } else {
            path.with_file_name(format!("{stem} ({index}).{extension}"))
        };
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("unbounded index iterator always yields an unused filename")
}
