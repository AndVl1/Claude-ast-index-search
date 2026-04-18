# Verify before claiming done

"Done" means the code compiles and every test in the workspace passes on
the current commit. Nothing short of that. No "should work", no "probably
fine", no "the types line up" without a build.

## What to run, and when

Default commands for this repo (release profile — debug is too slow on
filesystem-heavy tests):

```bash
cargo build --release --workspace   # after any Rust edit
cargo test  --release --workspace   # after any functional change
```

When each step is required:

- **Edited any `.rs` file** → `cargo build --release` (scope to `--workspace`
  if your change touches `crates/ast-index-mcp`).
- **Changed behaviour, logic, or a public API** → `cargo test --release
  --workspace`.
- **Edited only `.md`, `.toml` comments, `README.md`, or
  `.claude/rules/**`** → no build/test needed, but a quick `git diff`
  review is.
- **Touched `Cargo.toml` dependencies** → `cargo build --release --workspace`
  (lockfile changes) and then `cargo test --release --workspace` (new deps
  can break compilation transitively).
- **Added a tree-sitter grammar or a `.scm` query** → test run is mandatory;
  tree-sitter query errors are compile-time, not runtime.

Scope a faster loop when iterating on one file:

```bash
cargo test --release -p ast-index path_resolver   # single integration file
cargo check --release                              # type-check only, ~5× faster than build
```

Use `cargo check` during rapid iteration, but **always** finish with a real
`cargo build --release` before claiming done — `check` skips codegen and
misses a class of errors (`monomorphization`, linker, `build.rs`).

## Green ≠ done

Passing tests don't prove feature correctness — they prove the tests you
chose pass. For anything user-visible:

- Run the binary on a real input. `./target/release/ast-index <subcommand>`
  against this repo itself is a quick smoke test (it's a Rust project, so
  ast-index indexes itself).
- If you changed MCP, send a sample JSON-RPC request over stdin and verify
  the response shape. See `docs/mcp-setup.md` for ready-to-paste commands.
- If you can't test the behaviour (missing fixtures, external service),
  say so explicitly in the summary. Do not claim success you didn't verify.

## Reporting results

When you report done, state facts, not feelings:

```
cargo build --release --workspace   → ok (46s)
cargo test  --release --workspace   → 600 passed, 0 failed
Ran  ./target/release/ast-index search Foo → returned 3 results as expected.
```

Not:

```
Should compile and work.
Changes look good.
```

## Anti-patterns

- **Claiming done without running anything.** If the harness shows zero
  tool calls to `cargo`, you haven't verified.
- **Running tests in debug mode** (plain `cargo test`) for FS-heavy
  integration tests. They're 3× slower and CI runs release — you'll miss
  timing-sensitive issues. Always `--release`.
- **Passing `--no-verify`, `--no-run`, or `--skip-ignored` to silence
  failures.** If a test is broken, fix it or mark it `#[ignore]` with a
  reason — don't hide it.
- **"Build passes" as proof of "feature works".** The Rust compiler
  confirms types, not intent. A function that returns `Ok(())` and does
  nothing compiles fine.
- **Running tests once, committing, then discovering a regression.** After
  the commit, re-run the suite at the tip — a rebase, an auto-format
  hook, or a stray unstaged hunk can change behaviour between commit and
  push. Two extra seconds of `cargo test` catches it.
