// sqlx::migrate! embeds a checksum of each migration file's exact bytes at compile time.
// A CRLF checkout (e.g. Windows with core.autocrlf=true) changes those bytes, producing a
// checksum that no longer matches one already recorded in a database, which breaks startup
// with "migration N was previously applied but has been modified". .gitattributes is supposed
// to force LF for these files, but that rule has silently stopped matching before (when the
// migrations directory moved) without failing the build. Fail loudly here instead.
use std::path::Path;

fn main() {
    let dir = Path::new("migrations");
    println!("cargo:rerun-if-changed={}", dir.display());

    let mut offenders = Vec::new();
    for entry in std::fs::read_dir(dir).expect("failed to read migrations directory") {
        let path = entry.expect("failed to read migrations dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("sql") {
            continue;
        }
        let bytes = std::fs::read(&path).expect("failed to read migration file");
        if bytes.contains(&b'\r') {
            offenders.push(path.display().to_string());
        }
    }

    if !offenders.is_empty() {
        panic!(
            "CRLF line endings found in migration file(s), which will change sqlx's compiled-in \
             checksum and can break startup for any existing database (\"migration N was \
             previously applied but has been modified\"):\n  {}\n\
             Fix your checkout (git add --renormalize crates/atk-db/migrations && git checkout \
             HEAD -- crates/atk-db/migrations) and confirm .gitattributes' `**/migrations/*.sql \
             text eol=lf` rule actually matches this path.",
            offenders.join("\n  ")
        );
    }
}
