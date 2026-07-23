use anyhow::{Context, Result};
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};

/// The name GitHub Apps under; distinct from the "actions-toolkit" commit-status context so the
/// two mechanisms (this and `status.rs`) never collide if both end up posted for the same commit.
const CHECK_NAME: &str = "actions-toolkit";

#[derive(Serialize)]
struct CreateCheckRunRequest<'a> {
    name: &'a str,
    head_sha: &'a str,
    status: &'static str,
    started_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    details_url: Option<String>,
}

#[derive(Deserialize)]
struct CreatedCheckRun {
    id: u64,
}

/// Starts a GitHub check run for a commit-triggered workflow run, the same mechanism real
/// GitHub Actions uses for the check mark/X/yellow-circle shown next to a commit, in a PR's
/// checks list, and in branch protection, rather than the older, plainer commit-status dot.
/// `details_url`, when given, is what a click on that check in GitHub's UI links to.
pub async fn start(client: &Octocrab, owner: &str, repo: &str, head_sha: &str, details_url: Option<String>) -> Result<u64> {
    let body = CreateCheckRunRequest {
        name: CHECK_NAME,
        head_sha,
        status: "in_progress",
        started_at: chrono::Utc::now().to_rfc3339(),
        details_url,
    };
    let created: CreatedCheckRun = client
        .post(format!("/repos/{owner}/{repo}/check-runs"), Some(&body))
        .await
        .context("failed to create the GitHub check run")?;
    Ok(created.id)
}

#[derive(Serialize)]
struct UpdateCheckRunRequest {
    status: &'static str,
    conclusion: &'static str,
    completed_at: String,
}

/// Marks a check run started by `start` as completed, with the run's actual outcome. GitHub
/// renders `conclusion: "failure"` as the red X and `"success"` as the green check next to the
/// commit; `status: "in_progress"` (what `start` set) is what renders as the yellow spinner in
/// the meantime.
pub async fn complete(client: &Octocrab, owner: &str, repo: &str, check_run_id: u64, succeeded: bool) -> Result<()> {
    let body = UpdateCheckRunRequest {
        status: "completed",
        conclusion: if succeeded { "success" } else { "failure" },
        completed_at: chrono::Utc::now().to_rfc3339(),
    };
    let _: serde_json::Value = client
        .patch(format!("/repos/{owner}/{repo}/check-runs/{check_run_id}"), Some(&body))
        .await
        .context("failed to update the GitHub check run")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use wiremock::matchers::{method, path};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    async fn test_client(mock_server: &MockServer) -> Octocrab {
        Octocrab::builder().base_uri(mock_server.uri()).unwrap().personal_token("test-token".to_string()).build().unwrap()
    }

    #[tokio::test]
    async fn start_posts_in_progress_and_returns_the_check_run_id() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/check-runs"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": 4242 })))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        let id = start(&client, "octocat", "hello-world", "abc123", Some("https://example.com/runs/1".to_string())).await.unwrap();
        assert_eq!(id, 4242);

        let requests = mock_server.received_requests().await.unwrap();
        let body: serde_json::Value = requests[0].body_json().unwrap();
        assert_eq!(body["status"], "in_progress");
        assert_eq!(body["head_sha"], "abc123");
        assert_eq!(body["details_url"], "https://example.com/runs/1");
    }

    #[tokio::test]
    async fn complete_patches_the_matching_conclusion() {
        let mock_server = MockServer::start().await;
        Mock::given(method("PATCH"))
            .and(path("/repos/octocat/hello-world/check-runs/4242"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        complete(&client, "octocat", "hello-world", 4242, true).await.unwrap();
        complete(&client, "octocat", "hello-world", 4242, false).await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let conclusions: Vec<String> =
            requests.iter().map(|r| r.body_json::<serde_json::Value>().unwrap()["conclusion"].as_str().unwrap().to_string()).collect();
        assert_eq!(conclusions, vec!["success", "failure"]);
        for r in &requests {
            let body: serde_json::Value = r.body_json().unwrap();
            assert_eq!(body["status"], "completed");
        }
    }

    #[tokio::test]
    async fn start_fails_when_github_rejects_the_request() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/check-runs"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        assert!(start(&client, "octocat", "hello-world", "abc123", None).await.is_err());
    }
}
