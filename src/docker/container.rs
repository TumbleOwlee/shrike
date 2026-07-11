use std::path::Path;
use std::process::{Command, Stdio};

use crate::display::output::{self, LifecycleBox};
use crate::env_spec;

pub struct ContainerSpec<'a> {
    pub name: &'a str,
    pub image: &'a str,
    pub platform: Option<&'a str>,
    pub ports: &'a [String],
    pub volumes: &'a [String],
    pub extra_env: &'a [(String, String)],
    pub profile_name: &'a str,
    pub git_root: &'a Path,
    pub global_file: Option<&'a Path>,
    pub project_file: Option<&'a Path>,
}

pub fn ensure(spec: &ContainerSpec, restart: bool) -> Result<bool, String> {
    if restart && container_exists(spec.name) {
        output::print_lifecycle_box(&LifecycleBox {
            action: "Remove",
            container: spec.name,
            image: None,
            setup_cmd: None,
            platform: spec.platform,
        });
        remove(spec.name);
    }

    output::print_lifecycle_box(&LifecycleBox {
        action: "Create",
        container: spec.name,
        image: Some(spec.image),
        setup_cmd: None,
        platform: spec.platform,
    });

    let profile_content = format!("{}\n{}", spec.profile_name, spec.git_root.display());
    let profile_file = write_profile_file(spec.name, &profile_content)?;

    if container_exists(spec.name) {
        ensure_running(spec.name)?;
        Ok(false) // existing container
    } else {
        create(spec, &profile_file)?;
        Ok(true) // new container
    }
}

fn container_exists(name: &str) -> bool {
    Command::new("docker")
        .args(["container", "inspect", "--format", "ok", name])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn ensure_running(name: &str) -> Result<(), String> {
    let running = Command::new("docker")
        .args([
            "container",
            "inspect",
            "--format",
            "{{.State.Running}}",
            name,
        ])
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "true")
        .unwrap_or(false);

    if !running {
        output::print_lifecycle_box(&LifecycleBox {
            action: "Start",
            container: name,
            image: None,
            setup_cmd: None,
            platform: None,
        });
        let status = Command::new("docker")
            .args(["start", name])
            .stdout(Stdio::null())
            .status()
            .map_err(|e| format!("docker start: {e}"))?;
        if !status.success() {
            return Err(format!("docker start {name} failed"));
        }
    }
    Ok(())
}

fn create(spec: &ContainerSpec, profile_file: &Path) -> Result<(), String> {
    let mut args: Vec<String> = vec![
        "run".into(),
        "-d".into(),
        "--name".into(),
        spec.name.into(),
        "-v".into(),
        format!("{}:/workspace", spec.git_root.display()),
        "-v".into(),
        format!("{}:/run/shrike/profile:ro", profile_file.display()),
    ];

    for port in spec.ports {
        args.push("-p".into());
        args.push(env_spec::eval_value(port));
    }
    for vol in spec.volumes {
        args.push("-v".into());
        args.push(env_spec::eval_value_with_env(vol, spec.extra_env));
    }

    if let Some(global) = spec.global_file {
        args.push("-v".into());
        args.push(format!("{}:/run/shrike/global.toml:ro", global.display()));
    }
    if let Some(project) = spec.project_file {
        args.push("-v".into());
        args.push(format!("{}:/run/shrike/project.toml:ro", project.display()));
    }

    if let Ok(exe) = std::env::current_exe() {
        args.push("-v".into());
        args.push(format!("{}:/usr/local/bin/shrike:ro", exe.display()));
    } else {
        eprintln!("Warning: failed to get current executable path, shrike CLI will not be available in the container");
    }

    if let Some(platform) = spec.platform {
        args.push("--platform".into());
        args.push(platform.into());
    }

    args.push("--entrypoint".into());
    args.push("sleep".into());
    args.push(spec.image.into());
    args.push("infinity".into());

    let status = Command::new("docker")
        .args(&args)
        .stdout(Stdio::null())
        .status()
        .map_err(|e| format!("docker run: {e}"))?;
    if !status.success() {
        return Err("docker run failed".into());
    }
    Ok(())
}

fn write_profile_file(container_name: &str, content: &str) -> Result<std::path::PathBuf, String> {
    let dir = std::env::temp_dir().join("shrike-profiles");
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{container_name}.profile"));
    std::fs::write(&path, content).map_err(|e| e.to_string())?;
    Ok(path)
}

pub fn remove(name: &str) {
    let _ = Command::new("docker")
        .args(["rm", "-f", name])
        .stdout(Stdio::null())
        .status();
}

pub fn stop(name: &str) {
    output::print_lifecycle_box(&LifecycleBox {
        action: "Remove",
        container: name,
        image: None,
        setup_cmd: None,
        platform: None,
    });
    remove(name);
}

pub fn stop_all() {
    let Ok(out) = Command::new("docker")
        .args([
            "ps",
            "-a",
            "--filter",
            "name=shrike-",
            "--format",
            "{{.Names}}",
        ])
        .output()
    else {
        return;
    };

    for name in String::from_utf8_lossy(&out.stdout).lines() {
        let name = name.trim();
        if !name.is_empty() {
            stop(name);
        }
    }
}

pub fn run_setup(container: &str, setup_cmd: &str) -> Result<(), String> {
    output::print_lifecycle_box(&LifecycleBox {
        action: "Setup",
        container,
        image: None,
        setup_cmd: Some(setup_cmd),
        platform: None,
    });
    let status = Command::new("docker")
        .args(["exec", container, "sh", "-c", setup_cmd])
        .status()
        .map_err(|e| format!("docker exec setup: {e}"))?;
    if !status.success() {
        return Err(format!(
            "setup command failed (exit {})",
            status.code().unwrap_or(1)
        ));
    }
    Ok(())
}
