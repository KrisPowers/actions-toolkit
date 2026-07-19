use std::collections::{HashMap, HashSet};

use anyhow::{bail, Result};

use super::model::Workflow;

/// Validate a workflow: every `needs:` reference resolves to a real job, and the `needs`
/// graph is acyclic.
pub fn validate(workflow: &Workflow) -> Result<()> {
    if workflow.jobs.is_empty() {
        bail!("workflow must define at least one job");
    }

    let job_keys: HashSet<&str> = workflow.jobs.keys().map(String::as_str).collect();
    for (job_key, job) in &workflow.jobs {
        for need in &job.needs {
            if !job_keys.contains(need.as_str()) {
                bail!("job '{job_key}' needs unknown job '{need}'");
            }
        }
        if job.container.as_ref().is_some_and(|c| c.image.trim().is_empty()) {
            bail!("job '{job_key}' declares a container but its image is empty");
        }
        if job.steps.is_empty() {
            bail!("job '{job_key}' must define at least one step");
        }
        for step in &job.steps {
            if step.run.is_none() && step.uses.is_none() {
                bail!("job '{job_key}' has a step with neither 'run' nor 'uses'");
            }
        }
    }

    detect_cycle(workflow)?;
    Ok(())
}

fn detect_cycle(workflow: &Workflow) -> Result<()> {
    #[derive(Clone, Copy, PartialEq)]
    enum Mark {
        Visiting,
        Done,
    }

    let mut marks: HashMap<&str, Mark> = HashMap::new();

    fn visit<'a>(
        workflow: &'a Workflow,
        key: &'a str,
        marks: &mut HashMap<&'a str, Mark>,
    ) -> Result<()> {
        match marks.get(key) {
            Some(Mark::Done) => return Ok(()),
            Some(Mark::Visiting) => bail!("cycle detected in job dependencies at '{key}'"),
            None => {}
        }
        marks.insert(key, Mark::Visiting);
        if let Some(job) = workflow.jobs.get(key) {
            for need in &job.needs {
                visit(workflow, need.as_str(), marks)?;
            }
        }
        marks.insert(key, Mark::Done);
        Ok(())
    }

    for key in workflow.jobs.keys() {
        visit(workflow, key.as_str(), &mut marks)?;
    }
    Ok(())
}
