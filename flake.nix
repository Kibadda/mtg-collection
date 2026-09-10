{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs/nixos-26.05";
  };

  outputs =
    { self, nixpkgs }:
    let
      system = "x86_64-linux";

      pkgs = import nixpkgs { inherit system; };

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
      packages.${system} = {
        default = pkg;
        server = pkg // {
          meta = pkg.meta // {
            mainProgram = "mtg-server";
          };
        };
      };

      nixosModules.default =
        { config, lib, ... }:
        let
          cfg = config.services.mtg-server;
        in
        {
          options.services.mtg-server = with lib; {
            enable = mkEnableOption "the mtg-collection remote server (mtg-server)";

            package = mkOption {
              type = types.package;
              default = self.packages.${system}.default;
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
              description = "Collection file the server reads and writes; must live under a StateDirectory the service can write.";
            };
          };

          config = lib.mkIf cfg.enable {
            systemd.services.mtg-server = {
              description = "mtg-collection remote collection server";
              wantedBy = [ "multi-user.target" ];
              after = [ "network.target" ];

              serviceConfig = {
                Type = "simple";
                ExecStart = ''
                  ${cfg.package}/bin/mtg-server \
                    --bind ${cfg.bind} \
                    --port ${toString cfg.port} \
                    --collection ${cfg.collectionPath}
                '';
                DynamicUser = true;
                StateDirectory = "mtg-collection";
                Restart = "on-failure";
                RestartSec = "2";
              };
            };
          };
        };

      devShells.${system}.default = pkgs.mkShell {
        buildInputs = with pkgs; [
          rustc
          cargo
          rustfmt
          clippy
          rust-analyzer
        ];

        RUST_SRC_PATH = pkgs.rustPlatform.rustLibSrc;
      };
    };
}
