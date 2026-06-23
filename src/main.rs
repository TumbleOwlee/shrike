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
mod trust;
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
    // Only when actually inside a container (/.dockerenv) AND the profile file
    // the container was created with is present. A stray host env var must not
    // flip us into running commands natively on the host.
    let direct_mode = std::path::Path::new("/.dockerenv").exists()
        && std::path::Path::new("/run/shrike/profile").exists();

    if direct_mode {
        run_direct(args);
        return;
    }

    // ── git root ─────────────────────────────────────────────────────────────
    let git_root = git::root().unwrap_or_else(|e| output::die(&format!("git root: {e}")));
    let cwd = std::env::current_dir().unwrap_or(git_root.clone());

    // ── stop-all ─────────────────────────────────────────────────────────────
    if args.stop_all {
        container::stop_all();
        return;
    }

    // ── load config ──────────────────────────────────────────────────────────
    let loaded = config::load(args.profile.as_deref())
        .unwrap_or_else(|e| output::die(&format!("load config: {e}")));
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
    let branch = git::branch(&git_root);
    let container_name = slug::container_name(&git_root, &state.profile_name, &branch);

    // ── --stop ───────────────────────────────────────────────────────────────
    if args.stop {
        container::stop(&container_name);
        return;
    }

    // ── trust gate for host-evaluated repo/project values ────────────────────
    trust::ensure_repo_trusted(&git_root, &loaded.repo, loaded.project.as_ref())
        .unwrap_or_else(|e| output::die(&e));

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
    image::ensure(
        &final_image,
        state.platform.as_deref(),
        dockerfile.as_deref(),
        args.rebuild,
    )
    .unwrap_or_else(|e| output::die(&format!("ensure image: {e}")));

    // ── ensure container ─────────────────────────────────────────────────────
    let is_new = {
        let profile_env_map = env_spec::env_map_from_flags(&build_env_flags(&state.env));
        let spec = ContainerSpec {
            name: &container_name,
            image: &final_image,
            platform: state.platform.as_deref(),
            ports: &state.ports,
            volumes: &state.volumes,
            extra_env: &profile_env_map,
            profile_name: &state.profile_name,
            git_root: &git_root,
            global_file: state.global_file.as_deref(),
            project_file: state.project_file.as_deref(),
        };
        container::ensure(&spec, args.restart)
            .unwrap_or_else(|e| output::die(&format!("ensure container: {e}")))
    };

    // ── run setup on new container ───────────────────────────────────────────
    if is_new {
        if let Some(ref setup) = state.setup {
            container::run_setup(&container_name, setup)
                .unwrap_or_else(|e| output::die(&format!("run setup: {e}")));
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

#[allow(clippy::too_many_arguments)]
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

#[allow(clippy::too_many_arguments)]
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
    let mut env_flags = build_env_flags(&state.env);
    if let Some(ref alias_env) = alias.env {
        env_flags.extend(build_env_flags(alias_env));
    }
    env_flags.extend_from_slice(cli_env);
    let env_map = env_spec::env_map_from_flags(&env_flags);

    let workdir = resolve_workdir(alias.workdir.as_deref(), git_root, cwd, &env_map);
    let user = alias
        .user
        .as_ref()
        .or(state.user.as_ref())
        .map(|u| env_spec::eval_value(u));

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

#[allow(clippy::too_many_arguments)]
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
    let workdir = resolve_workdir(None, git_root, cwd, &[]);
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
    let file_profile = profile_data
        .lines()
        .next()
        .filter(|s| !s.is_empty())
        .map(str::to_owned);
    let effective_profile = args.profile.as_deref().or(file_profile.as_deref());

    let loaded = config::load(effective_profile)
        .unwrap_or_else(|e| output::die(&format!("load config: {e}")));
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

    let git_root = git::root().unwrap_or_else(|_| std::path::PathBuf::from("/workspace"));
    let cwd = std::env::current_dir().unwrap_or(git_root.clone());
    let cmd = &args.command;
    let name = &cmd[0];
    let extra_args = &cmd[1..];
    let cli_env_flags = build_env_flags(&args.env);

    let force_interactive = args.interactive;
    let exit_code = if let Some(alias) = state.resolve_alias(name) {
        if let Some(ref steps) = alias.pipeline.clone() {
            run_pipeline_direct(
                steps,
                &state,
                &git_root,
                &cwd,
                &cli_env_flags,
                force_interactive,
            )
        } else {
            let interactive = alias.interactive == Some(true) || force_interactive;
            run_alias_direct(
                alias,
                &state,
                &git_root,
                &cwd,
                extra_args,
                &cli_env_flags,
                interactive,
            )
        }
    } else {
        run_literal_direct(
            cmd,
            &state,
            &git_root,
            &cwd,
            &cli_env_flags,
            force_interactive,
        )
    };
    std::process::exit(exit_code);
}

fn run_pipeline_direct(
    steps: &[String],
    state: &ConfigState,
    git_root: &std::path::Path,
    cwd: &std::path::Path,
    cli_env_flags: &[String],
    force_interactive: bool,
) -> i32 {
    for step_name in steps {
        let alias = state
            .get_alias_internal(step_name)
            .unwrap_or_else(|| output::die(&format!("pipeline step `{step_name}` not found")));
        let interactive = alias.interactive == Some(true) || force_interactive;
        let code = run_alias_direct(alias, state, git_root, cwd, &[], cli_env_flags, interactive);
        if code != 0 {
            return code;
        }
    }
    0
}

fn run_alias_direct(
    alias: &AliasConfig,
    state: &ConfigState,
    git_root: &std::path::Path,
    cwd: &std::path::Path,
    extra_args: &[String],
    cli_env_flags: &[String],
    interactive: bool,
) -> i32 {
    let mut env_specs = state.env.clone();
    if let Some(ref ae) = alias.env {
        env_specs.extend(ae.clone());
    }
    let mut env_flags = build_env_flags(&env_specs);
    env_flags.extend_from_slice(cli_env_flags);
    let env_map = env_spec::env_map_from_flags(&env_flags);
    let workdir = resolve_workdir(alias.workdir.as_deref(), git_root, cwd, &env_map);
    let env_disp = env_display(&env_flags);

    let cmd_str = alias.cmd.as_deref().unwrap_or("");
    let full = if extra_args.is_empty() {
        cmd_str.to_owned()
    } else {
        format!("{cmd_str} {}", extra_args.join(" "))
    };

    apply_env_direct(&env_specs, cli_env_flags);
    let mut cmd = std::process::Command::new("sh");
    cmd.args(["-c", &full]).current_dir(&workdir);
    docker::exec::run_native(
        cmd,
        &full,
        &workdir,
        &state.profile_name,
        &env_disp,
        interactive,
    )
    .exit_code
}

fn run_literal_direct(
    cmd: &[String],
    state: &ConfigState,
    git_root: &std::path::Path,
    cwd: &std::path::Path,
    cli_env_flags: &[String],
    interactive: bool,
) -> i32 {
    let workdir = resolve_workdir(None, git_root, cwd, &[]);
    let mut env_flags = build_env_flags(&state.env);
    env_flags.extend_from_slice(cli_env_flags);
    let env_disp = env_display(&env_flags);

    apply_env_direct(&state.env, cli_env_flags);
    let mut c = std::process::Command::new(&cmd[0]);
    c.args(&cmd[1..]).current_dir(&workdir);
    docker::exec::run_native(
        c,
        &cmd.join(" "),
        &workdir,
        &state.profile_name,
        &env_disp,
        interactive,
    )
    .exit_code
}

fn apply_env_direct(specs: &[String], cli_env_flags: &[String]) {
    for spec in specs {
        if let Some(eq) = spec.find('=') {
            std::env::set_var(&spec[..eq], env_spec::eval_value(&spec[eq + 1..]));
        } else if let Ok(v) = std::env::var(spec) {
            std::env::set_var(spec, v);
        }
    }
    let mut skip_next = false;
    for flag in cli_env_flags {
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
}
