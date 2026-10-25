//! Events streamed from the runner to the TUI.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Running,
    Success,
    Failure,
    Skipped,
    Cancelled,
}

impl Status {
    pub fn is_done(self) -> bool {
        !matches!(self, Status::Pending | Status::Running)
    }

    /// The string used in `steps.*.outcome` / `needs.*.result`.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pending | Status::Running => "",
            Status::Success => "success",
            Status::Failure => "failure",
            Status::Skipped => "skipped",
            Status::Cancelled => "cancelled",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogKind {
    Output,
    /// Echo of the script being run.
    Command,
    /// Messages from ATK itself.
    System,
    /// Start of a step (used in the combined log view).
    Header,
    Group,
    Notice,
    Warning,
    Error,
}

#[derive(Debug, Clone)]
pub enum RunEvent {
    Job {
        job: usize,
        status: Status,
    },
    Step {
        job: usize,
        step: usize,
        status: Status,
    },
    Log {
        job: usize,
        step: usize,
        kind: LogKind,
        text: String,
    },
    Finished {
        success: bool,
    },
}
