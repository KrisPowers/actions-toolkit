use std::path::Path;

use anyhow::{Context, Result};
use git2::{Cred, FetchOptions, RemoteCallbacks, Repository};

/// Shallow-clone (depth 1) a repo at a specific ref into `workspace_dir`, authenticating with
/// the repo's PAT. `git_ref` may be a branch/tag ref (`refs/heads/main`) or a commit-ish.
pub fn checkout(owner: &str, repo: &str, pat: &str, git_ref: &str, workspace_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(workspace_dir)?;
    let url = format!("https://github.com/{owner}/{repo}.git");

    let mut callbacks = RemoteCallbacks::new();
    let pat_owned = pat.to_string();
    callbacks.credentials(move |_url, _username, _allowed| Cred::userpass_plaintext("x-access-token", &pat_owned));

    let mut fetch_options = FetchOptions::new();
    fetch_options.remote_callbacks(callbacks);
    fetch_options.depth(1);

    let repo_handle = Repository::init(workspace_dir).context("failed to init workspace repo")?;
    {
        let mut remote = repo_handle
            .remote_anonymous(&url)
            .context("failed to create anonymous remote")?;
        remote
            .fetch(&[git_ref], Some(&mut fetch_options), None)
            .with_context(|| format!("failed to fetch ref '{git_ref}'"))?;
    }

    let fetch_head = repo_handle.find_reference("FETCH_HEAD")?;
    let commit = fetch_head.peel_to_commit()?;
    repo_handle.checkout_tree(commit.as_object(), None)?;
    repo_handle.set_head_detached(commit.id())?;

    Ok(())
}
