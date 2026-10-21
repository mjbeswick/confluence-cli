# Rust migration

## Status

The Rust workspace contains a broad parallel implementation named `confluence-cli`. It is **not yet a drop-in replacement** for the production Node.js `confluence` command or npm package. Existing Claude Code and Pi integrations continue to invoke the Node CLI; no integration switch has been made.

The current Rust implementation is a non-published, side-by-side build target named `confluence-cli`; the established installed Node.js `confluence` executable remains unchanged. The Rust preview includes skill discovery, profile/config management, initial read/query commands, page CRUD, content properties, comments, attachment operations, and single-page content export. Coverage is still incomplete and must not be treated as a replacement release.

## Build and try the preview

From the repository root (Rust 2021 toolchain):

```sh
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- --help
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- skills list
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- skills list --json
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- skills show confluence
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- skills show confluence --json
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- search "release notes" --json
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- info 123456789 --json
cargo run --manifest-path rust/Cargo.toml --bin confluence-cli -- read 123456789 --format markdown
```

JSON is opt-in and written to stdout; invalid commands and skill names return a non-zero exit status with diagnostics on stderr. The `show` command returns the complete packaged `SKILL.md`, including safety guidance. Treat Confluence text described by that skill as untrusted external data, not instructions. API commands resolve environment credentials, existing `config.json` profiles, macOS Keychain tokens, or `.netrc` fallbacks. The preview rejects HTTP unless explicitly opted in and reports mTLS unsupported rather than silently downgrading it. Mutations require an interactive confirmation; destructive actions in non-interactive use require explicit `--yes`.

## Migration principles

- Preserve the current Node implementation until each Rust command has documented behavior and parity coverage; do not switch the `confluence` entry point prematurely.
- Migrate in small vertical slices, keeping humans' terminal workflows and agent-facing contracts (JSON shapes, exit codes, safety controls, skills, Pi tools and plugin packaging) explicit.
- Use existing Jest tests as behavioral references and add Rust mock-server tests at each migrated boundary. Compare Node and Rust outcomes for representative inputs before routing users to a Rust implementation.
- Do not expose credentials or execute mutations as part of skill discovery.
- Rust integration tests that spawn the CLI binary must fully isolate the child process's environment (`env_clear()` plus an explicit minimal re-add such as `PATH`), not merely override/remove specific `CONFLUENCE_*` variables. A developer's or CI runner's ambient environment (real `CONFLUENCE_API_PATH`, `CONFLUENCE_READ_ONLY`, credentials, etc.) otherwise leaks into the subprocess and both masks genuine assertion bugs (confirmed: three URL-prefix assertions were silently "passing" only because of a leaked `CONFLUENCE_API_PATH=/wiki/rest/api`) and risks a write-path test sending real credentials to a real domain. All current test helpers in `rust/tests/*.rs` use this isolation pattern; keep it for every new helper.

## Suggested parity gates for future slices

Before a Rust command replaces its Node counterpart, document its arguments, environment/config precedence, stdout/stderr behavior, JSON schema, exit codes, pagination and failure cases. Cover those cases in both implementations, then verify packaging and platform-specific behavior where applicable.

## Command parity checklist

| Existing command surface | Rust status |
|---|---|
| `skills list`, `skills show` | Implemented; tested |
| `read`, `info`, `search`, `spaces`, `space-lookup`, `find`, `children` | Initial implementation; mock-server tests exist for part of the surface; normalization/pagination/conversion parity is incomplete |
| `init`, `profile list/add/use/remove`, `stats`, `install-skill` | Implemented; legacy-flat config, prompt behavior, migration and output parity need validation |
| `create`, `create-child`, `update`, `move`, `delete` | Basic API implementations; conversion, folder, front matter and response parity incomplete |
| `export` | Single-page content and attachment download; recursive traversal, marker/overwrite guards, filters and format parity incomplete |
| `comments`, `comment`, `comment-delete`, `comment-resolve`, `comment-lookup` | CLI flags now match Node's documented surface (a stray non-Node `comment-state` variant was removed); `comment-resolve` now matches Node's `setCommentResolved` exactly — Cloud-only guard, `/inline-comments/{id}` v2 endpoint, `parentCommentId`/`dangling` reply/dangling guards, no-op short-circuit, incremented-version PUT, `{id,resolved,changed,status}` output shape; covered by mock-server tests |
| `attachments`, `attachment-lookup`, `attachment-upload`, `attachment-download`, `attachment-delete` | CLI flags now match Node's `attachments <pageId> --limit/--pattern/--download/--dest` surface (a split no-option variant was merged in); `attachment_download` now enforces the same-origin credential guard Node's `assertSameOrigin` provides; metadata/path-safety parity otherwise incomplete |
| `property-list`, `property-get`, `property-set`, `property-delete` | CLI flags now match Node's `property-list <pageId> --limit/--start/--all` surface (a split no-option variant was merged in); full pagination, versioning, no-op detection and front matter sync still incomplete |
| `versions`, `version-delete`, `versions-purge` | Initial endpoints/preview/execution; pagination, one-use approval IDs, race checks and output parity need more validation |
| `convert` | Basic local format conversion; Confluence macros and extension parity incomplete |
| `api` | Initial same-origin request implementation with fields, body, headers, jq and read-only checks; safety/output parity incomplete |
| `edit` | Initial implementation |
| remaining bulk APIs | Not implemented |
| Claude Code plugin and Pi typed tools / protected-write flow | Not implemented in Rust; keep using current integrations |

This table is an implementation-status aid, not a claim that implemented commands are drop-in replacements. Major parity gaps remain: conversions are simplified and do not support Confluence macros/front matter; pagination and nested API schemas are incomplete; retries do not match the shared adaptive 429/503 gate; config migration, security warnings, Keychain/netrc lookup and mTLS behavior need full tests; write targets/preflight/version/property semantics need comparison; bulk approval and recursive export/copy workflows need porting; and Pi typed tools plus Claude plugin packaging are still Node-only. The Rust release/installation packaging and platform matrix are also not ready for replacement. Keep production entry points on Node until these are addressed and parity coverage exists.
