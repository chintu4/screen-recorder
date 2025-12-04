use std::process::Command;

#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub id: String, // For Windows: name. For Linux: /dev/videoX or alsa name
}

#[cfg(target_os = "windows")]
pub fn get_video_devices() -> Vec<Device> {
    parse_ffmpeg_dshow("video")
}

#[cfg(target_os = "windows")]
pub fn get_audio_devices() -> Vec<Device> {
    parse_ffmpeg_dshow("audio")
}

#[cfg(not(target_os = "windows"))]
pub fn get_video_devices() -> Vec<Device> {
    let mut devices = Vec::new();

    // Try v4l2-ctl first for nice names
    if let Ok(output) = Command::new("v4l2-ctl").arg("--list-devices").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut current_name = String::new();
        for line in stdout.lines() {
            if !line.starts_with('\t') && !line.starts_with(' ') && !line.is_empty() {
                // Device name (remove colon at end)
                current_name = line.trim_end_matches(':').to_string();
            } else if let Some(path) = line.trim().strip_prefix("/dev/video") {
                // We found a device path like /dev/video0
                // Usually the first one is capture, others might be metadata
                let full_path = format!("/dev/video{}", path);
                devices.push(Device {
                    name: format!("{} ({})", current_name, full_path),
                    id: full_path.clone(),
                });
                // We only take one path per device name to avoid duplicates for now
                // Or we can list all. Let's list all unique paths.
                current_name = String::new(); // Reset to avoid re-using name for next path if logic assumes 1:many
            }
        }
    }

    // Fallback: Check /dev/video* if list is empty
    if devices.is_empty() {
        if let Ok(entries) = std::fs::read_dir("/dev") {
            for entry in entries.flatten() {
                if let Ok(name) = entry.file_name().into_string() {
                    if name.starts_with("video") {
                        let path = format!("/dev/{}", name);
                        devices.push(Device {
                            name: path.clone(),
                            id: path,
                        });
                    }
                }
            }
        }
    }

    // Sort for consistency
    devices.sort_by(|a, b| a.id.cmp(&b.id));
    devices
}

#[cfg(not(target_os = "windows"))]
pub fn get_audio_devices() -> Vec<Device> {
    let mut devices = Vec::new();

    // Default is always safe
    devices.push(Device {
        name: "Default".to_string(),
        id: "default".to_string(),
    });

    // Try arecord -L
    if let Ok(output) = Command::new("arecord").arg("-L").output() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        // Format:
        // name
        //     Description
        //
        // We want 'name' for the ID.
        let mut lines = stdout.lines();
        while let Some(line) = lines.next() {
            if !line.starts_with(' ') && !line.starts_with('\t') && !line.is_empty() {
                let id = line.trim().to_string();
                // Skip cryptic names if possible, but for now include all
                if id == "null" { continue; }

                // Description is usually next line
                // let desc = lines.next().map(|s| s.trim()).unwrap_or(&id);
                // We'll just use ID as name for now, simpler.

                devices.push(Device {
                    name: id.clone(),
                    id,
                });
            }
        }
    }

    devices
}

#[cfg(target_os = "windows")]
fn parse_ffmpeg_dshow(device_type: &str) -> Vec<Device> {
    let mut devices = Vec::new();
    // ffmpeg -list_devices true -f dshow -i dummy
    // Output is in stderr
    let output = Command::new("ffmpeg")
        .args(&["-list_devices", "true", "-f", "dshow", "-i", "dummy"])
        .output();

    if let Ok(out) = output {
        let stderr = String::from_utf8_lossy(&out.stderr);
        let mut in_section = false;

        for line in stderr.lines() {
            if line.contains("DirectShow video devices") {
                in_section = device_type == "video";
            } else if line.contains("DirectShow audio devices") {
                in_section = device_type == "audio";
            } else if in_section {
                // Lines with devices look like: [dshow @ ...]  "Device Name"
                // Alternative names look like:  [dshow @ ...]     Alternative name ...
                if let Some(start) = line.find("\"") {
                    if let Some(end) = line[start+1..].find("\"") {
                        let name = &line[start+1..start+1+end];
                        // Avoid empty names
                        if !name.is_empty() {
                            devices.push(Device {
                                name: name.to_string(),
                                id: name.to_string(), // dshow uses name as ID
                            });
                        }
                    }
                }
            }
        }
    }
    devices
}
