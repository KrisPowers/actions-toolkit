//! Probes the host for common toolchain install directories under the user's home, so the
//! Bucket/sandbox settings page can suggest them for the "Extra host paths" allowlist instead of
//! requiring the operator to already know the exact path (see `failure_diagnostics` for the other
//! half of this: diagnosing the failure when a step hits one that isn't allowlisted yet).
//! Detection only, this never touches the allowlist itself, the operator still approves each path
//! explicitly, the sandbox's read-only grants stay opt-in by design.

use std::path::Path;

use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostPathSuggestion {
    pub path: String,
    pub label: String,
}

/// Fixed candidates, relative to `home`, checked for existence as-is. `label` is what the
/// operator sees next to the suggested path, not the path itself.
const CANDIDATES: &[(&str, &str)] = &[
    (".cargo/bin", "Cargo (Rust)"),
    (".nvm", "nvm (Node.js)"),
    (".pyenv/shims", "pyenv (Python)"),
    ("go/bin", "Go"),
    (".local/bin", "user-installed tools (pipx, etc.)"),
    ("AppData/Roaming/npm", "npm (Node.js, global installs)"),
];

/// Every fixed candidate under `home` that exists, plus one entry per installed rustup toolchain
/// (`~/.rustup/toolchains/<name>/bin`, enumerated rather than guessed since the toolchain name
/// varies), minus whatever's already in `already_allowed`. Pure filesystem reads, no sandbox or
/// process involved, safe to call on every settings-page load.
pub fn detect(home: &Path, already_allowed: &[String]) -> Vec<HostPathSuggestion> {
    let mut found = Vec::new();

    for (relative, label) in CANDIDATES {
        let candidate = home.join(relative);
        if candidate.is_dir() {
            found.push(HostPathSuggestion { path: candidate.to_string_lossy().to_string(), label: label.to_string() });
        }
    }

    let toolchains_dir = home.join(".rustup").join("toolchains");
    if let Ok(entries) = std::fs::read_dir(&toolchains_dir) {
        for entry in entries.flatten() {
            let bin = entry.path().join("bin");
            if bin.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                found.push(HostPathSuggestion { path: bin.to_string_lossy().to_string(), label: format!("Rust toolchain ({name})") });
            }
        }
    }

    found.retain(|s| !already_allowed.iter().any(|allowed| paths_equal(allowed, &s.path)));
    found
}

/// Compares two path strings for equality the way the underlying filesystem would treat them
/// (backslash/forward-slash and trailing-separator insensitive), since a path an operator typed
/// by hand and one this module built from `Path::join` won't always be byte-identical even when
/// they name the same directory.
fn paths_equal(a: &str, b: &str) -> bool {
    fn normalize(p: &str) -> String {
        p.replace('\\', "/").trim_end_matches('/').to_ascii_lowercase()
    }
    normalize(a) == normalize(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_home() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("atk-host-toolchains-test-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn finds_only_the_candidates_that_actually_exist() {
        let home = fixture_home();
        std::fs::create_dir_all(home.join(".cargo/bin")).unwrap();
        // .nvm deliberately not created.

        let found = detect(&home, &[]);
        assert!(found.iter().any(|s| s.label == "Cargo (Rust)"));
        assert!(!found.iter().any(|s| s.label.contains("nvm")));
    }

    #[test]
    fn enumerates_every_installed_rustup_toolchain_by_name() {
        let home = fixture_home();
        std::fs::create_dir_all(home.join(".rustup/toolchains/stable-x86_64-pc-windows-msvc/bin")).unwrap();
        std::fs::create_dir_all(home.join(".rustup/toolchains/1.96.1-x86_64-pc-windows-msvc/bin")).unwrap();

        let found = detect(&home, &[]);
        assert!(found.iter().any(|s| s.label == "Rust toolchain (stable-x86_64-pc-windows-msvc)"));
        assert!(found.iter().any(|s| s.label == "Rust toolchain (1.96.1-x86_64-pc-windows-msvc)"));
    }

    /// Rule-proving test: a path the operator already allowlisted must not be suggested again,
    /// regardless of slash style or a trailing separator, both of which are cosmetic differences
    /// an operator's hand-typed path and this module's `Path::join`-built one can genuinely have.
    #[test]
    fn excludes_paths_already_allowlisted_regardless_of_slash_style() {
        let home = fixture_home();
        std::fs::create_dir_all(home.join(".cargo/bin")).unwrap();
        let already = vec![home.join(".cargo").join("bin").to_string_lossy().replace('/', "\\") + "\\"];

        let found = detect(&home, &already);
        assert!(!found.iter().any(|s| s.label == "Cargo (Rust)"));
    }
}
