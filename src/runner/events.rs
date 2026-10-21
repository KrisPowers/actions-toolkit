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
