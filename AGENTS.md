# conversor-stickers

## Language

- Code, identifiers, comments, documentation and commit messages are in English.
- Everything the user sees (UI text, error messages shown in the app, CLI help and messages, Android
  strings) is in Spanish.
- Talk to the user in Spanish.

## Rust

### Commands

```sh
cargo fmt --all                                            # format
cargo clippy --all-targets --all-features -- -D warnings   # lint: zero warnings
cargo test --all-features                                  # tests
cargo build --release                                      # optimized build
```

The project is a workspace, so run these commands from the root.

### Desktop app (`app/`)

The UI is built before the `conversor-app` crate: `tauri::generate_context!` requires `app/dist/`
to exist, so without it clippy and the workspace tests fail.

```sh
cd app
npm ci                 # exact dependencies from package-lock.json
npm run check          # tsc --noEmit + node --test (pure UI logic)
npm run build          # dist/ with Vite
cargo tauri dev        # app with hot reload (tauri-cli 2.11.4)
cargo tauri build      # .deb and AppImage packages
```

On Linux you need `webkit2gtk-4.1` and, for the video preview, the GStreamer plugins `gst-plugins-good`
and `gst-libav`.

Tauri commands:

- The frontend is in `app/src/`. Its contract with the backend is in `contract*.ts`, and
  `contract*.examples.json` are the shared examples checked by the Rust tests.
- The backend commands are in `app/src-tauri/src/commands*.rs` (for example `commands_packs.rs`,
  `commands_community.rs`, `commands_gifs.rs`, `commands_mobile.rs`). Their logic lives in the modules
  next to them (`library.rs`, `community.rs`, `session.rs`, ...).
- `app/src-tauri/build.rs` lists every command in `COMMANDS`, so each one needs its
  `allow-<command>` permission in `app/src-tauri/capabilities/main.json`. Keep that file as narrow as
  the features need.

## Conventions

- No `unwrap()` in production code. `expect("...")` only for invariants that cannot fail, and the
  message explains why. Tests may use both.
- Errors use `thiserror`: one `enum` per module or layer, with variants that name the domain failure
  (`OrderNotFound`, not `Error1`). Propagate with `?` and convert with `#[from]`. `anyhow` only in
  `main` or in internal tools.
- Logs use `tracing` (`info!`, `warn!`, `error!` with structured fields). No `println!` or `dbg!`
  outside tests and examples.
- `clippy` with no warnings. Every `#[allow(...)]` has a comment that justifies it.
- No `unsafe` unless it is necessary and proven, and then with a `// SAFETY:` comment.
- Prefer types that make invalid states impossible (newtypes, enums) over validating the same thing in
  several places.
- Avoid `clone()` just to satisfy the borrow checker; rethink ownership instead.
- Minimal visibility: `pub(crate)` before `pub`.
- Unit tests go in `#[cfg(test)] mod tests` inside the module; integration tests go in `tests/`.

## Backend

- `tokio` as the runtime and `axum` for HTTP, unless `PLAN.md` says otherwise.
- Separate layers: handlers are thin (they parse, validate and call the domain), business logic does
  not know about HTTP or the database, and persistence sits behind its own module.
- Never block the async runtime: CPU-bound or blocking I/O goes to `spawn_blocking`.
- SQL with `sqlx` and compile-time checked queries when possible; versioned migrations in the
  repository.
- Configuration comes from environment variables, validated at startup: if something is missing, the
  process fails with a clear message.

## Recommended lints in `Cargo.toml`

```toml
[lints.rust]
unsafe_code = "forbid"

[lints.clippy]
unwrap_used = "deny"
dbg_macro = "deny"
todo = "deny"
print_stdout = "warn"
```

And in `clippy.toml`, to allow `unwrap` and `expect` in tests:

```toml
allow-unwrap-in-tests = true
allow-expect-in-tests = true
```

## Rust skills (plugin `beautiful-rust`)

The project enables the `beautiful-rust` plugin in `.claude/settings.json`: `rust-best-practices` when
writing code, `rust-review` for reviews and `rust-port` for porting. If its rules conflict with this
file, this file wins. Here, `unwrap_used` and `expect_used` are `deny` (the plugin only warns), and its
`clippy.toml` is not adopted: the lints in the workspace `Cargo.toml` stay.

## Code review rules

Rules for anyone reviewing a PR in this repository (Claude, Codex or a person). Mark as blocking:

- New or fixed behavior without a test, or disabled tests.
- Errors ignored or swallowed silently: `unwrap()` in production, empty `catch`, `except: pass`,
  discarded results.
- Secrets, tokens or real `.env` files in the diff; sensitive data in logs.
- External input (user, network, files, Tauri IPC) not validated at the boundary.
- Blocking work in async code; N+1 queries or allocations in hot loops.
- Tauri permissions (capabilities) broader than the feature needs.

Mark as important:

- Abstractions, layers or dependencies the change does not need.
- Dead code, forgotten debugging or changes unrelated to the PR's goal.
- Generic names (`data`, `manager`, `utils`) where a domain term exists.

Do not comment on style that the formatter and linter already cover.
