# 31: Settings modal

**What to build:** the user opens Settings as a modal (never part of the Back stack) and controls quality, output and playback preferences. Combinations that don't work can't be chosen, and the user is told when output isn't bit-perfect. See user stories 32, 107–112, 114 and the spec's "Settings modal" and "Playback model" (volume and bit-perfect).

**Blocked by:** 26

**Status:** ready-for-agent

- [ ] Settings opens as a Shell modal from the header, and Back never reopens it
- [ ] Max quality (Hi-Res lossless, Lossless, High) feeds `resolve_stream`
- [ ] Exclusive mode with the output device list. Turning exclusive mode or bit-perfect on or off changes the other. Enabling bit-perfect with no exclusive device picks the first listed device
- [ ] Bit-perfect locks the volume slider at 100% and turns normalization off, ramping with `Effect::RampVolume { to, over }` rather than jumping
- [ ] Gapless (disabled while unsupported, or while exclusive mode or bit-perfect is on), volume normalization, Autoplay, allow explicit content, and report plays to TIDAL
- [ ] Each control sends a message that changes `Settings` (and playback state where it applies) and returns the matching effects and a save `Task`
- [ ] Info toasts when the audio engine resamples or changes bit depth
- [ ] Seam 1 tests cover the bit-perfect and exclusive linkage
