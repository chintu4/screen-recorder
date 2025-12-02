use std::io::Write;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Clone, Debug)]
pub struct RecordingConfig {
    pub output_path: PathBuf,
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    pub audio_enabled: bool,
    pub audio_device: String,     // e.g., "default" or "hw:0,0"
    pub container_format: String, // "mp4", "webm"
    pub ffmpeg_path: String,
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

        let mut cmd = Command::new(&config.ffmpeg_path);

        #[cfg(target_os = "windows")]
        {
            // Windows: Use gdigrab for screen
            cmd.arg("-f").arg("gdigrab").arg("-framerate").arg("30");

            // Region Selection
            cmd.arg("-offset_x")
                .arg(config.x.to_string())
                .arg("-offset_y")
                .arg(config.y.to_string())
                .arg("-video_size")
                .arg(format!("{}x{}", config.width, config.height));

            cmd.arg("-i").arg("desktop");

            if config.audio_enabled {
                // Windows: Use dshow for audio
                cmd.arg("-f")
                    .arg("dshow")
                    .arg("-i")
                    .arg(format!("audio={}", config.audio_device));
                cmd.arg("-ac").arg("2");
            }
        }

        #[cfg(not(target_os = "windows"))]
        {
            // Linux/X11
            cmd.arg("-f")
                .arg("x11grab")
                .arg("-video_size")
                .arg(format!("{}x{}", config.width, config.height))
                .arg("-framerate")
                .arg("30")
                .arg("-i")
                .arg(format!(":0.0+{},{}", config.x, config.y));

            if config.audio_enabled {
                cmd.arg("-f")
                    .arg("alsa")
                    .arg("-i")
                    .arg(&config.audio_device);
                cmd.arg("-ac").arg("2");
            }
        }

        // Encoding options
        match config.container_format.as_str() {
            "webm" => {
                cmd.arg("-c:v").arg("libvpx-vp9").arg("-b:v").arg("2M");
            }
            _ => {
                cmd.arg("-c:v")
                    .arg("libx264")
                    .arg("-preset")
                    .arg("ultrafast")
                    .arg("-crf")
                    .arg("23")
                    .arg("-pix_fmt")
                    .arg("yuv420p");
            }
        }

        cmd.arg("-y").arg(&config.output_path);

        // Pipe stdin to allow sending 'q' to stop gracefully
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::null());
        cmd.stderr(Stdio::null());

        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to start ffmpeg: {}", e))?;

        self.child = Some(child);
        self.start_time = Some(Instant::now());
        self.paused_duration = Duration::new(0, 0);
        self.last_pause_time = None;

        Ok(())
    }

    pub fn stop(&mut self) -> Result<(), String> {
        if let Some(mut child) = self.child.take() {
            #[cfg(target_os = "linux")]
            {
                // Try SIGTERM first on Linux
                let _ = Command::new("kill")
                    .arg("-SIGTERM")
                    .arg(child.id().to_string())
                    .output();

                // Also try writing 'q' just in case
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"q");
                }
            }

            #[cfg(target_os = "windows")]
            {
                // On Windows, write 'q' to stdin to stop gracefully
                if let Some(mut stdin) = child.stdin.take() {
                    let _ = stdin.write_all(b"q");
                }
            }

            // Wait for finish
            match child.wait_timeout(Duration::from_secs(5)) {
                Ok(Some(_)) => {}
                Ok(None) => {
                    // Timed out, force kill
                    let _ = child.kill();
                    let _ = child.wait();
                }
                Err(_) => {
                    let _ = child.kill();
                    let _ = child.wait();
                }
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

            #[cfg(target_os = "linux")]
            {
                let _ = Command::new("kill")
                    .arg("-SIGSTOP")
                    .arg(child.id().to_string())
                    .output();
                self.last_pause_time = Some(Instant::now());
                Ok(())
            }
            #[cfg(target_os = "windows")]
            {
                Err("Pause not supported on Windows".to_string())
            }
            #[cfg(not(any(target_os = "linux", target_os = "windows")))]
            {
                Err("Pause not supported".to_string())
            }
        } else {
            Err("Not recording".to_string())
        }
    }

    pub fn resume(&mut self) -> Result<(), String> {
        if let Some(ref child) = self.child {
            if let Some(pause_time) = self.last_pause_time {
                #[cfg(target_os = "linux")]
                {
                    let _ = Command::new("kill")
                        .arg("-SIGCONT")
                        .arg(child.id().to_string())
                        .output();
                    self.paused_duration += pause_time.elapsed();
                    self.last_pause_time = None;
                    Ok(())
                }
                #[cfg(not(target_os = "linux"))]
                {
                    Err("Resume not supported".to_string())
                }
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

trait WaitTimeout {
    fn wait_timeout(
        &mut self,
        duration: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>>;
}

impl WaitTimeout for Child {
    fn wait_timeout(
        &mut self,
        duration: Duration,
    ) -> std::io::Result<Option<std::process::ExitStatus>> {
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
