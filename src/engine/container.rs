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
