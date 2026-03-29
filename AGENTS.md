# Repository Guidelines

## Project Structure & Module Organization
This crate implements a GitBucket MCP server over stdio. Keep GitBucket REST access in `src/api/`, MCP tool definitions in `src/tools/`, and shared request/response types in `src/models/`. Runtime wiring belongs in `src/main.rs`, `src/server.rs`, `src/config.rs`, and `src/error.rs`. Current tool coverage is grouped by repository, issue, pull request, and user features; new functionality should follow the same module split. `tests/common/mod.rs` contains shared `wiremock` helpers, and `README.md` should be updated whenever setup, configuration, or exposed tools change.

## Build, Test, and Development Commands
Use Cargo for the full workflow:

- `cargo build` builds the debug binary.
- `cargo build --release` produces `target/release/gitbucket-mcp-server`.
- `cargo test` runs unit and integration tests.
- `cargo fmt --all` formats the workspace.
- `cargo clippy --all-targets --all-features -- -D warnings` treats lint warnings as errors.
- `cargo run` starts the server over stdio; configure it with `GITBUCKET_URL` and `GITBUCKET_TOKEN` or `~/.config/gitbucket-mcp-server/config.toml`.

## Coding Style & Naming Conventions
Follow Rust 2021 conventions: 4-space indentation, `snake_case` for functions and modules, and `PascalCase` for structs and enums. Use names that mirror GitBucket API concepts such as `PullRequest`, `CreateRepository`, or `list_branches`. Keep transport concerns in the MCP server/tool layer and HTTP details in the API client layer. Run `cargo fmt` before submitting and clear all `clippy` warnings.

## Testing Guidelines
Integration tests use `#[tokio::test]` plus `wiremock` to simulate GitBucket endpoints. Add coverage near `tests/api_client_test.rs`; if the suite grows, split by domain using names consistent with the plan, such as `tool_repository_test.rs` or `tool_issue_test.rs`. Name tests after observable behavior, for example `test_list_repositories_fallback_to_org`. Cover both success paths and API/config error cases.

## Commit & Pull Request Guidelines
Recent commits use short imperative subjects such as `Add integration tests with wiremock`. Keep that style and limit each commit to one logical change. Pull requests should explain the user-visible impact, list test coverage (`cargo test`, `cargo clippy`, `cargo fmt`), link related issues, and call out config or protocol changes. Include README updates when adding or renaming tools.

## Security & Configuration Tips
Never commit real tokens or instance URLs. Configuration priority is environment variables first, then `~/.config/gitbucket-mcp-server/config.toml`; preserve that behavior and the current `0600` file permissions when changing config code. Use `GITBUCKET_MCP_CONFIG_DIR` only for local overrides and tests.
