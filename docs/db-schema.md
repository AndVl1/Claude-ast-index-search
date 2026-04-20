# Index database schema

The ast-index stores every project's code graph in a SQLite database under
`~/Library/Caches/ast-index/<hash>/index.db` (macOS) or
`$XDG_CACHE_HOME/ast-index/<hash>/index.db` (Linux). Where `<hash>` is a
stable hash of the canonicalised project root.

Schema lives in `src/db.rs` (`init_db`). 14 tables, adjacency-list shape.

## ER overview

```
CORE — always populated
═══════════════════════════════════════════════════════════════════════════

 ┌─────────────────────┐
 │ files               │◄─────────┬─────────────────────────────────┐
 ├─────────────────────┤          │                                 │
 │ id        PK        │          │                                 │
 │ path      TEXT NN   │          │                                 │
 │ mtime     INT  NN   │          │                                 │
 │ size      INT  NN   │          │                                 │
 └─────────────────────┘          │                                 │
                                  │ file_id                         │ file_id
                                  │                                 │
 ┌─────────────────────┐          │      ┌──────────────────────┐   │
 │ symbols             │          │      │ refs                 │   │
 ├─────────────────────┤          │      ├──────────────────────┤   │
 │ id          PK      │──────────┘      │ id           PK      │   │
 │ file_id     FK→fil. │                 │ file_id      FK→fil. │───┘
 │ name        TEXT NN │                 │ name         TEXT NN │
 │ kind        TEXT NN │                 │ line         INT  NN │
 │ line        INT  NN │                 │ context      TEXT    │
 │ parent_id   FK→sym. │◄───┐            │                      │
 │ signature   TEXT    │    │            │ (import / usage site)│
 └─────────────────────┘    │            └──────────────────────┘
                            │ containment
                            │ (self-ref: method → enclosing class)
                            │
 ┌─────────────────────┐    │
 │ inheritance         │    │
 ├─────────────────────┤    │
 │ id          PK      │    │
 │ child_id    FK→sym. │────┼── extends / implements (plural)
 │ parent_name TEXT    │    │    ← TEXT by design, NOT a FK —
 │ kind        TEXT    │    │      supertype may be outside index
 └─────────────────────┘    │

       *** Two distinct "parent" concepts — do not conflate ***
       symbols.parent_id   = CONTAINMENT   (enclosing scope)
       inheritance.*       = SUPERTYPE     (extends / implements)
```

```
MODULE-LEVEL — populated for projects with a module system
═══════════════════════════════════════════════════════════════════════════

 ┌─────────────────┐         ┌─────────────────────┐
 │ modules         │◄────────│ module_deps         │
 ├─────────────────┤  m_id   ├─────────────────────┤
 │ id       PK     │◄────────│ module_id     FK    │
 │ name     TEXT NN│  dep_id │ dep_module_id FK    │
 │ path     TEXT NN│◄────────│ dep_kind      TEXT  │  ← impl / api / test
 │ kind     TEXT   │         └─────────────────────┘
 └─────────────────┘
        ▲
        │ module_id / dep_id
        │
 ┌─────────────────────┐
 │ transitive_deps     │       ← precomputed reachability:
 ├─────────────────────┤         module → every transitive dep
 │ id          PK      │         with depth and full path chain
 │ module_id   FK      │
 │ dependency_id FK    │
 │ depth       INT  NN │
 │ path        TEXT    │
 └─────────────────────┘
```

```
ANDROID — populated when ProjectType == Android
═══════════════════════════════════════════════════════════════════════════

 modules ◄── resources ◄── resource_usages
              ├ type, name, file_path, line
              └ (@drawable/ic_foo, R.string.x, etc.)

 xml_usages
 ├ module_id, file_path, line
 └ class_name, usage_type, element_id
   (class mentions in XML layouts)
```

```
IOS — populated when ProjectType == iOS
═══════════════════════════════════════════════════════════════════════════

 modules ◄── ios_assets ◄── ios_asset_usages
              ├ type, name, file_path
              └ (UIImage(named:), SwiftUI Image, …)

 storyboard_usages
 ├ module_id, file_path, line
 └ class_name, usage_type, storyboard_id
   (ViewController classes in .storyboard / .xib)
```

```
META
═══════════════════════════════════════════════════════════════════════════

 metadata  ← KV-store
 ├ key   TEXT PK
 └ value TEXT NN
   project_root, no_ignore, extra_roots, schema_version, …
```

## Table-by-table

### `files` (the file registry)

Every source file discovered during `rebuild` / `update`. `path` is
relative to the root that owns the file — primary root or an extra root
added via `add-root`. `mtime` and `size` are cached so `update` can skip
unchanged files without re-parsing.

### `symbols`

The main symbol table. One row per named declaration (class, function,
method, struct, interface, enum, constant, variable, type alias, etc.).

- `kind` is a short string tag (`class`, `function`, `method`, `import`,
  `constant`, …). The full vocabulary is `SymbolKind` in `src/db.rs`.
- `parent_id` references another row in the same table — this is
  **containment** (method inside class, inner class inside outer class,
  function inside module scope). Nullable because top-level declarations
  have no enclosing symbol.
- `signature` is the full type signature as a string for display
  (`fn foo(x: i32) -> String`). Nullable because not every parser
  produces signatures.

### `refs` (references / usages)

Every non-definition mention of a name — imports, function-call sites,
type references. Stored loosely:

- `name` is a free-form identifier string, **not** an FK to `symbols`.
- `context` is the line of source where the reference appears, for
  grep-like preview.

The "loose" design means refs aren't resolved to specific symbol IDs —
just "this name appears at this file:line". This keeps indexing fast
and language-agnostic, at the cost of not supporting semantic
jump-to-definition. Commands that need resolution (`refs`, `usages`)
join on `name` and accept that multiple symbols with the same name
will all match.

### `inheritance`

One row per `extends` / `implements` / `mixin` relation.

- `child_id` FK → `symbols.id` — the subclass / implementor.
- `parent_name` — name of the supertype as **TEXT, not an FK**. A
  supertype frequently lives outside the indexed code (stdlib,
  third-party, generated code) so we can't always resolve it to an ID.
  Queries do fuzzy suffix matching: `parent_name = "Foo"` OR
  `parent_name LIKE "%.Foo"` to catch both short names and fully
  qualified ones.
- `kind` distinguishes direct extension, interface implementation,
  mixin, trait impl, etc.

### `modules`, `module_deps`, `transitive_deps`

Project-specific module concept — Gradle subproject, Cargo crate,
Python package, Go module, etc. `module_deps` is direct edges;
`transitive_deps` is precomputed closure with `depth` and `path`
string for quick "all indirect deps of X" queries.

### Android (`resources`, `resource_usages`, `xml_usages`)

Populated only for Android projects. `resources` catalogues
`R.string.*`, `R.drawable.*` etc. declarations; `resource_usages` is
where they're referenced; `xml_usages` tracks fully-qualified class
names mentioned in layout XML.

### iOS (`ios_assets`, `ios_asset_usages`, `storyboard_usages`)

Analogues for Swift/ObjC projects. `ios_assets` enumerates
`.xcassets` entries; `ios_asset_usages` catches `UIImage(named:)` /
SwiftUI `Image` references; `storyboard_usages` pins ViewController
classes to `.storyboard` / `.xib` files.

### `metadata`

Plain KV table for DB-level config that shouldn't live in a schema
column: `project_root` (canonical path for cache-lookup validation),
`no_ignore` (whether `.gitignore` was bypassed at last rebuild),
`extra_roots` (JSON list of additional source roots added via
`add-root`), `schema_version` (for future migrations).

## Design decisions worth knowing

### Adjacency list, not materialized path

Both hierarchies (containment via `parent_id`, inheritance via
`inheritance.*`) store **only the immediate parent**, not the full
chain. To walk ancestors, use a recursive CTE:

```sql
WITH RECURSIVE ancestors AS (
    SELECT id, name, kind, parent_id, 0 AS depth
    FROM symbols WHERE id = ?1
  UNION ALL
    SELECT s.id, s.name, s.kind, s.parent_id, a.depth + 1
    FROM symbols s
    JOIN ancestors a ON s.id = a.parent_id
)
SELECT * FROM ancestors ORDER BY depth;
```

SQLite recursive CTEs are fast on realistic depths (5–10 levels) —
milliseconds. The trade-off is cheap writes (insert one edge, no
cascade on rename) at the cost of recursive reads.

### `refs` is string-based, not ID-based

`refs.name` is TEXT, not a FK to `symbols`. Consequence: commands
searching references (`usages`, `refs`, `callers`) match purely by
name. This:
- avoids brittle name-resolution during indexing (tree-sitter doesn't
  do scope resolution);
- keeps indexing fast and language-agnostic;
- means multiple definitions with the same name all match a single
  query — use `--in-file` / `--module` scope filters to narrow.

### Extra roots

When `add-root` is used, additional source trees are indexed alongside
the primary. Paths in `files.path` and `symbols.*` stay relative to
**the root that owns the file**. Query output is resolved to absolute
paths through `commands::PathResolver` when extra roots are present,
so output is never ambiguous about which root a result came from.

### Indexes

Indexes are created in `init_db` but not shown by `ast-index schema`
(which lists only table schemas). To see them:

```bash
sqlite3 $(ast-index db-path) ".indexes"
sqlite3 $(ast-index db-path) ".schema"      # full DDL including indexes
```

Notable ones:
- `idx_symbols_name`, `idx_symbols_name_lower` — fast symbol lookup
- `idx_symbols_file`, `idx_symbols_parent` — join paths
- `idx_refs_file_name`, `idx_refs_name` — reference queries
- `idx_inheritance_parent` — `implementations` / `hierarchy` lookups
- `symbols_fts` (FTS5 virtual table) — fuzzy full-text search

## Common query patterns

Full qualified name (walk containment chain):

```sql
WITH RECURSIVE ancestors AS (
    SELECT id, name, parent_id, 0 AS depth FROM symbols WHERE id = ?1
  UNION ALL
    SELECT s.id, s.name, s.parent_id, a.depth + 1
    FROM symbols s JOIN ancestors a ON s.id = a.parent_id
)
SELECT GROUP_CONCAT(name, '.') FROM (
    SELECT name FROM ancestors ORDER BY depth DESC
);
-- → "MyApp.billing.PaymentService.processPayment"
```

All supertypes of a class (walk inheritance chain):

```sql
WITH RECURSIVE supers AS (
    SELECT parent_name AS name, 1 AS depth FROM inheritance WHERE child_id = ?1
  UNION ALL
    SELECT i.parent_name, s.depth + 1
    FROM inheritance i
    JOIN symbols ss ON ss.name = s.name
    JOIN supers   s ON 1 = 1
    WHERE i.child_id = ss.id
)
SELECT DISTINCT name, depth FROM supers ORDER BY depth;
```

Implementations of an interface:

```sql
SELECT s.name, f.path, s.line
FROM inheritance i
JOIN symbols s ON s.id = i.child_id
JOIN files f ON f.id = s.file_id
WHERE i.parent_name = ?1
   OR i.parent_name LIKE '%.' || ?1;
```

Symbols inside a specific file:

```sql
SELECT s.name, s.kind, s.line
FROM symbols s
JOIN files f ON f.id = s.file_id
WHERE f.path = ?1
ORDER BY s.line;
```

## Inspecting the live database

```bash
ast-index db-path       # path to the .db file
ast-index schema        # JSON dump of all tables + row counts
ast-index query "SELECT * FROM symbols WHERE name = ?1 LIMIT 20" foo

# Raw SQLite
sqlite3 $(ast-index db-path) ".tables"
sqlite3 $(ast-index db-path) "SELECT kind, COUNT(*) FROM symbols GROUP BY kind ORDER BY 2 DESC"
```

The `query` command has an allowlist — only `SELECT`/`WITH`/`EXPLAIN`
are permitted; `INSERT`/`UPDATE`/`DELETE`/`DROP`/`PRAGMA` are rejected
to prevent accidental damage. Use raw `sqlite3` for mutations.
