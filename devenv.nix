{
  pkgs,
  lib,
  ...
}:

let
  # Loaded at runtime via dlopen by winit/wgpu, so they must be on LD_LIBRARY_PATH.
  runtimeLibs = with pkgs; [
    wayland
    libxkbcommon
    vulkan-loader
    libGL
    xorg.libX11
    xorg.libXcursor
    xorg.libXi
    xorg.libXrandr
  ];

  # The audio engine (syzygy-audio), as in sone's flake.
  gstPlugins = with pkgs.gst_all_1; [
    gstreamer
    gst-plugins-base
    gst-plugins-good
    gst-plugins-bad
    gst-libav
  ];
in
{
  # https://devenv.sh/languages/
  languages.rust = {
    enable = true;
    components = [
      "rustc"
      "cargo"
      "clippy"
      "rustfmt"
      "rust-analyzer"
    ];
  };

  # https://devenv.sh/packages/
  packages =
    with pkgs;
    [
      git
      pkg-config
      # Bit-perfect ALSA output (alsa crate, as in sone/src-tauri)
      alsa-lib
      openssl
      fontconfig
      freetype
      gst_all_1.gstreamer.dev
      gst_all_1.gst-plugins-base.dev
    ]
    ++ runtimeLibs;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;
  env.GST_PLUGIN_SYSTEM_PATH_1_0 = lib.makeSearchPath "lib/gstreamer-1.0" gstPlugins;
  # TLS for GStreamer's HTTPS sources: without it every stream fails to fetch.
  env.GIO_EXTRA_MODULES = "${pkgs.glib-networking}/lib/gio/modules";

  enterTest = ''
    cargo test
  '';

  # See full reference at https://devenv.sh/reference/options/
}
