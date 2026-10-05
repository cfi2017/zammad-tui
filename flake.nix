{
  description = "zammad-tui – a TUI client for Zammad helpdesk";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    flake-utils.url = "github:numtide/flake-utils";
    crane.url = "github:ipetkov/crane";
  };

  outputs = {
    self,
    nixpkgs,
    flake-utils,
    crane,
    ...
  }:
    (flake-utils.lib.eachDefaultSystem (system: let
      pkgs = nixpkgs.legacyPackages.${system};
      craneLib = crane.mkLib pkgs;

      src = craneLib.cleanCargoSource ./.;

      commonArgs = {
        inherit src;
        strictDeps = true;

        # aws-lc-sys (rustls crypto backend) needs cmake + perl
        nativeBuildInputs = with pkgs; [
          cmake
          perl
        ];

        meta.mainProgram = "zui";
      };

      # Build deps separately for caching
      cargoArtifacts = craneLib.buildDepsOnly commonArgs;
    in {
      packages.default = craneLib.buildPackage (commonArgs
        // {
          inherit cargoArtifacts;
        });

      checks = {
        build = self.packages.${system}.default;
        rustfmt = craneLib.cargoFmt {inherit src;};
      };

      devShells.default = craneLib.devShell {
        checks = self.checks.${system};
        packages = with pkgs; [
          clippy
          rust-analyzer
        ];
      };
    }))
    // {
      overlays.default = final: prev: {
        zammad-tui = self.packages.${prev.stdenv.hostPlatform.system}.default;
      };
    };
}
