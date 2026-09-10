# mtg-collection

Manage your MTG card collection from the command line. Search Scryfall, preview
card images in your terminal, and add cards to a local file or a remote
`mtg-server` on your LAN.

## Features

- Scaffolding-free Scryfall search with paper-only (no Arena/MTGO) defaults
- Per-printing set selection and promo filtering
- Terminal card image preview (kitty graphics protocol)
- Local collection file plus a remote mode: point the client at an
  `mtg-server` instance and the collection lives on one machine only
- Server-side quantity merging (the collection file is the single source of
  truth)

## Build & install

### With Cargo

```console
$ cargo build --release
```

This produces two binaries in `target/release/`:

- `mtg-collection` — the client
- `mtg-server` — the collection server

### With Nix

Supported on `x86_64-linux` and `aarch64-linux`:

```console
$ nix build .#default    # both binaries
$ nix run .#server       # run mtg-server
```

## Usage

```
mtg-collection <COMMAND>
```

| Command | Description |
|---|---|
| `search <query>` | Search Scryfall and preview cards |
| `add -n <count> <query>` | Add a card to your collection |
| `add-many` | Interactive session to add many cards |
| `remove <name>` | Remove a card (one copy per confirmation) |
| `list` | List the whole collection |
| `show <name>` | Show one card (fuzzy match) |
| `config set-server <url>` \| `show` \| `clear` | Point at a remote server, inspect, or go local |

### Local mode (default)

Without a configured server the collection lives in
`~/.mtg-collection/collection.json`.

```console
$ mtg-collection add -n 4 "Lightning Bolt"
$ mtg-collection list
```

### Remote mode

The `mtg-server` binary hosts the collection over plain HTTP. Run it on the
machine you want to keep the collection on.

```console
$ mtg-server --bind 0.0.0.0 --port 8080 --collection /path/to/collection.json
mtg-server listening on 0.0.0.0:8080
```

| Flag | Default | Description |
|---|---|---|
| `--bind` | `0.0.0.0` | Address to bind to |
| `--port` | `8080` | Port to listen on |
| `--collection` | `~/.mtg-collection/collection.json` | Collection file to read/write |

Point the client at it (LAN-only, plain HTTP):

```console
$ mtg-collection config set-server http://192.168.1.50:8080
$ mtg-collection add -n 2 -f foil "Sol Ring"
$ mtg-collection list
$ mtg-collection remove "Sol Ring"
```

`add` quantities merge server-side: adding the same card twice results in a
single entry with the summed quantity. Scryfall search and image preview still
run on the client.

To go back to the local file:

```console
$ mtg-collection config clear
```

The client configuration lives in `$XDG_CONFIG_HOME/mtg-collection/config.json`
(defaults to `~/.config/mtg-collection/config.json`). Missing or corrupt
config means local mode.

### NixOS module

The flake provides a `nixosModules.default` that runs `mtg-server` as a systemd
service under a dedicated system user (`mtg-server`); the collection's parent
directory is created by systemd-tmpfiles from the `collectionPath` option.

```nix
{
  inputs.mtg-collection.url = "github:Kibadda/mtg-collection";

  outputs = { self, nixpkgs, mtg-collection, ... }: {
    nixosConfigurations.myServer = nixpkgs.lib.nixosSystem {
      modules = [
        mtg-collection.nixosModules.default
        {
          services.mtg-server = {
            enable = true;
            # bind = "0.0.0.0";
            # port = 8080;
            # collectionPath = "/var/lib/mtg-collection/collection.json";
          };
        }
      ];
    };
  };
}
```

| Option | Default | Description |
|---|---|---|
| `services.mtg-server.enable` | `false` | Enable the systemd unit |
| `services.mtg-server.package` | flake default | Package providing `mtg-server` |
| `services.mtg-server.bind` | `0.0.0.0` | Address to bind |
| `services.mtg-server.port` | `8080` | Port |
| `services.mtg-server.collectionPath` | `/var/lib/mtg-collection/collection.json` | Collection file; its parent dir is created via tmpfiles and owned by the `mtg-server` user |

### Home-manager module

The flake also provides `homeManagerModules.default` for the client. It adds
`mtg-collection` to `home.packages` and writes
`~/.config/mtg-collection/config.json` from the `serverUrl` option, so you can
point the CLI at your `mtg-server` declaratively.

```nix
{
  programs.mtg-collection = {
    enable = true;
    serverUrl = "http://mtg.internal:8080"; # null/left out → local mode
  };
}
```

| Option | Default | Description |
|---|---|---|
| `programs.mtg-collection.enable` | `false` | Install the client and manage its config |
| `programs.mtg-collection.package` | flake default | Package providing `mtg-collection` and `mtg-server` |
| `programs.mtg-collection.serverUrl` | `null` | URL of the `mtg-server` to talk to; `null` keeps the collection local |

## Development

```console
$ cargo test
$ cargo fmt
$ cargo clippy --all-targets
$ nix flake check
```