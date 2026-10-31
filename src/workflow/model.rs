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

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Strategy {
    pub matrix: Option<Yaml>,
    pub fail_fast: Option<Yaml>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum Container {
    Image(String),
    Spec(ContainerSpec),
}

#[derive(Debug, Clone, Default, Deserialize)]
pub struct ContainerSpec {
    pub image: String,
    #[serde(default)]
    pub env: IndexMap<String, Yaml>,
    pub options: Option<String>,
    #[serde(default)]
    pub volumes: Vec<String>,
}

impl Container {
    pub fn spec(&self) -> ContainerSpec {
        match self {
            Container::Image(image) => ContainerSpec {
                image: image.clone(),
                ..Default::default()
            },
            Container::Spec(spec) => spec.clone(),
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub struct Step {
    pub id: Option<String>,
    pub name: Option<String>,
    pub uses: Option<String>,
    pub run: Option<String>,
    pub shell: Option<String>,
    #[serde(default)]
    pub with: IndexMap<String, Yaml>,
    #[serde(default)]
    pub env: IndexMap<String, Yaml>,
    #[serde(rename = "if")]
    pub condition: Option<Yaml>,
    pub working_directory: Option<String>,
    pub continue_on_error: Option<Yaml>,
    pub timeout_minutes: Option<Yaml>,
}

/// `action.yml`
#[derive(Debug, Clone, Deserialize)]
pub struct ActionDef {
    #[serde(default)]
    pub inputs: IndexMap<String, Yaml>,
    #[serde(default)]
    pub outputs: IndexMap<String, Yaml>,
    pub runs: ActionRuns,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ActionRuns {
    pub using: String,
    pub main: Option<String>,
    #[serde(default)]
    pub steps: Vec<Step>,
    /// Docker actions: a Dockerfile in the action, or `docker://image`.
    pub image: Option<String>,
    pub entrypoint: Option<String>,
    #[serde(rename = "pre-entrypoint")]
    pub pre_entrypoint: Option<String>,
    #[serde(default)]
    pub args: Vec<Yaml>,
    #[serde(default)]
    pub env: IndexMap<String, Yaml>,
}

impl Workflow {
    /// Event names this workflow triggers on.
    pub fn events(&self) -> Vec<String> {
        match &self.on {
            Some(Yaml::String(s)) => vec![s.clone()],
            Some(Yaml::Sequence(seq)) => seq
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            Some(Yaml::Mapping(map)) => map
                .keys()
                .filter_map(|k| k.as_str().map(String::from))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Default values of `on.workflow_dispatch.inputs`.
    pub fn dispatch_inputs(&self) -> IndexMap<String, String> {
        let mut out = IndexMap::new();
        let inputs = self
            .on
            .as_ref()
            .and_then(|on| on.get("workflow_dispatch"))
            .and_then(|wd| wd.get("inputs"))
            .and_then(|i| i.as_mapping());
        for (k, v) in inputs.into_iter().flatten() {
            if let Some(name) = k.as_str() {
                let default = v.get("default").map(scalar).unwrap_or_default();
                out.insert(name.to_string(), default);
            }
        }
        out
    }

    pub fn run_defaults(&self) -> Option<&RunDefaults> {
        self.defaults.as_ref().and_then(|d| d.run.as_ref())
    }
}

impl Job {
    pub fn needs(&self) -> Vec<String> {
        match &self.needs {
            Some(Yaml::String(s)) => vec![s.clone()],
            Some(Yaml::Sequence(seq)) => seq
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect(),
            _ => Vec::new(),
        }
    }

    /// Raw `runs-on` labels (may still contain expressions).
    pub fn runs_on(&self) -> Vec<String> {
        match &self.runs_on {
            Some(Yaml::String(s)) => vec![s.clone()],
            Some(Yaml::Sequence(seq)) => seq.iter().map(scalar).collect(),
            Some(m @ Yaml::Mapping(_)) => match m.get("labels") {
                Some(Yaml::String(s)) => vec![s.clone()],
                Some(Yaml::Sequence(seq)) => seq.iter().map(scalar).collect(),
                _ => Vec::new(),
            },
            _ => Vec::new(),
        }
    }

    pub fn run_defaults(&self) -> Option<&RunDefaults> {
        self.defaults.as_ref().and_then(|d| d.run.as_ref())
    }
}

/// Renders a YAML scalar the way GitHub stringifies it.
pub fn scalar(v: &Yaml) -> String {
    match v {
        Yaml::Null => String::new(),
        Yaml::Bool(b) => b.to_string(),
        Yaml::Number(n) => n.to_string(),
        Yaml::String(s) => s.clone(),
        other => serde_yaml::to_string(other)
            .unwrap_or_default()
            .trim_end()
            .to_string(),
    }
}
