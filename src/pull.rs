//! Deciding whether a repository can be fast-forwarded, and doing it.

use crate::forge::Forges;
use std::{io, path::Path, process::Command};

#[derive(Debug)]
pub enum PullOutcome {
    Pulled,
    SkippedDirty,
    SkippedNoOrigin,
    SkippedHttpRemote,
    SkippedInProgress, // mid-merge/rebase/cherry-pick
    RateLimited(String),
    Failed(String),
}

pub trait PathExt {
    fn pull_if_clean(&self, forges: &Forges) -> PullOutcome;
}

impl PathExt for Path {
    fn pull_if_clean(&self, forges: &Forges) -> PullOutcome {
        // Bail if there is no `origin` remote configured
        let origin = match run_git(self, &["remote", "get-url", "origin"]) {
            Ok(output) if output.status.success() => {
                String::from_utf8_lossy(&output.stdout).trim().to_owned()
            }
            Ok(_) => return PullOutcome::SkippedNoOrigin,
            Err(e) => return PullOutcome::Failed(e.to_string()),
        };

        // Bail if cloned using http
        if origin.starts_with("http://") || origin.starts_with("https://") {
            return PullOutcome::SkippedHttpRemote;
        }

        // Bail if a merge/rebase/cherry-pick is WIP
        if self.join(".git/MERGE_HEAD").exists()
            || self.join(".git/rebase-merge").exists()
            || self.join(".git/rebase-apply").exists()
        {
            return PullOutcome::SkippedInProgress;
        }

        // Bail if the working tree is not clean
        match run_git(self, &["status", "--porcelain"]) {
            Ok(output) if !output.stdout.is_empty() => return PullOutcome::SkippedDirty,
            Err(e) => return PullOutcome::Failed(e.to_string()),
            Ok(_) => {}
        }

        // Everything above is local; the pull is the only call that reaches
        // the forge, so it is the only one that needs a slot.
        forges.wait_turn(&origin);

        // Actually pull
        match run_git(self, &["pull", "--ff-only"]) {
            Ok(output) if output.status.success() => PullOutcome::Pulled,
            Ok(output) => {
                let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
                match rate_limit_reason(&stderr) {
                    Some(reason) => PullOutcome::RateLimited(reason),
                    None => PullOutcome::Failed(stderr),
                }
            }
            Err(e) => PullOutcome::Failed(e.to_string()),
        }
    }
}

fn run_git(dir: &Path, args: &[&str]) -> io::Result<std::process::Output> {
    Command::new("git").args(args).current_dir(dir).output()
}

/// Returns the stderr line reporting that we are being throttled, if any.
///
/// Neither GitLab's docs nor its source pin down one literal string, and the
/// wording differs per forge and version, so match the phrases these responses
/// are known to carry rather than an exact message. The server's own line is
/// returned so the table shows what it actually said. A false negative here
/// just surfaces as a plain failure, which is the pre-existing behaviour.
fn rate_limit_reason(stderr: &str) -> Option<String> {
    // The 429 forms stay reachable despite http(s) origins being skipped:
    // an ssh remote still fetches LFS objects over https.
    const SIGNALS: [&str; 5] = [
        "rate limit",
        "too many requests",
        "requested too many times",
        "error: 429",
        "http 429",
    ];

    stderr
        .lines()
        .find(|line| {
            let line = line.to_lowercase();
            SIGNALS.iter().any(|signal| line.contains(signal))
        })
        .map(|line| line.trim().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rate_limit_reason_finds_throttling_but_ignores_other_failures() {
        assert_eq!(
            rate_limit_reason("remote: GitLab: This endpoint has been requested too many times.")
                .as_deref(),
            Some("remote: GitLab: This endpoint has been requested too many times.")
        );
        assert_eq!(
            rate_limit_reason("error: RPC failed; HTTP 429 curl 22").as_deref(),
            Some("error: RPC failed; HTTP 429 curl 22")
        );
        assert_eq!(
            rate_limit_reason("fatal: Not possible to fast-forward, aborting."),
            None
        );
    }

    #[test]
    fn rate_limit_reason_picks_the_offending_line_out_of_noise() {
        let stderr = "Downloading f.txt (2.0 KB)\n\
                      batch response: Rate limit exceeded. Try again in 60 seconds.\n\
                      fatal: Could not read from remote repository.";

        assert_eq!(
            rate_limit_reason(stderr).as_deref(),
            Some("batch response: Rate limit exceeded. Try again in 60 seconds.")
        );
    }
}
