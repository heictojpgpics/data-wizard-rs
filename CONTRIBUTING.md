# Contributing

Thanks for your interest in contributing! This document is short on
purpose — the real rules live in [`docs/`](docs/) and
[`SECURITY.md`](SECURITY.md).

## Clean-room policy (read this first)

This project is a **clean-room re-implementation**. We never copy code,
assets, strings, icons, or UI from any third-party product. We never
bypass, patch, or emulate any third-party license check. See
[`docs/architecture/licensing-boundary.md`](docs/architecture/licensing-boundary.md)
for the full policy.

If you're unsure whether a contribution crosses the line, the test is
simple: **"Could I have written this code having only read the public
specification, never having seen any third-party product's source or
binaries?"** If yes, it's clean. If no, stop and ask.

## Workflow

1. Fork the repo.
2. Create a feature branch (`feat/...`, `fix/...`, `docs/...`).
3. Make your change. Keep commits atomic and write clear messages.
4. Run `cargo fmt --all` and `cargo clippy --workspace --all-targets -- -D warnings`.
5. Run `cargo test --workspace`.
6. Open a PR. CI runs the same checks; green CI is required to merge.
7. Sign off on the clean-room policy in your PR description.

## Code style

- Rust: edition 2024, `#![forbid(unsafe_code)]` in every crate except
  the audited FFI shim in `dw-disk/src/windows_raw.rs`.
- TypeScript: strict mode, no `any`.
- All public APIs have doc comments (`///`).
- All panics are bugs — prefer `Result<T, E>` over `unwrap()`/`expect()`
  in library code.

## Commit messages

Use [Conventional Commits](https://www.conventionalcommits.org/):

```
feat(ntfs): parse $ATTRIBUTE_LIST to follow fragmented attributes
fix(carver): bifragment gap off-by-one for < 1 MiB files
docs(security): add winchecksec CI step
chore(deps): bump tokio to 1.48
```

## Issue labels

- `bug` — something is broken
- `feat` — new feature request
- `docs` — documentation
- `security` — security-relevant
- `good first issue` — small, self-contained, good for newcomers
- `help wanted` — we'd appreciate community help

## License

By contributing, you agree that your contributions are dual-licensed
under the MIT and Apache-2.0 licenses.
