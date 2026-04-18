//! Compact-text formatter for ast-index JSON responses.
//!
//! Reasoning: agent context is the scarcest resource. ast-index's
//! `--format json` produces pretty-printed JSON (whitespace + quoting) that
//! eats 2-3× the tokens of a plain-text summary carrying the same
//! information. For MCP use we default to a TOON-inspired compact format
//! and keep JSON as an opt-in via the `format: "json"` tool parameter.
//!
//! Size comparison on a typical `search` response (3 content matches):
//!   pretty JSON:   ~280 bytes, ~90 tokens
//!   compact JSON:  ~180 bytes, ~60 tokens
//!   this format:   ~120 bytes, ~35 tokens
//!
//! Shape detection is best-effort: anything we don't recognise falls
//! through to compact JSON (`serde_json::to_string`) which still beats
//! pretty JSON by ~40%.

use serde_json::Value;
use std::fmt::Write;

/// Format an ast-index JSON response as compact text.
///
/// `tool` is the MCP tool name (`search`, `usages`, etc.) and drives
/// shape-aware formatting. If the response shape doesn't match what we
/// expect for that tool, we fall back to compact JSON so no information
/// is lost.
pub fn to_compact(tool: &str, raw_json: &str) -> String {
    let value: Value = match serde_json::from_str(raw_json) {
        Ok(v) => v,
        // Not JSON (e.g. `outline` prints plain text) — pass through.
        Err(_) => return raw_json.trim_end().to_string(),
    };

    let mut out = String::with_capacity(raw_json.len() / 2);
    let rendered = match tool {
        "search" => render_search(&value, &mut out),
        "refs" => render_refs(&value, &mut out),
        "usages" | "callers" => render_ref_list(&value, &mut out),
        "symbol" | "class" | "implementations" => render_symbol_list(&value, &mut out),
        "file" | "find_file" => render_file_list(&value, &mut out),
        "stats" => render_stats(&value, &mut out),
        _ => false,
    };

    if !rendered {
        return serde_json::to_string(&value).unwrap_or_else(|_| raw_json.to_string());
    }

    let trimmed = out.trim_end().to_string();
    if trimmed.is_empty() {
        "(no results)".to_string()
    } else {
        trimmed
    }
}

fn render_search(v: &Value, out: &mut String) -> bool {
    let Some(obj) = v.as_object() else { return false };

    let mut any_section = false;

    if let Some(files) = obj.get("files").and_then(Value::as_array) {
        if !files.is_empty() {
            any_section = true;
            writeln!(out, "Files:").ok();
            for f in files {
                if let Some(s) = f.as_str() {
                    writeln!(out, "  {s}").ok();
                }
            }
        }
    }

    if let Some(symbols) = obj.get("symbols").and_then(Value::as_array) {
        if !symbols.is_empty() {
            any_section = true;
            writeln!(out, "\nSymbols:").ok();
            for s in symbols {
                write_symbol_line(s, "  ", out);
            }
        }
    }

    if let Some(refs) = obj.get("references").and_then(Value::as_array) {
        if !refs.is_empty() {
            any_section = true;
            writeln!(out, "\nReferences (usage counts):").ok();
            for r in refs {
                if let (Some(name), Some(count)) = (
                    r.get("name").and_then(Value::as_str),
                    r.get("usage_count").and_then(Value::as_i64),
                ) {
                    writeln!(out, "  {name} ×{count}").ok();
                }
            }
        }
    }

    if let Some(content) = obj.get("content_matches").and_then(Value::as_array) {
        if !content.is_empty() {
            any_section = true;
            writeln!(out, "\nContent:").ok();
            for m in content {
                if let (Some(path), Some(line), Some(snippet)) = (
                    m.get("path").and_then(Value::as_str),
                    m.get("line").and_then(Value::as_i64),
                    m.get("content").and_then(Value::as_str),
                ) {
                    writeln!(out, "  {path}:{line}  {}", truncate(snippet, 100)).ok();
                }
            }
        }
    }

    any_section || obj.contains_key("files")
}

fn render_refs(v: &Value, out: &mut String) -> bool {
    let Some(obj) = v.as_object() else { return false };

    let mut any = false;

    if let Some(defs) = obj.get("definitions").and_then(Value::as_array) {
        if !defs.is_empty() {
            any = true;
            writeln!(out, "Definitions:").ok();
            for d in defs {
                write_symbol_line(d, "  ", out);
            }
        }
    }

    if let Some(imports) = obj.get("imports").and_then(Value::as_array) {
        if !imports.is_empty() {
            any = true;
            writeln!(out, "\nImports:").ok();
            for i in imports {
                if let (Some(path), Some(line)) = (
                    i.get("path").and_then(Value::as_str),
                    i.get("line").and_then(Value::as_i64),
                ) {
                    let sig = i.get("signature").and_then(Value::as_str).unwrap_or("");
                    if sig.is_empty() {
                        writeln!(out, "  {path}:{line}").ok();
                    } else {
                        writeln!(out, "  {path}:{line}  {}", truncate(sig, 80)).ok();
                    }
                }
            }
        }
    }

    if let Some(usages) = obj.get("usages").and_then(Value::as_array) {
        if !usages.is_empty() {
            any = true;
            writeln!(out, "\nUsages:").ok();
            for u in usages {
                write_ref_line(u, "  ", out);
            }
        }
    }

    any || obj.contains_key("definitions")
}

fn render_ref_list(v: &Value, out: &mut String) -> bool {
    // usages / callers: array of {name, line, context, path}
    let Some(arr) = v.as_array() else { return false };
    for r in arr {
        write_ref_line(r, "", out);
    }
    true
}

fn render_symbol_list(v: &Value, out: &mut String) -> bool {
    // symbol / class / implementations: array of SearchResult
    let Some(arr) = v.as_array() else { return false };
    for s in arr {
        write_symbol_line(s, "", out);
    }
    true
}

fn render_file_list(v: &Value, out: &mut String) -> bool {
    let Some(arr) = v.as_array() else { return false };
    for f in arr {
        if let Some(s) = f.as_str() {
            writeln!(out, "{s}").ok();
        }
    }
    true
}

fn render_stats(v: &Value, out: &mut String) -> bool {
    let Some(obj) = v.as_object() else { return false };

    let project = obj.get("project").and_then(Value::as_str).unwrap_or("?");
    let db_size = obj
        .get("db_size_bytes")
        .and_then(Value::as_i64)
        .unwrap_or(0);
    let db_path = obj.get("db_path").and_then(Value::as_str).unwrap_or("");

    writeln!(out, "project: {project}").ok();
    if let Some(stats) = obj.get("stats").and_then(Value::as_object) {
        // Known counters first (stable order, skip zeros to save tokens).
        let keys = [
            "file_count", "symbol_count", "refs_count", "module_count",
            "xml_usages_count", "resources_count",
            "storyboard_usages_count", "ios_assets_count",
        ];
        for k in keys {
            if let Some(n) = stats.get(k).and_then(Value::as_i64) {
                if n > 0 {
                    writeln!(out, "{k}: {n}").ok();
                }
            }
        }
    }
    if db_size > 0 {
        writeln!(
            out,
            "db_size_mb: {:.2}",
            db_size as f64 / 1024.0 / 1024.0
        )
        .ok();
    }
    if !db_path.is_empty() {
        writeln!(out, "db_path: {db_path}").ok();
    }
    true
}

fn write_symbol_line(s: &Value, indent: &str, out: &mut String) {
    let name = s.get("name").and_then(Value::as_str).unwrap_or("?");
    let kind = s.get("kind").and_then(Value::as_str).unwrap_or("?");
    let path = s.get("path").and_then(Value::as_str).unwrap_or("?");
    let line = s.get("line").and_then(Value::as_i64).unwrap_or(0);
    writeln!(out, "{indent}{name} [{kind}] {path}:{line}").ok();

    if let Some(sig) = s.get("signature").and_then(Value::as_str) {
        if !sig.is_empty() {
            writeln!(out, "{indent}  {}", truncate(sig, 80)).ok();
        }
    }
}

fn write_ref_line(r: &Value, indent: &str, out: &mut String) {
    let path = r.get("path").and_then(Value::as_str).unwrap_or("?");
    let line = r.get("line").and_then(Value::as_i64).unwrap_or(0);
    writeln!(out, "{indent}{path}:{line}").ok();

    if let Some(ctx) = r.get("context").and_then(Value::as_str) {
        if !ctx.is_empty() {
            writeln!(out, "{indent}  {}", truncate(ctx, 80)).ok();
        }
    }
}

fn truncate(s: &str, max: usize) -> String {
    let s = s.trim();
    if s.chars().count() <= max {
        s.to_string()
    } else {
        let cut: String = s.chars().take(max).collect();
        format!("{cut}…")
    }
}
