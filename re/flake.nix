{
  description = "Reverse-engineering environment: REA, Ghidra, innoextract (see README.md)";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs =
    { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
      ];
      forAllSystems = f: nixpkgs.lib.genAttrs systems (system: f nixpkgs.legacyPackages.${system});
    in
    {
      packages = forAllSystems (pkgs: rec {
        rea = pkgs.callPackage ./rea/package.nix { };
        default = rea;
      });

      apps = forAllSystems (pkgs: rec {
        rea = {
          type = "app";
          program = "${self.packages.${pkgs.stdenv.hostPlatform.system}.rea}/bin/rea";
        };
        default = rea;
      });

      devShells = forAllSystems (pkgs: rec {
        # `nix develop ./re`: REA with Ghidra wired up, plus innoextract for Inno Setup installers.
        analysis = pkgs.mkShell {
          packages = [
            self.packages.${pkgs.stdenv.hostPlatform.system}.rea
            pkgs.nodejs_22
            pkgs.procps
            pkgs.ghidra
            pkgs.innoextract
            pkgs.python3
          ];
          GHIDRA_INSTALL_DIR = "${pkgs.ghidra}/lib/ghidra";
          JAVA_HOME = "${pkgs.ghidra.passthru.jdk or pkgs.jdk21}/lib/openjdk";
        };
        default = analysis;
      });
    };
}
