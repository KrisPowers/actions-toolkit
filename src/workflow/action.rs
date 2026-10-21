//! Resolving `uses:` references to runnable actions.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::model::ActionDef;
use crate::util;
