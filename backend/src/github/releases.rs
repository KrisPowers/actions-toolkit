use anyhow::Result;
use octocrab::models::repos::Release;
use octocrab::Octocrab;

pub async fn list_releases(client: &Octocrab, owner: &str, repo: &str) -> Result<Vec<Release>> {
    let page = client.repos(owner, repo).releases().list().per_page(50).send().await?;
    Ok(page.items)
}

pub async fn get_release(client: &Octocrab, owner: &str, repo: &str, id: u64) -> Result<Release> {
    Ok(client.repos(owner, repo).releases().get(id).await?)
}

pub struct CreateReleaseParams<'a> {
    pub tag_name: &'a str,
    pub name: Option<&'a str>,
    pub body: Option<&'a str>,
    pub draft: bool,
    pub prerelease: bool,
}

pub async fn create_release(client: &Octocrab, owner: &str, repo: &str, params: CreateReleaseParams<'_>) -> Result<Release> {
    let repo_handler = client.repos(owner, repo);
    let releases_handler = repo_handler.releases();
    let mut builder = releases_handler
        .create(params.tag_name)
        .draft(params.draft)
        .prerelease(params.prerelease);
    if let Some(name) = params.name {
        builder = builder.name(name);
    }
    if let Some(body) = params.body {
        builder = builder.body(body);
    }
    Ok(builder.send().await?)
}

pub async fn update_release(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    id: u64,
    name: Option<&str>,
    body: Option<&str>,
    draft: Option<bool>,
    prerelease: Option<bool>,
) -> Result<Release> {
    let repo_handler = client.repos(owner, repo);
    let releases_handler = repo_handler.releases();
    let mut builder = releases_handler.update(id);
    if let Some(name) = name {
        builder = builder.name(name);
    }
    if let Some(body) = body {
        builder = builder.body(body);
    }
    if let Some(draft) = draft {
        builder = builder.draft(draft);
    }
    if let Some(prerelease) = prerelease {
        builder = builder.prerelease(prerelease);
    }
    Ok(builder.send().await?)
}
