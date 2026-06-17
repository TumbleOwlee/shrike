use std::path::PathBuf;
use std::process::Command;

pub fn root() -> Result<PathBuf, String> {
    let out = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
        .map_err(|e| format!("failed to run git: {e}"))?;
    if !out.status.success() {
        return Err("not inside a git repository".into());
    }
    let path = String::from_utf8_lossy(&out.stdout).trim().to_owned();
    Ok(PathBuf::from(path))
}
