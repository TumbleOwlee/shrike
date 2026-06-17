mod cli;
mod config;
mod display;
mod docker;
mod env_spec;
mod generate;
mod git;
mod list;
mod logfile;
mod signal;
mod slug;
mod workdir;

use std::sync::atomic::Ordering;

use clap::Parser;

use cli::Args;
use config::resolve::ConfigState;
use config::types::AliasConfig;
use display::output::{self, env_display};
use docker::container::{self, ContainerSpec};
use docker::exec::{ExecStep, StepCmd};
use docker::image;
use env_spec::build_env_flags;
use workdir::resolve as resolve_workdir;

fn main() {
    logfile::cleanup_old();

    let args = Args::parse();

    // ── direct mode detection ────────────────────────────────────────────────
    let direct_mode = std::path::Path::new("/run/shrike/profile").exists()
        || std::env::var("SHRIKE_CONTAINER_PROFILE").is_ok();

    if direct_mode {
        run_direct(args);
        return;
    }

    // ── git root ─────────────────────────────────────────────────────────────
    let git_root = git::root().unwrap_or_else(|e| output::die(&e));
    let cwd = std::env::current_dir().unwrap_or(git_root.clone());

    // ── stop-all ─────────────────────────────────────────────────────────────
    if args.stop_all {
        container::stop_all();
        return;
    }

    // ── load config ──────────────────────────────────────────────────────────
    let loaded = config::load(args.profile.as_deref()).unwrap_or_else(|e| output::die(&e));
    let state = loaded.state;

    // ── --new ────────────────────────────────────────────────────────────────
    if let Some(ref tmpl) = args.new {
        generate::generate(tmpl.as_deref(), &git_root);
        return;
    }

    // ── --list-profiles ──────────────────────────────────────────────────────
    if args.list_profiles {
        list::list_profiles(
            &loaded.all_profiles,
            &state.profile_name,
            state.global_file.as_deref(),
            state.repo_file.as_deref(),
            state.project_file.as_deref(),
            &loaded.global,
            &loaded.repo,
            loaded.project.as_ref(),
        );
        return;
    }

    // ── --list ───────────────────────────────────────────────────────────────
    if args.list {
        list::list_aliases(
            &state,
            state.global_file.as_deref(),
            state.repo_file.as_deref(),
            state.project_file.as_deref(),
        );
        return;
    }

    // ── container name ───────────────────────────────────────────────────────
    let container_name = slug::container_name(&git_root, &state.profile_name);

    // ── --stop ───────────────────────────────────────────────────────────────
    if args.stop {
        container::stop(&container_name);
        return;
    }

    // ── resolve image ────────────────────────────────────────────────────────
    let dockerfile = state.dockerfile.clone();
    let final_image = match state.image.as_deref() {
        Some(img) => img.to_owned(),
        None => match &dockerfile {
            Some(df) => format!("shrike-{}", slug::slug(&df.to_string_lossy())),
            None => output::die("no image or dockerfile defined for profile"),
        },
    };

    // ── ensure image ─────────────────────────────────────────────────────────
    image::ensure(&final_image, dockerfile.as_deref(), args.rebuild)
        .unwrap_or_else(|e| output::die(&e));

    // ── ensure container ─────────────────────────────────────────────────────
    let is_new = {
        let spec = ContainerSpec {
            name: &container_name,
            image: &final_image,
            ports: &state.ports,
            volumes: &state.volumes,
            profile_name: &state.profile_name,
            git_root: &git_root,
        };
        container::ensure(&spec, args.restart).unwrap_or_else(|e| output::die(&e))
    };

    // ── run setup on new container ───────────────────────────────────────────
    if is_new {
        if let Some(ref setup) = state.setup {
            container::run_setup(&container_name, setup).unwrap_or_else(|e| output::die(&e));
        }
    }

    // ── no command ───────────────────────────────────────────────────────────
    if args.command.is_empty() {
        return;
    }

    // ── dispatch ─────────────────────────────────────────────────────────────
    let exit_code = dispatch(
        &args,
        &state,
        &container_name,
        &final_image,
        &git_root,
        &cwd,
    );
    std::process::exit(exit_code);
}

fn dispatch(
    args: &Args,
    state: &ConfigState,
    container: &str,
    image: &str,
    git_root: &std::path::Path,
    cwd: &std::path::Path,
) -> i32 {
    let cmd = &args.command;
    let name = &cmd[0];
    let extra_args = &cmd[1..];
    let cli_env = build_env_flags(&args.env);

    if let Some(alias) = state.resolve_alias(name) {
        if let Some(steps) = alias.pipeline.clone() {
            return run_pipeline(
                &steps, args, state, container, image, git_root, cwd, &cli_env,
            );
        }
        let step = build_alias_step(
            name,
            alias,
            state,
            container,
            image,
            git_root,
            cwd,
            extra_args,
            &cli_env,
            args.interactive,
        );
        let result = docker::exec::run(container, &step);
        if signal::KILLED.load(Ordering::SeqCst) {
            signal::reraise();
        }
        return result.exit_code;
    }

    let step = build_literal_step(
        cmd,
        state,
        container,
        image,
        git_root,
        cwd,
        &cli_env,
        args.interactive,
    );
    docker::exec::run(container, &step).exit_code
}

fn run_pipeline(
    steps: &[String],
    args: &Args,
    state: &ConfigState,
    container: &str,
    image: &str,
    git_root: &std::path::Path,
    cwd: &std::path::Path,
    cli_env: &[String],
) -> i32 {
    let total = steps.len();
    for (i, step_name) in steps.iter().enumerate() {
        let alias = state
            .get_alias_internal(step_name)
            .unwrap_or_else(|| output::die(&format!("pipeline step `{step_name}` not found")));
        let mut step = build_alias_step(
            step_name,
            alias,
            state,
            container,
            image,
            git_root,
            cwd,
            &[],
            cli_env,
            args.interactive,
        );
        step.display_cmd = format!("{step_name} [{}/{}]", i + 1, total);

        let result = docker::exec::run(container, &step);
        if result.exit_code != 0 {
            if signal::KILLED.load(Ordering::SeqCst) {
                signal::reraise();
            }
            return result.exit_code;
        }
    }
    0
}

fn build_alias_step(
    name: &str,
    alias: &AliasConfig,
    state: &ConfigState,
    container: &str,
    image: &str,
    git_root: &std::path::Path,
    cwd: &std::path::Path,
    extra_args: &[String],
    cli_env: &[String],
    force_interactive: bool,
) -> ExecStep {
    let workdir = resolve_workdir(alias.workdir.as_deref(), git_root, cwd);
    let user = alias
        .user
        .as_ref()
        .or(state.user.as_ref())
        .map(|u| env_spec::eval_value(u));

    let mut env_flags = build_env_flags(&state.env);
    if let Some(ref alias_env) = alias.env {
        env_flags.extend(build_env_flags(alias_env));
    }
    env_flags.extend_from_slice(cli_env);
    let env_disp = env_display(&env_flags);

    let cmd_str = alias.cmd.as_deref().unwrap_or("");
    let full_cmd = if extra_args.is_empty() {
        cmd_str.to_owned()
    } else {
        format!("{cmd_str} {}", extra_args.join(" "))
    };

    ExecStep {
        cmd: StepCmd::Alias(full_cmd),
        workdir,
        display_cmd: name.to_owned(),
        env_flags,
        env_display: env_disp,
        user,
        interactive: alias.interactive == Some(true) || force_interactive,
        profile: state.profile_name.clone(),
        image: image.to_owned(),
        container: container.to_owned(),
        ports: state.ports.clone(),
        volumes: state.volumes.clone(),
    }
}

fn build_literal_step(
    cmd: &[String],
    state: &ConfigState,
    container: &str,
    image: &str,
    git_root: &std::path::Path,
    cwd: &std::path::Path,
    cli_env: &[String],
    force_interactive: bool,
) -> ExecStep {
    let workdir = resolve_workdir(None, git_root, cwd);
    let user = state.user.as_ref().map(|u| env_spec::eval_value(u));

    let mut env_flags = build_env_flags(&state.env);
    env_flags.extend_from_slice(cli_env);
    let env_disp = env_display(&env_flags);

    ExecStep {
        cmd: StepCmd::Literal(cmd.to_vec()),
        workdir,
        display_cmd: cmd.join(" "),
        env_flags,
        env_display: env_disp,
        user,
        interactive: force_interactive,
        profile: state.profile_name.clone(),
        image: image.to_owned(),
        container: container.to_owned(),
        ports: state.ports.clone(),
        volumes: state.volumes.clone(),
    }
}

fn run_direct(args: Args) {
    if args.rebuild || args.restart {
        output::warn("--rebuild/--restart have no effect inside a container");
    }
    if args.stop || args.stop_all {
        output::warn("container management flags have no effect in direct mode");
        return;
    }

    let profile_data = std::fs::read_to_string("/run/shrike/profile").unwrap_or_default();
    let git_root_str = profile_data.lines().nth(1).unwrap_or("/workspace");
    let git_root = std::path::PathBuf::from(git_root_str);

    let loaded = config::load(args.profile.as_deref()).unwrap_or_else(|e| output::die(&e));
    let state = loaded.state;

    if args.list {
        list::list_aliases(
            &state,
            state.global_file.as_deref(),
            state.repo_file.as_deref(),
            state.project_file.as_deref(),
        );
        return;
    }
    if args.list_profiles {
        list::list_profiles(
            &loaded.all_profiles,
            &state.profile_name,
            state.global_file.as_deref(),
            state.repo_file.as_deref(),
            state.project_file.as_deref(),
            &loaded.global,
            &loaded.repo,
            loaded.project.as_ref(),
        );
        return;
    }
    if args.command.is_empty() {
        return;
    }

    let cwd = std::env::current_dir().unwrap_or(git_root.clone());
    let cmd = &args.command;
    let name = &cmd[0];
    let extra_args = &cmd[1..];
    let cli_env_flags = build_env_flags(&args.env);

    let (exec_cmd, workdir, env_specs) = if let Some(alias) = state.resolve_alias(name) {
        let w = resolve_workdir(alias.workdir.as_deref(), &git_root, &cwd);
        let mut env = state.env.clone();
        if let Some(ref ae) = alias.env {
            env.extend(ae.clone());
        }
        let cmd_str = alias.cmd.as_deref().unwrap_or("");
        let full = if extra_args.is_empty() {
            cmd_str.to_owned()
        } else {
            format!("{cmd_str} {}", extra_args.join(" "))
        };
        (vec!["sh".to_owned(), "-c".to_owned(), full], w, env)
    } else {
        let w = resolve_workdir(None, &git_root, &cwd);
        (cmd.to_vec(), w, state.env.clone())
    };

    for spec in &env_specs {
        if let Some(eq) = spec.find('=') {
            std::env::set_var(&spec[..eq], env_spec::eval_value(&spec[eq + 1..]));
        } else if let Ok(v) = std::env::var(spec) {
            std::env::set_var(spec, v);
        }
    }
    let mut skip_next = false;
    for flag in &cli_env_flags {
        if skip_next {
            skip_next = false;
            continue;
        }
        if flag == "-e" {
            skip_next = true;
            continue;
        }
        if let Some(eq) = flag.find('=') {
            std::env::set_var(&flag[..eq], &flag[eq + 1..]);
        }
    }

    let _ = std::env::set_current_dir(&workdir);
    let status = std::process::Command::new(&exec_cmd[0])
        .args(&exec_cmd[1..])
        .status()
        .unwrap_or_else(|e| output::die(&format!("exec: {e}")));

    std::process::exit(status.code().unwrap_or(1));
}
