{
  description = "Digicode coding agent with explicit jcode compatibility";

  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";

  outputs = { self, nixpkgs }:
    let
      systems = [
        "x86_64-linux"
        "aarch64-linux"
        "x86_64-darwin"
        "aarch64-darwin"
      ];
      forAllSystems = nixpkgs.lib.genAttrs systems;
    in {
      packages = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          digicode = pkgs.callPackage ./nix/digicode.nix { };
        in {
          inherit digicode;
          # Keep the old package selector available as an explicit compatibility
          # alias, not as the primary product name.
          jcode = digicode;
          default = digicode;
        });

      apps = forAllSystems (system: {
        digicode = {
          type = "app";
          program = "${self.packages.${system}.digicode}/bin/digicode";
          meta = { description = "Run the Digicode executable"; };
        };
        jcode = {
          type = "app";
          program = "${self.packages.${system}.digicode}/bin/jcode";
          meta = { description = "Run the jcode compatibility executable"; };
        };
        default = self.apps.${system}.digicode;
      });

      checks = forAllSystems (system:
        let
          pkgs = import nixpkgs { inherit system; };
          package = self.packages.${system}.digicode;
        in {
          digicode-install = pkgs.runCommand "digicode-install-check" { } ''
            test -x ${package}/bin/digicode
            test -L ${package}/bin/jcode
            test "$(readlink ${package}/bin/jcode)" = digicode
            cmp ${package}/bin/digicode ${package}/bin/jcode
            mkdir -p "$out"
            ${package}/bin/digicode --no-update --no-selfdev --version > "$out/digicode-version.txt"
            ${package}/bin/jcode --no-update --no-selfdev --version > "$out/jcode-version.txt"
            grep -q '0.71.2' "$out/digicode-version.txt"
            cmp "$out/digicode-version.txt" "$out/jcode-version.txt"
          '';
        });
    };
}
