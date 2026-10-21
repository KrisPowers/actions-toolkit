//! Virtual machines. Each hypervisor is reached through a "transport" that can run a command in
//! the guest with stdin attached: native CLIs where they exist (Lima, Multipass, WSL, Parallels)
//! and SSH for everything else (VirtualBox, VMware, QEMU, Hyper-V, Tart).
//!
//! Commands passed through a transport are always plain words (paths under a slug-only
//! directory), because several transports re-join argv with spaces. Scripts, env vars and the
//! working directory travel inside a generated wrapper script uploaded via stdin.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::{
    apply_probe, find_binary, probe_unix, version_of, Availability, Engine, EngineInfo,
    ExecRequest, Kind, Session, SessionInfo, SessionRequest,
};
use crate::config::{Config, EngineConfig};
use crate::util::{self, shell_quote, Cancel, ExecOutcome};
