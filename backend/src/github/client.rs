use anyhow::Result;
use octocrab::Octocrab;

use crate::app::AppState;
use crate::db::models::Repo;

/// Build (or fetch a cached) octocrab client authenticated with the given repo's decrypted PAT.
/// Clients are cached per repo id and invalidated whenever the stored PAT is updated.
pub async fn for_repo(state: &AppState, repo: &Repo) -> Result<Octocrab> {
    if let Some(client) = state.github_clients.get(&repo.id) {
        return Ok(client.clone());
    }

    let pat = state.enc.decrypt_str(&repo.pat_encrypted, &repo.pat_nonce)?;
    let client = Octocrab::builder().personal_token(pat).build()?;
    state.github_clients.insert(repo.id.clone(), client.clone());
    Ok(client)
}

pub fn invalidate(state: &AppState, repo_id: &str) {
    state.github_clients.remove(repo_id);
}
