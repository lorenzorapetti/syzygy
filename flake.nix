{
  description = "syzygy — a desktop TIDAL client";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-unstable";
    systems.url = "github:nix-systems/default-linux";
    rust-overlay.url = "github:oxalica/rust-overlay";
    rust-overlay.inputs.nixpkgs.follows = "nixpkgs";
  };

  outputs = {
    self,
    nixpkgs,
    systems,
    rust-overlay,
  }: let
    forEachSystem = f:
      nixpkgs.lib.genAttrs (import systems) (system:
        f (import nixpkgs {
          inherit system;
          overlays = [rust-overlay.overlays.default];
        }));
  in {
    # The dev shell lives in devenv.nix; this flake is for installing the program.
    packages = forEachSystem (pkgs: let
      # The toolchain pinned by rust-toolchain.toml.
      toolchain = pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
    in {
      syzygy = pkgs.callPackage ./nix/package.nix {
        rustPlatform = pkgs.makeRustPlatform {
          cargo = toolchain;
          rustc = toolchain;
        };
      };
      default = self.packages.${pkgs.stdenv.hostPlatform.system}.syzygy;
    });

    apps = forEachSystem (pkgs: {
      default = {
        type = "app";
        program = pkgs.lib.getExe self.packages.${pkgs.stdenv.hostPlatform.system}.syzygy;
      };
    });

    formatter = forEachSystem (pkgs: pkgs.alejandra);
  };
}
