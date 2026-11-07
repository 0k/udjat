# udjat

Cargo workspace of the libraries that put every LLM coding harness
behind one Rust API (`udjat-core`'s `Harness` trait), plus the harness
box binary.

## Vocabulary

- **harness**: anything that runs or holds LLM sessions (opencode, pi,
  Codex, Claude Code, or a read-only archive of their session files).
  Over ACP it plays ACP's *agent* role; udjat plays the *client* role.
- **capability group**: a part of the `Harness` trait a harness may or
  may not implement (`read`, `events`, `status`, `run`,
  `interactions`, `models`). Callers check the declared groups; an
  archive declares `read` only.
- **connector**: how a harness is reached: in-process (a linked crate),
  HTTP to an opencode V1 compatible server, or ACP. Connectors are
  linked statically and chosen per harness by configuration.
- **upstream**: vigil-watch's word for a configured connection to a
  harness. Not used in udjat code.
- **agent**: a persona inside a harness (opencode's `build`, `plan`, …).
  Never call a harness an "agent" in code, docs or identifiers; write
  "harness" and, where ACP's role name is unavoidable, say "ACP agent
  role".

## Layout

- `Cargo.toml`: workspace; every external dependency is declared once
  in `[workspace.dependencies]` and used with `x.workspace = true`.
- `crates/udjat-core`: udjat's session model (`Session`, `Message`,
  `MessageRecord`, `MessageInfo`, typed `PartKind` and `ToolState`),
  lossless OpenCode V1 projections, `SessionRow` / `StructuralStatus`,
  the flat `TranscriptEntry` projection, cursor pages, `HarnessKind`,
  and the async `Harness` trait with its `Capability` groups. No I/O.
- `crates/udjat-archive-{claude-code,opencode,pi}`: in-process,
  read-only readers of each harness's session files and database, moved
  from ai-audit. Each implements `Harness` with the `read` group, and
  also exposes the synchronous reader API ai-audit calls directly.
  Their `rich` modules convert native storage into `MessageRecord`s;
  opt-in corpus tests compare the rich projection with the flat readers.
- `crates/udjat-connector-opencode-v1`: `Harness` over HTTP to an
  opencode V1 compatible server (opencode, or a server for pi sessions
  with the `pi` dialect). List filtering is sent to the server as `GET /session`
  parameters, never evaluated over fetched transcripts; transcripts
  are converted with the archive crates' own converters.
- `crates/udjat-fanout`: queries several harnesses in order, merges and
  deduplicates by session id (earlier harness wins), and merges the
  transcripts of a session held by several stores.
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
- `udjat-core`'s model is udjat's own, at least as detailed as the
  harness formats it is read from. Consumer output formats (ai-audit's
  lines and JSON, vigil-ui's wire) are projections of it, done by the
  consumer. ai-audit's `session list` and `session transcript` output
  must stay byte-identical to its pre-udjat output: a reader change
  that alters it is a regression, whatever the reason.
- Archive crates read harness files and databases strictly read-only
  (SQLite opened read-only / query-only); they never write, lock or
  migrate a harness's storage.
- Integration tests use real harnesses, never mocks of ACP. Every
  harness process a test starts runs in a cleared environment with a
  temporary database, home and XDG directories (one helper,
  `tests/support/mod.rs`). Never let a test reach a real installation's
  data: `opencode acp` starts a full opencode server on whatever
  database its environment selects.
- Before committing: `cargo fmt --check`,
  `cargo clippy --all-targets --all-features -- -D warnings`,
  `cargo test`.
