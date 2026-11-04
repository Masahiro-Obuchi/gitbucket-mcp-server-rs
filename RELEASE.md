# Release Process

This project publishes GitHub Releases from version tags. The release workflow builds platform archives, attaches checksums, and publishes the GitHub Release.

## Unreleased P07 input and model changes

- All MCP tools reject unknown arguments before HTTP. Clients must remove
  unsupported fields, including inline locations on ordinary comment tools.
- Bodies/comments, descriptions and merge messages retain original whitespace.
  Empty text still clears optional update content; whitespace is not a substitute
  for an empty string. Standard web Issue updates that cannot preserve a changed
  body return `unsupported_error` before writes, including accompanying changes.
- Missing Issue `closed_at` now returns null, replacing the old updated_at-based
  estimate. Consumers must treat it as unknown and avoid closure-time calculations
  from updated_at.
- Issue and PR labels/assignees and repository fork are optional Rust fields and
  serialize as null when absent. Explicit empty arrays and false remain explicit.
  PR draft/merged_by/assignee/URLs/branch users and Issue milestone/assignee/URLs/PR
  links are retained. Do not interpret null mergeable or draft as false.
- `GITBUCKET_READ_ONLY=true` is an env-only MCP-server setting, default false.
  It hides mutations and rejects write calls with `read_only_error`, including
  public Rust tool handlers. Rust servers use `with_access_mode(AccessMode::ReadOnly)`.
- Every tool now carries behavioral annotations. They are advisory; read-only
  enforcement is independent, and writes are never automatically retried.

These changes intentionally correct the protocol and public response models;
there are no legacy aliases or synthetic timestamp/boolean fallbacks. Include
these migration notes in the next release. No version tag is created by this work.

## Versioning

Use semantic versions in `Cargo.toml`.

- Stable release tags must be `vX.Y.Z`, for example `v0.2.0`.
- Pre-release tags may include a suffix, for example `v0.2.0-rc.1`.
- The tag must exactly match the package version in `Cargo.toml` with a leading `v`.

## Pre-Release Checklist

Before tagging:

1. Confirm the intended user-visible changes are merged to `main`.
2. Update `Cargo.toml` and `Cargo.lock` to the release version.
3. Update README examples that mention a specific release tag.
4. Run local checks:

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features -- -D warnings
cargo test
```

5. Confirm the CI workflow is green on `main`.

## Create a Release

Create and push an annotated tag from the release commit on `main`:

```bash
./scripts/release/create_tag.sh v0.2.0
```

Pushing the tag starts `.github/workflows/release.yml`.

The workflow:

1. runs formatting, clippy, and tests;
2. validates E2E scripts, boots Docker GitBucket, and runs E2E tests;
3. verifies that the tag matches `Cargo.toml`;
4. builds release binaries for Linux, macOS Intel, macOS Apple Silicon, and Windows;
5. packages archives with `README.md`, `LICENSE`, and `VERIFICATION.md`;
6. generates `.sha256` checksum files;
7. publishes a GitHub Release with generated release notes.

Manual `workflow_dispatch` defaults to dry-run mode (`publish=false`), which runs checks and packaging and uploads workflow artifacts without publishing a GitHub Release.

If you run `workflow_dispatch` on a tag ref and set `publish=true`, the workflow also publishes the GitHub Release.

## Verify a Published Release

After the workflow finishes:

1. Open the GitHub Release page for the tag.
2. Confirm all expected archives and `.sha256` files are attached.
3. Download the archive for your platform.
4. Verify the checksum:

```bash
shasum -a 256 -c gitbucket-mcp-server-0.2.0-x86_64-unknown-linux-gnu.tar.gz.sha256
```

5. Follow `VERIFICATION.md` from the archive to confirm the binary starts and the MCP client can list tools.

## Failed Releases

If the release workflow fails before publishing, fix the problem on `main`, delete the local and remote tag, then create the tag again from the corrected commit:

```bash
git tag -d v0.2.0
git push origin :refs/tags/v0.2.0
./scripts/release/create_tag.sh v0.2.0
```

If a GitHub Release was already published, prefer creating a new patch version instead of replacing released artifacts.
