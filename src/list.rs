use std::path::Path;

use crate::config::resolve::ConfigState;
use crate::config::types::{AliasConfig, ConfigFile};
use crate::display::output::{line_ext, Colors};

pub fn list_aliases(
    state: &ConfigState,
    global_file: Option<&Path>,
    repo_file: Option<&Path>,
    project_file: Option<&Path>,
) {
    let c = Colors::stdout();
    let ext = line_ext();
    let hdr = format!("─────────────────────────────────────────────────────{ext}");
    let sep = format!("└───────────────────────────────────────────────────────────────────{ext}");
    let ftr = format!("  └─────────────────────────────────────────────────────────────────{ext}");

    println!("{mg} ┌─ shrike aliases {hdr}{r}", mg = c.mg, r = c.r);
    println!(
        "{mg} │{r} {b}Profile  :{r} {yl}{p}{r}",
        mg = c.mg,
        b = c.b,
        r = c.r,
        yl = c.yl,
        p = state.profile_name,
    );
    if let Some(img) = &state.image {
        println!(
            "{mg} │{r} {b}Image    :{r} {img}",
            mg = c.mg,
            b = c.b,
            r = c.r
        );
    }
    if let Some(p) = global_file {
        println!(
            "{mg} │{r} {b}Global   :{r} {dim}{p}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            dim = c.dim,
            p = p.display(),
        );
    }
    if let Some(p) = repo_file {
        println!(
            "{mg} │{r} {b}Repo     :{r} {cy}{p}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            cy = c.cy,
            p = p.display(),
        );
    }
    if let Some(p) = project_file {
        println!(
            "{mg} │{r} {b}Local    :{r} {dim}{p}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            dim = c.dim,
            p = p.display(),
        );
    }
    if !state.env.is_empty() {
        println!(
            "{mg} │{r} {b}Base env :{r} {gr}{env}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            gr = c.gr,
            env = state.env.join(", "),
        );
    }
    println!("{rd} {sep}{r}", rd = c.rd, r = c.r);

    // collect visible aliases sorted
    let mut names: Vec<&str> = state
        .aliases
        .iter()
        .filter(|(_, a)| a.hidden != Some(true))
        .map(|(n, _)| n.as_str())
        .collect();
    names.sort_unstable();

    if names.is_empty() {
        println!(
            "{cy}   │{r}  {dim}(no aliases defined for this profile){r}",
            cy = c.cy,
            r = c.r,
            dim = c.dim,
        );
    } else {
        let name_width = names.iter().map(|n| n.len()).max().unwrap_or(10).max(10);
        let mut first = true;
        for name in &names {
            if !first {
                println!("{rd}   │{r}", rd = c.rd, r = c.r);
            }
            first = false;

            let alias = &state.aliases[*name];
            if alias.pipeline.is_some() {
                print_pipeline_entry(&c, name, alias, name_width, &state.env);
            } else {
                print_alias_entry(&c, name, alias, name_width);
            }
        }
    }

    println!("{rd} {ftr}{r}", rd = c.rd, r = c.r);
}

fn print_pipeline_entry(
    c: &Colors,
    name: &str,
    alias: &AliasConfig,
    width: usize,
    _profile_env: &[String],
) {
    let steps = alias.pipeline.as_ref().unwrap();
    let steps_fmt = steps.join("  →  ");

    if let Some(desc) = &alias.desc {
        println!(
            "{rd}   │{r}  {mg}{name:<width$}{r} {b}desc:{r}    {yl}{desc}{r}",
            rd = c.rd,
            mg = c.mg,
            b = c.b,
            r = c.r,
            yl = c.yl,
        );
        println!(
            "{rd}   │{r}  {mg}{blank:<width$}{r} {mg}[pipeline]{r}  {rd}{steps}{r}",
            rd = c.rd,
            mg = c.mg,
            r = c.r,
            blank = "",
            steps = steps_fmt,
        );
    } else {
        println!(
            "{rd}   │{r}  {mg}{name:<width$}{r} {mg}[pipeline]{r}  {rd}{steps}{r}",
            rd = c.rd,
            mg = c.mg,
            r = c.r,
            steps = steps_fmt,
        );
    }

    if let Some(ref penv) = alias.env {
        println!(
            "{rd}   │{r}  {blank:<width$} {b}env:{r}     {yln}{env}{r}",
            rd = c.rd,
            b = c.b,
            r = c.r,
            yln = c.yln,
            blank = "",
            env = penv.join(", "),
        );
    }
}

fn print_alias_entry(c: &Colors, name: &str, alias: &AliasConfig, width: usize) {
    let cmd = alias.cmd.as_deref().unwrap_or("");

    if let Some(desc) = &alias.desc {
        println!(
            "{rd}   │{r}  {yl}{name:<width$}{r} {b}desc:{r}    {yl}{desc}{r}",
            rd = c.rd,
            yl = c.yl,
            b = c.b,
            r = c.r,
        );
        let itag = if alias.interactive == Some(true) {
            format!("  {rd}(interactive){r}", rd = c.rd, r = c.r)
        } else {
            String::new()
        };
        println!(
            "{rd}   │{r}  {yl}{blank:<width$}{r} {b}command:{r} {cyb}{cmd}{r}{itag}",
            rd = c.rd,
            yl = c.yl,
            b = c.b,
            r = c.r,
            cyb = c.cyb,
            blank = "",
        );
    } else {
        let itag = if alias.interactive == Some(true) {
            format!("  {rd}(interactive){r}", rd = c.rd, r = c.r)
        } else {
            String::new()
        };
        println!(
            "{rd}   │{r}  {yl}{name:<width$}{r} {b}command:{r} {cyb}{cmd}{r}{itag}",
            rd = c.rd,
            yl = c.yl,
            b = c.b,
            r = c.r,
            cyb = c.cyb,
        );
    }

    if let Some(wd) = &alias.workdir {
        println!(
            "{rd}   │{r}  {blank:<width$} {b}workdir:{r} {gr}{wd}{r}",
            rd = c.rd,
            b = c.b,
            r = c.r,
            gr = c.gr,
            blank = "",
        );
    }
    if let Some(env) = &alias.env {
        println!(
            "{rd}   │{r}  {blank:<width$} {b}env:{r}     {yln}{env}{r}",
            rd = c.rd,
            b = c.b,
            r = c.r,
            yln = c.yln,
            blank = "",
            env = env.join(", "),
        );
    }
    if let Some(user) = &alias.user {
        println!(
            "{rd}   │{r}  {blank:<width$} {b}user:{r}    {yln}{user}{r}",
            rd = c.rd,
            b = c.b,
            r = c.r,
            yln = c.yln,
            blank = "",
        );
    }
}

pub fn list_profiles(
    profiles: &[String],
    active: &str,
    global_file: Option<&Path>,
    repo_file: Option<&Path>,
    project_file: Option<&Path>,
    global: &ConfigFile,
    repo: &ConfigFile,
    project: Option<&ConfigFile>,
) {
    let c = Colors::stdout();
    let ext = line_ext();
    let hdr = format!("──────────────────────────────────────────────────────{ext}");
    let sep = format!("└───────────────────────────────────────────────────────────────────{ext}");
    let ftr = format!("  └─────────────────────────────────────────────────────────────────{ext}");

    println!("{mg} ┌─ shrike profiles {hdr}{r}", mg = c.mg, r = c.r);
    println!(
        "{mg} │{r} {b}Active   :{r} {yl}{active}{r}",
        mg = c.mg,
        b = c.b,
        r = c.r,
        yl = c.yl,
    );
    if let Some(p) = global_file {
        println!(
            "{mg} │{r} {b}Global   :{r} {dim}{p}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            dim = c.dim,
            p = p.display(),
        );
    }
    if let Some(p) = repo_file {
        println!(
            "{mg} │{r} {b}Repo     :{r} {cy}{p}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            cy = c.cy,
            p = p.display(),
        );
    }
    if let Some(p) = project_file {
        println!(
            "{mg} │{r} {b}Local    :{r} {dim}{p}{r}",
            mg = c.mg,
            b = c.b,
            r = c.r,
            dim = c.dim,
            p = p.display(),
        );
    }
    println!("{rd} {sep}{r}", rd = c.rd, r = c.r);

    if profiles.is_empty() {
        println!(
            "{cy}   │{r}  {dim}(no profiles defined){r}",
            cy = c.cy,
            r = c.r,
            dim = c.dim,
        );
    } else {
        let mut sorted = profiles.to_vec();
        sorted.sort_unstable();
        let mut first = true;
        for name in &sorted {
            if !first {
                println!("{rd}   │{r}", rd = c.rd, r = c.r);
            }
            first = false;

            let is_active = name.as_str() == active;
            let in_global = global.profiles.contains_key(name.as_str());
            let in_repo = repo.profiles.contains_key(name.as_str());
            let in_project = project.map_or(false, |p| p.profiles.contains_key(name.as_str()));

            let mut sources = Vec::new();
            if in_global {
                sources.push("Global");
            }
            if in_repo {
                sources.push("Repo");
            }
            if in_project {
                sources.push("Local");
            }
            let src_tag = format!("[{}]", sources.join(", "));

            // resolved image/dockerfile: project > repo > global
            let image = project
                .and_then(|p| p.profiles.get(name.as_str()))
                .and_then(|p| p.image.as_deref())
                .or_else(|| {
                    repo.profiles
                        .get(name.as_str())
                        .and_then(|p| p.image.as_deref())
                })
                .or_else(|| {
                    global
                        .profiles
                        .get(name.as_str())
                        .and_then(|p| p.image.as_deref())
                });
            let dockerfile = project
                .and_then(|p| p.profiles.get(name.as_str()))
                .and_then(|p| p.dockerfile.as_deref())
                .or_else(|| {
                    repo.profiles
                        .get(name.as_str())
                        .and_then(|p| p.dockerfile.as_deref())
                })
                .or_else(|| {
                    global
                        .profiles
                        .get(name.as_str())
                        .and_then(|p| p.dockerfile.as_deref())
                });

            if is_active {
                println!(
                    "{rd}   │{r}  {rd}[★]{r} {b}{yl}{name:<14}{r} {dim}{src}{r}",
                    rd = c.rd,
                    b = c.b,
                    yl = c.yl,
                    r = c.r,
                    dim = c.dim,
                    src = src_tag,
                );
            } else {
                println!(
                    "{rd}   │{r}      {yl}{name:<14}{r} {dim}{src}{r}",
                    rd = c.rd,
                    yl = c.yl,
                    r = c.r,
                    dim = c.dim,
                    src = src_tag,
                );
            }
            if let Some(img) = image {
                println!(
                    "{rd}   │{r}      {dim}image:{r}         {cy}{img}{r}",
                    rd = c.rd,
                    r = c.r,
                    dim = c.dim,
                    cy = c.cy,
                );
            }
            if let Some(df) = dockerfile {
                println!(
                    "{rd}   │{r}      {dim}dockerfile:{r}    {cy}{df}{r}",
                    rd = c.rd,
                    r = c.r,
                    dim = c.dim,
                    cy = c.cy,
                );
            }
        }
    }

    println!("{rd} {ftr}{r}", rd = c.rd, r = c.r);
}
