{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-26.05";
  };

  outputs =
    { self, nixpkgs }:
    let
      supportedSystems = [
        "x86_64-linux"
        "aarch64-linux"
      ];

      forAllSystems = nixpkgs.lib.genAttrs supportedSystems;

      pkgsFor = forAllSystems (system: import nixpkgs { inherit system; });
    in
    {
      packages = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};

          pkg = pkgs.rustPlatform.buildRustPackage {
            pname = "mtg-collection";
            version = "0.1.0";
            src = self;

            cargoLock.lockFile = ./Cargo.lock;

            meta = {
              description = "Manage your MTG card collection; ships the mtg-collection client and mtg-server";
              license = pkgs.lib.licenses.mit;
              mainProgram = "mtg-collection";
            };
          };
        in
        {
          default = pkg;
          server = pkg // {
            meta = pkg.meta // {
              mainProgram = "mtg-server";
            };
          };
        }
      );

      nixosModules.default =
        { config, lib, pkgs, ... }:
        let
          cfg = config.services.mtg-server;
        in
        {
          options.services.mtg-server = with lib; {
            enable = mkEnableOption "the mtg-collection remote server (mtg-server)";

            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.system}.default;
              description = "Package providing the mtg-server binary.";
            };

            bind = mkOption {
              type = types.str;
              default = "0.0.0.0";
              description = "Address to bind the server to.";
            };

            port = mkOption {
              type = types.port;
              default = 8080;
              description = "Port to listen on.";
            };

            collectionPath = mkOption {
              type = types.str;
              default = "/var/lib/mtg-collection/collection.json";
              description = "Collection file the server reads and writes; its parent directory is created via tmpfiles and owned by the mtg-server user.";
            };
          };

          config = lib.mkIf cfg.enable {
            users.groups.mtg-server = { };

            users.users.mtg-server = {
              isSystemUser = true;
              group = "mtg-server";
              description = "mtg-server daemon user";
            };

            systemd.tmpfiles.rules = [
              "d ${lib.dirOf cfg.collectionPath} 0755 mtg-server mtg-server -"
            ];

            systemd.services.mtg-server = {
              description = "mtg-collection remote collection server";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              serviceConfig = {
                Type = "simple";
                User = "mtg-server";
                Group = "mtg-server";
                ExecStart = ''
                  ${cfg.package}/bin/mtg-server \
                    --bind ${cfg.bind} \
                    --port ${toString cfg.port} \
                    --collection ${cfg.collectionPath}
                '';
                Restart = "on-failure";
                RestartSec = "2";
              };
            };
          };
        };

      homeManagerModules.default =
        { config, lib, pkgs, ... }:
        let
          cfg = config.programs.mtg-collection;
          configFile = pkgs.writeText "mtg-collection-config.json" (
            builtins.toJSON {
              server_url = cfg.serverUrl;
            }
          );
        in
        {
          options.programs.mtg-collection = with lib; {
            enable = mkEnableOption "the mtg-collection client and its config";

            package = mkOption {
              type = types.package;
              default = self.packages.${pkgs.system}.default;
              description = "Package providing the mtg-collection and mtg-server binaries.";
            };

            serverUrl = mkOption {
              type = types.nullOr types.str;
              default = null;
              description = "URL of the mtg-server to talk to. null (default) keeps the collection local.";
            };
          };

          config = lib.mkIf cfg.enable {
            home.packages = [ cfg.package ];

            home.file.".config/mtg-collection/config.json".source = configFile;
          };
        };

      devShells = forAllSystems (
        system:
        let
          pkgs = pkgsFor.${system};
        in
        {
          default = pkgs.mkShell {
            buildInputs = with pkgs; [
              rustc
              cargo
              rustfmt
              clippy
              rust-analyzer
            ];

            RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
          };
        }
      );
    };
}