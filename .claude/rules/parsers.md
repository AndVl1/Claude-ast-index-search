# Parsers

ast-index supports a language if `parsers::parse_file_symbols(content,
file_type)` returns symbols for it. The preferred path is tree-sitter; a
regex fallback is the exception (Perl, WSDL/XSD, Vue/Svelte script blocks).

## Adding a tree-sitter parser — checklist

1. Add the grammar crate to `Cargo.toml`:
   `tree-sitter-<lang> = "0.x"`.
2. Create `src/parsers/treesitter/<lang>.rs` (see template below).
3. Drop the S-expression query into
   `src/parsers/treesitter/queries/<lang>.scm` — tracked in git, loaded via
   `include_str!`.
4. Register the module in `src/parsers/treesitter/mod.rs`:
   - `pub mod <lang>;`
   - `FileType::<Name> => Some(&<lang>::<NAME>_PARSER),` inside
     `get_treesitter_parser`.
5. Extend `FileType` in `src/parsers/mod.rs` and map extensions in
   `FileType::from_extension`.
6. Add project-type detection in `indexer::detect_project_type` if the
   language implies a distinct project kind (optional).
7. Update `README.md`'s "Supported Projects" table.
8. Add parser tests under `tests/<lang>_tests.rs` (real code snippets → expected symbols).

## File template

```rust
//! Tree-sitter based <Lang> parser.

use anyhow::Result;
use tree_sitter::{Language, Query, QueryCursor, StreamingIterator};
use std::sync::LazyLock;

use crate::db::SymbolKind;
use crate::parsers::ParsedSymbol;
use super::{LanguageParser, parse_tree, node_text, node_line};

static LANG: LazyLock<Language> = LazyLock::new(|| tree_sitter_<lang>::LANGUAGE.into());

static QUERY: LazyLock<Query> = LazyLock::new(|| {
    Query::new(&LANG, include_str!("queries/<lang>.scm"))
        .expect("Failed to compile <Lang> tree-sitter query")
});

pub static <LANG>_PARSER: <Lang>Parser = <Lang>Parser;

pub struct <Lang>Parser;

impl LanguageParser for <Lang>Parser {
    fn parse_symbols(&self, content: &str) -> Result<Vec<ParsedSymbol>> {
        let tree = parse_tree(content, &LANG)?;
        let mut symbols = Vec::new();
        let mut cursor = QueryCursor::new();
        let query = &*QUERY;

        // Map capture name → index once per call — queries have stable capture sets.
        let cap = |name: &str| query.capture_names().iter().position(|n| *n == name);
        let idx_class = cap("class_name");
        let idx_func  = cap("func_name");

        let mut matches = cursor.matches(query, tree.root_node(), content.as_bytes());
        while let Some(m) = matches.next() {
            // Extract named captures in a defined priority order; highest wins.
            // Push ParsedSymbol { name, kind, line, signature, … }.
        }

        Ok(symbols)
    }
}
```

The trait default impl for `extract_refs` is usually fine; override only
when the generic regex-based ref extraction misses language-specific
constructs (e.g. Kotlin extension receivers, Swift key paths).

## Good: `LazyLock` for grammar + query

`Query::new` parses the `.scm` string; it's expensive and allocation-heavy.
Do it once per process via `LazyLock`, not per file. `expect()` is the right
call here — a broken query is a compile-time invariant (ship-stopper), not a
runtime condition.

## Anti-patterns

- **Printing or logging from inside a parser.** Parsers are pure: content
  in, `(Vec<ParsedSymbol>, Vec<ParsedRef>)` out. Diagnostics belong in the
  caller (`indexer::index_directory`, or the specific command).
- **Allocating the `Language` / `Query` per call.** Always `static LazyLock`.
- **Swallowing tree-sitter errors.** Return `Err` (via `?`). The indexer
  logs per-file parse failures and moves on; if a parser silently returns
  `Vec::new()`, we lose the signal.
- **Using regex when tree-sitter is available.** The fallback exists for
  languages with no usable grammar. For everything else, add a new
  tree-sitter module even if the query starts small — it pays off.
- **Queries inline as string literals in `.rs`.** Keep them in
  `queries/<lang>.scm`; editors highlight them, diffs are readable, and
  `include_str!` keeps the binary self-contained.
