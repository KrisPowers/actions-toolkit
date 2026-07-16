use std::path::Path;

/// `rust-embed` scans `ui/dist` at compile time and fails outright if the folder doesn't
/// exist. That folder is gitignored build output, so a fresh clone (or CI, before the UI has
/// been built) would otherwise fail `cargo build` immediately. Ensure it exists with at least
/// a placeholder so the backend can always be built on its own; running `npm run build` in
/// ui/ afterward replaces the placeholder with the real UI.
fn main() {
    let dist = Path::new("ui/dist");
    if !dist.exists() {
        std::fs::create_dir_all(dist).expect("failed to create ui/dist placeholder");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>actions-toolkit</title><p>Run npm run build in ui/ to produce the real UI.</p>",
        )
        .expect("failed to write ui/dist placeholder index.html");
    }
    println!("cargo:rerun-if-changed=ui/dist");
}
