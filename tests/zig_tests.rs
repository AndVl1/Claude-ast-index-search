//! End-to-end integration test for Zig support.
//!
//! Walks a real temp directory via `indexer::index_directory`, then asserts
//! that Zig symbols (top-level function, struct constant, struct fields, test
//! block) land in the SQLite index and can be found by name.

use std::fs;
use std::path::Path;

use ast_index::{db, indexer};
use rusqlite::Connection;
use tempfile::TempDir;

fn open_fresh_db(project_root: &Path) -> Connection {
    if db::db_exists(project_root) {
        db::delete_db(project_root).unwrap();
    }
    let conn = db::open_db(project_root).unwrap();
    db::init_db(&conn).unwrap();
    conn
}

fn symbol_exists(conn: &Connection, name: &str) -> bool {
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM symbols WHERE name = ?1",
            rusqlite::params![name],
            |row| row.get(0),
        )
        .unwrap();
    count > 0
}

#[test]
fn zig_symbols_are_indexed_end_to_end() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    // `build.zig` so detect_project_type labels it Zig.
    fs::write(
        root.join("build.zig"),
        "pub fn build(b: *std.Build) void { _ = b; }\n",
    )
    .unwrap();

    let src = root.join("src");
    fs::create_dir_all(&src).unwrap();
    fs::write(
        src.join("main.zig"),
        r#"
const std = @import("std");

pub const Point = struct {
    x: f32,
    y: f32,
};

pub fn add(a: i32, b: i32) i32 {
    return a + b;
}

test "basic addition" {
    try std.testing.expect(add(1, 2) == 3);
}
"#,
    )
    .unwrap();

    let mut conn = open_fresh_db(root);
    let result = indexer::index_directory(&mut conn, root, false, false).unwrap();
    assert!(result.file_count > 0, "indexer should walk .zig files");

    assert!(symbol_exists(&conn, "add"), "fn `add` should be indexed");
    assert!(
        symbol_exists(&conn, "Point"),
        "struct alias `Point` should be indexed"
    );
    assert!(
        symbol_exists(&conn, "x"),
        "struct field `x` should be indexed"
    );
    assert!(
        symbol_exists(&conn, "test basic addition"),
        "test block should be indexed under its quoted name"
    );
}

#[test]
fn zig_project_type_detected_from_build_zig() {
    let tmp = TempDir::new().unwrap();
    fs::write(tmp.path().join("build.zig"), "").unwrap();
    assert_eq!(
        indexer::detect_project_type(tmp.path()),
        indexer::ProjectType::Zig
    );
}
