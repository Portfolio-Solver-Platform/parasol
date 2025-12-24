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
          minizinc = pkgs.callPackage ./nix/minizinc { };
          mzn2feat = pkgs.callPackage ./nix/mzn2feat.nix { src = inputs.mzn2feat; };

          rustToolchain = pkgs.pkgsBuildHost.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
        in
        {
          default = pkgs.mkShell {
            packages = [
              rustToolchain
              mzn2feat
              pkgs.util-linux
              pkgs.rustup
              pkgs.cargo-audit
              minizinc
            ];
          };
        }
      );
    };
}
