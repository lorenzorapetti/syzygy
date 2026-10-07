# 34: Play reporting and clean Quit

**What to build:** plays are reported to TIDAL so they show in "Recently Played", tracks from an Autoplay radio are reported as not chosen by the user, and unsent reports survive restarts. Quitting from the window or the desktop stops playback, flushes reports and saves everything. See user stories 118–121 and the spec's "Crate responsibilities" (`syzygy-report`) and "Bridge between engines and iced" (shutdown).

**Blocked by:** 27, 28

**Status:** ready-for-agent

- [ ] `syzygy-report`: TIDAL play reporting with a persisted queue and its own flush loop
- [ ] The reporter is reached only through `Report…` effects (non-blocking enqueues), and only when reporting is enabled in `Settings`
- [ ] "Chosen by user" is false only for tracks from an Autoplay-started Track radio, whether the advance was gapless or not
- [ ] iced's exit-on-close is off. Close requests and MPRIS Quit map to `Message::Quit`, which runs stop, report flush (~2 s timeout), settings save and snapshot save together, then `iced::exit()`
- [ ] Seam 1 tests cover the "chosen by user" flag on reports
