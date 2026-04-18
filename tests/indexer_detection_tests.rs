//! Integration tests for the filesystem-detection helpers in `indexer`.
//!
//! `detect_project_type` is well-covered by inline unit tests, but the
//! VCS / repo-marker family (`has_git_repo`, `has_arc_repo`,
//! `find_arc_root`, `has_android_markers`, `has_ios_markers`) and the
//! `quick_file_count` walker had zero coverage. These helpers feed into
//! `cmd_changed`, `cmd_rebuild`, and the auto-sub-projects threshold —
//! a wrong answer here breaks user-visible behaviour silently.

use std::fs;

use ast_index::indexer::{
    find_arc_root, has_android_markers, has_arc_repo, has_git_repo, has_ios_markers,
    quick_file_count,
};
use tempfile::TempDir;

// ----------------------------------------------------------------------
// has_git_repo
// ----------------------------------------------------------------------

#[test]
fn has_git_repo_true_when_dot_git_exists() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".git")).unwrap();
    assert!(has_git_repo(dir.path()));
}

#[test]
fn has_git_repo_false_for_bare_dir() {
    let dir = TempDir::new().unwrap();
    assert!(!has_git_repo(dir.path()));
}

// ----------------------------------------------------------------------
// has_arc_repo / find_arc_root
// ----------------------------------------------------------------------

#[test]
fn has_arc_repo_true_when_arc_head_present() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".arc")).unwrap();
    fs::write(dir.path().join(".arc").join("HEAD"), "trunk\n").unwrap();
    assert!(has_arc_repo(dir.path()));
}

#[test]
fn has_arc_repo_false_when_only_arc_dir_without_head() {
    // Bug surface: `~/.arc` (client storage) has a `.arc` directory but
    // no `HEAD` file. Without the HEAD check, every test under $HOME
    // would falsely report "in an arc repo".
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".arc")).unwrap();
    // intentionally no HEAD
    assert!(
        !has_arc_repo(dir.path()),
        ".arc without HEAD must not be considered a repo"
    );
}

#[test]
fn find_arc_root_walks_up_to_marker() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join(".arc")).unwrap();
    fs::write(dir.path().join(".arc").join("HEAD"), "trunk\n").unwrap();

    let nested = dir.path().join("a").join("b").join("c");
    fs::create_dir_all(&nested).unwrap();

    let found = find_arc_root(&nested).expect("must walk up to the arc root");

    // Compare canonicalized paths — TempDir on macOS is under /var which
    // symlinks to /private/var; both are valid representations.
    let want = dir.path().canonicalize().unwrap();
    let got = found.canonicalize().unwrap();
    assert_eq!(got, want, "arc root must be the dir containing .arc/HEAD");
}

#[test]
fn find_arc_root_returns_none_for_unrelated_dir() {
    let dir = TempDir::new().unwrap();
    // No .arc anywhere in this temp dir.
    let result = find_arc_root(dir.path());
    // Result may walk up beyond the tempdir; only assert it isn't a
    // child of the tempdir itself (i.e. didn't falsely match anything
    // we created).
    if let Some(p) = result {
        assert!(
            !p.starts_with(dir.path()),
            "no .arc under tempdir, must not match inside it: {:?}",
            p
        );
    }
}

// ----------------------------------------------------------------------
// has_android_markers
// ----------------------------------------------------------------------

#[test]
fn has_android_markers_true_for_settings_gradle_kts() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("settings.gradle.kts"), "").unwrap();
    assert!(has_android_markers(dir.path()));
}

#[test]
fn has_android_markers_true_for_pom_xml() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("pom.xml"), "<project/>").unwrap();
    assert!(has_android_markers(dir.path()));
}

#[test]
fn has_android_markers_false_for_unrelated_dir() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("Cargo.toml"), "[package]").unwrap();
    assert!(!has_android_markers(dir.path()));
}

// ----------------------------------------------------------------------
// has_ios_markers
// ----------------------------------------------------------------------

#[test]
fn has_ios_markers_true_for_package_swift() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("Package.swift"), "").unwrap();
    assert!(has_ios_markers(dir.path()));
}

#[test]
fn has_ios_markers_true_for_xcodeproj_directory() {
    let dir = TempDir::new().unwrap();
    fs::create_dir(dir.path().join("MyApp.xcodeproj")).unwrap();
    assert!(has_ios_markers(dir.path()));
}

#[test]
fn has_ios_markers_false_for_unrelated_dir() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("README.md"), "").unwrap();
    assert!(!has_ios_markers(dir.path()));
}

// ----------------------------------------------------------------------
// quick_file_count
// ----------------------------------------------------------------------

#[test]
fn quick_file_count_zero_for_empty_dir() {
    let dir = TempDir::new().unwrap();
    let n = quick_file_count(dir.path(), false, 1000);
    assert_eq!(n, 0);
}

#[test]
fn quick_file_count_finds_supported_sources() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join("a.kt"), "class A").unwrap();
    fs::write(dir.path().join("b.swift"), "class B {}").unwrap();
    fs::write(dir.path().join("c.py"), "def c(): pass").unwrap();
    fs::write(dir.path().join("ignored.txt"), "not source").unwrap();

    let n = quick_file_count(dir.path(), false, 1000);
    assert!(
        n >= 3,
        "expected at least 3 supported files, got {} (txt should be ignored)",
        n
    );
}

#[test]
fn quick_file_count_caps_at_limit() {
    let dir = TempDir::new().unwrap();
    for i in 0..20 {
        fs::write(dir.path().join(format!("f{}.py", i)), "x = 1").unwrap();
    }
    let n = quick_file_count(dir.path(), false, 5);
    assert_eq!(n, 5, "must stop walking once limit is reached");
}
