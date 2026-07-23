use anyhow::{Context, Result};
use octocrab::Octocrab;
use serde::Serialize;

/// The context string GitHub groups this instance's statuses under on a commit, distinct from
/// GitHub's own `github-actions` context so the two never collide on the same commit.
const STATUS_CONTEXT: &str = "actions-toolkit";

#[derive(Serialize)]
struct CreateStatusRequest {
    state: &'static str,
    description: String,
    context: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_url: Option<String>,
}

/// Posts a commit status so a push triggering a run shows up on GitHub itself (the commit's
/// status list, PR checks, branch protection), the same way the run already shows up in this
/// instance's own UI. `target_url`, when given, links that status back to the run here.
async fn create_status(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    sha: &str,
    state: &'static str,
    description: &str,
    target_url: Option<String>,
) -> Result<()> {
    let body = CreateStatusRequest { state, description: description.to_string(), context: STATUS_CONTEXT, target_url };
    let _: serde_json::Value = client
        .post(format!("/repos/{owner}/{repo}/statuses/{sha}"), Some(&body))
        .await
        .context("failed to create the GitHub commit status")?;
    Ok(())
}

pub async fn mark_pending(client: &Octocrab, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<()> {
    create_status(client, owner, repo, sha, "pending", "Workflow run in progress", target_url).await
}

pub async fn mark_success(client: &Octocrab, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<()> {
    create_status(client, owner, repo, sha, "success", "Workflow run succeeded", target_url).await
}

pub async fn mark_failure(client: &Octocrab, owner: &str, repo: &str, sha: &str, target_url: Option<String>) -> Result<()> {
    create_status(client, owner, repo, sha, "failure", "Workflow run failed", target_url).await
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
    async fn mark_pending_posts_the_expected_state_and_context() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/statuses/abc123"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        mark_pending(&client, "octocat", "hello-world", "abc123", Some("https://example.com/runs/1".to_string()))
            .await
            .unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let body: serde_json::Value = requests[0].body_json().unwrap();
        assert_eq!(body["state"], "pending");
        assert_eq!(body["context"], "actions-toolkit");
        assert_eq!(body["target_url"], "https://example.com/runs/1");
    }

    #[tokio::test]
    async fn mark_success_and_failure_post_the_matching_state() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/statuses/abc123"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({})))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        mark_success(&client, "octocat", "hello-world", "abc123", None).await.unwrap();
        mark_failure(&client, "octocat", "hello-world", "abc123", None).await.unwrap();

        let requests = mock_server.received_requests().await.unwrap();
        let states: Vec<String> =
            requests.iter().map(|r| r.body_json::<serde_json::Value>().unwrap()["state"].as_str().unwrap().to_string()).collect();
        assert_eq!(states, vec!["success", "failure"]);
    }

    #[tokio::test]
    async fn create_status_fails_when_github_rejects_the_request() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/statuses/abc123"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        assert!(mark_pending(&client, "octocat", "hello-world", "abc123", None).await.is_err());
    }
}
