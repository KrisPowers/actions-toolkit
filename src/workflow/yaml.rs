use anyhow::{Context, Result};

use super::model::Workflow;

pub fn parse(yaml_source: &str) -> Result<Workflow> {
    serde_yaml_ng::from_str(yaml_source).context("failed to parse workflow YAML")
}

pub fn to_yaml(workflow: &Workflow) -> Result<String> {
    serde_yaml_ng::to_string(workflow).context("failed to serialize workflow to YAML")
}

pub fn to_json(workflow: &Workflow) -> Result<String> {
    Ok(serde_json::to_string(workflow)?)
}
