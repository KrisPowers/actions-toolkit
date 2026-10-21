//! User (`<config dir>/maverick/config.toml`) and project (`.maverick.toml`) configuration.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
