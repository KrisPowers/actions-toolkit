//! Events streamed from the runner to whichever front-end is attached (TUI or plain).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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

    /// The string GitHub uses in `steps.*.outcome` / `needs.*.result`.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogKind {
    Output,
    /// Echo of the script being run.
    Command,
    /// Messages from maverick itself.
    System,
    /// Start of a step (used in the combined log view).
    Header,
    Group,
    Notice,
    Warning,
    Error,
}
