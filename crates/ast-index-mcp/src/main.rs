//! MCP server for ast-index.
//!
//! Speaks MCP over stdio: reads JSON-RPC 2.0 messages from stdin, writes
//! responses to stdout. Any diagnostic output MUST go to stderr — stdout
//! is the protocol channel.
//!
//! Strategy: each tool invocation spawns `ast-index <subcommand> --format
//! json <args>`, parses the JSON, and returns it as the MCP tool result.
//! Keeps this crate tiny (no dependency on the `ast-index` library crate)
//! and lets users upgrade the `ast-index` binary independently of the MCP
//! server.
//!
//! Root resolution: each tool call may pass `project_root`; otherwise the
//! server falls back to `$AST_INDEX_ROOT`, then the CWD of the mcp server
//! process, then the agent's CWD.

use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};

use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

mod format;

const PROTOCOL_VERSION: &str = "2024-11-05";
const SERVER_NAME: &str = "ast-index-mcp";
const SERVER_VERSION: &str = env!("CARGO_PKG_VERSION");

fn main() -> Result<()> {
    let ast_index_bin = env::var("AST_INDEX_BIN").unwrap_or_else(|_| "ast-index".to_string());
    let default_root = env::var("AST_INDEX_ROOT")
        .map(PathBuf::from)
        .ok()
        .or_else(|| env::current_dir().ok())
        .ok_or_else(|| anyhow!("cannot determine default project root"))?;

    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    let mut line = String::new();

    for raw in stdin.lock().lines() {
        line.clear();
        let raw = raw.context("stdin read failed")?;
        if raw.trim().is_empty() {
            continue;
        }

        let request: JsonRpcRequest = match serde_json::from_str(&raw) {
            Ok(r) => r,
            Err(e) => {
                eprintln!("[ast-index-mcp] malformed request: {e}");
                continue;
            }
        };

        let response = handle_request(request, &ast_index_bin, &default_root);

        // Notifications (no `id`) produce no response. Everything else gets one.
        if let Some(response) = response {
            let json = serde_json::to_string(&response)?;
            writeln!(stdout, "{json}")?;
            stdout.flush()?;
        }
    }

    Ok(())
}

#[derive(Debug, Deserialize)]
struct JsonRpcRequest {
    #[serde(default)]
    jsonrpc: String,
    #[serde(default)]
    id: Option<Value>,
    method: String,
    #[serde(default)]
    params: Value,
}

#[derive(Debug, Serialize)]
struct JsonRpcResponse {
    jsonrpc: &'static str,
    id: Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<JsonRpcError>,
}

#[derive(Debug, Serialize)]
struct JsonRpcError {
    code: i32,
    message: String,
}

fn ok(id: Value, result: Value) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: Some(result),
        error: None,
    }
}

fn err(id: Value, code: i32, message: impl Into<String>) -> JsonRpcResponse {
    JsonRpcResponse {
        jsonrpc: "2.0",
        id,
        result: None,
        error: Some(JsonRpcError {
            code,
            message: message.into(),
        }),
    }
}

fn handle_request(
    req: JsonRpcRequest,
    ast_index_bin: &str,
    default_root: &PathBuf,
) -> Option<JsonRpcResponse> {
    let _ = req.jsonrpc; // ignored; we always emit 2.0

    let id = match req.id.clone() {
        Some(id) => id,
        None => {
            // Notification — no response.
            return None;
        }
    };

    let response = match req.method.as_str() {
        "initialize" => ok(
            id,
            json!({
                "protocolVersion": PROTOCOL_VERSION,
                "capabilities": { "tools": {} },
                "serverInfo": {
                    "name": SERVER_NAME,
                    "version": SERVER_VERSION
                }
            }),
        ),
        "tools/list" => ok(id, json!({ "tools": tool_descriptors() })),
        "tools/call" => match call_tool(req.params, ast_index_bin, default_root) {
            Ok(content) => ok(
                id,
                json!({
                    "content": [ { "type": "text", "text": content } ],
                    "isError": false
                }),
            ),
            Err(e) => ok(
                id,
                json!({
                    "content": [ { "type": "text", "text": format!("ast-index-mcp error: {e}") } ],
                    "isError": true
                }),
            ),
        },
        "ping" => ok(id, json!({})),
        "shutdown" => ok(id, json!({})),
        other => err(id, -32601, format!("method not found: {other}")),
    };

    Some(response)
}

fn tool_descriptors() -> Vec<Value> {
    vec![
        json!({
            "name": "search",
            "description": "Universal code search across file paths, symbol definitions, imports/usages, and file contents. Use this FIRST for any 'find X in the codebase' question — it returns files, matching symbols (classes, functions, etc.), and content matches in one call. Prefer this over grep.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query":        { "type": "string", "description": "Search query. Comma-separated for OR: 'email,mail'." },
                    "limit":        { "type": "integer", "description": "Max results per category (default 50)." },
                    "kind":         { "type": "string",  "description": "Filter symbols by kind: class, interface, function, method, struct, enum, etc." },
                    "in_file":      { "type": "string",  "description": "Restrict to files whose path contains this substring." },
                    "module":       { "type": "string",  "description": "Restrict to files whose path starts with this prefix." },
                    "fuzzy":        { "type": "boolean", "description": "Enable typo-tolerant fuzzy matching." },
                    "project_root": { "type": "string",  "description": "Absolute path to project root. Optional if the server was started with --root or AST_INDEX_ROOT." },
                    "format":       { "type": "string",  "enum": ["text", "json"], "description": "Output format. Default 'text' (compact, token-efficient). Pass 'json' only if you need structured parsing — costs ~2-3× more tokens." }
                },
                "required": ["query"]
            }
        }),
        json!({
            "name": "outline",
            "description": "Extract the structural outline (classes, functions, methods with line numbers) of a single source file. ALWAYS call this BEFORE reading a file larger than 500 lines — then read only the targeted slice by offset/limit instead of the whole file.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "file":         { "type": "string", "description": "Path to the source file (relative to project root or absolute)." },
                    "project_root": { "type": "string", "description": "Absolute path to project root. Optional." },
                    "format":       { "type": "string", "enum": ["text", "json"], "description": "Output format. Default 'text' (compact)." }
                },
                "required": ["file"]
            }
        }),
        json!({
            "name": "usages",
            "description": "Find every usage (call site, import, downcast, DI registration) of a symbol anywhere in the indexed codebase. Use this when the question is 'who uses X' / 'where is X called from'. Returns file:line + surrounding context.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol":       { "type": "string",  "description": "Symbol name (class, function, method)." },
                    "limit":        { "type": "integer", "description": "Max results (default 50)." },
                    "in_file":      { "type": "string",  "description": "Restrict to files whose path contains this substring." },
                    "module":       { "type": "string",  "description": "Restrict to files whose path starts with this prefix." },
                    "project_root": { "type": "string",  "description": "Absolute path to project root. Optional." },
                    "format":       { "type": "string",  "enum": ["text", "json"], "description": "Output format. Default 'text' (compact, token-efficient). Pass 'json' only if you need structured parsing — costs ~2-3× more tokens." }
                },
                "required": ["symbol"]
            }
        }),
        json!({
            "name": "callers",
            "description": "Find every function that calls the given function, one level up. Use for 'who calls processPayment' questions. For the full transitive caller tree, call this repeatedly or use `search` with deeper queries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "function":     { "type": "string",  "description": "Function or method name." },
                    "limit":        { "type": "integer", "description": "Max results (default 50)." },
                    "project_root": { "type": "string",  "description": "Absolute path to project root. Optional." },
                    "format":       { "type": "string",  "enum": ["text", "json"], "description": "Output format. Default 'text' (compact, token-efficient). Pass 'json' only if you need structured parsing — costs ~2-3× more tokens." }
                },
                "required": ["function"]
            }
        }),
        json!({
            "name": "implementations",
            "description": "Find every class/struct/type that implements (Java/Kotlin/Swift/Scala) or extends (C++, Rust trait, etc.) the given interface, protocol, or abstract class. Use this for 'what implements PaymentProcessing' questions.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "parent":       { "type": "string",  "description": "Name of the interface, protocol, trait, or abstract class." },
                    "limit":        { "type": "integer", "description": "Max results (default 50)." },
                    "in_file":      { "type": "string",  "description": "Restrict to files whose path contains this substring." },
                    "module":       { "type": "string",  "description": "Restrict to files whose path starts with this prefix." },
                    "project_root": { "type": "string",  "description": "Absolute path to project root. Optional." },
                    "format":       { "type": "string",  "enum": ["text", "json"], "description": "Output format. Default 'text' (compact, token-efficient). Pass 'json' only if you need structured parsing — costs ~2-3× more tokens." }
                },
                "required": ["parent"]
            }
        }),
        json!({
            "name": "refs",
            "description": "Show cross-references for a symbol in one shot: every definition, every import, every usage. Use this when you want the complete picture in a single response, rather than calling `usages` / `callers` separately.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "symbol":       { "type": "string",  "description": "Symbol name." },
                    "limit":        { "type": "integer", "description": "Max results per category (default 50)." },
                    "project_root": { "type": "string",  "description": "Absolute path to project root. Optional." },
                    "format":       { "type": "string",  "enum": ["text", "json"], "description": "Output format. Default 'text' (compact, token-efficient). Pass 'json' only if you need structured parsing — costs ~2-3× more tokens." }
                },
                "required": ["symbol"]
            }
        }),
        json!({
            "name": "rebuild",
            "description": "Rebuild the code index from scratch. Only needed on first setup or if `update` (incremental) is producing stale results. This can take minutes on large repositories — prefer `update` for everyday use (run it manually between sessions, NOT via this tool).",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_root": { "type": "string", "description": "Absolute path to project root. Optional." },
                    "format":       { "type": "string", "enum": ["text", "json"], "description": "Output format. Default 'text' (compact)." }
                }
            }
        }),
        json!({
            "name": "find_file",
            "description": "Find files in the indexed project by name pattern. Much cheaper than listing a directory tree when you only need a few matches (e.g. 'where is PaymentViewModel.kt').",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "pattern":      { "type": "string",  "description": "File name substring, or full name when 'exact' is true." },
                    "exact":        { "type": "boolean", "description": "Match the file name exactly instead of as a substring." },
                    "limit":        { "type": "integer", "description": "Max results (default 20)." },
                    "project_root": { "type": "string",  "description": "Absolute path to project root. Optional." }
                },
                "required": ["pattern"]
            }
        }),
        json!({
            "name": "stats",
            "description": "Show index statistics: detected project type, counts of files / symbols / refs / modules, DB size, extra roots. Call this to verify the index is populated and up-to-date before other queries.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_root": { "type": "string", "description": "Absolute path to project root. Optional." },
                    "format":       { "type": "string", "enum": ["text", "json"], "description": "Output format. Default 'text' (compact)." }
                }
            }
        }),
        json!({
            "name": "update",
            "description": "Incrementally update the code index — reindex only changed and deleted files since the last run. Fast (seconds) even on large repos. Call this instead of `rebuild` whenever you suspect the index is slightly stale.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "project_root": { "type": "string", "description": "Absolute path to project root. Optional." }
                }
            }
        }),
    ]
}

fn call_tool(
    params: Value,
    ast_index_bin: &str,
    default_root: &PathBuf,
) -> Result<String> {
    let name = params
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow!("missing 'name' in tools/call params"))?;
    let arguments = params.get("arguments").cloned().unwrap_or(json!({}));

    let resolved_root = arguments
        .get("project_root")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .unwrap_or_else(|| default_root.clone());

    // Default output format is compact text (token-efficient). Agents can
    // request raw JSON via `format: "json"` when they need structured
    // parsing — cost is ~2-3× more tokens.
    let output_format = arguments
        .get("format")
        .and_then(Value::as_str)
        .unwrap_or("text");

    // Whether the underlying ast-index command honours `--format json`.
    // Commands not in this set print plain text regardless of the flag, so
    // we avoid passing it to keep the argv honest.
    let supports_json_format = matches!(
        name,
        "search" | "usages" | "implementations" | "refs" | "stats"
    );

    let mut argv: Vec<String> = Vec::new();
    match name {
        "search" => {
            argv.push("search".into());
            argv.push(require_string(&arguments, "query")?);
            push_if_num(&mut argv, &arguments, "limit", "--limit");
            push_if_str(&mut argv, &arguments, "kind", "--type");
            push_if_str(&mut argv, &arguments, "in_file", "--in-file");
            push_if_str(&mut argv, &arguments, "module", "--module");
            if arguments.get("fuzzy").and_then(Value::as_bool).unwrap_or(false) {
                argv.push("--fuzzy".into());
            }
        }
        "outline" => {
            argv.push("outline".into());
            argv.push(require_string(&arguments, "file")?);
        }
        "usages" => {
            argv.push("usages".into());
            argv.push(require_string(&arguments, "symbol")?);
            push_if_num(&mut argv, &arguments, "limit", "--limit");
            push_if_str(&mut argv, &arguments, "in_file", "--in-file");
            push_if_str(&mut argv, &arguments, "module", "--module");
        }
        "callers" => {
            argv.push("callers".into());
            argv.push(require_string(&arguments, "function")?);
            push_if_num(&mut argv, &arguments, "limit", "--limit");
        }
        "implementations" => {
            argv.push("implementations".into());
            argv.push(require_string(&arguments, "parent")?);
            push_if_num(&mut argv, &arguments, "limit", "--limit");
            push_if_str(&mut argv, &arguments, "in_file", "--in-file");
            push_if_str(&mut argv, &arguments, "module", "--module");
        }
        "refs" => {
            argv.push("refs".into());
            argv.push(require_string(&arguments, "symbol")?);
            push_if_num(&mut argv, &arguments, "limit", "--limit");
        }
        "rebuild" => {
            argv.push("rebuild".into());
        }
        "find_file" => {
            argv.push("file".into());
            argv.push(require_string(&arguments, "pattern")?);
            if arguments.get("exact").and_then(Value::as_bool).unwrap_or(false) {
                argv.push("--exact".into());
            }
            push_if_num(&mut argv, &arguments, "limit", "--limit");
        }
        "stats" => {
            argv.push("stats".into());
        }
        "update" => {
            argv.push("update".into());
        }
        other => return Err(anyhow!("unknown tool: {other}")),
    }

    if supports_json_format {
        argv.push("--format".into());
        argv.push("json".into());
    }

    let output = Command::new(ast_index_bin)
        .args(&argv)
        .current_dir(&resolved_root)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .with_context(|| format!("failed to spawn {ast_index_bin} — is it on PATH?"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow!(
            "ast-index exited with {}: {stderr}",
            output.status
        ));
    }

    let stdout = String::from_utf8(output.stdout)
        .context("ast-index produced non-UTF8 output")?;

    let rendered = match output_format {
        "json" => stdout, // caller asked for raw JSON — pass through
        _ => format::to_compact(name, &stdout),
    };
    Ok(rendered)
}

fn require_string(args: &Value, key: &str) -> Result<String> {
    args.get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| anyhow!("missing required argument '{key}'"))
}

fn push_if_str(argv: &mut Vec<String>, args: &Value, key: &str, flag: &str) {
    if let Some(s) = args.get(key).and_then(Value::as_str) {
        argv.push(flag.into());
        argv.push(s.into());
    }
}

fn push_if_num(argv: &mut Vec<String>, args: &Value, key: &str, flag: &str) {
    if let Some(n) = args.get(key).and_then(|v| v.as_i64().or_else(|| v.as_f64().map(|f| f as i64))) {
        argv.push(flag.into());
        argv.push(n.to_string());
    }
}
