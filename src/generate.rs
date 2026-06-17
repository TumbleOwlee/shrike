use std::path::Path;

const DEFAULT_TEMPLATE: &str = r#"[project]
profile = "default"

[default]
image = "ubuntu:22.04"
# env     = ["CC", "CXX", "MAKEFLAGS"]
# ports   = ["8080:8080"]
# volumes = ["/host/path:/container/path"]
# user    = "$(id -u):$(id -g)"
# setup   = "sudo chown -R $(id -u):$(id -g) /workspace"

[default.shell]
cmd         = "bash"
workdir     = "/workspace"
interactive = true
"#;

const CMAKE_TEMPLATE: &str = r#"[project]
profile = "default"

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
"#;

pub fn generate(template: Option<&str>, git_root: &Path) {
    let content = match template.unwrap_or("default") {
        "cmake" => CMAKE_TEMPLATE,
        "default" => DEFAULT_TEMPLATE,
        other => {
            eprintln!("shrike:unknown template `{other}`; use `default` or `cmake`");
            std::process::exit(1);
        }
    };

    let target = git_root.join(".shrike.toml");
    if target.exists() {
        eprintln!("shrike:.shrike.toml already exists; not overwriting");
        std::process::exit(1);
    }
    std::fs::write(&target, content).unwrap_or_else(|e| {
        eprintln!("shrike:{e}");
        std::process::exit(1);
    });
    println!("created {}", target.display());
}
