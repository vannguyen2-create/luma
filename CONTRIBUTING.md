# Contributing

## Reporting Issues

- Bugs: open an issue with steps to reproduce, expected vs actual behavior, and environment info.
- Features: open an issue to discuss before writing code.
- Security: see [SECURITY.md](SECURITY.md). Do not open a public issue.

## Setup

```bash
git clone https://github.com/nghyane/luma.git
cd luma
cargo build
cargo test
cargo clippy -- -D warnings
```

Requires Rust 1.85+ and macOS 12+.

## Making Changes

1. Create a branch: `feature/name`, `fix/name`, or `refactor/name`.
2. Write code following [RULES.md](RULES.md).
3. Add tests for new or changed behavior.
4. Verify before submitting:
   ```bash
   cargo test
   cargo clippy -- -D warnings
   cargo fmt --check
   ```
5. Open a PR with a clear title and description of what changed and why.

## Code Standards

All rules are in [RULES.md](RULES.md). The non-negotiable ones:

- `cargo clippy -- -D warnings` must pass
- `cargo test` must pass
- Every `pub fn` has a one-line doc comment
- No `.unwrap()` outside tests
- No `unsafe`

## Review

A maintainer will review your PR. Fix feedback on the same branch. Once approved, the maintainer merges.

## License

By contributing, you agree your code is licensed under the MIT license.
