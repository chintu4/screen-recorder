mod recorder;
mod device_list;

use display_info::DisplayInfo;
use eframe::egui;
use recorder::{Recorder, RecordingConfig, RecordingMode};
use device_list::{Device, get_video_devices, get_audio_devices};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq)]
struct MonitorInfo {
    name: String,
    width: u32,
    height: u32,
    x: i32,
    y: i32,
}

fn get_monitors() -> Vec<MonitorInfo> {
    let mut monitors = Vec::new();

    if let Ok(display_infos) = DisplayInfo::all() {
        for (i, info) in display_infos.iter().enumerate() {
            monitors.push(MonitorInfo {
                name: if info.is_primary {
                    format!("Monitor {} (Primary)", i + 1)
                } else {
                    format!("Monitor {}", i + 1)
                },
                width: info.width,
                height: info.height,
                x: info.x,
                y: info.y,
            });
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

    monitors
}

struct ScreenRecorderApp {
    recorder: Recorder,
    monitors: Vec<MonitorInfo>,
    selected_monitor_index: usize,

    // Devices
    video_devices: Vec<Device>,
    audio_devices: Vec<Device>,

    // Config state
    mode: RecordingMode,
    output_dir: PathBuf,
    filename: String,
    format: String, // "mp4", "webm"
    audio_enabled: bool,
    audio_devices: Vec<String>,
    selected_audio_device: String,

    // Region state
    region_custom: bool,
    reg_x: i32,
    reg_y: i32,
    reg_w: u32,
    reg_h: u32,

    status_message: String,
}

impl ScreenRecorderApp {
    fn new(_cc: &eframe::CreationContext<'_>) -> Self {
        let monitors = get_monitors();
        let video_devices = get_video_devices();
        let audio_devices = get_audio_devices();

        // Default paths
        let output_dir = if let Some(user_dirs) = directories::UserDirs::new() {
            user_dirs.video_dir().unwrap_or(user_dirs.home_dir()).to_path_buf()
        } else {
            PathBuf::from(".")
        };

        // Ensure we default to a safe monitor if something goes wrong
        let default_mon = monitors.first().unwrap();

        // Initial audio scan
        let audio_devices = recorder::get_audio_devices();
        let selected_audio_device = audio_devices.first().cloned().unwrap_or_else(|| "default".to_string());

        Self {
            recorder: Recorder::new(),
            monitors: monitors.clone(),
            selected_monitor_index: 0,
            video_devices,
            audio_devices,
            mode: RecordingMode::Screen,
            output_dir,
            filename: "recording.mp4".to_string(),
            format: "mp4".to_string(),
            audio_enabled: false,
            audio_devices,
            selected_audio_device,
            region_custom: false,
            reg_x: default_mon.x,
            reg_y: default_mon.y,
            reg_w: default_mon.width,
            reg_h: default_mon.height,
            status_message: "Ready".to_string(),
        }
    }

    fn refresh_audio_devices(&mut self) {
        self.audio_devices = recorder::get_audio_devices();
        if !self.audio_devices.contains(&self.selected_audio_device) {
             if let Some(first) = self.audio_devices.first() {
                 self.selected_audio_device = first.clone();
             }
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
                let time_str = format!("{:02}:{:02}", duration.as_secs() / 60, duration.as_secs() % 60);

                if self.recorder.is_recording() {
                    if self.recorder.is_paused() {
                        ui.colored_label(egui::Color32::YELLOW, format!("Paused ({})", time_str));
                    } else {
                        ui.colored_label(egui::Color32::RED, format!("Recording... ({})", time_str));
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

            // Settings (Disable if recording)
            ui.add_enabled_ui(!self.recorder.is_recording(), |ui| {

                // Mode Selection
                ui.horizontal(|ui| {
                    ui.label("Mode:");
                    egui::ComboBox::from_id_source("mode_combo")
                        .selected_text(match self.mode {
                            RecordingMode::Screen => "Screen Only",
                            RecordingMode::Camera => "Camera Only",
                            RecordingMode::PiP => "Screen + Camera",
                        })
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.mode, RecordingMode::Screen, "Screen Only");
                            ui.selectable_value(&mut self.mode, RecordingMode::Camera, "Camera Only");
                            ui.selectable_value(&mut self.mode, RecordingMode::PiP, "Screen + Camera");
                        });
                });

                // Monitor Selection (Only for Screen modes)
                if self.mode != RecordingMode::Camera {
                    ui.horizontal(|ui| {
                        ui.label("Monitor:");
                        egui::ComboBox::from_id_source("monitor_combo")
                            .selected_text(&self.monitors[self.selected_monitor_index].name)
                            .show_ui(ui, |ui| {
                                for (i, mon) in self.monitors.iter().enumerate() {
                                    if ui.selectable_value(&mut self.selected_monitor_index, i, &mon.name).clicked() {
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
                                ui.label("X:"); ui.add(egui::DragValue::new(&mut self.reg_x));
                                ui.label("Y:"); ui.add(egui::DragValue::new(&mut self.reg_y));
                            });
                            ui.horizontal(|ui| {
                                ui.label("W:"); ui.add(egui::DragValue::new(&mut self.reg_w));
                                ui.label("H:"); ui.add(egui::DragValue::new(&mut self.reg_h));
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
                }

                // Camera Selection (Only for Camera or PiP modes)
                if self.mode != RecordingMode::Screen {
                    ui.horizontal(|ui| {
                        ui.label("Camera:");
                        if self.video_devices.is_empty() {
                            ui.colored_label(egui::Color32::RED, "No cameras found");
                        } else {
                            egui::ComboBox::from_id_source("camera_combo")
                                .selected_text(&self.video_devices[self.selected_video_device_index].name)
                                .show_ui(ui, |ui| {
                                    for (i, dev) in self.video_devices.iter().enumerate() {
                                        ui.selectable_value(&mut self.selected_video_device_index, i, &dev.name);
                                    }
                                });
                        }
                    });
                }

                // Audio
                ui.collapsing("Audio", |ui| {
                    ui.checkbox(&mut self.audio_enabled, "Record Audio");
                    if self.audio_enabled {
                        ui.horizontal(|ui| {
                            ui.label("Device:");
                            egui::ComboBox::from_id_source("audio_combo")
                                .selected_text(&self.selected_audio_device)
                                .width(200.0)
                                .show_ui(ui, |ui| {
                                    for dev in &self.audio_devices {
                                        ui.selectable_value(&mut self.selected_audio_device, dev.clone(), dev);
                                    }
                                });

                            if ui.button("🔄").on_hover_text("Refresh Devices").clicked() {
                                self.refresh_audio_devices();
                            }
                        });
                        ui.small("Select your input device (e.g., Microphone)");
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
                                ui.selectable_value(&mut self.format, "mp4".to_string(), "MP4 (H.264)");
                                ui.selectable_value(&mut self.format, "webm".to_string(), "WebM (VP9)");
                            });
                    });
                });
            });

            ui.separator();

            // Controls
            ui.horizontal(|ui| {
                if !self.recorder.is_recording() {
                    let can_record = if self.mode != RecordingMode::Screen && self.video_devices.is_empty() {
                        false
                    } else if self.audio_enabled && self.audio_devices.is_empty() {
                        false
                    } else {
                        true
                    };

                    if ui.add_enabled(can_record, egui::Button::new("🔴 Record")).clicked() {
                        let path = self.output_dir.join(&self.filename);

                        let camera_dev = if !self.video_devices.is_empty() {
                            self.video_devices[self.selected_video_device_index].id.clone()
                        } else {
                            String::new()
                        };

                        let audio_dev = if !self.audio_devices.is_empty() {
                             self.audio_devices[self.selected_audio_device_index].id.clone()
                        } else {
                             "default".to_string()
                        };

                        let config = RecordingConfig {
                            output_path: path.clone(),
                            width: self.reg_w,
                            height: self.reg_h,
                            x: self.reg_x,
                            y: self.reg_y,
                            mode: self.mode.clone(),
                            camera_device: camera_dev,
                            audio_enabled: self.audio_enabled,
                            audio_device: self.selected_audio_device.clone(),
                            container_format: self.format.clone(),
                        };

                        match self.recorder.start(&config) {
                            Ok(_) => self.status_message = format!("Recording to {:?}", path),
                            Err(e) => self.status_message = format!("Error: {}", e),
                        }
                    }

                    if !can_record {
                        ui.small("Missing required devices");
                    }
                } else {
                    if ui.button("⏹ Stop").clicked() {
                        match self.recorder.stop() {
                            Ok(_) => self.status_message = "Saved.".to_string(),
                            Err(e) => self.status_message = format!("Error stopping: {}", e),
                        }
                    }

                    // Pause/Resume Logic
                    if cfg!(target_os = "windows") {
                        // Disable buttons on Windows
                        ui.add_enabled(false, egui::Button::new("⏸ Pause")).on_disabled_hover_text("Pause is not supported on Windows");
                    } else {
                        if self.recorder.is_paused() {
                            if ui.button("▶ Resume").clicked() {
                                let _ = self.recorder.resume();
                            }
                        } else {
                            if ui.button("⏸ Pause").clicked() {
                                let _ = self.recorder.pause();
                            }
                        }
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
            .with_inner_size([400.0, 500.0])
            .with_min_inner_size([300.0, 300.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Screen Recorder",
        native_options,
        Box::new(|cc| Ok(Box::new(ScreenRecorderApp::new(cc)))),
    )
}
