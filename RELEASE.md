# Release Process

This project publishes GitHub Releases from version tags. The release workflow builds platform archives, attaches checksums, and publishes the GitHub Release.

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
git switch main
git pull --ff-only origin main
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0
```

Pushing the tag starts `.github/workflows/release.yml`.

The workflow:

1. runs formatting, clippy, and tests;
2. verifies that the tag matches `Cargo.toml`;
3. builds release binaries for Linux, macOS Intel, macOS Apple Silicon, and Windows;
4. packages archives with `README.md`, `LICENSE`, and `VERIFICATION.md`;
5. generates `.sha256` checksum files;
6. publishes a GitHub Release with generated release notes.

Manual `workflow_dispatch` builds the same archives as workflow artifacts, but it does not publish a GitHub Release.

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
git tag -a v0.2.0 -m "Release v0.2.0"
git push origin v0.2.0
```

If a GitHub Release was already published, prefer creating a new patch version instead of replacing released artifacts.
