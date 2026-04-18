# Commands

Every CLI subcommand maps to one `cmd_<name>` function in
`src/commands/<topic>.rs`. Wiring lives in `src/main.rs`:
`Commands` enum → match arm → call `commands::<topic>::cmd_<name>(&root, …)`.

## Function signature

```rust
pub fn cmd_<name>(
    root: &Path,
    // command-specific args (owned `String` for user input, `Option<&str>`
    // for optional scalars, `&[String]` for repeatable flags, `usize` for
    // limits with a default)
    format: &str,           // only if the command supports JSON output
    scope: &SearchScope,    // only if the command is scoped by in-file/module
) -> Result<()>;
```

Rules of thumb:

1. First parameter is always `root: &Path`.
2. Return `Result<()>`; errors bubble up to `main` which prints them.
3. Print results to **stdout**. Errors, progress, timings go to **stderr**.
4. Accept `format: &str` only if the command emits JSON. Don't take it and ignore it.
5. Short-circuit on missing index: print a red hint and return `Ok(())`.

## Good: plain-text command with JSON branch

```rust
pub fn cmd_symbol(root: &Path, name: Option<&str>, /* … */ format: &str, /* … */) -> Result<()> {
    if !db::db_exists(root) {
        println!("{}", "Index not found. Run 'ast-index rebuild' first.".red());
        return Ok(());
    }

    let conn = db::open_db(root)?;
    let symbols = db::find_symbols(&conn, /* … */)?;

    if format == "json" {
        println!("{}", serde_json::to_string_pretty(&symbols)?);
        return Ok(());                       // JSON mode never prints colour.
    }

    println!("{}", format!("Symbols matching '{}':", name.unwrap_or("")).bold());
    for s in &symbols {
        println!("  {} [{}]: {}:{}", s.name.cyan(), s.kind, s.path, s.line);
    }
    if symbols.is_empty() {
        println!("  No symbols found.");
    }
    Ok(())
}
```

## Good: path resolution with extra roots

When a command prints any `path` column from the DB, it **must** resolve
through `PathResolver` before print, or the output is ambiguous under
`add-root`:

```rust
let resolver = PathResolver::from_conn(root, &conn);
for s in &mut symbols {
    s.path = resolver.resolve(&s.path);
}
```

`PathResolver::resolve` is a no-op when no extra roots are configured, so
single-root output stays byte-for-byte identical.

## Good: verbose-gated timing

```rust
pub fn cmd_update(root: &Path, verbose: bool) -> Result<()> {
    let start = Instant::now();
    /* … */
    if verbose {
        eprintln!("\n{}", format!("Time: {:?}", start.elapsed()).dimmed());
    }
    Ok(())
}
```

Default output stays quiet; power-users get timing on demand.

## Anti-patterns

- **Accepting `format: &str` but never checking it.** Agents will see the
  flag advertised and pass `--format json` expecting JSON; they'll get
  colourful text. Either honour it or drop the parameter.
- **Printing `Time:` without a `verbose` flag.** Timing output pollutes
  agent context. Gate it, or leave it out entirely.
- **Writing to stderr what the caller wants to parse.** Diagnostics only.
  If the MCP server or a shell pipeline needs it, it goes to stdout.
- **Calling `db::open_db` twice in one command.** Open once, pass `&conn`
  to helpers. SQLite connections are cheap but not free, and you lose
  transaction locality.
- **Hard-coding a `limit`.** Always surface it as a CLI arg with a sensible
  `#[arg(default_value = "50")]` — different integrations need different caps.

## Adding a new command (checklist)

1. Add `Commands::<Name> { … }` variant in `src/main.rs`.
2. Add match arm in the dispatch block further down that file.
3. Write `pub fn cmd_<name>(root: &Path, …) -> Result<()>` in the right
   `src/commands/<topic>.rs` (or create a new topic file and `pub mod` it
   from `commands/mod.rs`).
4. If it emits structured data, add a `format == "json"` branch.
5. If it prints any path from the DB, wire `PathResolver`.
6. If it has timing, gate it behind a per-command `verbose` flag.
7. Add a test under `tests/<name>_tests.rs` (see `.claude/rules/testing.md`).
8. Document the command in `README.md` under the matching section.
