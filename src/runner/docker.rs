use std::collections::HashMap;
use std::path::Path;

use anyhow::{anyhow, Context, Result};
use bollard::container::{
    Config, CreateContainerOptions, LogsOptions, RemoveContainerOptions, StartContainerOptions,
    WaitContainerOptions,
};
use bollard::exec::{CreateExecOptions, StartExecResults};
use bollard::image::CreateImageOptions;
use bollard::secret::HostConfig;
use bollard::Docker;
use futures_util::StreamExt;

pub const RUN_LABEL: &str = "actions-toolkit.run_id";
pub const JOB_LABEL: &str = "actions-toolkit.job_run_id";

/// Connect to the local Docker Engine (unix socket on Linux/macOS, named pipe on Windows).
/// `override_host`, if set, is passed through as `DOCKER_HOST` before connecting.
pub fn connect(override_host: Option<&str>) -> Result<Docker> {
    if let Some(host) = override_host {
        return Docker::connect_with_http(host, 120, bollard::API_DEFAULT_VERSION)
            .context("failed to connect to Docker via configured DOCKER_HOST");
    }
    Docker::connect_with_local_defaults().context(
        "failed to connect to local Docker Engine; is Docker running? set DOCKER_HOST to override",
    )
}

pub async fn ping(docker: &Docker) -> Result<()> {
    docker.ping().await.context("Docker ping failed")?;
    Ok(())
}

pub async fn pull_image(docker: &Docker, image: &str) -> Result<()> {
    let mut stream = docker.create_image(
        Some(CreateImageOptions {
            from_image: image,
            ..Default::default()
        }),
        None,
        None,
    );
    while let Some(item) = stream.next().await {
        item.with_context(|| format!("failed to pull image '{image}'"))?;
    }
    Ok(())
}

/// Create and start a long-lived job container with the workspace bind-mounted at
/// `/workspace`, kept alive so multiple steps can `exec` into it sequentially (mirrors GHA's
/// one-container-per-job execution model).
pub async fn create_job_container(
    docker: &Docker,
    image: &str,
    workspace_host_path: &Path,
    run_id: &str,
    job_run_id: &str,
    env: &[String],
) -> Result<String> {
    let mut labels = HashMap::new();
    labels.insert(RUN_LABEL.to_string(), run_id.to_string());
    labels.insert(JOB_LABEL.to_string(), job_run_id.to_string());

    let bind = format!("{}:/workspace", workspace_host_path.display());

    let config = Config {
        image: Some(image.to_string()),
        working_dir: Some("/workspace".to_string()),
        entrypoint: Some(vec!["/bin/sh".to_string(), "-c".to_string(), "tail -f /dev/null".to_string()]),
        env: Some(env.to_vec()),
        labels: Some(labels),
        host_config: Some(HostConfig {
            binds: Some(vec![bind]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let container = docker
        .create_container(
            Some(CreateContainerOptions {
                name: format!("actions-toolkit-{job_run_id}"),
                platform: None,
            }),
            config,
        )
        .await
        .context("failed to create job container")?;

    docker
        .start_container(&container.id, None::<StartContainerOptions<String>>)
        .await
        .context("failed to start job container")?;

    Ok(container.id)
}

pub struct ExecResult {
    pub exit_code: i64,
}

/// Resolves a step's `shell:` override into the `Cmd` array docker `exec` expects, defaulting to
/// `sh` (present in virtually any Linux image, unlike `bash`) when unset. Recognizes the same
/// shell keywords GitHub Actions does; anything else is passed through as a literal program name
/// invoked with `-c`, best-effort.
fn shell_cmd(shell: Option<&str>, shell_command: &str) -> Vec<String> {
    let (program, arg) = match shell.map(str::to_ascii_lowercase).as_deref() {
        None | Some("sh") => ("/bin/sh", "-c"),
        Some("bash") => ("/bin/bash", "-c"),
        Some("pwsh") => ("pwsh", "-Command"),
        Some("powershell") => ("powershell", "-Command"),
        Some(other) => return vec![other.to_string(), "-c".to_string(), shell_command.to_string()],
    };
    vec![program.to_string(), arg.to_string(), shell_command.to_string()]
}

/// Run a shell command inside an already-running container, streaming each output line to
/// `on_line(stream, message)` as it arrives.
pub async fn exec_step<F>(
    docker: &Docker,
    container_id: &str,
    shell_command: &str,
    shell: Option<&str>,
    working_dir: Option<&str>,
    env: &[String],
    mut on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    let exec = docker
        .create_exec(
            container_id,
            CreateExecOptions {
                cmd: Some(shell_cmd(shell, shell_command)),
                attach_stdout: Some(true),
                attach_stderr: Some(true),
                working_dir: working_dir.map(|s| s.to_string()),
                env: Some(env.to_vec()),
                ..Default::default()
            },
        )
        .await
        .context("failed to create exec")?;

    let start = docker
        .start_exec(&exec.id, None)
        .await
        .context("failed to start exec")?;

    if let StartExecResults::Attached { mut output, .. } = start {
        while let Some(chunk) = output.next().await {
            let chunk = chunk.context("error reading exec output stream")?;
            let (stream, message) = match chunk {
                bollard::container::LogOutput::StdOut { message } => ("stdout", message),
                bollard::container::LogOutput::StdErr { message } => ("stderr", message),
                bollard::container::LogOutput::StdIn { message } => ("stdin", message),
                bollard::container::LogOutput::Console { message } => ("stdout", message),
            };
            let text = String::from_utf8_lossy(&message).to_string();
            for line in text.lines() {
                on_line(stream, line.to_string());
            }
        }
    }

    let inspect = docker
        .inspect_exec(&exec.id)
        .await
        .context("failed to inspect exec result")?;
    let exit_code = inspect.exit_code.unwrap_or(-1);

    Ok(ExecResult { exit_code })
}

/// Run a one-off `uses: docker://image` container action: its own short-lived container,
/// same workspace mount, streamed the same way, removed after it finishes.
pub async fn run_container_action<F>(
    docker: &Docker,
    image: &str,
    workspace_host_path: &Path,
    run_id: &str,
    job_run_id: &str,
    env: &[String],
    mut on_line: F,
) -> Result<ExecResult>
where
    F: FnMut(&str, String) + Send,
{
    pull_image(docker, image).await.ok(); // best-effort; create_container will surface a clearer error if truly missing

    let mut labels = HashMap::new();
    labels.insert(RUN_LABEL.to_string(), run_id.to_string());
    labels.insert(JOB_LABEL.to_string(), job_run_id.to_string());
    let bind = format!("{}:/workspace", workspace_host_path.display());

    let config = Config {
        image: Some(image.to_string()),
        working_dir: Some("/workspace".to_string()),
        env: Some(env.to_vec()),
        labels: Some(labels),
        host_config: Some(HostConfig {
            binds: Some(vec![bind]),
            ..Default::default()
        }),
        ..Default::default()
    };

    let container = docker.create_container(None::<CreateContainerOptions<String>>, config).await?;
    docker
        .start_container(&container.id, None::<StartContainerOptions<String>>)
        .await?;

    let mut logs = docker.logs(
        &container.id,
        Some(LogsOptions::<String> {
            follow: true,
            stdout: true,
            stderr: true,
            ..Default::default()
        }),
    );
    while let Some(chunk) = logs.next().await {
        let chunk = chunk.context("error reading container logs")?;
        let (stream, message) = match chunk {
            bollard::container::LogOutput::StdOut { message } => ("stdout", message),
            bollard::container::LogOutput::StdErr { message } => ("stderr", message),
            _ => continue,
        };
        let text = String::from_utf8_lossy(&message).to_string();
        for line in text.lines() {
            on_line(stream, line.to_string());
        }
    }

    let mut wait_stream = docker.wait_container(&container.id, None::<WaitContainerOptions<String>>);
    let exit_code = match wait_stream.next().await {
        Some(Ok(resp)) => resp.status_code,
        Some(Err(e)) => return Err(anyhow!("container wait failed: {e}")),
        None => -1,
    };

    remove_container(docker, &container.id).await?;

    Ok(ExecResult { exit_code })
}

pub async fn remove_container(docker: &Docker, container_id: &str) -> Result<()> {
    docker
        .remove_container(
            container_id,
            Some(RemoveContainerOptions {
                force: true,
                ..Default::default()
            }),
        )
        .await
        .context("failed to remove container")?;
    Ok(())
}

/// Copy a path out of a container into a host destination path (used for artifact capture),
/// via bollard's tar-stream download.
pub async fn download_path(docker: &Docker, container_id: &str, container_path: &str, dest_dir: &Path) -> Result<()> {
    use bollard::container::DownloadFromContainerOptions;

    std::fs::create_dir_all(dest_dir)?;
    let mut stream = docker.download_from_container(
        container_id,
        Some(DownloadFromContainerOptions {
            path: container_path.to_string(),
        }),
    );

    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next().await {
        bytes.extend_from_slice(&chunk.context("failed reading artifact tar stream")?);
    }

    let mut archive = tar::Archive::new(bytes.as_slice());
    archive.unpack(dest_dir).context("failed to unpack artifact tar")?;
    Ok(())
}

/// Startup reconciliation: find containers labeled with a run id whose workflow_run is not
/// in a terminal state and remove them, so a crash mid-run doesn't leak containers forever.
pub async fn list_labeled_containers(docker: &Docker, run_id: &str) -> Result<Vec<String>> {
    use bollard::container::ListContainersOptions;

    let mut filters = HashMap::new();
    filters.insert("label".to_string(), vec![format!("{RUN_LABEL}={run_id}")]);

    let containers = docker
        .list_containers(Some(ListContainersOptions {
            all: true,
            filters,
            ..Default::default()
        }))
        .await?;

    Ok(containers.into_iter().filter_map(|c| c.id).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_cmd_defaults_to_sh_when_unset() {
        assert_eq!(shell_cmd(None, "echo hi"), vec!["/bin/sh", "-c", "echo hi"]);
    }

    #[test]
    fn shell_cmd_honors_recognized_overrides() {
        assert_eq!(shell_cmd(Some("bash"), "echo hi"), vec!["/bin/bash", "-c", "echo hi"]);
        assert_eq!(shell_cmd(Some("BASH"), "echo hi"), vec!["/bin/bash", "-c", "echo hi"]);
        assert_eq!(shell_cmd(Some("pwsh"), "Write-Host hi"), vec!["pwsh", "-Command", "Write-Host hi"]);
        assert_eq!(shell_cmd(Some("powershell"), "Write-Host hi"), vec!["powershell", "-Command", "Write-Host hi"]);
    }

    #[test]
    fn shell_cmd_passes_through_unrecognized_shells() {
        assert_eq!(shell_cmd(Some("python3"), "print('hi')"), vec!["python3", "-c", "print('hi')"]);
    }
}
