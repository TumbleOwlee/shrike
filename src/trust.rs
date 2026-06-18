//! Trust gate for repo/project-layer config values that get shell-evaluated on
//! the host (env `KEY=$(...)`, ports, volumes, user). A malicious repo could
//! otherwise run arbitrary host commands the moment `shrike` is invoked inside
//! it. We prompt once per (repo, value-set) and cache the grant under
//! `~/.cache/shrike/trusted`.

use std::fs;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};

use crate::config::types::ConfigFile;
use crate::display::output::{self, Colors};

fn has_eval(s: &str) -> bool {
    s.contains("$(") || s.contains('`') || s.contains("${")
}

/// Collect every repo/project value that would be shell-evaluated on the host.
fn collect_eval_values(file: &ConfigFile, out: &mut Vec<String>) {
    let push = |v: &str, out: &mut Vec<String>| {
        if has_eval(v) {
            out.push(v.to_owned());
        }
    };
    for profile in file.profiles.values() {
        for v in profile.env.iter().flatten() {
            push(v, out);
        }
        for v in profile.ports.iter().flatten() {
            push(v, out);
        }
        for v in profile.volumes.iter().flatten() {
            push(v, out);
        }
        if let Some(u) = &profile.user {
            push(u, out);
        }
        for alias in profile.aliases.values() {
            for v in alias.env.iter().flatten() {
                push(v, out);
            }
            if let Some(u) = &alias.user {
                push(u, out);
            }
        }
    }
}

fn fnv1a(data: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in data {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

fn cache_file() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("shrike").join("trusted"))
}

fn is_cached(hash: u64) -> bool {
    let Some(path) = cache_file() else {
        return false;
    };
    let Ok(content) = fs::read_to_string(&path) else {
        return false;
    };
    let tag = format!("{hash:016x}");
    content
        .lines()
        .filter_map(|l| l.split('\t').next())
        .any(|h| h == tag)
}

fn store(hash: u64, git_root: &Path) -> Result<(), String> {
    let Some(path) = cache_file() else {
        return Err("cannot locate cache directory".into());
    };
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| format!("creating cache dir: {e}"))?;
    }
    let line = format!("{hash:016x}\t{}\n", git_root.display());
    let mut f = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|e| format!("writing trust cache: {e}"))?;
    f.write_all(line.as_bytes())
        .map_err(|e| format!("writing trust cache: {e}"))
}

/// Ensure repo/project config values that run on the host are user-approved.
/// No-op when no such values exist. Errors (aborts) when not approved or when
/// approval can't be requested (non-interactive session).
pub fn ensure_repo_trusted(
    git_root: &Path,
    repo: &ConfigFile,
    project: Option<&ConfigFile>,
) -> Result<(), String> {
    let mut vals = Vec::new();
    collect_eval_values(repo, &mut vals);
    if let Some(p) = project {
        collect_eval_values(p, &mut vals);
    }
    if vals.is_empty() {
        return Ok(());
    }

    // Hash includes the repo path so a moved/cloned repo re-prompts, and the
    // exact values so editing them re-prompts.
    let mut input = git_root.to_string_lossy().into_owned();
    for v in &vals {
        input.push('\0');
        input.push_str(v);
    }
    let hash = fnv1a(input.as_bytes());

    if is_cached(hash) {
        return Ok(());
    }

    prompt_and_store(git_root, &vals, hash)
}

fn prompt_and_store(git_root: &Path, vals: &[String], hash: u64) -> Result<(), String> {
    if !output::stdin_is_tty() {
        return Err(format!(
            "untrusted repo config at {} contains values evaluated on the host:\n    {}\nrun shrike interactively once to approve it, or remove these values",
            git_root.display(),
            vals.join("\n    "),
        ));
    }

    let c = Colors::stderr();
    eprintln!(
        "\n {rd}{b}⚠  trust check:{r}  repo config at {yl}{root}{r} evaluates these on the {b}host{r}:",
        rd = c.rd,
        b = c.b,
        r = c.r,
        yl = c.yl,
        root = git_root.display(),
    );
    for v in vals {
        eprintln!("    {dim}{v}{r}", dim = c.dim, r = c.r);
    }
    eprint!(
        " {b}Trust this repo and run them on the host? [y/N]{r} ",
        b = c.b,
        r = c.r,
    );
    io::stderr().flush().ok();

    let mut line = String::new();
    io::stdin()
        .lock()
        .read_line(&mut line)
        .map_err(|e| format!("reading input: {e}"))?;
    let ans = line.trim().to_ascii_lowercase();
    if ans == "y" || ans == "yes" {
        store(hash, git_root)?;
        Ok(())
    } else {
        Err("repo config not trusted; aborting".into())
    }
}
