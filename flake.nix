{
  description = "Show random Gray–Scott reaction-diffusion patterns in the terminal";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixpkgs-unstable";

  outputs =
    { nixpkgs, ... }:
    let
      systems = [
        "aarch64-darwin"
        "x86_64-darwin"
        "aarch64-linux"
        "x86_64-linux"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
      fs = nixpkgs.lib.fileset;
      mkPackage =
        pkgs:
        pkgs.rustPlatform.buildRustPackage {
          pname = "turing-pattern";
          version = "0.1.0";
          src = fs.toSource {
            root = ./.;
            fileset = fs.unions [
              ./Cargo.toml
              ./Cargo.lock
              (fs.fileFilter (file: file.hasExt "rs") ./src)
              (fs.fileFilter (file: file.hasExt "rs") ./tests)
              ./examples
            ];
          };
          cargoLock.lockFile = ./Cargo.lock;
          nativeBuildInputs = [ pkgs.makeWrapper ];
          # cargo test runs during the build (unit, exactness, and CLI tests).
          postInstall = ''
            wrapProgram $out/bin/turing-pattern --prefix PATH : ${pkgs.lib.makeBinPath [ pkgs.chafa ]}
          '';
          meta = {
            description = "Show random Gray–Scott reaction-diffusion patterns via chafa";
            mainProgram = "turing-pattern";
            platforms = systems;
          };
        };
    in
    {
      packages = forAllSystems (
        system:
        let
          package = mkPackage nixpkgs.legacyPackages.${system};
        in
        {
          default = package;
          turing-pattern = package;
        }
      );

      devShells = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
        in
        {
          default = pkgs.mkShell {
            packages = [
              pkgs.cargo
              pkgs.rustc
              pkgs.clippy
              pkgs.rustfmt
              pkgs.rust-analyzer
              pkgs.chafa
              pkgs.git
            ];
          };
        }
      );

      checks = forAllSystems (
        system:
        let
          pkgs = nixpkgs.legacyPackages.${system};
          package = mkPackage pkgs;
        in
        {
          inherit package;
          smoke = pkgs.runCommand "turing-pattern-smoke" { nativeBuildInputs = [ package ]; } ''
            turing-pattern --size 17 --steps 2 --seed 42 > smoke.txt
            test -s smoke.txt
            touch "$out"
          '';
        }
      );

      overlays.default = final: _prev: {
        turing-pattern = mkPackage final;
      };
    };
}
