//! Pacing pulls against the rate limits forges publish for git operations.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

/// Minimum spacing between git read operations against `host`, derived from
/// the forge's documented limit. `None` for hosts we have no figure for,
/// including self-hosted instances, which are configurable per instance.
///
/// - github.com: 15 operations per second, scoped *per repository*
///   <https://docs.github.com/en/repositories/creating-and-managing-repositories/repository-limits>
/// - gitlab.com: 600 Git SSH operations per minute, scoped per user
///   <https://docs.gitlab.com/administration/settings/rate_limits_on_git_ssh_operations/>
/// - bitbucket.org: 60,000 git operations per hour (ssh and https), per user
///   <https://support.atlassian.com/bitbucket-cloud/docs/api-request-limits/>
fn documented_pace(host: &str) -> Option<Duration> {
    let (operations, per) = match host {
        "github.com" => (15, Duration::from_secs(1)),
        "gitlab.com" => (600, Duration::from_secs(60)),
        "bitbucket.org" => (60_000, Duration::from_secs(60 * 60)),
        _ => return None,
    };

    Some(per / operations)
}

/// Extracts the host from a git remote URL: `scheme://[user@]host[:port]/path`
/// or scp-style `[user@]host:path`. Local paths have no host.
fn host_of(origin: &str) -> Option<String> {
    let authority = match origin.split_once("://") {
        Some((_, rest)) => rest.split(['/', ':']).next()?,
        None => {
            let (authority, _) = origin.split_once(':')?;
            // A local path such as /srv/git/repo.git is not an authority.
            if authority.contains('/') {
                return None;
            }
            authority
        }
    };

    let host = authority.rsplit('@').next()?;
    (!host.is_empty()).then(|| host.to_lowercase())
}

/// Spaces requests to a single host at least `interval` apart.
struct Pacer {
    interval: Duration,
    next_slot: Mutex<Instant>,
}

impl Pacer {
    fn new(interval: Duration) -> Self {
        Self {
            interval,
            next_slot: Mutex::new(Instant::now()),
        }
    }

    /// Claims the next slot, then blocks until it comes up.
    fn wait_turn(&self) {
        let slot = {
            let mut next_slot = self.next_slot.lock().unwrap();
            let slot = (*next_slot).max(Instant::now());
            *next_slot = slot + self.interval;
            slot
        };

        // Sleep outside the lock, so waiting threads claim later slots
        // concurrently rather than queueing up behind this one.
        if let Some(wait) = slot.checked_duration_since(Instant::now()) {
            thread::sleep(wait);
        }
    }
}

/// The hosts being pulled from, each paced to its documented limit.
#[derive(Default)]
pub struct Forges {
    pacers: Mutex<HashMap<String, Arc<Pacer>>>,
}

impl Forges {
    /// Blocks until this repo may hit `origin`'s host. Hosts without a
    /// documented limit are not paced.
    pub fn wait_turn(&self, origin: &str) {
        let Some(host) = host_of(origin) else { return };
        let Some(interval) = documented_pace(&host) else {
            return;
        };

        let pacer = {
            let mut pacers = self.pacers.lock().unwrap();
            Arc::clone(
                pacers
                    .entry(host)
                    .or_insert_with(|| Arc::new(Pacer::new(interval))),
            )
        };

        pacer.wait_turn();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_of_reads_scp_style_remotes() {
        assert_eq!(
            host_of("git@github.com:acme/repo.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("git@GitLab.com:acme/repo.git").as_deref(),
            Some("gitlab.com")
        );
    }

    #[test]
    fn host_of_reads_url_style_remotes() {
        assert_eq!(
            host_of("ssh://git@github.com/acme/repo.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("ssh://git@github.com:22/acme/repo.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            host_of("https://bitbucket.org/acme/repo.git").as_deref(),
            Some("bitbucket.org")
        );
    }

    #[test]
    fn host_of_ignores_local_paths() {
        assert_eq!(host_of("/srv/git/repo.git"), None);
        assert_eq!(host_of("../sibling/repo.git"), None);
    }

    #[test]
    fn documented_pace_matches_published_limits() {
        // 15 operations per second
        assert_eq!(
            documented_pace("github.com"),
            Some(Duration::from_nanos(66_666_666))
        );
        // 600 per minute
        assert_eq!(
            documented_pace("gitlab.com"),
            Some(Duration::from_millis(100))
        );
        // 60,000 per hour
        assert_eq!(
            documented_pace("bitbucket.org"),
            Some(Duration::from_millis(60))
        );
    }

    #[test]
    fn documented_pace_is_absent_for_unknown_hosts() {
        assert_eq!(documented_pace("git.internal.example"), None);
    }

    #[test]
    fn pacer_spaces_out_turns() {
        let pacer = Pacer::new(Duration::from_millis(50));
        let started = Instant::now();

        thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| pacer.wait_turn());
            }
        });

        // Four slots at 50ms apart: the last starts 150ms in.
        assert!(started.elapsed() >= Duration::from_millis(150));
    }

    #[test]
    fn unpaced_hosts_do_not_block() {
        let forges = Forges::default();
        let started = Instant::now();

        for _ in 0..100 {
            forges.wait_turn("git@git.internal.example:acme/repo.git");
        }

        assert!(started.elapsed() < Duration::from_millis(50));
    }
}

