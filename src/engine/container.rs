//! Docker / Podman: one long-lived container per job, steps run via `exec`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use tempfile::TempDir;

use super::{
    apply_probe, find_binary, probe_unix, version_of, Availability, Engine, EngineInfo,
    ExecRequest, Kind, Session, SessionInfo, SessionRequest,
};
use crate::util::{self, Cancel, ExecOutcome};

const WORKSPACE: &str = "/github/workspace";
const RUNNER: &str = "/github/runner";

fn locate(kind: Kind) -> Option<PathBuf> {
    match kind {
        Kind::Docker => find_binary(
            &["docker"],
            &[
                "/usr/local/bin/docker",
                "/opt/homebrew/bin/docker",
                "~/.docker/bin/docker",
                "~/.orbstack/bin/docker",
                "~/.rd/bin/docker",
                "/Applications/Docker.app/Contents/Resources/bin/docker",
                r"C:\Program Files\Docker\Docker\resources\bin\docker.exe",
            ],
        ),
        _ => find_binary(
            &["podman"],
            &[
                "/opt/podman/bin/podman",
                "/opt/homebrew/bin/podman",
                r"C:\Program Files\RedHat\Podman\podman.exe",
            ],
        ),
    }
}

pub(super) fn detect(kind: Kind) -> EngineInfo {
    let Some(bin) = locate(kind) else {
        return EngineInfo::missing(kind);
    };
    let version = version_of(&bin, &["--version"]);
    // `info` talks to the daemon / VM, so it tells us whether containers can actually run.
    let alive = util::probe(&bin, &["info", "--format", "{{json .}}"]).is_some_and(|o| o.success);
    if alive {
        EngineInfo::new(kind, Availability::Ready, version, None)
    } else {
        let hint = match kind {
            Kind::Docker => "start Docker Desktop (or the docker daemon)",
            _ => "run `podman machine start`",
        };
        EngineInfo::new(kind, Availability::NotRunning, version, hint.to_string())
    }
}

pub struct ContainerEngine {
    kind: Kind,
    binary: PathBuf,
}
