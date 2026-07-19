use globset::Glob;
use serde_json::Value;

use super::model::{IssuesEventType, PrEventType, ReleaseEventType, Workflow};

/// Details about how/why a workflow should run, extracted from a matched webhook event.
pub struct MatchedRun {
    pub ref_name: Option<String>,
    pub commit_sha: Option<String>,
}

/// Determine whether `workflow` should run in response to a given GitHub webhook event.
pub fn matches(workflow: &Workflow, github_event: &str, payload: &Value) -> Option<MatchedRun> {
    match github_event {
        "push" => match_push(workflow, payload),
        "pull_request" => match_pull_request(workflow, payload),
        "release" => match_release(workflow, payload),
        "issues" => match_issues(workflow, payload),
        _ => None,
    }
}

fn match_push(workflow: &Workflow, payload: &Value) -> Option<MatchedRun> {
    let push = workflow.on.push.as_ref()?;
    let git_ref = payload.get("ref")?.as_str()?;

    let (kind, short_ref) = if let Some(b) = git_ref.strip_prefix("refs/heads/") {
        ("branch", b)
    } else if let Some(t) = git_ref.strip_prefix("refs/tags/") {
        ("tag", t)
    } else {
        ("branch", git_ref)
    };

    let patterns = if kind == "tag" { &push.tags } else { &push.branches };
    if !patterns.is_empty() && !glob_any(patterns, short_ref) {
        return None;
    }

    if !push.paths.is_empty() {
        let changed_paths = extract_changed_paths(payload);
        if !changed_paths.is_empty() && !changed_paths.iter().any(|p| glob_any(&push.paths, p)) {
            return None;
        }
    }

    Some(MatchedRun {
        ref_name: Some(git_ref.to_string()),
        commit_sha: payload
            .get("after")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

fn match_pull_request(workflow: &Workflow, payload: &Value) -> Option<MatchedRun> {
    let pr_trigger = workflow.on.pull_request.as_ref()?;
    let action = payload.get("action")?.as_str()?;
    let event_type = pr_event_type_from_action(action)?;

    if !pr_trigger.types.is_empty() && !pr_trigger.types.contains(&event_type) {
        return None;
    }

    let pr = payload.get("pull_request")?;
    let base_ref = pr.get("base")?.get("ref")?.as_str()?;
    if !pr_trigger.branches.is_empty() && !glob_any(&pr_trigger.branches, base_ref) {
        return None;
    }

    Some(MatchedRun {
        ref_name: Some(format!("refs/pull/{}/head", payload.get("number")?.as_u64()?)),
        commit_sha: pr
            .get("head")
            .and_then(|h| h.get("sha"))
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    })
}

fn pr_event_type_from_action(action: &str) -> Option<PrEventType> {
    Some(match action {
        "opened" => PrEventType::Opened,
        "synchronize" => PrEventType::Synchronize,
        "reopened" => PrEventType::Reopened,
        "closed" => PrEventType::Closed,
        "labeled" => PrEventType::Labeled,
        "unlabeled" => PrEventType::Unlabeled,
        "ready_for_review" => PrEventType::ReadyForReview,
        _ => return None,
    })
}

fn match_release(workflow: &Workflow, payload: &Value) -> Option<MatchedRun> {
    let release_trigger = workflow.on.release.as_ref()?;
    let action = payload.get("action")?.as_str()?;
    let event_type = release_event_type_from_action(action)?;

    if !release_trigger.types.is_empty() && !release_trigger.types.contains(&event_type) {
        return None;
    }

    let release = payload.get("release")?;
    Some(MatchedRun {
        ref_name: release.get("tag_name").and_then(|v| v.as_str()).map(|s| s.to_string()),
        commit_sha: None,
    })
}

fn release_event_type_from_action(action: &str) -> Option<ReleaseEventType> {
    Some(match action {
        "published" => ReleaseEventType::Published,
        "created" => ReleaseEventType::Created,
        "edited" => ReleaseEventType::Edited,
        "deleted" => ReleaseEventType::Deleted,
        "prereleased" => ReleaseEventType::Prereleased,
        "released" => ReleaseEventType::Released,
        _ => return None,
    })
}

/// Issues have no commit to attach a run to, so the correlated ref is a synthetic
/// `refs/issues/{number}` marker (mirroring the `refs/pull/{number}/head` scheme used for PRs)
/// purely so the UI can look up which runs belong to a given issue.
fn match_issues(workflow: &Workflow, payload: &Value) -> Option<MatchedRun> {
    let issues_trigger = workflow.on.issues.as_ref()?;
    let action = payload.get("action")?.as_str()?;
    let event_type = issues_event_type_from_action(action)?;

    if !issues_trigger.types.is_empty() && !issues_trigger.types.contains(&event_type) {
        return None;
    }

    let number = payload.get("issue")?.get("number")?.as_u64()?;
    Some(MatchedRun { ref_name: Some(format!("refs/issues/{number}")), commit_sha: None })
}

fn issues_event_type_from_action(action: &str) -> Option<IssuesEventType> {
    Some(match action {
        "opened" => IssuesEventType::Opened,
        "edited" => IssuesEventType::Edited,
        "closed" => IssuesEventType::Closed,
        "reopened" => IssuesEventType::Reopened,
        "labeled" => IssuesEventType::Labeled,
        "unlabeled" => IssuesEventType::Unlabeled,
        "assigned" => IssuesEventType::Assigned,
        "unassigned" => IssuesEventType::Unassigned,
        _ => return None,
    })
}

fn glob_any(patterns: &[String], candidate: &str) -> bool {
    patterns.iter().any(|p| {
        Glob::new(p)
            .map(|g| g.compile_matcher().is_match(candidate))
            .unwrap_or(false)
    })
}

fn extract_changed_paths(payload: &Value) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(commits) = payload.get("commits").and_then(|c| c.as_array()) {
        for commit in commits {
            for key in ["added", "removed", "modified"] {
                if let Some(arr) = commit.get(key).and_then(|v| v.as_array()) {
                    for p in arr {
                        if let Some(s) = p.as_str() {
                            paths.push(s.to_string());
                        }
                    }
                }
            }
        }
    }
    paths
}
