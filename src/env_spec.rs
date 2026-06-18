use std::process::Command;

/// Evaluate a single env spec value, running shell command substitution if present.
pub fn eval_value(raw: &str) -> String {
    if raw.contains("$(") || raw.contains('`') || raw.contains("${") {
        let out = Command::new("sh")
            .arg("-c")
            .arg(format!("printf '%s' \"{raw}\""))
            .output();
        match out {
            Ok(o) => String::from_utf8_lossy(&o.stdout)
                .trim_end_matches('\n')
                .to_owned(),
            Err(_) => raw.to_owned(),
        }
    } else {
        raw.to_owned()
    }
}

/// Parse a list of env specs and return `-e KEY=VAL` flag pairs for docker exec.
/// Specs:
///   "KEY"         — pass host value (skip if unset)
///   "KEY=VAL"     — set to literal or evaluated value
///   "KEY=$(cmd)"  — evaluate cmd on host
pub fn build_env_flags(specs: &[String]) -> Vec<String> {
    let mut flags = Vec::new();
    for spec in specs {
        let spec = spec.trim();
        if spec.is_empty() {
            continue;
        }

        if let Some(eq) = spec.find('=') {
            let key = &spec[..eq];
            let raw_val = &spec[eq + 1..];
            let val = eval_value(raw_val);
            flags.push("-e".into());
            flags.push(format!("{key}={val}"));
        } else {
            // pass-through: skip if not set in host environment
            if let Ok(val) = std::env::var(spec) {
                flags.push("-e".into());
                flags.push(format!("{spec}={val}"));
            }
        }
    }
    flags
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn literal_value() {
        let flags = build_env_flags(&["KEY=value".into()]);
        assert_eq!(flags, vec!["-e", "KEY=value"]);
    }

    #[test]
    fn passthrough_set() {
        std::env::set_var("DEX_TEST_PASS", "hello");
        let flags = build_env_flags(&["DEX_TEST_PASS".into()]);
        assert_eq!(flags, vec!["-e", "DEX_TEST_PASS=hello"]);
    }

    #[test]
    fn passthrough_unset() {
        std::env::remove_var("DEX_TEST_MISSING_XYZ");
        let flags = build_env_flags(&["DEX_TEST_MISSING_XYZ".into()]);
        assert!(flags.is_empty());
    }

    #[test]
    fn cmd_substitution() {
        let flags = build_env_flags(&["RESULT=$(echo hello)".into()]);
        assert_eq!(flags, vec!["-e", "RESULT=hello"]);
    }

    #[test]
    fn value_with_spaces_preserved() {
        let result = eval_value("$(echo hello world)");
        assert_eq!(result, "hello world");
    }

    #[test]
    fn bash_default_expansion_unset() {
        std::env::remove_var("DEX_TEST_UNSET_PORT");
        let result = eval_value("${DEX_TEST_UNSET_PORT:-3061}:3061");
        assert_eq!(result, "3061:3061");
    }

    #[test]
    fn bash_default_expansion_set() {
        std::env::set_var("DEX_TEST_PORT", "8080");
        let result = eval_value("${DEX_TEST_PORT:-3061}:3061");
        assert_eq!(result, "8080:3061");
    }

    #[test]
    fn multiple_specs() {
        std::env::set_var("CC", "gcc");
        let flags = build_env_flags(&["CC".into(), "CXX=g++".into()]);
        assert_eq!(flags, vec!["-e", "CC=gcc", "-e", "CXX=g++"]);
    }
}
