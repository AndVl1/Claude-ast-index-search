# Smoke Testing

`scripts/smoke.sh` is the CLI equivalent of a Playwright suite. It exercises
the **real release binary** end-to-end against synthetic projects and asserts
on stdout / stderr / exit code / SQLite state. Each scenario is independent
and runs in its own throwaway tempdir under `$TMPDIR/ast-index-smoke-$$/`.

## When to run

- After touching anything in `src/commands/`, `src/db.rs`, `src/indexer.rs`,
  or the MCP server (`crates/ast-index-mcp/`).
- Before cutting a release tag (regression net before `./scripts/bump.sh`).
- When debugging a "works on my machine" report — smoke isolates the binary
  from your real `~/Library/Caches/ast-index/` state.

Unit tests cover modules in isolation; smoke covers user flows.

## Running

```bash
bash scripts/smoke.sh
```

Builds release binaries if missing (`cargo build --release --workspace`),
then runs all scenarios. Exit `0` iff all scenarios pass.

Useful env vars:

| Var | Effect |
| --- | --- |
| `SMOKE_KEEP=1` | Don't delete the workdir on exit (logs + fixtures stay). |
| `SMOKE_BIN_DIR=path` | Use prebuilt binaries from `path` instead of `target/release`. |

## Scenarios

| Name | Asserts |
| --- | --- |
| `fresh-project` | `rebuild` detects Rust, indexes 2 files, `search` finds known symbol, `stats --format json` reports correct project + file count, sqlite has the symbol. |
| `incremental-update` | After `rebuild`, adding a `.rs` file + `update` makes the new symbol searchable while preserving existing ones. |
| `extra-roots` | After `add-root`, a search for a symbol defined under the extra root returns an **absolute** path (PathResolver regression — see `src/commands/mod.rs::PathResolver`). |
| `json-format` | `search --format json` and `stats --format json` produce parseable JSON with the expected top-level keys (`symbols`, `files`, `content_matches`, `references` / `project`, `stats`, `db_path`, `db_size_bytes`). |
| `mcp-stdio` | `ast-index-mcp` accepts `initialize` then `tools/call stats` over stdin and replies with well-formed JSON-RPC 2.0 envelopes (`id` matches, `serverInfo.name == "ast-index-mcp"`, `result.isError == false`). |
| `perf-budget` | `rebuild` of this repo's `src/` finishes under 30s; max latency over 5 `search --format json` calls under 500ms; no-op `update` under 1s. Tunable via `PERF_REBUILD_MS_MAX`, `PERF_SEARCH_MS_MAX`, `PERF_UPDATE_MS_MAX` env vars. Catches catastrophic regressions, not microbench drift. |

## Interpreting a failure

The runner prints `PASS` or `FAIL` per scenario and the last 20 log lines on
failure. The full per-scenario log (every command run, its output, its exit
code) lives at:

```
$TMPDIR/ast-index-smoke-$$/logs/<scenario>.log
```

Re-run with `SMOKE_KEEP=1 bash scripts/smoke.sh` to inspect both the logs
and the synthetic project after the run.

Common failure shapes:

- **`rebuild file count` mismatch** — the indexer skipped or duplicated a
  file. Check `is_excluded_dir` / project-type detection.
- **`extra-root path is absolute` failing** — the `PathResolver` regression
  is back. Check `src/commands/mod.rs::PathResolver::resolve` and
  `db::get_extra_roots`.
- **`mcp tools/call shape` failing** — the MCP server changed its response
  schema. Update both `crates/ast-index-mcp/src/main.rs` and the assertion.

## Adding a new scenario

1. Add a `scenario_<name>()` function in `scripts/smoke.sh`. It receives
   one arg — its workdir — and uses the helpers `run`, `assert_eq`,
   `assert_contains`, `assert_json_valid`, `assert_json_key`. Return non-zero
   to fail.
2. Append `<name>` to the `SCENARIOS=( ... )` array near the bottom of
   `smoke.sh`.
3. Re-run `bash scripts/smoke.sh` — your scenario should appear in the
   output and the total `N/N passed` should increase by one.

Keep each scenario hermetic: no shared state, no reliance on the order of
execution, no writes outside its own workdir (the cleanup trap calls
`ast-index clear` per scenario to avoid polluting `~/Library/Caches/`).

## After shipping a feature — refresh smoke

When you add a new user-visible command or change behaviour someone might
script against, the smoke suite must learn about it. Workflow:

1. **Identify the new flow.** Walk through how a user would invoke the
   feature: which command, which flags, what they expect on stdout, what
   the on-disk side-effects are (DB rows, files, exit code).
2. **Write a scenario** per "Adding a new scenario" above. The minimum bar
   is: one happy path + one failure mode (bad input, missing dependency).
3. **Update an existing scenario** if the feature changes an
   already-covered path. Don't add a duplicate next to it.
4. **Re-run everything**: `bash scripts/check-pr.sh` (see
   `scripts/check-pr.sh` — it runs build, full test suite, smoke, bench
   compile-check in one shot). All steps must pass before the feature
   ships.
5. **README** changelog entry mentions the new command — see
   `.claude/rules/release.md`.

If you can't write a smoke for the feature (it depends on an external
service, a macOS-specific path, etc.), say so explicitly in the PR
description. Untested user-visible behaviour is a known liability, not a
detail to gloss over.

## The PR gate — `scripts/check-pr.sh`

A single command that runs every gate:

```bash
bash scripts/check-pr.sh
```

It chains `cargo build --release --workspace`, `cargo test --release
--workspace`, `bash scripts/smoke.sh`, and `cargo bench --no-run`.
Combined summary at the end with per-step duration. Exits non-zero on
any failure. Use `RUN_BENCHES=1` to also actually run the benches in
criterion's `--quick` mode (otherwise just the compile-check runs).
