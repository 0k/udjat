# udjat

Cargo workspace hosting LLM coding harnesses behind the Agent Client
Protocol (ACP).

## Vocabulary

- **harness**: a program that runs LLM sessions (opencode, pi, Codex,
  Claude Code). It plays ACP's *agent* role; `udjat-harness` plays the
  *client* role.
- **agent**: a persona inside a harness (opencode's `build`, `plan`, …).
  Never call a harness an "agent" in code, docs or identifiers; write
  "harness" and, where ACP's role name is unavoidable, say "ACP agent
  role".

## Layout

- `Cargo.toml`: workspace; every external dependency is declared once
  in `[workspace.dependencies]` and used with `x.workspace = true`.
- `crates/udjat-harness`: the harness box binary. `src/acp.rs` holds
  the ACP client logic; `src/cli/` follows the standard Rust CLI layout
  (`def.rs` clap definitions, `log.rs` kal-log adapter, `mod.rs`
  orchestration and output rendering).
- A crate shared between components (protocol types, identity kinds)
  is added only when a second component needs the types.

## Rules

- Dependencies are published crates.io versions or pinned git URLs.
  Never commit a `path =` dependency to anything outside this
  workspace.
- stdout carries command data only; diagnostics go through the `log`
  crate (kal-log) to stderr.
- Integration tests use real harnesses, never mocks of ACP. Every
  harness process a test starts runs in a cleared environment with a
  temporary database, home and XDG directories (one helper,
  `tests/support/mod.rs`). Never let a test reach a real installation's
  data: `opencode acp` starts a full opencode server on whatever
  database its environment selects.
- Before committing: `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test`.
