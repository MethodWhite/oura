# Contributing to Oura

## Standards

### Commit messages — Conventional Commits

```
<type>(<scope>): <description>

feat: add oura_working_dir tool
fix(mcp): prevent thread leak on multiple start()
refactor: extract collect_feedback_static helper
ci: use Swatinem/rust-cache instead of raw cache action
docs: add PR template with database migration checklist
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `ci`, `config`, `perf`, `style`, `chore`.
Optional scope: `(mcp)`, `(engine)`, `(config)`, `(github)`.
Breaking changes: append `!` after type/scope, e.g. `feat!: remove deprecated API`.

### Tool descriptions — must follow format

```json
{
  "name": "oura_<verb>_<noun>",
  "description": "<verb> <direct object> [optional context].",
  "inputSchema": {
    "type": "object",
    "properties": {
      "param": { "type": "string", "description": "What it does" }
    },
    "required": ["param"]
  }
}
```

- Descriptions are sentence fragments starting with a verb: `Start a new...`, `Get current...`, `Scan and clean...`
- Period at the end of each description.
- All params have `type`, `description`, and `default` where applicable.
- Param descriptions are lowercase, no period.

### Output format

```
<Section>
- Key: Value
- List: item
---
<Next section>
```

- No emojis — use `[+]`, `[*]`, text markers instead.
- Errors go in `error` field, never `result.content`.
- Success in `result.content` with `type: "text"`.
- No `{:?}` debug format in user-facing output.

### Code style

- No `// comments` unless explaining *why* (never *what*).
- `eprintln!` only for errors. Info logs use `QUIET=1` env var.
- All Mutex locks: `lock().unwrap_or_else(|e| e.into_inner())` (poison-safe).
- All `std::fs::read_dir`: skip symlinks, track inodes to prevent loops.

## Pull request lifecycle

```
1. Open PR with conventional commit title
2. Labeler runs → auto-adds type + size labels
3. CI runs → check → fmt → clippy → test (3 OS) → msrv → security
4. PR Review runs → analyze changes → check conventions
5. Auto-approve fires IF:
   - All CI checks pass
   - PR < 500 additions, < 20 files
   - No `breaking` or `blocked` label
   - Not a draft
6. Maintainer merges (squash recommended)
```

## Development

```bash
cargo build --release
cargo test
OURA_QUIET=1 cargo run

# Test MCP protocol:
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | ./target/release/oura
```

## Release

1. Release drafter maintains changelog from PR labels.
2. Tag `vX.Y.Z` (semver) → CI builds 5 targets → publishes GitHub Release.
3. Release notes include auto-generated changelog.
