//! End-to-end integration test for SQL project support.
//!
//! Regression test for GitHub issue #25: README advertised SQL support,
//! but `detect_project_type` had no SQL branch, so a folder of `.sql`
//! files was labelled `Unknown`. Indexing still walked the files, but
//! the user-visible "project type is unknown" message contradicted the
//! README and made the tool look broken.

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
fn sql_project_type_detected_from_sql_files() {
    let tmp = TempDir::new().unwrap();
    fs::write(
        tmp.path().join("schema.sql"),
        "CREATE TABLE users (id INTEGER PRIMARY KEY);\n",
    )
    .unwrap();
    assert_eq!(
        indexer::detect_project_type(tmp.path()),
        indexer::ProjectType::Sql,
        "folder containing a .sql file should detect as SQL, not Unknown"
    );
}

#[test]
fn sql_from_str_round_trip() {
    assert_eq!(
        indexer::ProjectType::from_str("sql"),
        Some(indexer::ProjectType::Sql)
    );
    assert_eq!(indexer::ProjectType::Sql.as_str(), "SQL");
}

#[test]
fn sql_symbols_are_indexed_end_to_end() {
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();

    fs::write(
        root.join("schema.sql"),
        r#"
CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT);
CREATE INDEX idx_users_name ON users (name);
"#,
    )
    .unwrap();

    let mut conn = open_fresh_db(root);
    let result = indexer::index_directory(&mut conn, root, false, false).unwrap();
    assert!(result.file_count > 0, "indexer should walk .sql files");

    assert!(
        symbol_exists(&conn, "users"),
        "CREATE TABLE `users` should be indexed"
    );
    assert!(
        symbol_exists(&conn, "idx_users_name"),
        "CREATE INDEX `idx_users_name` should be indexed"
    );
}
