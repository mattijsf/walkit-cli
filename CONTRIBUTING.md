# Contributing

## Setup

You need a stable [Rust](https://rustup.rs) toolchain; `rust-toolchain.toml` selects it and
adds rustfmt and clippy. Install [lefthook](https://lefthook.dev) and turn the git hooks on
once per clone:

```
lefthook install
```

## Checks

The same three checks run in the hooks and in CI:

```
cargo fmt --all -- --check
cargo clippy --all-targets --locked -- -D warnings
cargo test --locked
```

- Before a commit, the hooks check formatting and lints.
- The commit message is checked against the format below.
- Before a push, the hooks check formatting and lints again and run the tests.

CI runs the lints on Linux and the tests on Linux, macOS and Windows for every pull request
and every push to `main`.

## Commit messages

Commits follow [Conventional Commits](https://www.conventionalcommits.org/):

```
<type>(<optional scope>): <description>
```

The types are `feat`, `fix`, `docs`, `style`, `refactor`, `perf`, `test`, `build`, `ci`,
`chore` and `revert`. A `!` before the colon, or a `BREAKING CHANGE:` footer, marks a
breaking change. The description starts in lower case and has no full stop.

```
feat(render): draw a classic playlist at any size
fix: keep the batch going when a download times out
```

`scripts/check-commit-message.sh` is the check the hook and CI use. CI applies it to every
commit of a pull request and to the pull request's title, and runs again when the title is
edited.

The types matter: releases and the changelog are made from them. `feat` and `fix` show up in
the changelog. Before 1.0, both raise the patch version and a breaking change raises the
minor version; from 1.0 on, `feat` raises the minor version and `fix` the patch version.

## Merging

`main` takes changes only through pull requests, and only once the checks pass: commit
messages, formatting and lints, and the tests on Linux, macOS and Windows. Pull requests are
squashed: each lands on `main` as one commit whose message is the pull request's title, so
the title must follow the format above. The branch is deleted after the merge. `main` keeps a straight history and cannot be force-pushed
or deleted.

## Parameters

A new parameter goes in `src/params.rs` with a default, and `params/museum.json` has to list
it. Regenerate the file instead of editing it:

```
cargo run -- params > params/museum.json
```

A test fails when the file and the defaults drift apart. Document the field in the README's
table. If the default is not what the museum's screenshots show, say why in the README.

## Test skins

`tests/skins` holds two original skins. Skins made by other people, Winamp's base skin and
the wasabi base do not belong in the repository.

## Releases

[release-please](https://github.com/googleapis/release-please) keeps a release pull request
open on `main`. It collects the commits since the last release, proposes the next version,
and updates `CHANGELOG.md`, `Cargo.toml` and `Cargo.lock`. Merging that pull request tags the
release and publishes it on GitHub. The release workflow then runs the tests and builds the
binaries on each platform and attaches them, each with a SHA-256 checksum:

| Target | Archive |
| --- | --- |
| Linux x86-64 | `walkit-cli-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz` |
| Linux ARM64 | `walkit-cli-vX.Y.Z-aarch64-unknown-linux-gnu.tar.gz` |
| macOS Apple silicon | `walkit-cli-vX.Y.Z-aarch64-apple-darwin.tar.gz` |
| macOS Intel | `walkit-cli-vX.Y.Z-x86_64-apple-darwin.tar.gz` |
| Windows x86-64 | `walkit-cli-vX.Y.Z-x86_64-pc-windows-msvc.zip` |

Once every target has built, the release workflow publishes the crate to
[crates.io](https://crates.io/crates/walkit-cli) with `cargo publish --locked`. It needs a
crates.io API token with the publish-new and publish-update scopes in the repository secret
`CARGO_REGISTRY_TOKEN`. The job skips while `Cargo.toml` says `publish = false`.

For release-please to open its pull request, the repository setting "Allow GitHub Actions to
create and approve pull requests" (Settings, Actions, General) has to be on.
