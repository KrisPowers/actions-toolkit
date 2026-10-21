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
