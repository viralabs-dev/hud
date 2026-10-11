# Local Linux preview packaging and rollback

[Português brasileiro](PACKAGING.pt-BR.md) · [installed preview](../docs/local-preview.en.md)

`package_local.py` packages an existing Linux ELF64 x86_64 executable; it builds no code, installs nothing and publishes nothing. Explicit new archive path is required. Include executable, project MIT license, README/docs, Cargo.lock, full cached license/notice files for active normal/build dependencies, inventory and SHA256SUMS. Optional fake fixture must match the core's compile-time SHA pin; no real providers, Python runtime or personal configuration are bundled. SPDX metadata is not a legal audit.

Inspect available arguments without packaging:

```sh
python3 scripts/package_local.py --help
```

When packaging is explicitly authorized, run from the Rust workspace with an existing tested binary and a new output path:

```sh
python3 scripts/package_local.py --repo "$PWD" --binary "$PWD/target/debug/hud-rs" --output /tmp/hud-rs-reviewed-new.tar.gz --cargo "$HOME/.cargo/bin/cargo"
```

The script uses locked/offline Cargo metadata/tree, verifies ELF/architecture and runs isolated-HOME smoke outside checkout: help/version/invalid options/path/corrupt state. It refuses existing archives. Smoke proves CLI/nonmutation, not physical usability, real account billing or native cross-platform support. Record host distribution and dynamic dependencies; this glibc-linked development build is not a portable static release.

Installation is a separate verified user-directory operation. Record binary/archive hashes, symlink target, version, data scope, actual rollback rehearsal and tests in `installation.json`. `local_cutover.py` captures link/data digests, checks before switch, writes rollback expectations before publication and refuses concurrent edits. Data restoration is explicit opt-in; selecting an older launcher alone does not restore state. Do not reuse an old receipt for a new version.

Current 0.10.0-preview.12 receipt: 391 workspace tests, verified rollback recorded by cutover, default readiness/core/fixture PTYs, zero subscription model calls. See [support matrix](../docs/support.en.md) for remaining native/MSRV/provider/physical gates. No local archive authorizes release/remote Actions or performing a new primary-launcher replacement.

Links to crates/scripts/source refer to the repository checkout; the archive does not bundle source/build tools. Use bundled dependency notices and installation receipt for artifact validation.

Packaging scripts use a development-time Python interpreter. The shipped runtime/core and primary hud command are Rust; this does not depend on the legacy application.
