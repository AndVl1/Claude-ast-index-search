//! Integration tests for Swift-specific issues found in code review.
//!
//! Issues #5 (SwiftUI modern patterns), #6 (async-funcs multi-line),
//! and #7 (SQL injection in iOS commands).
//!
//! Run with: cargo test --test swift_issues_tests

use std::fs;
use std::path::Path;
use tempfile::TempDir;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Create a temp directory with Swift source files.
fn create_swift_project(files: &[(&str, &str)]) -> TempDir {
    let dir = TempDir::new().expect("failed to create temp dir");
    for (name, content) in files {
        let path = dir.path().join(name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&path, content).unwrap();
    }
    dir
}

/// Test the format!()-based query pattern from cmd_storyboard_usages.
/// Returns Ok(()) if the query succeeds, Err if it fails.
fn try_storyboard_query(class_name: &str) -> Result<(), String> {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    ast_index::db::init_db(&conn).unwrap();

    // Insert a storyboard usage with a safe parameterized query
    conn.execute(
        "INSERT INTO storyboard_usages (file_path, line, class_name, usage_type) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["Main.storyboard", 10, class_name, "viewController"],
    ).unwrap();

    // This is the format!()-based query pattern from cmd_storyboard_usages
    let query = format!(
        r#"
        SELECT file_path, line, class_name, usage_type, storyboard_id
        FROM storyboard_usages
        WHERE class_name LIKE '%{}%'
        ORDER BY file_path, line
        "#,
        class_name
    );

    conn.prepare(&query)
        .map(|_| ())
        .map_err(|e| format!("{}", e))
}

// ---------------------------------------------------------------------------
// Issue #5: SwiftUI command misses modern patterns
// ---------------------------------------------------------------------------

/// Collects matches from search_files for a given pattern + extensions.
fn grep_swift_matches(root: &Path, pattern: &str) -> Vec<(String, usize, String)> {
    let mut results = Vec::new();
    ast_index::commands::search_files(root, pattern, &["swift"], |path, line_num, line| {
        let rel = path
            .strip_prefix(root)
            .unwrap_or(path)
            .to_string_lossy()
            .to_string();
        results.push((rel, line_num, line.to_string()));
    })
    .unwrap();
    results
}

#[test]
fn test_swiftui_misses_environment() {
    let dir = create_swift_project(&[("View.swift", r#"
import SwiftUI

struct SettingsView: View {
    @Environment(\.dismiss) var dismiss
    @Environment(\.colorScheme) var colorScheme

    var body: some View {
        Text("Hello")
    }
}
"#)]);

    // This is the pattern used by cmd_swiftui
    let pattern = r"@(State|Binding|Published|ObservedObject|StateObject|EnvironmentObject)\s+(private\s+)?(var|let)\s+\w+";
    let results = grep_swift_matches(dir.path(), pattern);

    // @Environment is a commonly used SwiftUI property wrapper but is not in the pattern
    assert!(
        results.iter().any(|r| r.2.contains("@Environment")),
        "@Environment should be matched by swiftui command pattern, but it's missing. \
         Found results: {:?}",
        results
    );
}

#[test]
fn test_swiftui_misses_observable() {
    let dir = create_swift_project(&[("Model.swift", r#"
import Observation

@Observable
class UserModel {
    var name: String = ""
    var age: Int = 0
}

struct ContentView: View {
    @Bindable var model: UserModel

    var body: some View {
        TextField("Name", text: $model.name)
    }
}
"#)]);

    let pattern = r"@(State|Binding|Published|ObservedObject|StateObject|EnvironmentObject)\s+(private\s+)?(var|let)\s+\w+";
    let results = grep_swift_matches(dir.path(), pattern);

    // @Bindable (iOS 17+, Observation framework) is not in the pattern
    assert!(
        results.iter().any(|r| r.2.contains("@Bindable")),
        "@Bindable should be matched by swiftui command pattern, but it's missing. \
         Found results: {:?}",
        results
    );
}

#[test]
fn test_swiftui_misses_appstorage() {
    let dir = create_swift_project(&[("Settings.swift", r#"
import SwiftUI

struct SettingsView: View {
    @AppStorage("username") var username: String = ""
    @SceneStorage("selectedTab") var selectedTab: Int = 0

    var body: some View {
        Text(username)
    }
}
"#)]);

    let pattern = r"@(State|Binding|Published|ObservedObject|StateObject|EnvironmentObject)\s+(private\s+)?(var|let)\s+\w+";
    let results = grep_swift_matches(dir.path(), pattern);

    assert!(
        results.iter().any(|r| r.2.contains("@AppStorage")),
        "@AppStorage should be matched by swiftui command pattern, but it's missing. \
         Found results: {:?}",
        results
    );
}

// ---------------------------------------------------------------------------
// Issue #6: async-funcs command misses multi-line signatures
// ---------------------------------------------------------------------------

#[test]
fn test_async_funcs_misses_multiline() {
    let dir = create_swift_project(&[("Service.swift", r#"
import Foundation

class NetworkService {
    func fetchData(
        from url: URL,
        headers: [String: String]
    ) async throws -> Data {
        fatalError()
    }

    func singleLine() async throws -> Data { fatalError() }
}
"#)]);

    // This is the pattern used by cmd_async_funcs
    let pattern = r"func\s+\w+[^{]*\basync\b";
    let results = grep_swift_matches(dir.path(), pattern);

    let found_names: Vec<&str> = results
        .iter()
        .filter_map(|r| {
            let re = regex::Regex::new(r"func\s+(\w+)").unwrap();
            re.captures(&r.2).map(|c| c.get(1).unwrap().as_str())
        })
        .collect();

    // singleLine should be found (same line)
    assert!(
        found_names.contains(&"singleLine"),
        "singleLine should be found, got: {:?}",
        found_names
    );

    // fetchData has async on a different line than func — currently missed
    assert!(
        found_names.contains(&"fetchData"),
        "fetchData with multi-line signature should be found by async-funcs, \
         but it's missed because 'async' is on a different line than 'func'. \
         Found: {:?}",
        found_names
    );
}

// ---------------------------------------------------------------------------
// Issue #7: SQL injection in iOS commands
// ---------------------------------------------------------------------------

#[test]
fn test_storyboard_query_sql_injection() {
    // A class name containing a single quote should not break the query.
    // cmd_storyboard_usages uses format!() interpolation, so this will fail.
    let result = try_storyboard_query("O'Brien");
    assert!(
        result.is_ok(),
        "Query with single-quote in class_name should not error. \
         format!()-based query is vulnerable to SQL injection. \
         Use parameterized queries instead. Error: {:?}",
        result.err()
    );
}

#[test]
fn test_storyboard_query_normal_name() {
    // Normal class names should work fine
    let result = try_storyboard_query("MyViewController");
    assert!(result.is_ok(), "Query should work for normal class names");
}

#[test]
fn test_asset_query_sql_injection() {
    // Same issue exists in cmd_asset_usages.
    // A single quote in the asset name breaks the query or causes unexpected behavior.
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    ast_index::db::init_db(&conn).unwrap();

    // Insert test data
    conn.execute(
        "INSERT INTO ios_assets (type, name, file_path) VALUES (?1, ?2, ?3)",
        rusqlite::params!["imageset", "icon_test", "Assets.xcassets/icon_test.imageset"],
    )
    .unwrap();

    // A class name with a single quote — common in real projects (e.g., O'Reilly branding assets)
    let asset_name = "icon_o'reilly";

    // This is the format!()-based query pattern from cmd_asset_usages
    let query = format!(
        r#"
        SELECT a.name, a.type, au.usage_file, au.usage_line
        FROM ios_assets a
        JOIN ios_asset_usages au ON a.id = au.asset_id
        WHERE a.name LIKE '%{}%'
        ORDER BY au.usage_file, au.usage_line
        "#,
        asset_name
    );

    // The unescaped single quote produces invalid SQL
    let result = conn.prepare(&query);
    assert!(
        result.is_ok(),
        "Query with single-quote in asset name should not error. \
         format!()-based query is vulnerable to SQL injection. \
         Use parameterized queries instead. Error: {:?}",
        result.err()
    );
}
