//! Turns a failed `run:` step's raw output into a specific, actionable diagnosis when the failure
//! matches a known "command not found" shape (PowerShell, cmd.exe, or a POSIX shell). Every one
//! of these has the same root cause on this codebase's sandbox: the Shard's default read-only
//! slice only covers base OS directories, so a toolchain installed under the user profile isn't
//! visible until its path is added under Settings -> Bucket / sandbox -> Extra host paths. Turning
//! that into a specific instruction, instead of leaving a cryptic shell error as the only clue,
//! is the whole point of this module -- see issue #129's follow-up discussion.

/// Scans a failed step's combined stderr for a known "command not found" pattern and, if found,
/// returns a plain-language hint naming the specific command. `None` means the failure didn't
/// match a recognized shape (most failures: an actual build/test failure, not a missing command)
/// -- callers should treat `None` as "no hint to add," not "nothing went wrong."
pub fn diagnose(output: &str) -> Option<String> {
    let command = extract_missing_command(output)?;
    Some(format!(
        "'{command}' isn't visible inside the sandbox. If it's installed under your user profile \
         (like cargo, nvm, or pyenv), add its install directory under Settings \u{2192} Bucket / \
         sandbox \u{2192} Extra host paths, then re-run this step."
    ))
}

fn extract_missing_command(output: &str) -> Option<String> {
    for line in output.lines() {
        if let Some(cmd) = extract_powershell_not_recognized(line) {
            return Some(cmd);
        }
        if let Some(cmd) = extract_cmd_not_recognized(line) {
            return Some(cmd);
        }
        if let Some(cmd) = extract_posix_not_found(line) {
            return Some(cmd);
        }
    }
    None
}

/// PowerShell: `The term 'cargo' is not recognized as the name of a cmdlet, function, script
/// file, or operable program.` Matches equally well inside the CLIXML wrapper a non-interactive
/// host serializes its error stream as (`<S S="Error">...</S>`), since that only escapes newlines
/// (as `_x000D__x000A_`) and wraps the same text, it doesn't alter it.
fn extract_powershell_not_recognized(line: &str) -> Option<String> {
    let start = line.find("The term '")? + "The term '".len();
    let rest = &line[start..];
    let end = rest.find('\'')?;
    let command = &rest[..end];
    rest[end..].starts_with("' is not recognized as the name of a cmdlet").then(|| command.to_string())
}

/// cmd.exe: `'cargo' is not recognized as an internal or external command, operable program or
/// batch file.`
fn extract_cmd_not_recognized(line: &str) -> Option<String> {
    let start = line.find('\'')? + 1;
    let rest = &line[start..];
    let end = rest.find('\'')?;
    let command = &rest[..end];
    rest[end..].starts_with("' is not recognized as an internal or external command").then(|| command.to_string())
}

/// POSIX shells: bash/zsh's `cargo: command not found`, and dash/sh/busybox's `sh: 1: cargo: not
/// found` (an optional leading `shellname: linenumber:` prefix, then the command, then the
/// suffix). Rejects a match whose extracted token contains whitespace, a sign a line matched the
/// suffix by coincidence rather than actually naming a single missing command.
fn extract_posix_not_found(line: &str) -> Option<String> {
    let line = line.trim();
    let without_suffix = line.strip_suffix(": command not found").or_else(|| line.strip_suffix(": not found"))?;
    let command = without_suffix.rsplit(':').next()?.trim();
    (!command.is_empty() && !command.contains(char::is_whitespace)).then(|| command.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rule-proving test: this exact text (CLIXML wrapper included) is what a real Windows Shard
    /// step produces when a step invokes an unallowlisted user-profile toolchain -- reproduced
    /// verbatim from a real failed run, not a simplified stand-in.
    #[test]
    fn diagnoses_the_real_windows_clixml_error() {
        let output = r#"#< CLIXML
<Objs Version="1.1.0.1" xmlns="http://schemas.microsoft.com/powershell/2004/04"><S S="Error">cargo : The term 'cargo' is not recognized as the name of a cmdlet, function, script file, or operable program. Check _x000D__x000A_</S><S S="Error">the spelling of the name, or if a path was included, verify that the path is correct and try again._x000D__x000A_</S></Objs>"#;
        let hint = diagnose(output).expect("should recognize the PowerShell not-recognized pattern");
        assert!(hint.contains("'cargo'"), "hint should name the missing command: {hint}");
        assert!(hint.contains("Extra host paths"), "hint should point at the fix: {hint}");
    }

    #[test]
    fn diagnoses_cmd_exe_not_recognized() {
        let output = "'cargo' is not recognized as an internal or external command,\r\noperable program or batch file.";
        let hint = diagnose(output).expect("should recognize the cmd.exe not-recognized pattern");
        assert!(hint.contains("'cargo'"));
    }

    #[test]
    fn diagnoses_bash_command_not_found() {
        assert!(diagnose("cargo: command not found").unwrap().contains("'cargo'"));
    }

    #[test]
    fn diagnoses_dash_style_not_found_with_line_number_prefix() {
        assert!(diagnose("sh: 1: cargo: not found").unwrap().contains("'cargo'"));
    }

    /// Rule-proving test: an ordinary build/test failure (the overwhelming majority of failed
    /// steps) must not get a misleading "isn't visible inside the sandbox" hint slapped onto it.
    #[test]
    fn does_not_diagnose_an_unrelated_test_failure() {
        let output = "test result: FAILED. 3 passed; 1 failed; 0 ignored\n\
                       thread 'it_works' panicked at src/lib.rs:42:5:\n\
                       assertion `left == right` failed";
        assert!(diagnose(output).is_none());
    }

    /// Rule-proving test: a generic "file not found" message (a real, common failure shape that
    /// happens to share the words "not found") must not falsely match the POSIX command-missing
    /// pattern -- there's no colon-then-bare-token immediately before it the way a shell's own
    /// "command not found" report has.
    #[test]
    fn does_not_diagnose_a_generic_file_not_found_message() {
        assert!(diagnose("Error: config file example.toml not found").is_none());
    }
}
