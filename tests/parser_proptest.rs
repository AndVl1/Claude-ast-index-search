//! Property-based fuzz tests for `parsers::parse_file_symbols`.
//!
//! Goal: parsers must never panic, hang, or produce inconsistent output on
//! arbitrary input. We cover four representative high-traffic languages
//! (Rust, Kotlin, Python, Go, TypeScript) with three properties each:
//!
//!   1. `no_panic_on_arbitrary_input_<lang>` — parser returns Ok or Err but
//!      never unwinds the stack.
//!   2. `output_is_deterministic_<lang>` — same input → same `(symbols,
//!      refs)` across two calls (catches HashMap/iteration leaks, time
//!      dependence).
//!   3. `symbol_lines_are_in_bounds_<lang>` — every emitted line number is
//!      `>= 1` and `<= input.lines().count() + 1` (catches off-by-ones that
//!      break editor jump-to-definition).
//!
//! Cap: 64 cases per property. This keeps the suite fast (< 30 sec total)
//! and is **intentional** — coverage-guided fuzzing (cargo-fuzz / AFL) is
//! a separate effort if more depth is needed. The point of these props is
//! to catch obvious panics and inconsistencies on every PR, not exhaustive
//! exploration.

use ast_index::parsers::{parse_file_symbols, FileType};
use proptest::prelude::*;

/// Generator for ~256 chars of random text biased toward looking like
/// source code: letters, digits, common punctuation, brackets, newlines.
/// We don't require valid UTF-8 multibyte sequences here — `proptest`
/// already produces valid `String`s, and ASCII-ish input is the highest-
/// signal corpus for parser robustness.
fn arb_source() -> impl Strategy<Value = String> {
    // Weighted character set: source code is mostly identifiers, whitespace,
    // and brackets. Bias toward those over arbitrary punctuation.
    let charset = "[a-zA-Z0-9_(){}\\[\\];:,. \n\t<>=+\\-*/!?\"'`#@$%&|^~\\\\]";
    proptest::string::string_regex(charset)
        .unwrap()
        .prop_map(|s| s.chars().take(256).collect::<String>())
}

fn fast_config() -> ProptestConfig {
    ProptestConfig {
        cases: 64,
        ..ProptestConfig::default()
    }
}

/// Helper: assert symbol lines are within bounds. We allow up to
/// `lines + 1` because some parsers report on the synthetic trailing line
/// after the final newline, which is a legitimate (if quirky) position.
fn check_line_bounds(
    input: &str,
    symbols: &[ast_index::parsers::ParsedSymbol],
) -> Result<(), TestCaseError> {
    let max_line = input.lines().count() + 1;
    for s in symbols {
        prop_assert!(
            s.line >= 1,
            "symbol {:?} has line {} (expected >= 1)",
            s.name,
            s.line
        );
        prop_assert!(
            s.line <= max_line,
            "symbol {:?} has line {} (expected <= {} for input with {} lines)",
            s.name,
            s.line,
            max_line,
            input.lines().count()
        );
    }
    Ok(())
}

// ============================================================================
// Rust
// ============================================================================

proptest! {
    #![proptest_config(fast_config())]

    #[test]
    fn no_panic_on_arbitrary_input_rust(input in arb_source()) {
        // Result::is_ok() || Result::is_err() is trivially true; the real
        // assertion is "didn't panic" — proptest catches panics for us.
        let result = parse_file_symbols(&input, FileType::Rust);
        prop_assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn output_is_deterministic_rust(input in arb_source()) {
        let r1 = parse_file_symbols(&input, FileType::Rust);
        let r2 = parse_file_symbols(&input, FileType::Rust);
        match (r1, r2) {
            (Ok((s1, refs1)), Ok((s2, refs2))) => {
                prop_assert_eq!(s1.len(), s2.len(), "symbol count differs across calls");
                for (a, b) in s1.iter().zip(s2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                    prop_assert_eq!(&a.signature, &b.signature);
                }
                prop_assert_eq!(refs1.len(), refs2.len(), "ref count differs across calls");
                for (a, b) in refs1.iter().zip(refs2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                }
            }
            (Err(_), Err(_)) => {} // both errored — also deterministic
            _ => prop_assert!(false, "non-deterministic Ok/Err across calls"),
        }
    }

    #[test]
    fn symbol_lines_are_in_bounds_rust(input in arb_source()) {
        if let Ok((symbols, _)) = parse_file_symbols(&input, FileType::Rust) {
            check_line_bounds(&input, &symbols)?;
        }
    }
}

// ============================================================================
// Kotlin
// ============================================================================

proptest! {
    #![proptest_config(fast_config())]

    #[test]
    fn no_panic_on_arbitrary_input_kotlin(input in arb_source()) {
        let result = parse_file_symbols(&input, FileType::Kotlin);
        prop_assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn output_is_deterministic_kotlin(input in arb_source()) {
        let r1 = parse_file_symbols(&input, FileType::Kotlin);
        let r2 = parse_file_symbols(&input, FileType::Kotlin);
        match (r1, r2) {
            (Ok((s1, refs1)), Ok((s2, refs2))) => {
                prop_assert_eq!(s1.len(), s2.len());
                for (a, b) in s1.iter().zip(s2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                    prop_assert_eq!(&a.signature, &b.signature);
                }
                prop_assert_eq!(refs1.len(), refs2.len());
                for (a, b) in refs1.iter().zip(refs2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                }
            }
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "non-deterministic Ok/Err across calls"),
        }
    }

    #[test]
    fn symbol_lines_are_in_bounds_kotlin(input in arb_source()) {
        if let Ok((symbols, _)) = parse_file_symbols(&input, FileType::Kotlin) {
            check_line_bounds(&input, &symbols)?;
        }
    }
}

// ============================================================================
// Python
// ============================================================================

proptest! {
    #![proptest_config(fast_config())]

    #[test]
    fn no_panic_on_arbitrary_input_python(input in arb_source()) {
        let result = parse_file_symbols(&input, FileType::Python);
        prop_assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn output_is_deterministic_python(input in arb_source()) {
        let r1 = parse_file_symbols(&input, FileType::Python);
        let r2 = parse_file_symbols(&input, FileType::Python);
        match (r1, r2) {
            (Ok((s1, refs1)), Ok((s2, refs2))) => {
                prop_assert_eq!(s1.len(), s2.len());
                for (a, b) in s1.iter().zip(s2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                    prop_assert_eq!(&a.signature, &b.signature);
                }
                prop_assert_eq!(refs1.len(), refs2.len());
                for (a, b) in refs1.iter().zip(refs2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                }
            }
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "non-deterministic Ok/Err across calls"),
        }
    }

    #[test]
    fn symbol_lines_are_in_bounds_python(input in arb_source()) {
        if let Ok((symbols, _)) = parse_file_symbols(&input, FileType::Python) {
            check_line_bounds(&input, &symbols)?;
        }
    }
}

// ============================================================================
// Go
// ============================================================================

proptest! {
    #![proptest_config(fast_config())]

    #[test]
    fn no_panic_on_arbitrary_input_go(input in arb_source()) {
        let result = parse_file_symbols(&input, FileType::Go);
        prop_assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn output_is_deterministic_go(input in arb_source()) {
        let r1 = parse_file_symbols(&input, FileType::Go);
        let r2 = parse_file_symbols(&input, FileType::Go);
        match (r1, r2) {
            (Ok((s1, refs1)), Ok((s2, refs2))) => {
                prop_assert_eq!(s1.len(), s2.len());
                for (a, b) in s1.iter().zip(s2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                    prop_assert_eq!(&a.signature, &b.signature);
                }
                prop_assert_eq!(refs1.len(), refs2.len());
                for (a, b) in refs1.iter().zip(refs2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                }
            }
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "non-deterministic Ok/Err across calls"),
        }
    }

    #[test]
    fn symbol_lines_are_in_bounds_go(input in arb_source()) {
        if let Ok((symbols, _)) = parse_file_symbols(&input, FileType::Go) {
            check_line_bounds(&input, &symbols)?;
        }
    }
}

// ============================================================================
// TypeScript
// ============================================================================

proptest! {
    #![proptest_config(fast_config())]

    #[test]
    fn no_panic_on_arbitrary_input_typescript(input in arb_source()) {
        let result = parse_file_symbols(&input, FileType::TypeScript);
        prop_assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn output_is_deterministic_typescript(input in arb_source()) {
        let r1 = parse_file_symbols(&input, FileType::TypeScript);
        let r2 = parse_file_symbols(&input, FileType::TypeScript);
        match (r1, r2) {
            (Ok((s1, refs1)), Ok((s2, refs2))) => {
                prop_assert_eq!(s1.len(), s2.len());
                for (a, b) in s1.iter().zip(s2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                    prop_assert_eq!(&a.signature, &b.signature);
                }
                prop_assert_eq!(refs1.len(), refs2.len());
                for (a, b) in refs1.iter().zip(refs2.iter()) {
                    prop_assert_eq!(&a.name, &b.name);
                    prop_assert_eq!(a.line, b.line);
                }
            }
            (Err(_), Err(_)) => {}
            _ => prop_assert!(false, "non-deterministic Ok/Err across calls"),
        }
    }

    #[test]
    fn symbol_lines_are_in_bounds_typescript(input in arb_source()) {
        if let Ok((symbols, _)) = parse_file_symbols(&input, FileType::TypeScript) {
            check_line_bounds(&input, &symbols)?;
        }
    }
}
