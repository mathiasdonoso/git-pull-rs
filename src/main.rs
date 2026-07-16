mod forge;
mod pull;
mod summary;

use forge::Forges;
use pull::PathExt;
use std::{fs, io, path::PathBuf, thread};
use summary::{GitPullSummary, PullResult};

/// The git repositories directly under `path`, as (name, path) pairs.
fn repos_in(path: &str) -> io::Result<Vec<(String, PathBuf)>> {
    let mut repos = Vec::new();

    for entry in fs::read_dir(path)? {
        let entry = entry?;
        let path = entry.path();

        if !entry.file_type()?.is_dir() || !path.join(".git").exists() {
            continue;
        }

        repos.push((entry.file_name().to_string_lossy().into_owned(), path));
    }

    Ok(repos)
}

fn run() -> io::Result<()> {
    let path = std::env::args().nth(1).unwrap_or_else(|| String::from("."));
    let repos = repos_in(&path)?;
    let forges = Forges::default();

    // One thread per repo: the work is a blocking `git` subprocess, so these
    // spend nearly all their time parked on IO rather than competing for CPU.
    let results = thread::scope(|scope| {
        let handles: Vec<_> = repos
            .into_iter()
            .map(|(name, path)| {
                let forges = &forges;
                scope.spawn(move || PullResult {
                    state: path.pull_if_clean(forges),
                    name,
                })
            })
            .collect();

        // Joining in spawn order keeps the table row order stable.
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap())
            .collect()
    });

    GitPullSummary { results }.print();

    Ok(())
}

fn main() {
    if let Err(e) = run() {
        eprintln!("gp: {e}");
        std::process::exit(1);
    }
}
