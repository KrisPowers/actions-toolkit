use std::path::Path;

/// `rust-embed` scans `../frontend/dist` at compile time and fails outright if the folder
/// doesn't exist. That folder is gitignored build output, so a fresh clone (or CI, before the
/// frontend has been built) would otherwise fail `cargo build` immediately. Ensure it exists
/// with at least a placeholder so the backend can always be built on its own; running
/// `npm run build` in frontend/ afterward replaces the placeholder with the real UI.
fn main() {
    let dist = Path::new("../frontend/dist");
    if !dist.exists() {
        std::fs::create_dir_all(dist).expect("failed to create frontend/dist placeholder");
        std::fs::write(
            dist.join("index.html"),
            "<!doctype html><title>actions-toolkit</title><p>Run npm run build in frontend/ to produce the real UI.</p>",
        )
        .expect("failed to write frontend/dist placeholder index.html");
    }
    println!("cargo:rerun-if-changed=../frontend/dist");
}
