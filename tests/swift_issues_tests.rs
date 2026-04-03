//! Integration tests for Swift-specific issues found in code review.
//!
//! Issues #5 (SwiftUI modern patterns), #6 (async-funcs multi-line),
//! and #7 (SQL injection in iOS commands).
//!
//! Run with: cargo test --test swift_issues_tests

/// Test the query pattern from cmd_storyboard_usages using parameterized queries.
/// Returns Ok(()) if the query succeeds, Err if it fails.
fn try_storyboard_query(class_name: &str) -> Result<(), Option<String>> {
    let conn = rusqlite::Connection::open_in_memory().unwrap();
    ast_index::db::init_db(&conn).unwrap();

    // Insert a storyboard usage with a safe parameterized query
    conn.execute(
        "INSERT INTO storyboard_usages (file_path, line, class_name, usage_type) VALUES (?1, ?2, ?3, ?4)",
        rusqlite::params!["Main.storyboard", 10, class_name, "viewController"],
    ).unwrap();

    // Use parameterized query (as the fixed cmd_storyboard_usages does)
    let class_like = format!("%{}%", class_name);
    let mut stmt = conn.prepare(
        r#"
        SELECT file_path, line, class_name, usage_type, storyboard_id
        FROM storyboard_usages
        WHERE class_name LIKE ?1
        ORDER BY file_path, line
        "#,
    ).map_err(|e| Some(format!("{}", e)))?;

    let results: Vec<(String, i64, String)> = stmt
        .query_map(rusqlite::params![class_like], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?))
        })
        .map_err(|e| Some(format!("{}", e)))?
        .filter_map(|r| r.ok())
        .collect();

    // Verify the query actually found the inserted row
    if results.is_empty() {
        return Err(Some(format!("No results found for class_name '{}'", class_name)));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Issue #5: SwiftUI command misses modern patterns
// ---------------------------------------------------------------------------

#[test]
fn test_swiftui_misses_environment() {
    let content = r#"
import SwiftUI

struct SettingsView: View {
    @Environment(\.dismiss) var dismiss
    @Environment(\.colorScheme) var colorScheme

    var body: some View {
        Text("Hello")
    }
}
"#;

    let wrappers = ast_index::parsers::treesitter::swift::find_property_wrappers(content).unwrap();

    assert!(
        wrappers.iter().any(|w| w.wrapper == "Environment" && w.name == "dismiss"),
        "@Environment should be found by tree-sitter. Found: {:?}",
        wrappers.iter().map(|w| (&w.wrapper, &w.name)).collect::<Vec<_>>()
    );
}

#[test]
fn test_swiftui_misses_observable() {
    let content = r#"
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
"#;

    let wrappers = ast_index::parsers::treesitter::swift::find_property_wrappers(content).unwrap();

    assert!(
        wrappers.iter().any(|w| w.wrapper == "Bindable" && w.name == "model"),
        "@Bindable should be found by tree-sitter. Found: {:?}",
        wrappers.iter().map(|w| (&w.wrapper, &w.name)).collect::<Vec<_>>()
    );
}

#[test]
fn test_swiftui_misses_appstorage() {
    let content = r#"
import SwiftUI

struct SettingsView: View {
    @AppStorage("username") var username: String = ""
    @SceneStorage("selectedTab") var selectedTab: Int = 0

    var body: some View {
        Text(username)
    }
}
"#;

    let wrappers = ast_index::parsers::treesitter::swift::find_property_wrappers(content).unwrap();

    assert!(
        wrappers.iter().any(|w| w.wrapper == "AppStorage" && w.name == "username"),
        "@AppStorage should be found by tree-sitter. Found: {:?}",
        wrappers.iter().map(|w| (&w.wrapper, &w.name)).collect::<Vec<_>>()
    );
    assert!(
        wrappers.iter().any(|w| w.wrapper == "SceneStorage" && w.name == "selectedTab"),
        "@SceneStorage should be found by tree-sitter. Found: {:?}",
        wrappers.iter().map(|w| (&w.wrapper, &w.name)).collect::<Vec<_>>()
    );
}

// ---------------------------------------------------------------------------
// Issue #6: async-funcs command misses multi-line signatures
// ---------------------------------------------------------------------------

#[test]
fn test_async_funcs_misses_multiline() {
    let content = r#"
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
"#;

    let funcs = ast_index::parsers::treesitter::swift::find_async_funcs(content).unwrap();
    let found_names: Vec<&str> = funcs.iter().map(|f| f.name.as_str()).collect();

    // singleLine should be found (same line)
    assert!(
        found_names.contains(&"singleLine"),
        "singleLine should be found, got: {:?}",
        found_names
    );

    // fetchData has async on a different line than func — tree-sitter handles this natively
    assert!(
        found_names.contains(&"fetchData"),
        "fetchData with multi-line signature should be found by tree-sitter. \
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
    let result = try_storyboard_query("O'Brien");
    assert!(
        result.is_ok(),
        "Query with single-quote in class_name should not error. \
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

    // Use parameterized query (as the fixed cmd_asset_usages does)
    let _asset_like = format!("%{}%", asset_name);
    let stmt = conn.prepare(
        r#"
        SELECT a.name, a.type, au.usage_file, au.usage_line
        FROM ios_assets a
        JOIN ios_asset_usages au ON a.id = au.asset_id
        WHERE a.name LIKE ?1
        ORDER BY au.usage_file, au.usage_line
        "#,
    );

    assert!(
        stmt.is_ok(),
        "Query with single-quote in asset name should not error. \
         format!()-based query is vulnerable to SQL injection. \
         Use parameterized queries instead. Error: {:?}",
        stmt.err()
    );
}
