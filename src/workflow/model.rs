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

#[derive(Debug, Clone, Default, Deserialize)]
pub struct Defaults {
    pub run: Option<RunDefaults>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct RunDefaults {
    pub shell: Option<String>,
    pub working_directory: Option<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Job {
    pub name: Option<String>,
    pub runs_on: Option<Yaml>,
    pub needs: Option<Yaml>,
    #[serde(rename = "if")]
    pub condition: Option<Yaml>,
    #[serde(default)]
    pub env: IndexMap<String, Yaml>,
    pub defaults: Option<Defaults>,
    pub strategy: Option<Strategy>,
    pub container: Option<Container>,
    pub services: Option<Yaml>,
    #[serde(default)]
    pub steps: Vec<Step>,
    pub timeout_minutes: Option<Yaml>,
    #[serde(default)]
    pub outputs: IndexMap<String, String>,
    /// Reusable workflow call.
    pub uses: Option<String>,
}
