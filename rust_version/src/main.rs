mod recorder;

use eframe::egui;
use recorder::{Recorder, RecordingConfig};
use std::path::PathBuf;
use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
struct MonitorInfo {
    name: String,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
}

fn get_monitors() -> Vec<MonitorInfo> {
    #[cfg(target_os = "linux")]
    {
        // Basic xrandr parsing for Linux
        let output = Command::new("xrandr").arg("--listmonitors").output();

        let mut monitors = Vec::new();

        if let Ok(output) = output {
            let stdout = String::from_utf8_lossy(&output.stdout);
            for line in stdout.lines() {
                // Format: 0: +*HDMI-1 1920/527x1080/296+0+0  HDMI-1
                if line.contains(':') && line.contains('+') && line.contains('x') {
                    let parts: Vec<&str> = line.split_whitespace().collect();
                    if parts.len() >= 3 {
                        if let Some(geo) = parts.iter().find(|p| p.contains('x') && p.contains('+'))
                        {
                            let plus_split: Vec<&str> = geo.split('+').collect();
                            if plus_split.len() >= 3 {
                                let x = plus_split[1].parse().unwrap_or(0);
                                let y = plus_split[2].parse().unwrap_or(0);

                                let size_part = plus_split[0];
                                let x_split: Vec<&str> = size_part.split('x').collect();
                                if x_split.len() >= 2 {
                                    let w_str = x_split[0].split('/').next().unwrap_or("1920");
                                    let h_str = x_split[1].split('/').next().unwrap_or("1080");

                                    let w: u32 = w_str.parse().unwrap_or(1920);
                                    let h: u32 = h_str.parse().unwrap_or(1080);

                                    let name = parts.last().unwrap_or(&"Unknown").to_string();

                                    monitors.push(MonitorInfo {
                                        name,
                                        width: w,
                                        height: h,
                                        x,
                                        y,
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        if monitors.is_empty() {
            monitors.push(MonitorInfo {
                name: "Default (Full Screen)".to_string(),
                width: 1920,
                height: 1080,
                x: 0,
                y: 0,
            });
        }
        return monitors;
    }

    #[cfg(target_os = "windows")]
    {
        // On Windows, return a single "Primary/Full Desktop" monitor.
        // gdigrab with -i desktop captures the entire virtual desktop.
        // Region selection is supported via -offset_x/-offset_y/-video_size.
        vec![MonitorInfo {
            name: "Primary/Full Desktop".to_string(),
            width: 1920, // Default fallback. Users can customize region.
            height: 1080,
            x: 0,
            y: 0,
        }]
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows")))]
    {
        vec![MonitorInfo {
            name: "Default".to_string(),
            width: 1920,
            height: 1080,
            x: 0,
            y: 0,
        }]
    }
}

struct ScreenRecorderApp {
    recorder: Recorder,
    monitors: Vec<MonitorInfo>,
    selected_monitor_index: usize,

    // Config state
    output_dir: PathBuf,
    filename: String,
    format: String, // "mp4", "webm"
    audio_enabled: bool,
    audio_device: String,
    ffmpeg_path: String,

    // Region state
    region_custom: bool,
    reg_x: i32,
    reg_y: i32,
    reg_w: u32,
    reg_h: u32,

    status_message: String,
}

impl ScreenRecorderApp {
    fn new(cc: &eframe::CreationContext<'_>) -> Self {
        let monitors = get_monitors();

        // Default paths
        let output_dir = if let Some(user_dirs) = directories::UserDirs::new() {
            user_dirs
                .video_dir()
                .unwrap_or(user_dirs.home_dir())
                .to_path_buf()
        } else {
            PathBuf::from(".")
        };

        let default_mon = monitors.first().unwrap();

        // Detect FFmpeg
        let ffmpeg_path = if std::path::Path::new("ffmpeg.exe").exists() {
            "ffmpeg.exe".to_string()
        } else if std::path::Path::new("ffmpeg").exists() {
            "./ffmpeg".to_string()
        } else {
            "ffmpeg".to_string()
        };

        Self {
            recorder: Recorder::new(),
            monitors: monitors.clone(),
            selected_monitor_index: 0,
            output_dir,
            filename: "recording.mp4".to_string(),
            format: "mp4".to_string(),
            audio_enabled: false,
            audio_device: "default".to_string(),
            ffmpeg_path,
            region_custom: false,
            reg_x: default_mon.x,
            reg_y: default_mon.y,
            reg_w: default_mon.width,
            reg_h: default_mon.height,
            status_message: "Ready".to_string(),
        }
    }
}

impl eframe::App for ScreenRecorderApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("Rust Screen Recorder");
            ui.separator();

            // Status Section
            ui.horizontal(|ui| {
                ui.label("Status: ");
                let duration = self.recorder.get_duration();
                let time_str = format!(
                    "{:02}:{:02}",
                    duration.as_secs() / 60,
                    duration.as_secs() % 60
                );

                if self.recorder.is_recording() {
                    if self.recorder.is_paused() {
                        ui.colored_label(egui::Color32::YELLOW, format!("Paused ({})", time_str));
                    } else {
                        ui.colored_label(
                            egui::Color32::RED,
                            format!("Recording... ({})", time_str),
                        );
                        ctx.request_repaint(); // Animation
                    }
                } else {
                    ui.label("Idle");
                }
            });

            if !self.status_message.is_empty() {
                ui.small(&self.status_message);
            }
            ui.separator();

            // FFmpeg Configuration
            ui.add_enabled_ui(!self.recorder.is_recording(), |ui| {
                ui.collapsing("FFmpeg Configuration", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Path:");
                        ui.text_edit_singleline(&mut self.ffmpeg_path);
                        if ui.button("Select...").clicked() {
                            if let Some(path) = rfd::FileDialog::new()
                                .add_filter("Executables", &["exe", ""])
                                .pick_file()
                            {
                                self.ffmpeg_path = path.to_string_lossy().to_string();
                            }
                        }
                    });
                    ui.small("If not in PATH, select the ffmpeg executable manually.");
                });
            });
            ui.separator();

            // Settings (Disable if recording)
            ui.add_enabled_ui(!self.recorder.is_recording(), |ui| {
                // Monitor Selection
                ui.horizontal(|ui| {
                    ui.label("Monitor:");
                    egui::ComboBox::from_id_source("monitor_combo")
                        .selected_text(&self.monitors[self.selected_monitor_index].name)
                        .show_ui(ui, |ui| {
                            for (i, mon) in self.monitors.iter().enumerate() {
                                if ui
                                    .selectable_value(
                                        &mut self.selected_monitor_index,
                                        i,
                                        &mon.name,
                                    )
                                    .clicked()
                                {
                                    // Reset region to monitor if not custom
                                    if !self.region_custom {
                                        self.reg_x = mon.x;
                                        self.reg_y = mon.y;
                                        self.reg_w = mon.width;
                                        self.reg_h = mon.height;
                                    }
                                }
                            }
                        });
                });

                // Region Selection
                ui.collapsing("Region / Crop", |ui| {
                    ui.checkbox(&mut self.region_custom, "Custom Region");
                    if self.region_custom {
                        ui.horizontal(|ui| {
                            ui.label("X:");
                            ui.add(egui::DragValue::new(&mut self.reg_x));
                            ui.label("Y:");
                            ui.add(egui::DragValue::new(&mut self.reg_y));
                        });
                        ui.horizontal(|ui| {
                            ui.label("W:");
                            ui.add(egui::DragValue::new(&mut self.reg_w));
                            ui.label("H:");
                            ui.add(egui::DragValue::new(&mut self.reg_h));
                        });
                    }
                    if ui.button("Reset to Monitor Size").clicked() {
                        let mon = &self.monitors[self.selected_monitor_index];
                        self.reg_x = mon.x;
                        self.reg_y = mon.y;
                        self.reg_w = mon.width;
                        self.reg_h = mon.height;
                        self.region_custom = false;
                    }
                });

                // Audio
                ui.collapsing("Audio", |ui| {
                    ui.checkbox(&mut self.audio_enabled, "Record Audio");
                    if self.audio_enabled {
                        ui.horizontal(|ui| {
                            ui.label("Device:");
                            ui.text_edit_singleline(&mut self.audio_device);
                        });
                        #[cfg(target_os = "linux")]
                        ui.small("Hint: 'default' or 'hw:0,0' (ALSA)");
                        #[cfg(target_os = "windows")]
                        ui.small("Hint: 'virtual-audio-capturer' or device name (dshow)");
                    }
                });

                // Output
                ui.collapsing("Output", |ui| {
                    ui.horizontal(|ui| {
                        ui.label("Path:");
                        ui.label(self.output_dir.to_string_lossy());
                        if ui.button("Browse...").clicked() {
                            if let Some(path) = rfd::FileDialog::new().pick_folder() {
                                self.output_dir = path;
                            }
                        }
                    });
                    ui.horizontal(|ui| {
                        ui.label("Filename:");
                        ui.text_edit_singleline(&mut self.filename);
                    });
                    ui.horizontal(|ui| {
                        ui.label("Format:");
                        egui::ComboBox::from_id_source("fmt_combo")
                            .selected_text(&self.format)
                            .show_ui(ui, |ui| {
                                ui.selectable_value(
                                    &mut self.format,
                                    "mp4".to_string(),
                                    "MP4 (H.264)",
                                );
                                ui.selectable_value(
                                    &mut self.format,
                                    "webm".to_string(),
                                    "WebM (VP9)",
                                );
                            });
                    });
                });
            });

            ui.separator();

            // Controls
            ui.horizontal(|ui| {
                if !self.recorder.is_recording() {
                    if ui.button("🔴 Record").clicked() {
                        let path = self.output_dir.join(&self.filename);
                        let config = RecordingConfig {
                            output_path: path.clone(),
                            width: self.reg_w,
                            height: self.reg_h,
                            x: self.reg_x,
                            y: self.reg_y,
                            audio_enabled: self.audio_enabled,
                            audio_device: self.audio_device.clone(),
                            container_format: self.format.clone(),
                            ffmpeg_path: self.ffmpeg_path.clone(),
                        };

                        match self.recorder.start(&config) {
                            Ok(_) => self.status_message = format!("Recording to {:?}", path),
                            Err(e) => self.status_message = format!("Error: {}", e),
                        }
                    }
                } else {
                    if ui.button("⏹ Stop").clicked() {
                        match self.recorder.stop() {
                            Ok(_) => self.status_message = "Saved.".to_string(),
                            Err(e) => self.status_message = format!("Error stopping: {}", e),
                        }
                    }

                    if self.recorder.is_paused() {
                        if ui.button("▶ Resume").clicked() {
                            match self.recorder.resume() {
                                Ok(_) => {}
                                Err(e) => self.status_message = format!("Error: {}", e),
                            }
                        }
                    } else {
                        // Pause only works on Linux for now
                        #[cfg(target_os = "linux")]
                        if ui.button("⏸ Pause").clicked() {
                            match self.recorder.pause() {
                                Ok(_) => {}
                                Err(e) => self.status_message = format!("Error: {}", e),
                            }
                        }
                        #[cfg(not(target_os = "linux"))]
                        ui.add_enabled(false, egui::Button::new("⏸ Pause"));
                    }
                }
            });

            // Add Open Folder button
            if !self.recorder.is_recording() && ui.button("Open Output Folder").clicked() {
                let _ = open::that(&self.output_dir);
            }
        });
    }
}

fn main() -> eframe::Result<()> {
    // Log info
    println!("Starting Screen Recorder...");

    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([400.0, 450.0])
            .with_min_inner_size([300.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Screen Recorder",
        native_options,
        Box::new(|cc| Ok(Box::new(ScreenRecorderApp::new(cc)))),
    )
}
