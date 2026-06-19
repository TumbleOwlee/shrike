use std::path::PathBuf;
use std::process::Command;

pub fn branch(git_root: &std::path::Path) -> String {
    let out = Command::new("git")
        .args([
            "-C",
            &git_root.to_string_lossy(),
            "rev-parse",
            "--abbrev-ref",
            "HEAD",
        ])
        .output();
    match out {
        Ok(o) if o.status.success() => {
            let b = String::from_utf8_lossy(&o.stdout).trim().to_owned();
            if b == "HEAD" {
                // detached HEAD — use short SHA instead
                Command::new("git")
                    .args([
                        "-C",
                        &git_root.to_string_lossy(),
                        "rev-parse",
                        "--short",
                        "HEAD",
                    ])
                    .output()
                    .ok()
                    .filter(|o| o.status.success())
                    .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_owned())
                    .unwrap_or_else(|| "detached".into())
            } else {
                b
            }
        }
        _ => "unknown".into(),
    }
}

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
