//! Rendering what happened to each repository as a table.

use crate::pull::PullOutcome;
use comfy_table::*;

pub struct PullResult {
    pub name: String,
    pub state: PullOutcome,
}

pub struct GitPullSummary {
    pub results: Vec<PullResult>,
}

impl GitPullSummary {
    pub fn print(&self) {
        let mut table = Table::new();
        table
            .load_preset(presets::UTF8_FULL)
            .set_content_arrangement(ContentArrangement::Dynamic)
            .set_header(vec!["Repository", "Status", "Details"]);

        for result in &self.results {
            let (label, color, details) = match &result.state {
                PullOutcome::Pulled => ("updated", Color::Green, String::new()),
                PullOutcome::SkippedDirty => ("skipped", Color::Yellow, "dirty tree".into()),
                PullOutcome::SkippedNoOrigin => ("skipped", Color::Yellow, "no origin".into()),
                PullOutcome::SkippedHttpRemote => ("skipped", Color::Yellow, "http remote".into()),
                PullOutcome::SkippedInProgress => {
                    ("skipped", Color::Yellow, "merge in progress".into())
                }
                PullOutcome::RateLimited(reason) => {
                    ("rate limited", Color::Magenta, reason.clone())
                }
                PullOutcome::Failed(e) => ("pull failed", Color::Red, first_line(e)),
            };

            table.add_row(vec![
                Cell::new(&result.name),
                Cell::new(label).fg(color),
                Cell::new(details),
            ]);
        }

        println!("{table}");
    }
}

/// Git errors are often multi-line; the first line carries the reason.
fn first_line(text: &str) -> String {
    text.lines().next().unwrap_or_default().trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_line_takes_the_reason_off_a_multi_line_error() {
        let stderr = "fatal: Could not read from remote repository.\n\n\
                      Please make sure you have the correct access rights.";

        assert_eq!(
            first_line(stderr),
            "fatal: Could not read from remote repository."
        );
    }

    #[test]
    fn first_line_handles_empty_input() {
        assert_eq!(first_line(""), "");
    }
}
