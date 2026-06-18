use std::fs::{self, File};
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const MAX_AGE: Duration = Duration::from_secs(24 * 60 * 60);

pub fn create() -> Result<(File, PathBuf), String> {
    let f = tempfile::Builder::new()
        .prefix("shrike-")
        .suffix(".log")
        .tempfile_in(std::env::temp_dir())
        .map_err(|e| format!("creating log file: {e}"))?;
    let path = f.path().to_owned();
    let file = f.into_file();
    Ok((file, path))
}

pub fn cleanup_old() {
    let tmp = std::env::temp_dir();
    let cutoff = SystemTime::now() - MAX_AGE;
    let Ok(entries) = fs::read_dir(&tmp) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if !name.starts_with("shrike-") || !name.ends_with(".log") {
            continue;
        }
        if let Ok(meta) = fs::metadata(&path) {
            if let Ok(modified) = meta.modified() {
                if modified < cutoff {
                    let _ = fs::remove_file(&path);
                }
            }
        }
    }
}
