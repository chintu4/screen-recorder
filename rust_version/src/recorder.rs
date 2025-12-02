use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use std::path::PathBuf;

#[derive(Clone, Debug)]
pub struct RecordingConfig {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub audio_enabled: bool,
    pub audio_device: String, // e.g., "default" or "hw:0,0"
    pub container_format: String, // "mp4", "webm"
}

pub struct Recorder {
    child: Option<Child>,
    start_time: Option<Instant>,
    paused_duration: Duration,
    last_pause_time: Option<Instant>,
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

        // Input: Screen (Linux x11grab)
        // Note: In a real cross-platform app, we would detect OS here.
        // Assuming Linux/X11 for this environment.
        cmd.arg("-f").arg("x11grab")
           .arg("-video_size").arg(format!("{}x{}", config.width, config.height))
           .arg("-framerate").arg("30")
           .arg("-i").arg(format!(":0.0+{},{}", config.x, config.y));

        // Input: Audio
        if config.audio_enabled {
            cmd.arg("-f").arg("alsa")
               .arg("-i").arg(&config.audio_device);
            // Sync audio/video
            cmd.arg("-ac").arg("2");
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
                   .arg("-crf").arg("23")
                   .arg("-pix_fmt").arg("yuv420p"); // compatible with most players
            }
        }

        // Overwrite output
        cmd.arg("-y").arg(&config.output_path);

        // Suppress output to keep stdout clean, but maybe log stderr to a file in production
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        let child = cmd.spawn().map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

        self.child = Some(child);
        self.start_time = Some(Instant::now());
        self.paused_duration = Duration::new(0, 0);
        self.last_pause_time = None;

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            // Send 'q' to stdin? Or SIGTERM.
            // FFmpeg usually handles SIGTERM (kill) gracefully by finishing the file.
            // Just killing child might corrupt file if it doesn't flush.
            // But std::process::Child doesn't support signals easily.
            // We can try to send SIGTERM via `kill` command.

            #[cfg(target_os = "linux")]
            {
                let _ = Command::new("kill")
                    .arg("-SIGTERM")
                    .arg(child.id().to_string())
                    .output();

                // Wait a bit for it to finish
                match child.wait_timeout(Duration::from_secs(5)) {
                    Ok(Some(_)) => {}, // Exited
                    Ok(None) => {
                        // Timed out, force kill
                        let _ = child.kill();
                        let _ = child.wait();
                    },
                    Err(_) => {
                        let _ = child.kill();
                        let _ = child.wait();
                    }
                }
            }
            #[cfg(not(target_os = "linux"))]
            {
                // Fallback for non-linux
                let _ = child.kill();
                let _ = child.wait();
            }

            self.start_time = None;
            self.last_pause_time = None;
            return Ok(());
        }
        Err("Not recording".to_string())
    }

    pub fn pause(&mut self) -> Result<(), String> {
        if let Some(ref child) = self.child {
            if self.last_pause_time.is_some() {
                return Err("Already paused".to_string());
            }

            // Send SIGSTOP
            #[cfg(target_os = "linux")]
            {
                let _ = Command::new("kill")
                    .arg("-SIGSTOP")
                    .arg(child.id().to_string())
                    .output();
            }

            self.last_pause_time = Some(Instant::now());
            Ok(())
        } else {
            Err("Not recording".to_string())
        }
    }

    pub fn resume(&mut self) -> Result<(), String> {
        if let Some(ref child) = self.child {
            if let Some(pause_time) = self.last_pause_time {
                // Send SIGCONT
                #[cfg(target_os = "linux")]
                {
                    let _ = Command::new("kill")
                        .arg("-SIGCONT")
                        .arg(child.id().to_string())
                        .output();
                }

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

// Helper trait to wait with timeout (unstable in std, using a simple spin wait or custom impl is needed or external crate)
// But since we don't want to add `wait-timeout` crate just for this, we will use a naive implementation or just wait.
// Actually, `wait_timeout` is not in std. I'll just use `wait` or a loop with try_wait.
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
