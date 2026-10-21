//! The built-in engine: runs steps directly on the host inside a throwaway sandbox directory
//! with a scrubbed environment. Zero dependencies and the fastest startup, but it isolates
//! files and environment variables, not the kernel; use a container or VM for untrusted code.

use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

use anyhow::Result;
use tempfile::TempDir;

use super::{
    Availability, Engine, EngineInfo, ExecRequest, Kind, Session, SessionInfo, SessionRequest,
};
use crate::util::{self, Cancel, ExecOutcome};
