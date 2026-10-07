# syzygy

A desktop TIDAL client written in Rust with [iced](https://iced.rs): explore
TIDAL's Catalog, edit your Library, and play it with bit-perfect output.

## Credits

syzygy is a port of [sone](https://github.com/lullabyX/sone) by lullabyX. Its
engine code (the TIDAL client, the encrypted disk cache, the audio engine,
MPRIS and play reporting) comes from sone, and its layout follows sone's.

## Licence

GPL-3.0-only, like sone. See [LICENSE](LICENSE).

## Logs

Logs are written to `~/.local/state/syzygy/logs/`. Set `RUST_LOG` to change
the level, e.g. `RUST_LOG=debug`.
