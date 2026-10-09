# 31: Settings modal

**What to build:** the user opens Settings as a modal (never part of the Back stack) and controls quality, output and playback preferences. Combinations that don't work can't be chosen, and the user is told when output isn't bit-perfect. See user stories 32, 107–112, 114 and the spec's "Settings modal" and "Playback model" (volume and bit-perfect).

**Blocked by:** 26

**Status:** ready-for-agent

- [x] Settings opens as a Shell modal from the header, and Back never reopens it
- [x] Max quality (Hi-Res lossless, Lossless, High) feeds `resolve_stream`
- [x] Exclusive mode with the output device list. Turning exclusive mode or bit-perfect on or off changes the other. Enabling bit-perfect with no exclusive device picks the first listed device
- [x] Bit-perfect locks the volume slider at 100% and turns normalization off, ramping with `Effect::RampVolume { to, over }` rather than jumping
- [x] Gapless (disabled while unsupported, or while exclusive mode or bit-perfect is on), volume normalization, Autoplay, allow explicit content, and report plays to TIDAL
- [x] Each control sends a message that changes `Settings` (and playback state where it applies) and returns the matching effects and a save `Task`
- [x] Info toasts when the audio engine resamples or changes bit depth
- [x] Seam 1 tests cover the bit-perfect and exclusive linkage

## Comments

Implemented. Notes for later tickets:

- **Playback holds the output.** `playback::Output { exclusive, device, bit_perfect }` and normalization are playback state now, with volume and mute. New messages: `Normalization(bool)`, `Exclusive { on, first_device }`, `BitPerfect { on, first_device }`, `OutputDevice(String)`. `first_device` is the first device the modal listed: exclusive mode or bit-perfect turned on with no device chosen takes it. (The ticket asks this only of bit-perfect. Exclusive mode does it too, because the engine can't play exclusive without a device.) New effects: `RampVolume { from, to, over }` (the spec's `{ to, over }` plus `from`, so the runner needn't track the engine's volume), `SetNormalization(bool)`, `SetOutput(Output)` and `SetGapless(bool)`.
- **Linkage.** Bit-perfect on puts `Levels { volume, normalization }` aside (saved as `Settings::before_bit_perfect`, so it outlives a restart), turns exclusive on, and slides to full volume with normalization off. Bit-perfect off slides back and leaves exclusive on. Exclusive off takes bit-perfect off the same way. While bit-perfect, `SetVolume`, `ToggleMute` and `Normalization` do nothing. Nothing is armed while exclusive (the engine never advances with no gap there), so turning it on clears the arm.
- **Gapless fix.** `Message::Gapless` (ticket 27) changed playback but never reached the engine, so turning gapless back on did nothing until a restart. It now emits `SetGapless`.
- **App.** One `save_preferences` replaces `save_volume` and `save_modes`: it copies every preference playback holds into `Settings`. Settings-only ones (`MaxQuality`, `ReportPlays`) go through `app::Message::Preference(settings::Preference)`. `configure_engine` now sends the saved output at boot too. The ramp is 12 `set_volume` steps over 300 ms in an abortable task; a `SetVolume` aborts it. Normalization off sets the gain to 1 at once. Turned on, it applies from the next track, because the App doesn't keep the playing track's ReplayGain.
- **Not done.** Changing the output applies from the next play, as in the engine: the playing track isn't restarted on the new output.
- **Modal** (`shell/settings_modal.rs`). The gear left of the avatar opens it. As it opens it lists the ALSA devices and asks whether gapless is supported, in `spawn_blocking` (the list can wait up to 2 s). Escape, the close button, a click outside, or navigating closes it, and it is never on the Back stack. Exclusive mode and bit-perfect can only be turned off until a device is known. Gapless is disabled while it's unsupported or exclusive. A saved `Quality::HiRes` shows as Hi-Res lossless. `style::modal` and `style::backdrop` are shared with the consent modal. Logout is ticket 32's.
- **Player bar.** While bit-perfect, mute is disabled and a static full rail stands in for the volume slider (iced 0.14's slider can't be disabled).
- **Toasts.** `Resampled` gives "Resampling 96 → 48 kHz" and `BitDepthChanged` gives "Bit depth changed: S24LE → S32LE", both info toasts.
- **CONTEXT.md** gains an Output section: Max quality, Exclusive mode, Bit-perfect.
- **Tested:** 13 new Seam 1 tests (bit-perfect on and off, exclusive linkage, the first device, the lock, mute across bit-perfect, arming under exclusive, saved levels after a restart, gapless reaching the engine), 504 in the workspace. **Run:** the modal was opened by a temporary patch at boot and screenshotted. It drew as intended. The toggles weren't clicked, and no bit-perfect output was heard.
