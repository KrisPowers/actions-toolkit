use anyhow::{Context, Result};
use axum::http::StatusCode;
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};

#[derive(Serialize)]
struct CreateHookRequest {
    name: &'static str,
    active: bool,
    events: Vec<&'static str>,
    config: CreateHookConfig,
}

#[derive(Serialize)]
struct CreateHookConfig {
    url: String,
    content_type: &'static str,
    secret: String,
}

#[derive(Deserialize)]
struct CreatedHook {
    id: u64,
}

/// Creates a repo webhook pointed at this instance's own receiver (`webhooks::receive`, which
/// this migration leaves untouched), subscribed to the same push/PR/release events the previous
/// manual setup instructed users to add by hand. Returns the hook's GitHub-side ID so it can be
/// torn down later by `delete_webhook`. octocrab has no typed hooks-create API (only
/// `list_deliveries`/`retry_delivery`), so this goes through its generic `post`, which already
/// maps a non-2xx response into an `Err` for us.
pub async fn create_webhook(client: &Octocrab, owner: &str, repo: &str, payload_url: &str, secret: &str) -> Result<u64> {
    let body = CreateHookRequest {
        name: "web",
        active: true,
        events: vec!["push", "pull_request", "release"],
        config: CreateHookConfig { url: payload_url.to_string(), content_type: "json", secret: secret.to_string() },
    };

    let hook: CreatedHook = client
        .post(format!("/repos/{owner}/{repo}/hooks"), Some(&body))
        .await
        .context("failed to create the GitHub webhook")?;
    Ok(hook.id)
}

/// Deletes a repo webhook. A 404 (already gone on GitHub's side, e.g. removed by hand) is treated
/// as success rather than an error, since the caller's desired end state ("no webhook left
/// behind") holds either way. Uses the raw `_delete` (not the typed `delete`) specifically so a
/// 404 can be handled here instead of octocrab turning it into an `Err` before we get a look at
/// the status code, and because a successful delete's 204 has no body for a typed call to parse.
pub async fn delete_webhook(client: &Octocrab, owner: &str, repo: &str, hook_id: u64) -> Result<()> {
    let response = client
        ._delete(format!("/repos/{owner}/{repo}/hooks/{hook_id}"), None::<&()>)
        .await
        .context("failed to reach GitHub to delete the webhook")?;

    match response.status() {
        status if status.is_success() => Ok(()),
        StatusCode::NOT_FOUND => Ok(()),
        status => anyhow::bail!("GitHub rejected the webhook delete with status {status}"),
    }
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
    async fn create_webhook_posts_to_the_repo_and_returns_the_hook_id() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/hooks"))
            .respond_with(ResponseTemplate::new(201).set_body_json(serde_json::json!({ "id": 4242 })))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        let hook_id =
            create_webhook(&client, "octocat", "hello-world", "https://example.com/webhooks/github/repo-1", "s3cr3t").await.unwrap();
        assert_eq!(hook_id, 4242);
    }

    #[tokio::test]
    async fn create_webhook_fails_when_github_rejects_the_request() {
        let mock_server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/repos/octocat/hello-world/hooks"))
            .respond_with(ResponseTemplate::new(403).set_body_json(serde_json::json!({ "message": "insufficient permission" })))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        assert!(create_webhook(&client, "octocat", "hello-world", "https://example.com/webhooks/github/repo-1", "s3cr3t").await.is_err());
    }

    /// Rule-proving test: a 404 on delete (the hook is already gone on GitHub's side) is success,
    /// not an error.
    #[tokio::test]
    async fn delete_webhook_treats_a_404_as_success() {
        let mock_server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/octocat/hello-world/hooks/4242"))
            .respond_with(ResponseTemplate::new(404))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        assert!(delete_webhook(&client, "octocat", "hello-world", 4242).await.is_ok());
    }

    #[tokio::test]
    async fn delete_webhook_succeeds_on_a_204_from_github() {
        let mock_server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/octocat/hello-world/hooks/4242"))
            .respond_with(ResponseTemplate::new(204))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        assert!(delete_webhook(&client, "octocat", "hello-world", 4242).await.is_ok());
    }

    #[tokio::test]
    async fn delete_webhook_fails_on_an_unexpected_error_status() {
        let mock_server = MockServer::start().await;
        Mock::given(method("DELETE"))
            .and(path("/repos/octocat/hello-world/hooks/4242"))
            .respond_with(ResponseTemplate::new(403))
            .mount(&mock_server)
            .await;

        let client = test_client(&mock_server).await;
        assert!(delete_webhook(&client, "octocat", "hello-world", 4242).await.is_err());
    }
}
