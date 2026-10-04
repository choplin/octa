# Release automation

octa uses one release tag and two tools with non-overlapping ownership.

| Side effect | Owner |
|---|---|
| Version and changelog update | `cargo-release` |
| Release commit | `cargo-release` |
| crates.io publication | `cargo-release` |
| `v<version>` tag and push | `cargo-release` |
| Binary builds and archives | `dist` |
| SHA-256 checksums | `dist` |
| GitHub Release and artifact upload | `dist` |

The `Publish crate` GitHub Actions workflow is the only supported execution
path for a release. It runs `cargo-release` on the default branch and reads the
crates.io token from the `CARGO_REGISTRY_TOKEN` Actions secret. Do not store a
registry token in repository files or pass one as a workflow input.

The workflow pushes its commit and tag with `RELEASE_GITHUB_TOKEN`, which must
contain a service-account personal access token with repository contents write
access. A push made with the workflow's built-in `GITHUB_TOKEN` does not
trigger the tag-driven `Release` workflow.
Restrict creation of matching release tags to this release principal; the
generated workflow treats a matching tag and the commit it names as trusted
release input.

`cargo-release` publishes the crate before it pushes the release commit and
`v<version>` tag. The tag triggers the generated `Release` workflow, where
`dist` builds and publishes the binary artifacts. The dist workflow does not
publish to crates.io, update versions, create commits, or create tags.

## Validate a release

Install `cargo-release` 1.1.6 and `dist` 0.33.0, then run the package and both
planning paths from a clean checkout using Rust 1.90.0, the declared minimum.
Rustup-backed environments and generated dist builds select this version from
`rust-toolchain.toml`; the Nix development shell manages its toolchain
independently.

```sh
cargo package --locked
cargo release --dry-run 0.1.0
dist plan
dist generate --check
```

These commands do not publish, commit, tag, push, or create a GitHub Release.
The package command verifies the locked public dependency graph on the minimum
toolchain. The cargo-release dry run repeats package verification and prints the
planned release steps. The dist plan must contain only these targets:

- `aarch64-apple-darwin`
- `x86_64-apple-darwin`
- `x86_64-unknown-linux-gnu`

Pull requests run the generated dist workflow in `upload` mode. It builds the
same target matrix and retains the archives and SHA-256 checksums as workflow
artifacts without creating a GitHub Release.

## Publish 0.1.0

1. Confirm the default branch is clean and all required checks pass.
2. Confirm the repository Actions secrets `CARGO_REGISTRY_TOKEN` and
   `RELEASE_GITHUB_TOKEN` are present and the latter can create release tags.
3. Run the `Publish crate` workflow from the default branch with version
   `0.1.0`.
4. Confirm crates.io contains `octa-cli 0.1.0`.
5. Confirm the tag-triggered `Release` workflow publishes archives and SHA-256
   checksums for exactly the three supported targets.

Do not run the workflow again for an already-published version. Recovery after
a partial release requires inspecting which cargo-release step completed before
running a narrower recovery command; never recreate a side effect with `dist`.
