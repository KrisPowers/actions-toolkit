//! Serde model of the GitHub Actions workflow and action syntax.

use indexmap::IndexMap;
use serde::Deserialize;
use serde_yaml::Value as Yaml;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Workflow {
    pub name: Option<String>,
    pub on: Option<Yaml>,
    #[serde(default)]
    pub env: IndexMap<String, Yaml>,
    pub defaults: Option<Defaults>,
    #[serde(default)]
    pub jobs: IndexMap<String, Job>,
}
