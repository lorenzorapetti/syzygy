# syzygy

A desktop TIDAL client written in Rust with [iced](https://iced.rs): explore
TIDAL's Catalog, edit your Library, and play it with bit-perfect output.

## 🚨 WARNING! Vibe coded slop ahead! 🚨

This is only a vibe coded experiment. It is not a finished product, so expect bugs.
Maybe in the future I can manually check the code and change stuff, but for now, I just don't have the time.

## Credits

syzygy is a port of [sone](https://github.com/lullabyX/sone) by lullabyX. Its
engine code (the TIDAL client, the encrypted disk cache, the audio engine,
MPRIS and play reporting) comes from sone, and its layout follows sone's.

## Licence

GPL-3.0-only, like sone. See [LICENSE](LICENSE).

## Installing

With Nix, `nix profile install github:lorenzorapetti/syzygy` (or the flake's
`packages.default` in your configuration) installs the binary, the desktop
file and the icons.

Without Nix, build and install the binary, then the desktop file and icons
for your user:

```sh
cargo install --path crates/syzygy
./scripts/install-desktop-files.sh
```

The script puts the desktop file in `~/.local/share/applications/` and the
icons under `~/.local/share/icons/hicolor/`, then refreshes the desktop
database (and an existing icon cache) where those tools exist.

## Logs

Logs are written to `~/.local/state/syzygy/logs/`. Set `RUST_LOG` to change
the level, e.g. `RUST_LOG=debug`.
