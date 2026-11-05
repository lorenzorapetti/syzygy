{
  pkgs,
  lib,
  ...
}: let
  # Only read for its dependency lists, never built here. The toolchain it
  # defaults to is irrelevant: the shell's comes from languages.rust below.
  syzygy = pkgs.callPackage ./nix/package.nix {};
in {
  languages.rust = {
    enable = true;
    # Resolved through rust-overlay, as in the flake's package build.
    toolchainFile = ./rust-toolchain.toml;
  };

  packages =
    [
      pkgs.git
      pkgs.python3
      pkgs.pkg-config
    ]
    ++ syzygy.buildInputs;

  env = {
    LD_LIBRARY_PATH = lib.makeLibraryPath syzygy.passthru.runtimeLibs;
    GST_PLUGIN_SYSTEM_PATH_1_0 = lib.makeSearchPath "lib/gstreamer-1.0" syzygy.passthru.gstPlugins;
    # TLS for GStreamer's HTTPS sources: without it every stream fails to fetch.
    GIO_EXTRA_MODULES = "${pkgs.glib-networking}/lib/gio/modules";
  };
}
