# Architecture

## Data flow (single request)

```
clap parse  →  Commands::Variant  →  commands::module::cmd_fn(root, args, format)
                                        │
                                        ├──→ db::open_db(root)          # SQLite at ~/Library/Caches/ast-index/<hash>/index.db
                                        ├──→ db::find_* / search_*      # read queries
                                        ├──→ PathResolver.resolve()     # absolute paths if extra roots configured
                                        └──→ println!  or  serde_json::to_string_pretty
```

For `rebuild` / `update` the flow reverses: `indexer::index_directory` walks
the filesystem (honouring `.gitignore` unless `no_ignore`), calls
`parsers::parse_file_symbols` per file, batches `INSERT`s into `files`,
`symbols`, and `refs` tables. Extra roots are walked with a relative-path
scheme anchored to each root, not the primary.

## Module responsibilities (one line each)

- **`main.rs`** — clap CLI, global `--format`, `Commands` enum, dispatch.
- **`lib.rs`** — crate root; only re-exports `db`, `indexer`, `parsers`, `commands`.
- **`db.rs`** — schema, migrations, all SQL. Commands never write raw SQL outside this module.
- **`indexer.rs`** — filesystem walk, project-type detection, incremental-update bookkeeping, writes via `db::*`.
- **`commands/mod.rs`** — cross-cutting helpers (`PathResolver`, `search_files`, `relative_path`, `is_no_ignore_enabled`).
- **`commands/<topic>.rs`** — one file per topic; each exports several `cmd_*` functions.
- **`parsers/mod.rs`** — `FileType` enum, `parse_file_symbols` entry point, regex fallbacks.
- **`parsers/treesitter/mod.rs`** — `LanguageParser` trait, language dispatch.
- **`parsers/treesitter/<lang>.rs`** — one static parser per language, loads `queries/<lang>.scm`.

## Cache / DB location

```
~/Library/Caches/ast-index/<hash>/index.db   # macOS
$XDG_CACHE_HOME/ast-index/<hash>/index.db    # Linux
```

`<hash>` is a stable hash of the canonical project root path. Two different
projects never collide; one project's cache is stable across rebuilds.

## Crate layout

- **`ast-index`** (root) — library (`src/lib.rs`) + binary (`src/main.rs`).
- **`crates/ast-index-mcp`** — thin JSON-RPC server, spawns the `ast-index`
  binary per tool call. No dependency on the `ast-index` library crate, so
  the MCP server stays small (~500 KB) and users can upgrade the indexer
  independently.

## Anti-patterns

- **Absolute paths in the DB** — always store paths relative to the owning
  root. Store-and-resolve, don't store-and-hope.
- **Raw SQL in `commands/*.rs`** — add a typed helper in `db.rs` instead.
  Keeps schema churn contained.
- **Cross-topic calls between command modules** — `commands::ios::*`
  should not call `commands::android::*`. Shared logic belongs in
  `commands/mod.rs` or `db.rs`.
- **Parser that writes to stdout** — parsers return `Vec<ParsedSymbol>`
  and `Vec<ParsedRef>`; printing is the command's job.
