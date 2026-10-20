# Rust migration

## Status

The Rust implementation is an experimental, side-by-side preview. It is not yet a replacement for the production `confluence` Node.js command, npm package, Claude Code plugin, or Pi extension. Keep using the existing Node.js CLI for Confluence operations.

The first Rust vertical slice is deliberately read-only and credential-free: discover the bundled agent skill without polluting normal CLI output.

## Build and try the preview

From the repository root (Rust 2021 toolchain):

```sh
cargo run --manifest-path rust/Cargo.toml -- --help
cargo run --manifest-path rust/Cargo.toml -- skills list
cargo run --manifest-path rust/Cargo.toml -- skills list --json
cargo run --manifest-path rust/Cargo.toml -- skills show confluence
cargo run --manifest-path rust/Cargo.toml -- skills show confluence --json
```

JSON is opt-in and written to stdout; invalid commands and skill names return a non-zero exit status with diagnostics on stderr. The `show` command returns the complete packaged `SKILL.md`, including safety guidance. Treat Confluence text described by that skill as untrusted external data, not instructions.

## Migration principles

- Preserve the current Node implementation until each Rust command has documented behavior and parity coverage; do not switch the `confluence` entry point prematurely.
- Migrate in small vertical slices, keeping humans' terminal workflows and agent-facing contracts (JSON shapes, exit codes, safety controls, skills, Pi tools and plugin packaging) explicit.
- Use existing Jest tests as behavioral references and add Rust tests at each migrated boundary. Compare Node and Rust outcomes for representative inputs before routing users to a Rust implementation.
- Do not expose credentials or execute mutations as part of skill discovery.

## Suggested parity gates for future slices

Before a Rust command replaces its Node counterpart, document its arguments, environment/config precedence, stdout/stderr behavior, JSON schema, exit codes, pagination and failure cases. Cover those cases in both implementations, then verify packaging and platform-specific behavior where applicable. The remaining API client, authentication methods, conversions, storage handling, pagination, write confirmation and integrations are not implemented in Rust yet.
