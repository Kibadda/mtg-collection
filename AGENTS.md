# AGENTS.md

Guidance for AI assistants working in this repository.

## Project overview

`mtg-collection` is a Rust CLI for managing a Magic: The Gathering card
collection. Cards are searched on the [Scryfall API](https://scryfall.com/docs/api)
and added either to a local JSON file or to a remote LAN `mtg-server` via
plain HTTP.

Two binaries are shipped from one crate:

- `mtg-collection` — the interactive client (`src/main.rs` → `src/lib.rs::run`)
- `mtg-server` — a stateful axum server hosting the collection (`src/bin/mtg-server.rs` → `src/server.rs::serve`)

## Architecture

- `src/cli.rs` — clap definitions: `search`, `add`, `add-many`, `remove`,
  `list`, `show`, `config`. Global flags `--all-games`, `--all-promos`
  (paper-only and no-promos are the defaults).
- `src/lib.rs` — command glue, the interactive add flow, printing selection,
  prompts. Most of the client logic lives here.
- `src/scryfall.rs` — Scryfall HTTP wrapper. Set the `User-Agent` (always
  include one), `POST /cards/collection` batches by set + collector number
  (75 per chunk, follow pagination), `default_query()` appends `game:paper`
  and/or `not:promo` only when the user query doesn't already set one.
- `src/collection.rs` — `Card`/`Collection` data model and the local file.
- `src/store.rs` — `Store` enum abstracting local file vs. remote HTTP
  (`/cards` GET/POST, `/cards/remove` POST, 404 ⇒ not found).
- `src/config.rs` — client config (`server_url`); file lives at
  `$XDG_CONFIG_HOME/mtg-collection/config.json`. Missing/corrupt ⇒ local mode.
- `src/server.rs` — axum router + handlers; serializes mutations behind a
  single `tokio::sync::Mutex`, persists after every mutation.
- `src/display.rs` — terminal rendering with `console::style`.
- `src/image.rs` — card art via viuer (kitty graphics / half-blocks).

### Data model and merge semantics

`Card` stores only what identifies a physical copy plus user metadata:
`name`, `set`, `collector_number`, `quantity`, `finish`, `condition`, `lang`.
Everything else (oracle text, prices, art) is fetched live from Scryfall and
matched back via `set` + `collector_number` (see `card_key` in `lib.rs`).

Adding merges **only** when all of `set`, `collector_number`, `finish`,
`condition`, and `lang` match (`Card::same_printing`). Language is copy
metadata, not a printing dimension — distinct printings are deduped by
set/collector number in `distinct_printings`.

## Conventions

- Edition 2024, `async` everywhere (tokio + reqwest). Avoid blocking main /
  manual runtimes outside tests.
- Errors are `Box<dyn std::error::Error>`; interactive flows print via
  `console::style` and exit with `std::process::exit(1)` on hard failures.
- All prompting goes through the helpers in `lib.rs` (`fuzzy_pick`,
  `plain_pick`, `confirm`, `prompt_query`, …). These return defaults/`None`
  when stdin is not a TTY so tests and pipelines don't hang. Keep it that way.
- Defaults should be safe: paper-only, promos excluded, condition `NM`,
  finish `nonfoil`, language `en`, "local mode" when config is missing.
- Commit messages use the conventional-commits style seen in `git log`
  (`feat:`, `fix:`, `refactor:`, `docs:`, `test:`), short imperative summary.
- Do NOT add code comments unless they explain nontrivial intent (existing
  code uses doc comments on public items and brief `///` notes).

## Development

- The dev shell is a Nix flake (`nix develop`, or `direnv` via `.envrc`).
  `cargo`/`rustc`/`rustfmt`/`clippy`/`rust-analyzer` are provided.
- Verification commands:
  ```console
  $ cargo test        # unit tests; run all before finishing
  $ cargo fmt --check
  $ cargo clippy --all-targets -- -D warnings
  $ nix flake check   # checks flake eval; slower
  ```
- Tests that hit live Scryfall are marked `#[ignore = "requires network
  access to api.scryfall.com"]` — leave them that way.
- When touching search behavior, keep the tests in `cli.rs`, `lib.rs`,
  `scryfall.rs`, and `collection.rs` green.

## Nix / deployment

- `flake.nix` builds the package for `x86_64-linux` and `aarch64-linux`
  (`buildRustPackage`; `cargoLock.lockFile = ./Cargo.lock`).
- `nixosModules.default` runs `mtg-server` as a systemd service under the
  `mtg-server` system user; the collection dir is created via
  `systemd.tmpfiles.rules` so the path must be tmpfiles-compatible.
- `homeManagerModules.default` installs the client and writes the config from
  `programs.mtg-collection.serverUrl`.
- If you add a dependency in `Cargo.toml`, update `Cargo.lock` (and be mindful
  of the Nix locked build).

## Pitfalls

- Do not add auth/TLS expectations: the remote server is intentionally
  LAN-only plain HTTP.
- Scryfall pagination: the collection endpoint returns `has_more`/`next_page`;
  follow it (see `get_cards_by_identifiers`).
- The current-year tool default aside, Scryfall data (sets, card names) is
  user-facing — avoid inventing cards or sets in tests; fixture builders
  (`scard` in `lib.rs`, `card` in `collection.rs`/`store.rs`) already exist.