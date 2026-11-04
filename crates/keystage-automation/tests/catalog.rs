//! `mcpb/tools.json` is the tool list the connector bundle's manifest
//! names (`scripts/pack-mcpb.mjs` reads it). It is written from the real
//! catalog: `UPDATE_GOLDEN=1 cargo test -p keystage-automation --test
//! catalog` refreshes it; otherwise a difference fails.

use std::path::PathBuf;

#[test]
fn the_bundle_lists_the_catalog() {
    let tools: Vec<serde_json::Value> = keystage_automation::tools::catalog()
        .iter()
        .map(|t| serde_json::json!({"name": t.name, "description": t.description}))
        .collect();
    let text = serde_json::to_string_pretty(&tools).expect("json") + "\n";
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../mcpb/tools.json");
    if std::env::var_os("UPDATE_GOLDEN").is_some() {
        std::fs::write(&path, &text).expect("write");
    }
    let on_disk = std::fs::read_to_string(&path)
        .unwrap_or_default()
        .replace("\r\n", "\n");
    assert_eq!(
        on_disk, text,
        "mcpb/tools.json differs from the catalog; run with UPDATE_GOLDEN=1"
    );
}
