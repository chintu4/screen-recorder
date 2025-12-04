use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use std::path::PathBuf;
#[cfg(target_os = "windows")]
use std::io::Write; // Needed for writing to stdin

#[derive(Clone, Debug, PartialEq)]
pub enum RecordingMode {
    Screen,
    Camera,
    PiP, // Screen + Camera
}

#[derive(Clone, Debug)]
pub struct RecordingConfig {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub mode: RecordingMode,
    pub camera_device: String,
    pub audio_enabled: bool,
    pub audio_device: String, // e.g., "default" or "Microphone (Realtek Audio)"
    pub container_format: String, // "mp4", "webm"
}

pub struct Recorder {
    child: Option<Child>,
    start_time: Option<Instant>,
    paused_duration: Duration,
    last_pause_time: Option<Instant>,
}

pub fn get_audio_devices() -> Vec<String> {
    let mut devices = Vec::new();

    #[cfg(target_os = "windows")]
    {
        // Run ffmpeg -list_devices true -f dshow -i dummy
        // The output is in stderr.
        let output = Command::new("ffmpeg")
            .arg("-list_devices").arg("true")
            .arg("-f").arg("dshow")
            .arg("-i").arg("dummy")
            .output();

        if let Ok(output) = output {
            let stderr = String::from_utf8_lossy(&output.stderr);
            let mut in_audio_section = false;
            for line in stderr.lines() {
                if line.contains("DirectShow video devices") {
                    in_audio_section = false;
                } else if line.contains("DirectShow audio devices") {
                    in_audio_section = true;
                } else if in_audio_section {
                    // Lines look like:
                    // [dshow @ ...]  "Microphone (Realtek Audio)"
                    // [dshow @ ...]     Alternative name "..."
                    if let Some(start_quote) = line.find('"') {
                        if let Some(end_quote) = line[start_quote+1..].find('"') {
                             let device_name = &line[start_quote+1..start_quote+1+end_quote];
                             // Avoid "Alternative name" lines usually
                             if !line.contains("Alternative name") {
                                 devices.push(device_name.to_string());
                             }
                        }
                    }
                }
            }
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Linux: for now just return "default" and maybe some common ones
        devices.push("default".to_string());
        devices.push("hw:0,0".to_string());
    }

    if devices.is_empty() {
        devices.push("default".to_string());
    }

    devices
}

impl Recorder {
    pub fn new() -> Self {
        Self {
            child: None,
            start_time: None,
            paused_duration: Duration::new(0, 0),
            last_pause_time: None,
        }
    }

    pub fn start(&mut self, config: &RecordingConfig) -> Result<(), String> {
        if self.child.is_some() {
            return Err("Already recording".to_string());
        }

        let mut cmd = Command::new("ffmpeg");

        // --- Input 1: Desktop / Primary Video Source ---
        match config.mode {
            RecordingMode::Screen | RecordingMode::PiP => {
                #[cfg(target_os = "windows")]
                {
                    // Windows: gdigrab
                    cmd.arg("-f").arg("gdigrab")
                       .arg("-framerate").arg("30")
                       .arg("-offset_x").arg(config.x.to_string())
                       .arg("-offset_y").arg(config.y.to_string())
                       .arg("-video_size").arg(format!("{}x{}", config.width, config.height))
                       .arg("-i").arg("desktop");
                }
                #[cfg(not(target_os = "windows"))]
                {
                     // Linux: x11grab (Assuming X11)
                    cmd.arg("-f").arg("x11grab")
                       .arg("-video_size").arg(format!("{}x{}", config.width, config.height))
                       .arg("-framerate").arg("30")
                       .arg("-i").arg(format!(":0.0+{},{}", config.x, config.y));
                }
            },
            RecordingMode::Camera => {
                // If Camera only mode, the camera is the primary input [0:v]
                #[cfg(target_os = "windows")]
                {
                    cmd.arg("-f").arg("dshow")
                       .arg("-i").arg(format!("video={}", config.camera_device));
                }
                #[cfg(not(target_os = "windows"))]
                {
                    cmd.arg("-f").arg("v4l2")
                       .arg("-framerate").arg("30")
                       .arg("-video_size").arg("640x480") // Default safe resolution
                       .arg("-i").arg(&config.camera_device);
                }
            }
        }

        // --- Input 2: Camera (Only for PiP) ---
        if config.mode == RecordingMode::PiP {
             #[cfg(target_os = "windows")]
             {
                 cmd.arg("-f").arg("dshow")
                    .arg("-video_size").arg("320x240") // Fixed small size for PiP
                    .arg("-i").arg(format!("video={}", config.camera_device));
             }
             #[cfg(not(target_os = "windows"))]
             {
                 cmd.arg("-f").arg("v4l2")
                    .arg("-framerate").arg("30")
                    .arg("-video_size").arg("320x240")
                    .arg("-i").arg(&config.camera_device);
             }
        }

        // --- Input 3 (or 2): Audio ---
        if config.audio_enabled {
            #[cfg(target_os = "windows")]
            {
                cmd.arg("-f").arg("dshow")
                   .arg("-i").arg(format!("audio={}", config.audio_device));
            }
            #[cfg(not(target_os = "windows"))]
            {
                cmd.arg("-f").arg("alsa")
                   .arg("-i").arg(&config.audio_device);
            }
            cmd.arg("-ac").arg("2");
        }

        // --- Filter Complex (For PiP) ---
        if config.mode == RecordingMode::PiP {
            // [0:v] is desktop, [1:v] is camera
            // Overlay camera on desktop at top right with 10px padding
            // main_w - overlay_w - 10 : 10
            cmd.arg("-filter_complex").arg("[0:v][1:v] overlay=main_w-overlay_w-10:10");
        }

        // Encoding options
        // Use libx264 for mp4, libvpx-vp9 for webm
        match config.container_format.as_str() {
            "webm" => {
                cmd.arg("-c:v").arg("libvpx-vp9")
                   .arg("-b:v").arg("2M"); // basic bitrate
            }
            _ => { // default mp4
                cmd.arg("-c:v").arg("libx264")
                   .arg("-preset").arg("ultrafast") // fast encoding for real-time
                   .arg("-crf").arg("23");

                // Ensure pixel format is valid (yuv420p is safe)
                cmd.arg("-pix_fmt").arg("yuv420p");
            }
        }

        // Overwrite output
        cmd.arg("-y").arg(&config.output_path);

        // Crucial for Windows stopping: We need to write to stdin.
        cmd.stdin(Stdio::piped());

        // Use inherit so user sees ffmpeg logs in terminal
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::inherit());

        let child = cmd.spawn().map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

        self.child = Some(child);
        self.start_time = Some(Instant::now());
        self.paused_duration = Duration::new(0, 0);
        self.last_pause_time = None;

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            #[cfg(target_os = "windows")]
            {
                // On Windows, killing the process corrupts the MP4.
                // We must send 'q' to stdin.
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"q");
                }
                // Wait for it to finish gracefully
                match child.wait_timeout(Duration::from_secs(5)) {
                     Ok(Some(_)) => {},
                     Ok(None) => {
                         // Timeout, force kill
                         let _ = child.kill();
                         let _ = child.wait();
                     },
                     Err(_) => {
                         let _ = child.kill();
                         let _ = child.wait();
                     }
                }
            }

            #[cfg(not(target_os = "windows"))]
            {
                // Linux: SIGTERM is standard and works well.
                let _ = Command::new("kill")
                    .arg("-SIGTERM")
                    .arg(child.id().to_string())
                    .output();

                match child.wait_timeout(Duration::from_secs(5)) {
                    Ok(Some(_)) => {},
                    Ok(None) => {
                        let _ = child.kill();
                        let _ = child.wait();
                    },
                    Err(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }

            self.start_time = None;
            self.last_pause_time = None;
            return Ok(());
        }
        Err("Not recording".to_string())
    }

    pub fn pause(&mut self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
            return Err("Pause not supported on Windows".to_string());
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Some(ref child) = self.child {
                if self.last_pause_time.is_some() {
                    return Err("Already paused".to_string());
                }

                let _ = Command::new("kill")
                    .arg("-SIGSTOP")
                    .arg(child.id().to_string())
                    .output();

                self.last_pause_time = Some(Instant::now());
                Ok(())
            } else {
                Err("Not recording".to_string())
            }
        }
    }

    pub fn resume(&mut self) -> Result<(), String> {
        #[cfg(target_os = "windows")]
        {
             return Err("Resume not supported on Windows".to_string());
        }

        #[cfg(not(target_os = "windows"))]
        {
            if let Some(ref child) = self.child {
                if let Some(pause_time) = self.last_pause_time {
                    let _ = Command::new("kill")
                        .arg("-SIGCONT")
                        .arg(child.id().to_string())
                        .output();

                    self.paused_duration += pause_time.elapsed();
                    self.last_pause_time = None;
                    Ok(())
                } else {
                    Err("Not paused".to_string())
                }
            } else {
                Err("Not recording".to_string())
            }
        }
    }

    pub fn is_recording(&self) -> bool {
        self.child.is_some()
    }

    pub fn is_paused(&self) -> bool {
        self.last_pause_time.is_some()
    }

    pub fn get_duration(&self) -> Duration {
        if let Some(start) = self.start_time {
            let current_duration = if let Some(pause_time) = self.last_pause_time {
                pause_time.duration_since(start)
            } else {
                start.elapsed()
            };
            current_duration.saturating_sub(self.paused_duration)
        } else {
            Duration::new(0, 0)
        }
    }
}

trait WaitTimeout {
    fn wait_timeout(&mut self, duration: Duration) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl WaitTimeout for Child {
    fn wait_timeout(&mut self, duration: Duration) -> std::io::Result<Option<std::process::ExitStatus>> {
        let start = Instant::now();
        loop {
            match self.try_wait() {
                Ok(Some(status)) => return Ok(Some(status)),
                Ok(None) => {
                    if start.elapsed() >= duration {
                        return Ok(None);
                    }
                    std::thread::sleep(Duration::from_millis(50));
                }
                Err(e) => return Err(e),
            }
        }
    }
}
