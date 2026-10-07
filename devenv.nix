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
    ]
    ++ runtimeLibs;

  env.LD_LIBRARY_PATH = lib.makeLibraryPath runtimeLibs;

  enterTest = ''
    cargo test
  '';

  # See full reference at https://devenv.sh/reference/options/
}
