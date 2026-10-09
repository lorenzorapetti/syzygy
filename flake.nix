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

    # The toolchain pinned by rust-toolchain.toml, for both the shell and the build.
    toolchainFor = pkgs: pkgs.rust-bin.fromRustupToolchainFile ./rust-toolchain.toml;
  in {
    packages = forEachSystem (pkgs: let
      toolchain = toolchainFor pkgs;
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

    devShells = forEachSystem (pkgs: let
      syzygy = self.packages.${pkgs.stdenv.hostPlatform.system}.syzygy;
    in {
      default = pkgs.mkShell {
        inputsFrom = [syzygy];
        packages = with pkgs; [
          (toolchainFor pkgs)
          git
          python3
        ];

        LD_LIBRARY_PATH = pkgs.lib.makeLibraryPath syzygy.passthru.runtimeLibs;
        GST_PLUGIN_SYSTEM_PATH_1_0 = pkgs.lib.makeSearchPath "lib/gstreamer-1.0" syzygy.passthru.gstPlugins;
        # TLS for GStreamer's HTTPS sources: without it every stream fails to fetch.
        GIO_EXTRA_MODULES = "${pkgs.glib-networking}/lib/gio/modules";
      };
    });

    formatter = forEachSystem (pkgs: pkgs.alejandra);
  };
}
