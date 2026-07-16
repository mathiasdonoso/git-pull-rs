# gp

Fast-forward all your git repositories in one go.

A Rust rewrite of [mathiasdonoso/git-pull](https://github.com/mathiasdonoso/git-pull), originally written in Go.

## Install

```sh
cargo install --path .
```

## Usage

```sh
gp [directory]
```

Scans the immediate subdirectories of `directory` (default: `.`) and runs
`git pull --ff-only` in each one that is a git repository. Repositories are
pulled concurrently.

```
┌────────────────┬──────────────┬────────────────────────────────────────────────┐
│ Repository     ┆ Status       ┆ Details                                        │
╞════════════════╪══════════════╪════════════════════════════════════════════════╡
│ api            ┆ updated      ┆                                                │
│ web            ┆ skipped      ┆ dirty tree                                     │
│ infra          ┆ rate limited ┆ remote: GitLab: ... requested too many times.  │
│ docs           ┆ pull failed  ┆ fatal: Not possible to fast-forward, aborting. │
└────────────────┴──────────────┴────────────────────────────────────────────────┘
```

## Rate limits

Pulls are paced per host to the limits each forge publishes for git operations:

| Host            | Documented limit                                                                                                             |
| --------------- | ---------------------------------------------------------------------------------------------------------------------------- |
| `github.com`    | [15 operations/second](https://docs.github.com/en/repositories/creating-and-managing-repositories/repository-limits) (per repository) |
| `gitlab.com`    | [600 Git SSH operations/minute](https://docs.gitlab.com/administration/settings/rate_limits_on_git_ssh_operations/) (per user)  |
| `bitbucket.org` | [60,000 git operations/hour](https://support.atlassian.com/bitbucket-cloud/docs/api-request-limits/) (per user)                 |

Other hosts, including self-hosted instances, are not paced — their limits are
configured per instance. If a forge throttles anyway, the repository is
reported as `rate limited` rather than as a failure.

## Development

```sh
cargo test
```
