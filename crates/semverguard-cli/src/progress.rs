//! Progress tracking for semverguard CLI.
//!
//! This module provides progress reporting during semver checks, including
//! a spinner for the current package and an overall progress bar.

use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use semverguard_types::PackageStatus;
use std::io::IsTerminal;
use std::time::Duration;

/// Progress display choice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProgressChoice {
    /// Automatically show progress if stderr is a TTY.
    Auto,
    /// Always show progress bar/spinner.
    Always,
    /// Never show progress (simple line output instead).
    Never,
}

impl Default for ProgressChoice {
    fn default() -> Self {
        ProgressChoice::Auto
    }
}

/// Events that can be reported to the progress tracker.
#[derive(Debug, Clone)]
pub enum ProgressEvent<'a> {
    /// Total number of packages to check is known.
    TotalPackages(usize),
    /// A package check is starting.
    PackageStarted {
        /// Package name.
        name: &'a str,
        /// Current index (0-based).
        index: usize,
        /// Total packages.
        total: usize,
    },
    /// A package check completed.
    PackageCompleted {
        /// Package name.
        name: &'a str,
        /// Result status.
        status: PackageStatus,
        /// Duration in milliseconds.
        duration_ms: u128,
    },
    /// A package was skipped (no check needed).
    PackageSkipped {
        /// Package name.
        name: &'a str,
        /// Reason for skipping.
        reason: &'a str,
    },
    /// All checks completed.
    Finished {
        /// Total passed.
        passed: usize,
        /// Total failed.
        failed: usize,
        /// Total skipped.
        skipped: usize,
    },
}

/// Trait for progress reporters.
pub trait ProgressReporter: Send + Sync {
    /// Report a progress event.
    fn report(&self, event: ProgressEvent<'_>);

    /// Finish and clear the progress display.
    #[allow(dead_code)]
    fn finish(&self);
}

/// A no-op progress reporter that does nothing.
/// Used when progress is disabled or output is piped.
pub struct NoopProgress;

impl NoopProgress {
    /// Create a new no-op progress reporter.
    pub fn new() -> Self {
        NoopProgress
    }
}

impl Default for NoopProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for NoopProgress {
    fn report(&self, _event: ProgressEvent<'_>) {
        // Do nothing
    }

    fn finish(&self) {
        // Do nothing
    }
}

/// A simple line-by-line progress reporter for non-TTY output.
/// Prints status updates as simple lines to stderr.
pub struct LineProgress;

impl LineProgress {
    /// Create a new line progress reporter.
    pub fn new() -> Self {
        LineProgress
    }
}

impl Default for LineProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for LineProgress {
    fn report(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::TotalPackages(total) => {
                eprintln!("Checking {} package(s)...", total);
            }
            ProgressEvent::PackageStarted { name, index, total } => {
                eprintln!("[{}/{}] Checking {}...", index + 1, total, name);
            }
            ProgressEvent::PackageCompleted {
                name,
                status,
                duration_ms,
            } => {
                let status_str = match status {
                    PackageStatus::Passed => "PASS",
                    PackageStatus::Failed => "FAIL",
                    PackageStatus::Skipped => "SKIP",
                };
                eprintln!(
                    "  {} {} ({:.1}s)",
                    status_str,
                    name,
                    duration_ms as f64 / 1000.0
                );
            }
            ProgressEvent::PackageSkipped { name, reason } => {
                eprintln!("  SKIP {} ({})", name, reason);
            }
            ProgressEvent::Finished {
                passed,
                failed,
                skipped,
            } => {
                eprintln!(
                    "Done: {} passed, {} failed, {} skipped",
                    passed, failed, skipped
                );
            }
        }
    }

    fn finish(&self) {
        // Nothing to clean up
    }
}

/// A terminal progress reporter with spinner and progress bar.
pub struct TerminalProgress {
    /// Held to keep progress bars alive.
    #[allow(dead_code)]
    multi: MultiProgress,
    progress_bar: ProgressBar,
    spinner: ProgressBar,
}

impl TerminalProgress {
    /// Create a new terminal progress reporter.
    pub fn new() -> Self {
        let multi = MultiProgress::new();

        // Overall progress bar
        let progress_bar = multi.add(ProgressBar::new(0));
        progress_bar.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} packages ({eta})")
                .expect("valid template")
                .progress_chars("#>-"),
        );
        progress_bar.enable_steady_tick(Duration::from_millis(100));

        // Spinner for current package
        let spinner = multi.add(ProgressBar::new_spinner());
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("  {spinner:.yellow} {msg}")
                .expect("valid template"),
        );
        spinner.enable_steady_tick(Duration::from_millis(80));

        Self {
            multi,
            progress_bar,
            spinner,
        }
    }
}

impl Default for TerminalProgress {
    fn default() -> Self {
        Self::new()
    }
}

impl ProgressReporter for TerminalProgress {
    fn report(&self, event: ProgressEvent<'_>) {
        match event {
            ProgressEvent::TotalPackages(total) => {
                self.progress_bar.set_length(total as u64);
            }
            ProgressEvent::PackageStarted { name, index, total } => {
                self.progress_bar.set_position(index as u64);
                self.progress_bar.set_length(total as u64);
                self.spinner.set_message(format!("Checking {}...", name));
            }
            ProgressEvent::PackageCompleted {
                name,
                status,
                duration_ms,
            } => {
                let status_str = match status {
                    PackageStatus::Passed => "PASS",
                    PackageStatus::Failed => "FAIL",
                    PackageStatus::Skipped => "SKIP",
                };
                self.spinner.set_message(format!(
                    "{} {} ({:.1}s)",
                    status_str,
                    name,
                    duration_ms as f64 / 1000.0
                ));
                self.progress_bar.inc(1);
            }
            ProgressEvent::PackageSkipped { name, reason } => {
                self.spinner
                    .set_message(format!("SKIP {} ({})", name, reason));
            }
            ProgressEvent::Finished {
                passed,
                failed,
                skipped,
            } => {
                self.spinner.finish_and_clear();
                self.progress_bar.finish_with_message(format!(
                    "Done: {} passed, {} failed, {} skipped",
                    passed, failed, skipped
                ));
            }
        }
    }

    fn finish(&self) {
        self.spinner.finish_and_clear();
        self.progress_bar.finish_and_clear();
    }
}

/// Create an appropriate progress reporter based on the choice.
pub fn create_progress_reporter(choice: ProgressChoice) -> Box<dyn ProgressReporter> {
    match choice {
        ProgressChoice::Never => Box::new(NoopProgress::new()),
        ProgressChoice::Always => Box::new(TerminalProgress::new()),
        ProgressChoice::Auto => auto_progress_reporter(std::io::stderr().is_terminal()),
    }
}

fn auto_progress_reporter(is_terminal: bool) -> Box<dyn ProgressReporter> {
    if is_terminal {
        Box::new(TerminalProgress::new())
    } else {
        Box::new(LineProgress::new())
    }
}

/// An adapter that bridges the domain's ProgressCallback trait to the CLI's ProgressReporter.
///
/// This allows the runner to report progress events that are then displayed
/// using the CLI's progress reporting infrastructure.
pub struct ProgressCallbackAdapter {
    reporter: Box<dyn ProgressReporter>,
}

impl ProgressCallbackAdapter {
    /// Create a new adapter wrapping a ProgressReporter.
    pub fn new(reporter: Box<dyn ProgressReporter>) -> Self {
        Self { reporter }
    }
}

impl semverguard_core::ProgressCallback for ProgressCallbackAdapter {
    fn on_progress(&self, event: semverguard_core::ProgressEvent) {
        use semverguard_core::ProgressEvent as DomainEvent;

        // Convert domain events to CLI events and report them
        match event {
            DomainEvent::TotalPackages { total } => {
                self.reporter.report(ProgressEvent::TotalPackages(total));
            }
            DomainEvent::PackageStarted { name, index, total } => {
                self.reporter.report(ProgressEvent::PackageStarted {
                    name: &name,
                    index,
                    total,
                });
            }
            DomainEvent::PackageCompleted {
                name,
                status,
                duration_ms,
            } => {
                self.reporter.report(ProgressEvent::PackageCompleted {
                    name: &name,
                    status,
                    duration_ms,
                });
            }
            DomainEvent::PackageSkipped { name, reason } => {
                self.reporter.report(ProgressEvent::PackageSkipped {
                    name: &name,
                    reason: &reason,
                });
            }
            DomainEvent::Finished {
                passed,
                failed,
                skipped,
            } => {
                self.reporter.report(ProgressEvent::Finished {
                    passed,
                    failed,
                    skipped,
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use semverguard_core::ProgressCallback;
    use std::sync::{Arc, Mutex};

    #[test]
    fn test_progress_choice_default() {
        assert_eq!(ProgressChoice::default(), ProgressChoice::Auto);
    }

    #[test]
    fn test_noop_progress_does_nothing() {
        let progress = NoopProgress::new();
        progress.report(ProgressEvent::TotalPackages(10));
        progress.report(ProgressEvent::PackageStarted {
            name: "test",
            index: 0,
            total: 10,
        });
        progress.report(ProgressEvent::PackageCompleted {
            name: "test",
            status: PackageStatus::Passed,
            duration_ms: 1000,
        });
        progress.finish();
        // No assertions needed - just verify it doesn't panic
    }

    #[test]
    fn test_line_progress_reports_events() {
        let progress = LineProgress::new();
        progress.report(ProgressEvent::TotalPackages(3));
        progress.report(ProgressEvent::PackageStarted {
            name: "alpha",
            index: 0,
            total: 3,
        });
        progress.report(ProgressEvent::PackageCompleted {
            name: "alpha",
            status: PackageStatus::Passed,
            duration_ms: 1200,
        });
        progress.report(ProgressEvent::PackageCompleted {
            name: "beta",
            status: PackageStatus::Failed,
            duration_ms: 800,
        });
        progress.report(ProgressEvent::PackageCompleted {
            name: "gamma",
            status: PackageStatus::Skipped,
            duration_ms: 10,
        });
        progress.report(ProgressEvent::PackageSkipped {
            name: "delta",
            reason: "filtered",
        });
        progress.report(ProgressEvent::Finished {
            passed: 1,
            failed: 1,
            skipped: 1,
        });
        progress.finish();
    }

    #[test]
    fn test_terminal_progress_reports_events() {
        let progress = TerminalProgress::new();
        progress.report(ProgressEvent::TotalPackages(2));
        progress.report(ProgressEvent::PackageStarted {
            name: "alpha",
            index: 0,
            total: 2,
        });
        progress.report(ProgressEvent::PackageCompleted {
            name: "alpha",
            status: PackageStatus::Passed,
            duration_ms: 500,
        });
        progress.report(ProgressEvent::PackageStarted {
            name: "beta",
            index: 1,
            total: 2,
        });
        progress.report(ProgressEvent::PackageCompleted {
            name: "beta",
            status: PackageStatus::Failed,
            duration_ms: 250,
        });
        progress.report(ProgressEvent::PackageSkipped {
            name: "gamma",
            reason: "not publishable",
        });
        progress.report(ProgressEvent::Finished {
            passed: 1,
            failed: 1,
            skipped: 0,
        });
        progress.finish();
    }

    #[test]
    fn test_create_progress_reporter_choices() {
        let reporter = create_progress_reporter(ProgressChoice::Never);
        reporter.report(ProgressEvent::TotalPackages(1));

        let reporter = create_progress_reporter(ProgressChoice::Always);
        reporter.report(ProgressEvent::TotalPackages(1));

        let reporter = create_progress_reporter(ProgressChoice::Auto);
        reporter.report(ProgressEvent::TotalPackages(1));
    }

    #[test]
    fn test_auto_progress_reporter_branches() {
        let reporter = auto_progress_reporter(true);
        reporter.report(ProgressEvent::TotalPackages(1));

        let reporter = auto_progress_reporter(false);
        reporter.report(ProgressEvent::TotalPackages(1));
    }

    #[test]
    fn test_progress_defaults_construct() {
        let _noop = NoopProgress::default();
        let _line = LineProgress::default();
        let _terminal = TerminalProgress::default();
    }

    #[test]
    fn test_terminal_progress_reports_skipped_status() {
        let progress = TerminalProgress::new();
        progress.report(ProgressEvent::PackageCompleted {
            name: "skippy",
            status: PackageStatus::Skipped,
            duration_ms: 5,
        });
        progress.finish();
    }

    #[derive(Clone, Default)]
    struct RecordingReporter {
        events: Arc<Mutex<Vec<String>>>,
    }

    impl ProgressReporter for RecordingReporter {
        fn report(&self, event: ProgressEvent<'_>) {
            let label = match event {
                ProgressEvent::TotalPackages(total) => format!("total:{total}"),
                ProgressEvent::PackageStarted { name, .. } => format!("start:{name}"),
                ProgressEvent::PackageCompleted { name, status, .. } => {
                    format!("done:{name}:{status:?}")
                }
                ProgressEvent::PackageSkipped { name, reason } => {
                    format!("skip:{name}:{reason}")
                }
                ProgressEvent::Finished {
                    passed,
                    failed,
                    skipped,
                } => {
                    format!("finished:{passed}:{failed}:{skipped}")
                }
            };
            self.events.lock().expect("lock").push(label);
        }

        fn finish(&self) {
            self.events.lock().expect("lock").push("finish".to_string());
        }
    }

    #[test]
    fn test_progress_callback_adapter_maps_events() {
        let reporter = RecordingReporter::default();
        let adapter = ProgressCallbackAdapter::new(Box::new(reporter.clone()));

        adapter.on_progress(semverguard_core::ProgressEvent::TotalPackages { total: 2 });
        adapter.on_progress(semverguard_core::ProgressEvent::PackageStarted {
            name: "alpha".to_string(),
            index: 0,
            total: 2,
        });
        adapter.on_progress(semverguard_core::ProgressEvent::PackageCompleted {
            name: "alpha".to_string(),
            status: PackageStatus::Passed,
            duration_ms: 10,
        });
        adapter.on_progress(semverguard_core::ProgressEvent::PackageSkipped {
            name: "beta".to_string(),
            reason: "filtered".to_string(),
        });
        adapter.on_progress(semverguard_core::ProgressEvent::Finished {
            passed: 1,
            failed: 0,
            skipped: 1,
        });

        let events = reporter.events.lock().expect("lock").clone();
        assert_eq!(
            events,
            vec![
                "total:2",
                "start:alpha",
                "done:alpha:Passed",
                "skip:beta:filtered",
                "finished:1:0:1",
            ]
        );
    }

    #[test]
    fn test_recording_reporter_finish() {
        let reporter = RecordingReporter::default();
        reporter.finish();
        let events = reporter.events.lock().expect("lock").clone();
        assert_eq!(events, vec!["finish"]);
    }
}
