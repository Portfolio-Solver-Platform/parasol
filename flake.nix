{
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-25.11";
    rust-overlay = {
      url = "github:oxalica/rust-overlay";
      inputs = {
        nixpkgs.follows = "nixpkgs";
      };
    };
    mzn2feat = {
      url = "github:CP-Unibo/mzn2feat";
      flake = false;
    };
  };

  outputs =
    {
      nixpkgs,
      rust-overlay,
      ...
    }@inputs:
    let
      inherit (nixpkgs) lib;
      forAllSystems = lib.genAttrs lib.systems.flakeExposed;
    in
    {
      devShells = forAllSystems (
        system:
        let
          pkgs = import nixpkgs {
            inherit system;
            overlays = [ (import rust-overlay) ];
          };
          lib = pkgs.lib;
          mzn2feat = pkgs.callPackage ./nix/mzn2feat.nix inputs;

          rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
              mzn2feat

              pkgs.rustup
              pkgs.cargo-audit
              pkgs.minizinc
            ];

            env = {
              LD_LIBRARY_PATH = lib.makeLibraryPath (
                with pkgs;
                [
                  # Provide the executables for some builtin solvers in minizinc
                  highs
                  scipopt-scip # TODO: Handle Apache 2.0 license
                  picat # TODO: Look into Mozilla public license 2.0
                ]
              );
            };
          };
        }
      );
    };
}
