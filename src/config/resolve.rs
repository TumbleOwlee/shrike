use indexmap::IndexMap;
use std::path::{Path, PathBuf};

use super::types::{AliasConfig, ConfigFile, ProfileSection};

#[derive(Debug, Default)]
pub struct ConfigState {
    pub profile_name: String,
    pub image: Option<String>,
    pub dockerfile: Option<PathBuf>,
    pub platform: Option<String>,
    pub env: Vec<String>,
    pub ports: Vec<String>,
    pub volumes: Vec<String>,
    pub user: Option<String>,
    pub setup: Option<String>,
    pub aliases: IndexMap<String, AliasConfig>,
    pub global_file: Option<PathBuf>,
    pub repo_file: Option<PathBuf>,
    pub project_file: Option<PathBuf>,
}

impl ConfigState {
    pub fn from_profile(file: &ConfigFile, profile_name: &str, base_dir: Option<&Path>) -> Self {
        let mut state = ConfigState {
            profile_name: profile_name.to_owned(),
            ..Default::default()
        };
        if let Some(profile) = file.profiles.get(profile_name) {
            state.apply_profile(profile, base_dir);
        }
        state
    }

    pub fn apply(&mut self, file: &ConfigFile, profile_name: &str, base_dir: Option<&Path>) {
        if let Some(profile) = file.profiles.get(profile_name) {
            self.apply_profile(profile, base_dir);
        }
    }

    fn apply_profile(&mut self, profile: &ProfileSection, base_dir: Option<&Path>) {
        if let Some(v) = &profile.image {
            self.image = Some(v.clone());
        }
        if let Some(v) = &profile.platform {
            self.platform = Some(v.clone());
        }
        if let Some(v) = &profile.dockerfile {
            let raw = PathBuf::from(v);
            self.dockerfile = Some(if raw.is_absolute() {
                raw
            } else {
                base_dir.unwrap_or(Path::new(".")).join(&raw)
            });
        }
        if let Some(v) = &profile.env {
            self.env = v.clone();
        }
        if let Some(v) = &profile.ports {
            self.ports = v.clone();
        }
        if let Some(v) = &profile.volumes {
            self.volumes = v.clone();
        }
        if let Some(v) = &profile.user {
            self.user = Some(v.clone());
        }
        if let Some(v) = &profile.setup {
            self.setup = Some(v.clone());
        }
        for (name, alias) in &profile.aliases {
            self.aliases.insert(name.clone(), alias.clone());
        }
    }

    pub fn resolve_alias(&self, name: &str) -> Option<&AliasConfig> {
        let a = self.aliases.get(name)?;
        if a.hidden == Some(true) {
            return None;
        }
        Some(a)
    }

    pub fn get_alias_internal(&self, name: &str) -> Option<&AliasConfig> {
        self.aliases.get(name)
    }
}

pub fn select_profile_name(
    global: &ConfigFile,
    repo: &ConfigFile,
    project: Option<&ConfigFile>,
    cli_profile: Option<&str>,
) -> String {
    if let Some(p) = cli_profile {
        return p.to_owned();
    }
    if let Some(p) = project
        .and_then(|f| f.project.as_ref())
        .and_then(|p| p.profile.as_ref())
    {
        return p.clone();
    }
    if let Some(p) = repo.project.as_ref().and_then(|p| p.profile.as_ref()) {
        return p.clone();
    }
    if let Some(p) = global.project.as_ref().and_then(|p| p.profile.as_ref()) {
        return p.clone();
    }
    "default".to_owned()
}

pub fn build_state(
    global: &ConfigFile,
    global_dir: Option<&Path>,
    repo: &ConfigFile,
    repo_dir: Option<&Path>,
    project: Option<&ConfigFile>,
    project_dir: Option<&Path>,
    profile_name: &str,
) -> ConfigState {
    let mut state = ConfigState::from_profile(global, profile_name, global_dir);
    state.apply(repo, profile_name, repo_dir);
    if let Some(p) = project {
        state.apply(p, profile_name, project_dir);
    }
    state
}

pub fn list_profiles(
    global: &ConfigFile,
    repo: &ConfigFile,
    project: Option<&ConfigFile>,
) -> Vec<String> {
    let mut names: IndexMap<String, ()> = IndexMap::new();
    for name in global.profiles.keys() {
        names.insert(name.clone(), ());
    }
    for name in repo.profiles.keys() {
        names.insert(name.clone(), ());
    }
    if let Some(p) = project {
        for name in p.profiles.keys() {
            names.insert(name.clone(), ());
        }
    }
    names.into_keys().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::parse::parse_str;

    #[test]
    fn merge_layers() {
        let global = parse_str(
            r#"
[default]
image = "ubuntu:22.04"
env   = ["CC", "CXX"]

[default.build]
cmd = "cmake --build /workspace/build"
workdir = "/workspace/build"
"#,
        )
        .unwrap();

        let repo = parse_str(
            r#"
[project]
profile = "default"

[default]
image = "my-org/project:latest"

[default.build]
cmd = "cmake --build build"
"#,
        )
        .unwrap();

        let project = parse_str(
            r#"
[project]
pattern = ".*/myproject$"
profile = "default"

[default.test]
cmd = "ctest"
"#,
        )
        .unwrap();

        let profile = select_profile_name(&global, &repo, Some(&project), None);
        assert_eq!(profile, "default");

        let state = build_state(&global, None, &repo, None, Some(&project), None, &profile);

        // repo image overrides global
        assert_eq!(state.image.as_deref(), Some("my-org/project:latest"));
        // repo env replaces global env
        assert_eq!(state.env, vec!["CC", "CXX"]);
        // repo alias overrides global alias
        let build = state.aliases.get("build").unwrap();
        assert_eq!(build.cmd.as_deref(), Some("cmake --build build"));
        // project alias added
        assert!(state.aliases.contains_key("test"));
    }

    #[test]
    fn hidden_alias_not_resolved() {
        let global = parse_str(
            r#"
[default]
image = "ubuntu:22.04"

[default.prepare]
cmd    = "make prepare"
hidden = true
"#,
        )
        .unwrap();
        let state = build_state(
            &global,
            None,
            &ConfigFile::default(),
            None,
            None,
            None,
            "default",
        );
        assert!(state.resolve_alias("prepare").is_none());
        assert!(state.get_alias_internal("prepare").is_some());
    }
}
