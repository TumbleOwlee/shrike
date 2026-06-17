use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Instant;

use crate::display::output::{self, LifecycleBox};
use crate::display::rolling::RollingDisplay;
use crate::logfile;

pub fn ensure(image: &str, dockerfile: Option<&Path>, rebuild: bool) -> Result<(), String> {
    if let Some(df) = dockerfile {
        ensure_built(image, df, rebuild)
    } else {
        ensure_pulled(image)
    }
}

fn ensure_built(tag: &str, dockerfile: &Path, rebuild: bool) -> Result<(), String> {
    if !rebuild && image_exists(tag) {
        return Ok(());
    }

    output::print_lifecycle_box(&LifecycleBox {
        action: "Build",
        container: tag,
        image: Some(&dockerfile.display().to_string()),
        setup_cmd: None,
    });

    let context_dir = dockerfile.parent().unwrap_or(Path::new("."));
    let (mut logfile, log_path) = logfile::create().map_err(|e| e)?;
    let is_tty = output::stdout_is_tty();
    let start = Instant::now();

    let mut child = Command::new("docker")
        .args(["build", "-t", tag, "-f"])
        .arg(dockerfile)
        .arg(context_dir)
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
        return Err(format!("docker build failed (see log above)"));
    }
    Ok(())
}

fn ensure_pulled(image: &str) -> Result<(), String> {
    if image_exists(image) {
        return Ok(());
    }

    output::print_lifecycle_box(&LifecycleBox {
        action: "Pull",
        container: image,
        image: None,
        setup_cmd: None,
    });

    let (mut logfile, log_path) = logfile::create().map_err(|e| e)?;
    let is_tty = output::stdout_is_tty();
    let start = Instant::now();

    let mut child = Command::new("docker")
        .args(["pull", image])
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
        return Err(format!("docker pull {image} failed"));
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
