use std::path::Path;

pub fn slug(s: &str) -> String {
    let mut out = String::new();
    let mut prev_hyphen = true;
    for c in s.to_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
            prev_hyphen = false;
        } else if !prev_hyphen {
            out.push('-');
            prev_hyphen = true;
        }
    }
    out.trim_end_matches('-').to_owned()
}

pub fn container_name(git_root: &Path, profile: &str, platform: Option<&str>) -> String {
    match platform {
        Some(p) => format!(
            "shrike--{}--{}--{}",
            slug(&git_root.to_string_lossy()),
            slug(profile),
            slug(p)
        ),
        None => format!(
            "shrike--{}--{}",
            slug(&git_root.to_string_lossy()),
            slug(profile),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_basic() {
        assert_eq!(
            slug("/home/user/projects/myapp"),
            "home-user-projects-myapp"
        );
    }

    #[test]
    fn slug_consecutive_separators() {
        assert_eq!(slug("/foo//bar"), "foo-bar");
    }

    #[test]
    fn slug_trailing() {
        assert_eq!(slug("/foo/bar/"), "foo-bar");
    }

    #[test]
    fn container_name_format() {
        let root = std::path::Path::new("/home/user/projects/myapp");
        assert_eq!(
            container_name(root, "default", "main", None),
            "shrike--home-user-projects-myapp--default--main"
        );
    }

    #[test]
    fn container_name_branch_slug() {
        let root = std::path::Path::new("/repo");
        assert_eq!(
            container_name(root, "dev", "feature/my-thing", None),
            "shrike--repo--dev--feature-my-thing"
        );
    }

    #[test]
    fn container_name_with_platform() {
        let root = std::path::Path::new("/repo");
        assert_eq!(
            container_name(root, "dev", "main", Some("linux/arm64")),
            "shrike--repo--dev--main--linux-arm64"
        );
    }
}
