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
