use anyhow::{Context, Result};
use octocrab::Octocrab;
use serde::{Deserialize, Serialize};

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

#[derive(Debug, Deserialize)]
struct InstallationRepositories {
    repositories: Vec<octocrab::models::Repository>,
}

#[derive(Debug, Deserialize)]
struct Installation {
    id: i64,
    app_slug: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListInstallationsResponse {
    installations: Vec<Installation>,
}

/// Finds the installation of `app_slug` (the actions-toolkit App) among the ones the connected
/// user administers. Device flow has no callback to carry an `installation_id` the way the
/// redirect-based authorize flow did, so this is looked up explicitly right after connecting
/// instead. Returns `None` if the user hasn't installed the App on any account yet, distinct from
/// a request failure, so the caller can prompt "install the App" rather than surface an error.
pub async fn find_installation_id(client: &Octocrab, app_slug: &str) -> Result<Option<i64>> {
    let resp: ListInstallationsResponse =
        client.get("/user/installations", None::<&()>).await.context("failed to list this user's installations")?;
    Ok(resp.installations.into_iter().find(|i| i.app_slug.as_deref() == Some(app_slug)).map(|i| i.id))
}

/// List repos a GitHub App installation was actually granted, for a `github_app`-connected
/// token. Unlike `list_accessible_repos` (which lists everything the token's account can see),
/// this reflects exactly what the installation picker on GitHub granted, so a repo the user
/// didn't select during install correctly doesn't show up here.
pub async fn list_accessible_repos_for_installation(client: &Octocrab, installation_id: i64) -> Result<Vec<AccessibleRepo>> {
    let mut repos = Vec::new();
    for page in 1..=3u8 {
        let route = format!("/user/installations/{installation_id}/repositories?per_page=100&page={page}");
        let response: InstallationRepositories = client
            .get(route, None::<&()>)
            .await
            .context("failed to list repos for this installation")?;

        let page_len = response.repositories.len();
        for repo in response.repositories {
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
