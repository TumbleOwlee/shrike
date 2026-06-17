use indexmap::IndexMap;
use std::path::Path;
use toml::Value;

use super::types::{AliasConfig, ConfigFile, ProfileSection, ProjectSection};

const PROFILE_KEYS: &[&str] = &[
    "image",
    "dockerfile",
    "env",
    "ports",
    "volumes",
    "user",
    "setup",
];

pub fn parse_file(path: &Path) -> Result<ConfigFile, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {}", path.display(), e))?;
    parse_str(&text).map_err(|e| format!("{}: {}", path.display(), e))
}

pub fn parse_str(text: &str) -> Result<ConfigFile, String> {
    let value: Value = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
    let table = match value {
        Value::Table(t) => t,
        _ => return Err("expected TOML table at top level".into()),
    };

    let mut file = ConfigFile::default();

    for (key, val) in &table {
        if key == "project" {
            file.project = Some(parse_project(val)?);
        } else if let Value::Table(profile_table) = val {
            file.profiles
                .insert(key.clone(), parse_profile(profile_table)?);
        } else {
            return Err(format!("unexpected top-level key `{key}`: must be a table"));
        }
    }

    Ok(file)
}

fn parse_project(val: &Value) -> Result<ProjectSection, String> {
    let table = as_table(val, "project")?;
    let mut s = ProjectSection::default();
    for (k, v) in table {
        match k.as_str() {
            "pattern" => s.pattern = Some(as_str(v, "project.pattern")?),
            "profile" => s.profile = Some(as_str(v, "project.profile")?),
            other => return Err(format!("unknown key in [project]: `{other}`")),
        }
    }
    Ok(s)
}

fn parse_profile(table: &toml::map::Map<String, Value>) -> Result<ProfileSection, String> {
    let mut p = ProfileSection::default();
    let mut aliases: IndexMap<String, AliasConfig> = IndexMap::new();

    for (k, v) in table {
        if PROFILE_KEYS.contains(&k.as_str()) {
            match k.as_str() {
                "image" => p.image = Some(as_str(v, "image")?),
                "dockerfile" => p.dockerfile = Some(as_str(v, "dockerfile")?),
                "user" => p.user = Some(as_str(v, "user")?),
                "setup" => p.setup = Some(as_str(v, "setup")?),
                "env" => p.env = Some(as_str_array(v, "env")?),
                "ports" => p.ports = Some(as_str_array(v, "ports")?),
                "volumes" => p.volumes = Some(as_str_array(v, "volumes")?),
                _ => unreachable!(),
            }
        } else if let Value::Table(alias_table) = v {
            aliases.insert(k.clone(), parse_alias(alias_table, k)?);
        } else {
            return Err(format!(
                "unexpected profile key `{k}`: must be a table (alias) or known profile field"
            ));
        }
    }

    p.aliases = aliases;
    Ok(p)
}

fn parse_alias(table: &toml::map::Map<String, Value>, name: &str) -> Result<AliasConfig, String> {
    let mut a = AliasConfig::default();
    for (k, v) in table {
        let ctx = format!("alias `{name}`.{k}");
        match k.as_str() {
            "cmd" => a.cmd = Some(as_str(v, &ctx)?),
            "desc" => a.desc = Some(as_str(v, &ctx)?),
            "workdir" => a.workdir = Some(as_str(v, &ctx)?),
            "user" => a.user = Some(as_str(v, &ctx)?),
            "env" => a.env = Some(as_str_array(v, &ctx)?),
            "pipeline" => a.pipeline = Some(as_str_array(v, &ctx)?),
            "interactive" => a.interactive = Some(as_bool(v, &ctx)?),
            "hidden" => a.hidden = Some(as_bool(v, &ctx)?),
            other => return Err(format!("unknown key in alias `{name}`: `{other}`")),
        }
    }
    Ok(a)
}

fn as_table<'a>(v: &'a Value, ctx: &str) -> Result<&'a toml::map::Map<String, Value>, String> {
    match v {
        Value::Table(t) => Ok(t),
        _ => Err(format!("`{ctx}` must be a table")),
    }
}

fn as_str(v: &Value, ctx: &str) -> Result<String, String> {
    match v {
        Value::String(s) => Ok(s.clone()),
        _ => Err(format!("`{ctx}` must be a string")),
    }
}

fn as_bool(v: &Value, ctx: &str) -> Result<bool, String> {
    match v {
        Value::Boolean(b) => Ok(*b),
        _ => Err(format!("`{ctx}` must be a boolean")),
    }
}

fn as_str_array(v: &Value, ctx: &str) -> Result<Vec<String>, String> {
    match v {
        Value::Array(arr) => arr
            .iter()
            .map(|item| match item {
                Value::String(s) => Ok(s.clone()),
                _ => Err(format!("`{ctx}` array must contain strings")),
            })
            .collect(),
        _ => Err(format!("`{ctx}` must be an array of strings")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GLOBAL_EXAMPLE: &str = r#"
[default]
image = "ubuntu:22.04"
env   = ["CC", "CXX", "MAKEFLAGS"]

[default.configure]
cmd     = "cmake -S /workspace -B /workspace/build"
workdir = "/workspace"

[default.build]
cmd     = "cmake --build /workspace/build"
workdir = "/workspace/build"
env     = ["JOBS=4"]

[default.test]
cmd     = "ctest --test-dir /workspace/build --output-on-failure"
workdir = "/workspace/build"

[default.shell]
cmd         = "bash"
workdir     = "/workspace"
interactive = true

[default.ci]
pipeline = ["configure", "build", "test"]

[default.prepare]
cmd    = "cmake -S /workspace -B /workspace/build -DCMAKE_BUILD_TYPE=Release"
hidden = true

[clang]
image = "silkeh/clang:18"
env   = ["CC=clang", "CXX=clang++", "MAKEFLAGS"]

[clang.configure]
cmd     = "cmake -S /workspace -B /workspace/build -DCMAKE_BUILD_TYPE=Release"
workdir = "/workspace"

[clang.build]
cmd     = "cmake --build /workspace/build -- -j$(nproc)"
workdir = "/workspace/build"
"#;

    const REPO_EXAMPLE: &str = r#"
[project]
profile = "default"

[default]
image = "my-org/my-project:latest"

[default.configure]
cmd = "cmake -S . -B build -DCMAKE_BUILD_TYPE=Release"

[default.build]
cmd = "cmake --build build"

[default.test]
cmd = "ctest --test-dir build --output-on-failure"

[default.shell]
cmd         = "bash"
interactive = true

[default.ci]
pipeline = ["configure", "build", "test"]
"#;

    const PROJECT_EXAMPLE: &str = r#"
[project]
pattern = ".*/Github/myproject(/.*)?$"
profile = "default"

[default]
image = "myorg/custom-image:1.2.3"

[default.configure]
cmd     = "cmake -S /workspace -B /workspace/build -DMYPROJECT_ENABLE_TESTS=ON"
workdir = "/workspace"

[default.test]
cmd     = "ctest --test-dir /workspace/build --output-on-failure -j4"
workdir = "/workspace/build"
env     = ["CTEST_OUTPUT_ON_FAILURE=1", "BRANCH=$(git branch --show-current)"]

[default.lint]
cmd     = "clang-tidy -p /workspace/build $(find /workspace/src -name '*.cpp')"
workdir = "/workspace"
"#;

    #[test]
    fn parse_global() {
        let f = parse_str(GLOBAL_EXAMPLE).unwrap();
        assert!(f.project.is_none());
        let def = f.profiles.get("default").unwrap();
        assert_eq!(def.image.as_deref(), Some("ubuntu:22.04"));
        assert_eq!(
            def.env,
            Some(vec!["CC".into(), "CXX".into(), "MAKEFLAGS".into()])
        );
        assert_eq!(def.aliases.len(), 6);
        let build = def.aliases.get("build").unwrap();
        assert_eq!(build.cmd.as_deref(), Some("cmake --build /workspace/build"));
        assert_eq!(build.env, Some(vec!["JOBS=4".into()]));
        let ci = def.aliases.get("ci").unwrap();
        assert_eq!(
            ci.pipeline,
            Some(vec!["configure".into(), "build".into(), "test".into()])
        );
        let prepare = def.aliases.get("prepare").unwrap();
        assert_eq!(prepare.hidden, Some(true));
        let clang = f.profiles.get("clang").unwrap();
        assert_eq!(clang.image.as_deref(), Some("silkeh/clang:18"));
        assert_eq!(clang.aliases.len(), 2);
    }

    #[test]
    fn parse_repo() {
        let f = parse_str(REPO_EXAMPLE).unwrap();
        let proj = f.project.as_ref().unwrap();
        assert_eq!(proj.profile.as_deref(), Some("default"));
        assert!(proj.pattern.is_none());
        let def = f.profiles.get("default").unwrap();
        assert_eq!(def.image.as_deref(), Some("my-org/my-project:latest"));
        assert_eq!(def.aliases.len(), 5);
        let shell = def.aliases.get("shell").unwrap();
        assert_eq!(shell.interactive, Some(true));
    }

    #[test]
    fn parse_project() {
        let f = parse_str(PROJECT_EXAMPLE).unwrap();
        let proj = f.project.as_ref().unwrap();
        assert_eq!(proj.pattern.as_deref(), Some(".*/Github/myproject(/.*)?$"));
        let def = f.profiles.get("default").unwrap();
        assert_eq!(def.image.as_deref(), Some("myorg/custom-image:1.2.3"));
        let test = def.aliases.get("test").unwrap();
        assert_eq!(
            test.env,
            Some(vec![
                "CTEST_OUTPUT_ON_FAILURE=1".into(),
                "BRANCH=$(git branch --show-current)".into()
            ])
        );
    }
}
