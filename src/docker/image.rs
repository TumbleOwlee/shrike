use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::display::output::{self, LifecycleBox};
use crate::display::rolling::RollingDisplay;
use crate::logfile;

/// `base_image` is the reference used to pull from a registry (unchanged by
/// platform). `local_tag` is the reference shrike actually runs containers
/// from and checks for existence — it's suffixed with the platform when one
/// is set, so switching `--platform` can't silently reuse an image cached
/// for a different architecture.
pub fn ensure(
    base_image: &str,
    local_tag: &str,
    platform: Option<&str>,
    dockerfile: Option<&Path>,
    rebuild: bool,
    interactive: bool,
) -> Result<(), String> {
    if let Some(df) = dockerfile {
        ensure_built(local_tag, df, platform, rebuild, interactive)
    } else {
        ensure_pulled(base_image, local_tag, platform)
    }
}

/// Build args for `docker build`, shared by the interactive and piped paths.
fn build_args(tag: &str, dockerfile: &Path, platform: Option<&str>, context_dir: &Path) -> Vec<String> {
    let mut args = vec!["build".to_owned(), "-t".to_owned(), tag.to_owned()];
    if let Some(p) = platform {
        args.push("--platform".into());
        args.push(p.to_owned());
    }
    args.push("-f".into());
    args.push(dockerfile.display().to_string());
    args.push(context_dir.display().to_string());
    args
}

fn ensure_built(
    tag: &str,
    dockerfile: &Path,
    platform: Option<&str>,
    rebuild: bool,
    interactive: bool,
) -> Result<(), String> {
    if !rebuild && image_exists(tag) {
        return Ok(());
    }

    output::print_lifecycle_box(&LifecycleBox {
        action: "Build",
        container: tag,
        image: Some(&dockerfile.display().to_string()),
        setup_cmd: None,
        platform,
    });

    let context_dir = dockerfile.parent().unwrap_or(Path::new("."));
    let args = build_args(tag, dockerfile, platform, context_dir);
    let start = Instant::now();

    if interactive {
        let status = Command::new("docker")
            .args(&args)
            .status()
            .map_err(|e| format!("docker build: {e}"))?;
        let elapsed = start.elapsed().as_millis();
        let code = status.code().unwrap_or(1);
        output::print_footer(code, elapsed, None);
        if !status.success() {
            return Err("docker build failed".to_string());
        }
        return Ok(());
    }

    let (mut logfile, log_path) = logfile::create()?;
    let is_tty = output::stdout_is_tty();

    let mut child = Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("docker build: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx2.send(line);
        }
    });

    let mut display = RollingDisplay::new(is_tty);
    for line in rx {
        display.feed(&line, &mut logfile);
    }
    let status = child
        .wait()
        .map_err(|e| format!("docker build wait: {e}"))?;
    let elapsed = start.elapsed().as_millis();
    let code = status.code().unwrap_or(1);

    let show_log = if code != 0 {
        Some(log_path.as_path())
    } else {
        None
    };
    output::print_footer(code, elapsed, show_log);

    if !status.success() {
        return Err("docker build failed (see log above)".to_string());
    }
    Ok(())
}

fn ensure_pulled(base_image: &str, local_tag: &str, platform: Option<&str>) -> Result<(), String> {
    if image_exists(local_tag) {
        return Ok(());
    }

    output::print_lifecycle_box(&LifecycleBox {
        action: "Pull",
        container: local_tag,
        image: None,
        setup_cmd: None,
        platform,
    });

    let (mut logfile, log_path) = logfile::create()?;
    let is_tty = output::stdout_is_tty();
    let start = Instant::now();

    let args = if let Some(p) = platform {
        vec!["pull", "--platform", p, base_image]
    } else {
        vec!["pull", base_image]
    };

    let mut child = Command::new("docker")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| format!("docker pull: {e}"))?;

    let stdout = child.stdout.take().unwrap();
    let stderr = child.stderr.take().unwrap();

    let (tx, rx) = std::sync::mpsc::channel::<String>();
    let tx2 = tx.clone();
    std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines().map_while(Result::ok) {
            let _ = tx.send(line);
        }
    });
    std::thread::spawn(move || {
        for line in BufReader::new(stderr).lines().map_while(Result::ok) {
            let _ = tx2.send(line);
        }
    });

    let mut display = RollingDisplay::new(is_tty);
    for line in rx {
        display.feed(&line, &mut logfile);
    }
    let status = child.wait().map_err(|e| format!("docker pull wait: {e}"))?;
    let elapsed = start.elapsed().as_millis();
    let code = status.code().unwrap_or(1);

    let show_log = if code != 0 {
        Some(log_path.as_path())
    } else {
        None
    };
    output::print_footer(code, elapsed, show_log);

    if !status.success() {
        return Err(format!("docker pull {base_image} failed"));
    }

    if base_image != local_tag {
        let status = Command::new("docker")
            .args(["tag", base_image, local_tag])
            .status()
            .map_err(|e| format!("docker tag: {e}"))?;
        if !status.success() {
            return Err(format!("docker tag {base_image} {local_tag} failed"));
        }
    }
    Ok(())
}

pub fn image_exists(image: &str) -> bool {
    Command::new("docker")
        .args(["image", "inspect", "--format", "ok", image])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_args_no_platform() {
        let args = build_args(
            "shrike-abc",
            Path::new("/repo/Dockerfile"),
            None,
            Path::new("/repo"),
        );
        assert_eq!(args, vec!["build", "-t", "shrike-abc", "-f", "/repo/Dockerfile", "/repo"]);
    }

    #[test]
    fn build_args_with_platform() {
        let args = build_args(
            "shrike-abc--shrike-linux-arm64",
            Path::new("/repo/Dockerfile"),
            Some("linux/arm64"),
            Path::new("/repo"),
        );
        assert_eq!(
            args,
            vec![
                "build",
                "-t",
                "shrike-abc--shrike-linux-arm64",
                "--platform",
                "linux/arm64",
                "-f",
                "/repo/Dockerfile",
                "/repo",
            ]
        );
    }
}
