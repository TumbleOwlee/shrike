use indexmap::IndexMap;

#[derive(Debug, Default)]
pub struct ConfigFile {
    pub project: Option<ProjectSection>,
    pub profiles: IndexMap<String, ProfileSection>,
}

#[derive(Debug, Default)]
pub struct ProjectSection {
    pub pattern: Option<String>,
    pub profile: Option<String>,
}

#[derive(Debug, Default)]
pub struct ProfileSection {
    pub image: Option<String>,
    pub dockerfile: Option<String>,
    pub env: Option<Vec<String>>,
    pub ports: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub user: Option<String>,
    pub setup: Option<String>,
    pub aliases: IndexMap<String, AliasConfig>,
}

#[derive(Debug, Default, Clone)]
pub struct AliasConfig {
    pub cmd: Option<String>,
    pub desc: Option<String>,
    pub workdir: Option<String>,
    pub env: Option<Vec<String>>,
    pub pipeline: Option<Vec<String>>,
    pub user: Option<String>,
    pub interactive: Option<bool>,
    pub hidden: Option<bool>,
}
