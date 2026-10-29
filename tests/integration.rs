//! SQL-level integration tests: LOAD the built `.duckdb_extension` into a real
//! in-memory DuckDB and exercise the full function surface (2 scalars).
//!
//! Run: `cargo test --release` (after the cdylib is built and stamped to
//! `target/release/evtx.duckdb_extension`).
//!
//! The `.evtx` under test is the committed `test/data/sample-security.evtx`
//! fixture (a 7-record Security.evtx). It is read with DuckDB's built-in
//! `read_blob` table function — so the input is a real on-disk `.evtx`, and the
//! expected record count (7) comes from the upstream parser as ground truth.

use quack_rs::testing::InMemoryDb;

fn db() -> InMemoryDb {
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("target/release/evtx.duckdb_extension");
    assert!(path.exists(), "build first: make release -> {path:?}");
    let d = InMemoryDb::open_unsigned().expect("open in-memory db");
    d.execute_batch(&format!("LOAD '{}'", path.display()))
        .expect("load extension");
    d
}

/// Absolute path to the committed sample `.evtx` fixture.
fn fixture() -> String {
    let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("test/data/sample-security.evtx");
    p.to_string_lossy().into_owned()
}

#[test]
fn real_evtx_count_and_validity() {
    let d = db();
    let sql = format!(
        "SELECT evtx_is_valid(content), evtx_record_count(content) \
         FROM read_blob('{}')",
        fixture()
    );
    // read_blob emits one BLOB row; query it into (valid, count).
    let valid: bool = d.query_one(&format!("{sql}")).unwrap();
    assert!(valid, "the sample .evtx must be valid");

    let count: i64 = d
        .query_one(&format!("SELECT evtx_record_count(content) FROM read_blob('{}')", fixture()))
        .unwrap();
    // Ground truth from the upstream evtx parser: the fixture has 7 records.
    assert_eq!(count, 7, "sample-security.evtx has 7 event records");
}

#[test]
fn total_on_garbage_and_null() {
    let d = db();
    // Non-.evtx bytes -> false / 0 (total, never an error).
    let g_valid: bool = d.query_one("SELECT evtx_is_valid('garbage not evtx')").unwrap();
    assert!(!g_valid);
    let g_cnt: i64 = d.query_one("SELECT evtx_record_count('garbage not evtx')").unwrap();
    assert_eq!(g_cnt, 0);
    // The ElfFile magic alone is not enough — a header with no records must not
    // count as a valid, non-empty log.
    let magic_only: bool = d
        .query_one("SELECT evtx_is_valid(from_hex('456c6646696c6500'))")
        .unwrap();
    // 'ElfFile\0' as hex is 8 bytes (the magic only) -> parser cannot construct.
    assert!(!magic_only, "magic-only 8-byte blob is not a parseable .evtx");

    // NULL input -> NULL (total). Route through a column to avoid the literal
    // NULL being constant-folded to an untyped Null result.
    let n_cnt: Option<i64> = d
        .query_one("SELECT evtx_record_count(c) FROM (SELECT CAST(NULL AS BLOB) AS c)")
        .unwrap();
    assert!(n_cnt.is_none());
    let n_valid: Option<bool> = d
        .query_one("SELECT evtx_is_valid(c) FROM (SELECT CAST(NULL AS BLOB) AS c)")
        .unwrap();
    assert!(n_valid.is_none());
}
