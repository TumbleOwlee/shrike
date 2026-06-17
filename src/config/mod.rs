pub mod parse;
pub mod resolve;
pub mod types;

use std::path::{Path, PathBuf};

use parse::parse_file;
use resolve::{build_state, list_profiles, select_profile_name, ConfigState};
use types::ConfigFile;

use crate::git;

pub struct LoadedConfig {
    pub state: ConfigState,
    pub all_profiles: Vec<String>,
    pub global: ConfigFile,
    pub repo: ConfigFile,
    pub project: Option<ConfigFile>,
}

pub fn load(cli_profile: Option<&str>) -> Result<LoadedConfig, String> {
    let global_path = global_config_path();
    let global = load_optional(global_path.as_deref());

    let git_root = git::root().map_err(|e| e.to_string())?;
    let repo_path = git_root.join(".shrike.toml");
    let repo = load_optional(Some(&repo_path));

    let project_result = find_project_config(&git_root)?;
    let (project_file, project_config) = match project_result {
        Some((f, p)) => (Some(p), Some(f)),
        None => (None, None),
    };

    let profile = select_profile_name(&global, &repo, project_config.as_ref(), cli_profile);
    let all = list_profiles(&global, &repo, project_config.as_ref());

    let global2 = load_optional(global_path.as_deref());
    let repo2 = load_optional(Some(&repo_path));
    let project2 = project_config
        .as_ref()
        .map(|_| load_optional(project_file.as_deref()));

    let global_dir = global_path.as_deref().and_then(|p| p.parent());
    let repo_dir = repo_path.parent();
    let project_dir = project_file.as_deref().and_then(|p| p.parent());

    let mut state = build_state(
        global2,
        global_dir,
        repo2,
        repo_dir,
        project_config,
        project_dir,
        &profile,
    );

    state.global_file = global_path.filter(|p| p.exists());
    state.repo_file = Some(repo_path).filter(|p| p.exists());
    state.project_file = project_file;

    Ok(LoadedConfig {
        state,
        all_profiles: all,
        global,
        repo,
        project: project2,
    })
}

fn global_config_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".shrike.toml"))
}

fn load_optional(path: Option<&Path>) -> ConfigFile {
    match path {
        Some(p) if p.exists() => parse_file(p).unwrap_or_else(|e| {
            eprintln!("shrike: warning: {e}");
            ConfigFile::default()
        }),
        _ => ConfigFile::default(),
    }
}

fn find_project_config(git_root: &Path) -> Result<Option<(ConfigFile, PathBuf)>, String> {
    let dex_d = match dirs::home_dir() {
        Some(h) => h.join(".shrike.d"),
        None => return Ok(None),
    };
    if !dex_d.exists() {
        return Ok(None);
    }
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&dex_d)
        .map_err(|e| format!("reading ~/.shrike.d: {e}"))?
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.extension().and_then(|e| e.to_str()) == Some("toml"))
        .collect();
    entries.sort();

    let root_str = git_root.to_string_lossy();
    for path in entries {
        let file = match parse_file(&path) {
            Ok(f) => f,
            Err(e) => {
                eprintln!("shrike: warning: {e}");
                continue;
            }
        };
        let pattern = file.project.as_ref().and_then(|p| p.pattern.as_ref());
        if let Some(pat) = pattern {
            let re = regex::Regex::new(pat)
                .map_err(|e| format!("{}: invalid pattern: {e}", path.display()))?;
            if re.is_match(&root_str) {
                return Ok(Some((file, path)));
            }
        }
    }
    Ok(None)
}
