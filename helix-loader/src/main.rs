use anyhow::Result;
use helix_loader::grammar::fetch_grammars;

// This binary is used in the Release CI as an optimization to cut down on
// compilation time. This is not meant to be run manually.

const STRICT: bool = true;

fn main() -> Result<()> {
    // Fetch into the workspace runtime directory so the grammar sources can be
    // packaged alongside the runtime (unlike `fx --grammar fetch`, which
    // installs into the user config directory).
    let install_dir = helix_loader::runtime_dirs()
        .first()
        .expect("No runtime directories provided") // guaranteed by post-condition
        .clone();
    fetch_grammars(STRICT, &install_dir)
}
