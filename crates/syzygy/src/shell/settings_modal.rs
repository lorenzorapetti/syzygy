//! The Settings modal: quality, output and playback preferences. It sits
//! over the Shell and is never on the Back stack.
//!
//! Each control sends the message that changes it: playback's through
//! `playback::Message`, which keeps bit-perfect, exclusive mode and the
//! volume in step, the rest through `settings::Preference`. Combinations
//! that don't work can't be chosen.

use iced::widget::{
    button, center, column, container, mouse_area, opaque, pick_list, row, scrollable, space, text,
    toggler,
};
use iced::{Alignment, Element, Length};
use std::fmt;
use syzygy_audio::AudioDevice;
use syzygy_tidal::Quality;

use crate::app;
use crate::icons::{Icon, icon};
use crate::playback::{self, Playback};
use crate::settings::{Preference, Settings};
use crate::style;

/// What the machine offers, listed off the UI thread as the modal opens.
#[derive(Debug, Clone)]
pub struct Hardware {
    /// ALSA devices for exclusive output, or why they couldn't be listed.
    pub devices: Result<Vec<AudioDevice>, String>,
    pub gapless_supported: bool,
}

impl Hardware {
    /// Ask GStreamer. Blocks for up to two seconds.
    pub fn list() -> Self {
        let devices = syzygy_audio::list_alsa_devices();
        Self {
            // Listing the devices starts GStreamer, which this asks.
            gapless_supported: syzygy_audio::gapless_supported(),
            devices,
        }
    }
}

#[derive(Default)]
pub struct SettingsModal {
    /// `None` while it's listed.
    hardware: Option<Hardware>,
}

#[derive(Debug, Clone)]
pub enum Message {
    Listed(Hardware),
    /// The close button, or a click outside.
    Close,
    Quality(QualityChoice),
    Exclusive(bool),
    Device(DeviceChoice),
    BitPerfect(bool),
    Gapless(bool),
    Normalization(bool),
    Autoplay(bool),
    AllowExplicit(bool),
    ReportPlays(bool),
}

impl SettingsModal {
    /// What to send for the control the user changed. Closing is the
    /// Shell's.
    pub fn update(&mut self, message: Message) -> Option<app::Message> {
        let to_playback = |message| Some(app::Message::Playback(message));
        let preference = |preference| Some(app::Message::Preference(preference));
        match message {
            Message::Listed(hardware) => {
                if let Err(e) = &hardware.devices {
                    log::warn!("Could not list the audio devices: {e}");
                }
                self.hardware = Some(hardware);
                None
            }
            Message::Close => None,
            Message::Quality(QualityChoice(quality)) => preference(Preference::MaxQuality(quality)),
            Message::Exclusive(on) => to_playback(playback::Message::Exclusive {
                on,
                first_device: self.first_device(),
            }),
            Message::Device(DeviceChoice(device)) => {
                to_playback(playback::Message::OutputDevice(device.id))
            }
            Message::BitPerfect(on) => to_playback(playback::Message::BitPerfect {
                on,
                first_device: self.first_device(),
            }),
            Message::Gapless(on) => to_playback(playback::Message::Gapless(on)),
            Message::Normalization(on) => to_playback(playback::Message::Normalization(on)),
            Message::Autoplay(on) => to_playback(playback::Message::Autoplay(on)),
            Message::AllowExplicit(on) => to_playback(playback::Message::AllowExplicit(on)),
            Message::ReportPlays(on) => preference(Preference::ReportPlays(on)),
        }
    }

    /// The device exclusive output takes when none was chosen.
    fn first_device(&self) -> Option<String> {
        let devices = self.hardware.as_ref()?.devices.as_ref().ok()?;
        devices.first().map(|device| device.id.clone())
    }

    /// The modal, on a backdrop that darkens what's under it and takes its
    /// clicks. A click outside closes it.
    pub fn view<'a>(
        &'a self,
        settings: &'a Settings,
        playback: &'a Playback,
    ) -> Element<'a, Message> {
        let output = playback.output();
        let close = button(icon(Icon::X, 20.0, style::TEXT_SECONDARY))
            .padding(4)
            .style(style::icon_button)
            .on_press(Message::Close);
        let title =
            row![text("Settings").size(20), space::horizontal(), close].align_y(Alignment::Center);

        let quality = setting(
            "Max quality",
            "The best TIDAL streams, up to this",
            pick_list(
                QualityChoice::ALL,
                QualityChoice::ALL
                    .into_iter()
                    .find(|choice| choice.offers(settings.max_quality)),
                Message::Quality,
            )
            .text_size(14)
            .into(),
        );

        // Exclusive output needs a device: until one is listed, it can only
        // be turned off.
        let has_device = output.device.is_some() || self.first_device().is_some();
        let exclusive = switch(
            "Exclusive mode",
            "Send sound straight to an output device, bypassing the system mixer",
            output.exclusive,
            (output.exclusive || has_device).then_some(Message::Exclusive as fn(bool) -> Message),
        );
        let device = output.exclusive.then(|| self.devices(output));
        let bit_perfect = switch(
            "Bit-perfect",
            "Untouched output: the volume stays at 100% and normalization is off",
            output.bit_perfect,
            (output.bit_perfect || has_device)
                .then_some(Message::BitPerfect as fn(bool) -> Message),
        );

        let gapless_supported = self
            .hardware
            .as_ref()
            .is_none_or(|hardware| hardware.gapless_supported);
        let gapless_note = if !gapless_supported {
            "Not supported by this GStreamer"
        } else if output.exclusive {
            "Not available with exclusive mode or bit-perfect"
        } else {
            "Change tracks with no silence between them"
        };
        let gapless = switch(
            "Gapless playback",
            gapless_note,
            playback.gapless(),
            // Bit-perfect output is always exclusive.
            (gapless_supported && !output.exclusive).then_some(Message::Gapless),
        );
        let normalization = switch(
            "Volume normalization",
            if output.bit_perfect {
                "Off while bit-perfect"
            } else {
                "Level tracks to the same loudness"
            },
            playback.normalization(),
            (!output.bit_perfect).then_some(Message::Normalization),
        );
        let autoplay = switch(
            "Autoplay",
            "When nothing is left, play the last track's radio",
            playback.autoplay(),
            Some(Message::Autoplay),
        );
        let explicit = switch(
            "Allow explicit content",
            "Play tracks TIDAL marks explicit",
            playback.allow_explicit(),
            Some(Message::AllowExplicit),
        );
        let report = switch(
            "Report plays to TIDAL",
            "Show what you play in TIDAL's Recently Played",
            settings.report_plays,
            Some(Message::ReportPlays),
        );

        let body = column![heading("Quality"), quality, heading("Output"), exclusive,]
            .push(device)
            .push(bit_perfect)
            .push(heading("Playback"))
            .push(gapless)
            .push(normalization)
            .push(autoplay)
            .push(explicit)
            .push(report)
            .spacing(16);

        let card = container(
            column![
                title,
                // Tall enough for every setting; only a short window
                // scrolls it, with no bar.
                scrollable(body)
                    .direction(scrollable::Direction::Vertical(
                        scrollable::Scrollbar::hidden(),
                    ))
                    .height(Length::Shrink)
            ]
            .spacing(16),
        )
        .padding(24)
        .max_width(520)
        .style(style::modal);
        let backdrop = center(opaque(card)).style(style::backdrop);
        opaque(mouse_area(backdrop).on_press(Message::Close))
    }

    /// The device list under exclusive mode, while it's listed, or why it
    /// couldn't be.
    fn devices<'a>(&'a self, output: &playback::Output) -> Element<'a, Message> {
        let control: Element<'a, Message> = match &self.hardware {
            None => note("Looking for devices\u{2026}"),
            Some(Hardware {
                devices: Err(_), ..
            }) => note("Couldn't list the output devices"),
            Some(Hardware {
                devices: Ok(devices),
                ..
            }) if devices.is_empty() => note("No output devices found"),
            Some(Hardware {
                devices: Ok(devices),
                ..
            }) => {
                let choices: Vec<_> = devices.iter().cloned().map(DeviceChoice).collect();
                let selected = choices
                    .iter()
                    .find(|choice| Some(&choice.0.id) == output.device.as_ref())
                    .cloned();
                // A device chosen before that isn't plugged in now.
                let placeholder = output.device.clone().unwrap_or_default();
                pick_list(choices, selected, Message::Device)
                    .placeholder(placeholder)
                    .text_size(14)
                    .width(240)
                    .into()
            }
        };
        setting("Output device", "Where exclusive output goes", control)
    }
}

/// A max quality the modal offers. TIDAL's Hi-Res tier isn't one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QualityChoice(Quality);

impl QualityChoice {
    const ALL: [QualityChoice; 3] = [
        QualityChoice(Quality::HiResLossless),
        QualityChoice(Quality::Lossless),
        QualityChoice(Quality::High),
    ];
}

impl QualityChoice {
    /// Whether this is the choice for a saved max quality. TIDAL's Hi-Res
    /// tier, from an older settings file, shows as Hi-Res lossless.
    fn offers(self, quality: Quality) -> bool {
        match quality {
            Quality::HiRes => self.0 == Quality::HiResLossless,
            quality => self.0 == quality,
        }
    }
}

impl fmt::Display for QualityChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self.0 {
            Quality::HiResLossless | Quality::HiRes => "Hi-Res lossless",
            Quality::Lossless => "Lossless",
            Quality::High => "High",
        })
    }
}

/// An output device, by its name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceChoice(AudioDevice);

impl fmt::Display for DeviceChoice {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0.name)
    }
}

fn heading<'a>(label: &'a str) -> Element<'a, Message> {
    text(label).size(13).color(style::TEXT_MUTED).into()
}

fn note<'a>(label: &'a str) -> Element<'a, Message> {
    text(label).size(14).color(style::TEXT_SECONDARY).into()
}

/// A labelled row with its control on the right.
fn setting<'a>(
    label: &'a str,
    description: &'a str,
    control: Element<'a, Message>,
) -> Element<'a, Message> {
    row![
        column![
            text(label).size(15),
            text(description).size(13).color(style::TEXT_SECONDARY),
        ]
        .spacing(2)
        .width(Length::Fill),
        control,
    ]
    .spacing(16)
    .align_y(Alignment::Center)
    .into()
}

/// An on/off setting; without `on_toggle` it can't be changed.
fn switch<'a>(
    label: &'a str,
    description: &'a str,
    on: bool,
    on_toggle: Option<fn(bool) -> Message>,
) -> Element<'a, Message> {
    setting(
        label,
        description,
        toggler(on).size(20).on_toggle_maybe(on_toggle).into(),
    )
}
