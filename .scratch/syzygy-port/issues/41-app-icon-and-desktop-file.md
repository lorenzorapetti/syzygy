# 41: App icon and desktop file

**What to build:** syzygy has its own icon. The window shows it, and a desktop file lets launchers list syzygy and lets the compositor match its window to that icon. The desktop file and icon install to standard freedesktop paths. Nothing in the spec covers this yet. The nearest is "MPRIS and desktop integration" (user stories 115–117).

**Blocked by:** none

**Status:** ready-for-agent

**Open question:** where does the artwork come from? There is no syzygy icon. sone's (`sone/src-tauri/icons/`) is sone's branding, and syzygy is a separate app (spec: "Workspace and identity"). Either the user supplies an SVG, or an agent draws a simple placeholder that the user approves. Once that's settled, this is `ready-for-agent`. *Settled for now with an agent-drawn placeholder; see Comments.*

- [x] One master icon, `com.lorenzorapetti.syzygy.svg`, plus PNG renders at 16, 32, 48, 64, 128, 256 and 512 px, kept in the repo (e.g. `crates/syzygy/assets/`). The file names use `identity::APP_ID`, so the icon theme, the desktop file and the Wayland `application_id` all agree
- [x] `com.lorenzorapetti.syzygy.desktop` with `Name=Syzygy`, `Exec=syzygy`, `Icon=com.lorenzorapetti.syzygy`, `Categories=AudioVideo;Audio;Player;` and `Terminal=false`. No `StartupWMClass`: syzygy is Wayland only, so the compositor matches the window by its `application_id`, which equals the desktop file's name. It passes `desktop-file-validate`
- [x] The window icon is set in `main.rs`'s `window::Settings` (`icon: Some(window::icon::from_file_data(include_bytes!(…), None)?)`) from an embedded PNG. A bad icon is logged and the window opens without it rather than failing
- [x] A documented way to install for the user: the icons under `~/.local/share/icons/hicolor/<size>/apps/` (SVG in `scalable/apps/`) and the desktop file in `~/.local/share/applications/`, plus `update-desktop-database` and `gtk-update-icon-cache` where present. This can be a README section or a small script. No distro packaging (out of scope, like sone's installers)
- [x] For ticket 33: the MPRIS `DesktopEntry` property is `com.lorenzorapetti.syzygy` (the desktop file name without `.desktop`). It comes from the binary's identity constants, like the bus name and `Identity`. Add a `DESKTOP_ENTRY` constant to `identity.rs` here, and leave a comment on 33 if that ticket is already done

Not tested: assets and packaging are checked by running `desktop-file-validate` and launching from the desktop's launcher, not by Seam tests.

## Comments

**Implementation:** the artwork is an agent-drawn placeholder (three bodies on one line, a syzygy), awaiting the user's approval. Replace `crates/syzygy/assets/com.lorenzorapetti.syzygy.svg` and re-render the PNGs under `assets/icons/<size>x<size>/` (e.g. `rsvg-convert -w N -h N`) to change it. The PNGs sit in hicolor's layout so both installers copy the tree as is. `identity::window_icon()` embeds the 256 px render; a unit test checks it decodes. Install: `scripts/install-desktop-files.sh` (into `$XDG_DATA_HOME`, documented in the README), and the Nix package's `postInstall` puts the same files under `$out/share`, so `nix profile install` exposes them. The script refreshes the icon cache only if one already exists, since creating a user-level cache hides icons other apps add later. `DESKTOP_ENTRY` already existed from ticket 33. `desktop-file-validate` passes.
