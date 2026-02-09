use std::{env, path::PathBuf};

use cherry_services::{AppServicesBuilder, import_from_legacy_dir};

fn main() -> anyhow::Result<()> {
    let mut args = env::args().skip(1);
    let Some(legacy_dir) = args.next() else {
        eprintln!(
            "Usage: cargo run -p cherry-app --bin migrate_legacy_dir -- <legacy-dir> [db-path]"
        );
        std::process::exit(2);
    };

    let db_path = args
        .next()
        .map(PathBuf::from)
        .unwrap_or_else(default_db_path);

    let services = AppServicesBuilder::new(db_path).build()?;
    let report = import_from_legacy_dir(&services, legacy_dir)?;

    println!("Legacy directory import completed:");
    println!("  providers: {}", report.providers);
    println!("  conversations: {}", report.conversations);
    println!("  messages: {}", report.messages);
    println!("  notes: {}", report.notes);
    println!("  files: {}", report.files);
    println!("  knowledge_documents: {}", report.knowledge_documents);

    Ok(())
}

fn default_db_path() -> PathBuf {
    let mut base = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    base.push("data");
    base.push("cherry_studio_rs.sqlite3");
    base
}
