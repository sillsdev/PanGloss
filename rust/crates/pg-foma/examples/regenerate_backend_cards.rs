use std::fs;
use std::path::PathBuf;

use pg_foma::backend_cards::{catalog, checked_in_relative_path, render_markdown};

fn main() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let repo_root = manifest_dir
        .parent()
        .and_then(|path| path.parent())
        .and_then(|path| path.parent())
        .expect("pg-foma must remain under rust/crates/pg-foma");
    for card in catalog() {
        let path = repo_root.join(checked_in_relative_path(card.backend_id));
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("create backend-card directory");
        }
        fs::write(&path, render_markdown(card)).expect("write deterministic backend card");
        println!("wrote {}", path.display());
    }
}
