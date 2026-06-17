use std::path::Path;

use crate::env_spec::eval_value;

pub fn resolve(alias_workdir: Option<&str>, git_root: &Path, cwd: &Path) -> String {
    match alias_workdir {
        Some(w) => eval_value(w),
        None => {
            let rel = cwd.strip_prefix(git_root).unwrap_or(Path::new(""));
            let rel_str = rel.to_string_lossy();
            if rel_str.is_empty() {
                "/workspace".to_owned()
            } else {
                format!("/workspace/{rel_str}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn explicit_workdir() {
        let root = PathBuf::from("/repo");
        let cwd = PathBuf::from("/repo/src");
        assert_eq!(
            resolve(Some("/workspace/build"), &root, &cwd),
            "/workspace/build"
        );
    }

    #[test]
    fn mirror_cwd_subdir() {
        let root = PathBuf::from("/repo");
        let cwd = PathBuf::from("/repo/src/lib");
        assert_eq!(resolve(None, &root, &cwd), "/workspace/src/lib");
    }

    #[test]
    fn mirror_cwd_at_root() {
        let root = PathBuf::from("/repo");
        let cwd = PathBuf::from("/repo");
        assert_eq!(resolve(None, &root, &cwd), "/workspace");
    }

    #[test]
    fn mirror_cwd_outside_root() {
        let root = PathBuf::from("/repo");
        let cwd = PathBuf::from("/other");
        assert_eq!(resolve(None, &root, &cwd), "/workspace");
    }
}
