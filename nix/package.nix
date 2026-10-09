{
  lib,
  rustPlatform,
  pkg-config,
  wrapGAppsNoGuiHook,
  alsa-lib,
  glib-networking,
  gst_all_1,
  wayland,
  libxkbcommon,
  vulkan-loader,
  libGL,
}: let
  cargoToml = lib.importTOML ../Cargo.toml;

  # Loaded at runtime via dlopen by winit/wgpu, so they must be on LD_LIBRARY_PATH.
  # Wayland only: iced is built without X11 (see the spec).
  runtimeLibs = [
    wayland
    libxkbcommon
    vulkan-loader
    libGL
  ];

  # The audio engine (syzygy-audio), as in sone's flake.
  gstPlugins = with gst_all_1; [
    gstreamer
    gst-plugins-base
    gst-plugins-good
    gst-plugins-bad
    gst-libav
  ];
in
  rustPlatform.buildRustPackage {
    pname = "syzygy";
    inherit (cargoToml.workspace.package) version;

    src = lib.fileset.toSource {
      root = ../.;
      fileset = lib.fileset.unions [
        ../Cargo.toml
        ../Cargo.lock
        ../crates
      ];
    };

    cargoLock.lockFile = ../Cargo.lock;
    cargoBuildFlags = ["--package" "syzygy"];
    cargoTestFlags = ["--workspace"];

    nativeBuildInputs = [
      pkg-config
      # Exports GST_PLUGIN_SYSTEM_PATH_1_0 and GIO_EXTRA_MODULES (TLS for
      # GStreamer's HTTPS sources) from buildInputs into the wrapper.
      wrapGAppsNoGuiHook
    ];

    buildInputs =
      [
        # Bit-perfect ALSA output (alsa crate, as in sone/src-tauri)
        alsa-lib
        glib-networking
      ]
      ++ gstPlugins;

    preFixup = ''
      gappsWrapperArgs+=(--prefix LD_LIBRARY_PATH : "${lib.makeLibraryPath runtimeLibs}")
    '';

    passthru = {inherit runtimeLibs gstPlugins;};

    meta = {
      description = "A desktop TIDAL client";
      license = lib.licenses.gpl3Only;
      mainProgram = "syzygy";
      platforms = lib.platforms.linux;
    };
  }
