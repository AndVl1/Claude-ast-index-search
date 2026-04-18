# Benchmarks

Microbenchmarks live under `benches/` and use [`criterion`][criterion] (a
dev-dependency only — it is not linked into the release binary).

[criterion]: https://bheisler.github.io/criterion.rs/book/

## How to run

```bash
# Compile every bench without running (sanity check)
cargo bench --no-run

# Run all benches (takes minutes — collect a baseline before optimisation)
cargo bench

# Run one bench file
cargo bench --bench parser
cargo bench --bench db_query
cargo bench --bench index_build

# Quick mode (rough numbers in seconds, good for local dev)
cargo bench --bench parser -- --quick

# Filter to one benchmark inside a file
cargo bench --bench parser -- parse_file_symbols/rust
```

HTML reports land in `target/criterion/` after a full run.

## What each bench measures

### `parser`
Calls `parsers::parse_file_symbols` on representative real-world snippets
for **Rust**, **Kotlin** and **TypeScript**. The Rust input is pulled via
`include_str!` from `src/commands/files.rs` so it grows naturally with the
codebase; Kotlin and TypeScript are fixtures under `benches/fixtures/`.
Throughput is reported in bytes/sec to make cross-language comparison
meaningful.

Use this when changing tree-sitter parsers, the symbol extraction loop,
or the per-language reference filters.

### `db_query`
Indexes this very repo (the `src/` tree) into a `TempDir` SQLite **once**,
then benches:

* `db::find_files`            — `path LIKE '%pattern%'`
* `db::find_symbols_by_name`  — name lookup with optional kind hint
* `db::search_refs`           — refs aggregation by usage count

Each iteration is a single query against the shared, populated DB. Build
cost is **not** included in the per-iter timing.

Use this when adding indexes, tweaking SQL, or considering FTS5 changes.

### `index_build`
Generates a small synthetic project (~12 files across Rust / Kotlin /
TypeScript / Python / Go, a few hundred lines total) inside a `TempDir`,
then times `indexer::index_directory` from a cold SQLite. Sample size is
deliberately small (10) so total wall time stays bounded.

Use this when changing the walker, the parallel parse pool, or the DB
write batching.

## Interpreting regressions

Criterion reports a confidence interval and whether the change is
statistically significant. Rough rules of thumb:

* **< 5 %** drift across runs is noise on most laptops; ignore.
* **5–10 %** flagged as `Change within noise threshold` — re-run before
  reading too much into it.
* **> 10 %** with `Performance has regressed` and a tight CI — real.
  Bisect or profile (`cargo flamegraph --bench <name>`).

CPU thermals, background load and SMT siblings all matter. For comparable
numbers, run `cargo bench` with the laptop plugged in and nothing else
heavy running.

## Workflow: baseline before optimisation

Before starting optimisation work, capture a baseline so criterion can
diff against it:

```bash
# Save current numbers as the baseline named "before"
cargo bench -- --save-baseline before

# ... make changes ...

# Compare new run against the saved baseline
cargo bench -- --baseline before
```

Criterion will print per-bench deltas with significance markers. Commit
the optimisation only if the bench you targeted moved and unrelated
benches did not regress.

## Determinism

All inputs are either embedded via `include_str!` or generated
deterministically from constants — no network, no random data, no
dependence on wall-clock state beyond criterion's own timing. The
`db_query` bench depends on the contents of `src/` at build time; if
that tree changes substantially, treat the new numbers as a fresh
baseline rather than a regression.
