# AGENTS.md

Guidance for coding agents working in this repository.

## Project Snapshot

- Language: Rust (edition 2024).
- Crate type: binary (`src/main.rs`), no `src/lib.rs` yet.
- Runtime target: Linux with `systemd` (see README and service code).
- CI workflow: `.github/workflows/build.yml` builds release artifacts for:
  - `x86_64-unknown-linux-gnu`
  - `x86_64-unknown-linux-musl`

## Rule Files Detected

- Checked `.cursor/rules/`: not present.
- Checked `.cursorrules`: not present.
- Checked `.github/copilot-instructions.md`: not present.
- Therefore, there are currently no Cursor/Copilot rule files to merge.
- If any of these files are added later, treat them as authoritative project-specific constraints.

## Build Commands

- Fast compile check:
  - `cargo check`
- Debug build:
  - `cargo build`
- Release build:
  - `cargo build --release`
- Release build for glibc target:
  - `cargo build --release --target x86_64-unknown-linux-gnu`
- Release build for musl target:
  - `cargo build --release --target x86_64-unknown-linux-musl`
- Run binary locally:
  - `cargo run -- --help`
  - `cargo run -- daemon`

Notes:
- Running the daemon or service-related flows may require elevated privileges and can touch `/etc` and `/var/log`.
- For musl builds, install system prerequisites first (`musl-tools` on Ubuntu/Debian).

## Format And Lint Commands

- Format code in place:
  - `cargo fmt --all`
- Check formatting in CI style:
  - `cargo fmt --all -- --check`
- Run clippy on normal targets:
  - `cargo clippy --all-targets --all-features`
- Strict clippy (recommended before merge):
  - `cargo clippy --all-targets --all-features -- -D warnings`

## Test Commands

Current state:
- No Rust tests were detected yet (`#[test]`, `#[cfg(test)]`, or `tests/` integration suite).
- Still use standard cargo test commands when adding tests.

Run all tests:
- `cargo test`

Run a single unit test (exact name):
- `cargo test module::tests::test_name -- --exact --nocapture`

Run a single integration test file:
- `cargo test --test integration_test_file`

Run a single test inside one integration file:
- `cargo test --test integration_test_file test_name -- --exact --nocapture`

List available tests:
- `cargo test -- --list`

Useful test workflow when adding new coverage:
- `cargo test <new_test_name> -- --exact --nocapture`
- `cargo test`
- `cargo clippy --all-targets --all-features -- -D warnings`

## Code Style Guidelines

### Formatting

- Use `rustfmt` defaults; do not hand-format against formatter output.
- Keep lines readable; prefer splitting long chains across lines.
- Prefer early returns to reduce nesting.

### Imports

- Group imports in this order when practical:
  1. `crate::...`
  2. external crates
  3. `std::...`
- Keep import lists minimal; remove unused items.
- Prefer explicit imports over wildcard/glob imports.

### Types And Data Modeling

- Use `struct` + `impl` for domain objects (existing pattern: `Config`, `GitRepo`, `RepoProcessor`).
- Use enums for finite state machines (existing pattern: `InputMode`, `PackageManager`).
- Derive traits intentionally (`Debug`, `Clone`, `Eq`, `PartialEq`, `Serialize`, `Deserialize`) as needed.
- Prefer concrete types unless generics improve call sites materially.

### Naming

- Types/traits/enums: `PascalCase`.
- Functions/methods/modules/variables: `snake_case`.
- Constants/statics: `UPPER_SNAKE_CASE`.
- Use descriptive names tied to behavior (`process_single`, `count_commits_behind`).

### Error Handling

- Prefer `Result<T, String>` in this codebase for operational flows.
- Add context via `map_err(|e| format!(...))` when wrapping IO/process errors.
- Return actionable messages; include path/command details when possible.
- Avoid `unwrap()` for recoverable runtime errors.
- Use `expect()` only when failure is truly unrecoverable and message is explicit.
- In loops/batch processing, collect and summarize per-item failures (existing `process_all` pattern).

### Logging And User Output

- Follow existing log style:
  - user-friendly Spanish messages
  - emoji severity markers (`✅`, `⚠️`, `❌`, `🚀`)
- Use professional, neutral Spanish for user-facing text; avoid slang and unnecessary Anglicisms.
- For daemon/service flows, prefer logging to both console and file via `Logger`.
- Keep logs concise but include enough context to diagnose failures.

### Process Execution

- Use `std::process::Command` with `current_dir(...)` for repo-scoped commands.
- Validate command exit status and surface stderr on failure.
- Avoid shell-dependent behavior when a direct executable call is possible.

### File System Safety

- Validate input paths before mutating (`exists`, `.git` presence, etc.).
- Be explicit with destructive operations (`remove_dir_all`) and guard them with checks.
- Preserve and set file permissions where required (existing unix permissions pattern).

### TUI Conventions

- Keep interaction states explicit via enums and small transition methods.
- On input flows, support clear cancel/submit behavior (`Esc`, `Enter`).
- Keep UI text consistent with current Spanish UX language.
- Prefer formal wording in labels, shortcuts, and status messages.

### Architecture And Organization

- Keep modules focused by responsibility:
  - config parsing/writing in `config.rs`
  - git operations in `git.rs`
  - orchestration in `processor.rs`
  - service install/uninstall in `service.rs`
  - UI in `tui.rs`
- Prefer adding helpers near the owning module over growing `main.rs` with unrelated logic.

## Contribution Checklist For Agents

- Run `cargo fmt --all`.
- Run `cargo clippy --all-targets --all-features -- -D warnings`.
- Run `cargo test` (or targeted test command if adding one test).
- Update README/inline docs when changing CLI behavior or config format.
- Do not introduce non-Linux assumptions unless explicitly requested.
