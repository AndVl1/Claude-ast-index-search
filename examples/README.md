# Examples

Sample configs and rule files for integrating ast-index into your project.
**None of the files here are drop-in defaults** — each one is a template
with a "What to change" section at the top. Read that, adapt, then copy
the result into your own project.

## `.claude/rules/ast-index.md`

Rules file for [Claude Code](https://claude.com/claude-code). Teaches the
agent to:

- Prefer ast-index over grep / bulk Read when searching code.
- Always `outline` a large file before `Read`-ing it.
- Pass the same instructions to any subagents it spawns (subagents don't
  inherit project rules automatically).

To use: open `examples/.claude/rules/ast-index.md`, follow the "What to
change before using this file" checklist at the top, and save the adapted
version to `.claude/rules/ast-index.md` in your own project. Claude Code
picks it up automatically on next session.
