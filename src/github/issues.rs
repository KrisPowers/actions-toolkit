use anyhow::Result;
use octocrab::models::issues::{Comment, Issue};
use octocrab::models::IssueState;
use octocrab::params;
use octocrab::Octocrab;

pub async fn list_issues(client: &Octocrab, owner: &str, repo: &str, state: &str) -> Result<Vec<Issue>> {
    let state_param = match state {
        "closed" => params::State::Closed,
        "all" => params::State::All,
        _ => params::State::Open,
    };
    let page = client
        .issues(owner, repo)
        .list()
        .state(state_param)
        .per_page(50)
        .send()
        .await?;
    Ok(page.items)
}

pub async fn get_issue(client: &Octocrab, owner: &str, repo: &str, number: u64) -> Result<Issue> {
    Ok(client.issues(owner, repo).get(number).await?)
}

pub async fn add_comment(client: &Octocrab, owner: &str, repo: &str, number: u64, body: &str) -> Result<Comment> {
    Ok(client.issues(owner, repo).create_comment(number, body).await?)
}

pub async fn close_issue(client: &Octocrab, owner: &str, repo: &str, number: u64) -> Result<Issue> {
    Ok(client
        .issues(owner, repo)
        .update(number)
        .state(IssueState::Closed)
        .send()
        .await?)
}

pub async fn reopen_issue(client: &Octocrab, owner: &str, repo: &str, number: u64) -> Result<Issue> {
    Ok(client
        .issues(owner, repo)
        .update(number)
        .state(IssueState::Open)
        .send()
        .await?)
}

pub async fn add_labels(client: &Octocrab, owner: &str, repo: &str, number: u64, labels: &[String]) -> Result<()> {
    client.issues(owner, repo).add_labels(number, labels).await?;
    Ok(())
}

pub async fn remove_label(client: &Octocrab, owner: &str, repo: &str, number: u64, label: &str) -> Result<()> {
    client.issues(owner, repo).remove_label(number, label).await?;
    Ok(())
}

pub async fn list_pull_requests(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    state: &str,
) -> Result<Vec<octocrab::models::pulls::PullRequest>> {
    let state_param = match state {
        "closed" => params::State::Closed,
        "all" => params::State::All,
        _ => params::State::Open,
    };
    let page = client
        .pulls(owner, repo)
        .list()
        .state(state_param)
        .per_page(50)
        .send()
        .await?;
    Ok(page.items)
}

pub async fn get_pull_request(
    client: &Octocrab,
    owner: &str,
    repo: &str,
    number: u64,
) -> Result<octocrab::models::pulls::PullRequest> {
    Ok(client.pulls(owner, repo).get(number).await?)
}
