use anyhow::{Context, Result};
use octocrab::Octocrab;
use serde::Serialize;

/// Validate a token by asking GitHub who it belongs to. Returns the authenticated login.
pub async fn validate_token(client: &Octocrab) -> Result<String> {
    let user = client.current().user().await.context("token was rejected by GitHub")?;
    Ok(user.login)
}

#[derive(Debug, Clone, Serialize)]
pub struct AccessibleRepo {
    pub owner: String,
    pub name: String,
    pub full_name: String,
    pub private: bool,
    pub default_branch: String,
}

/// List repos the configured token can access, for the "connect a repo" picker. Fetches up to
/// a few hundred repos (a handful of pages) which comfortably covers individual/small-org use;
/// very large orgs may not see their entire repo list here, but can still connect a repo by
/// exact owner/name if it's missing.
pub async fn list_accessible_repos(client: &Octocrab) -> Result<Vec<AccessibleRepo>> {
    let mut repos = Vec::new();
    for page in 1..=3u8 {
        let response = client
            .current()
            .list_repos_for_authenticated_user()
            .per_page(100)
            .page(page)
            .sort("full_name")
            .send()
            .await
            .context("failed to list repos for the configured token")?;

        let page_len = response.items.len();
        for repo in response.items {
            let Some(full_name) = repo.full_name else { continue };
            let Some((owner, name)) = full_name.split_once('/') else { continue };
            repos.push(AccessibleRepo {
                owner: owner.to_string(),
                name: name.to_string(),
                full_name: full_name.clone(),
                private: repo.private.unwrap_or(false),
                default_branch: repo.default_branch.unwrap_or_else(|| "main".to_string()),
            });
        }

        if page_len < 100 {
            break;
        }
    }
    Ok(repos)
}
