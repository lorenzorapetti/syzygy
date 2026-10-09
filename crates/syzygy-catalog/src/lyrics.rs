//! A track's lyrics: plain text, and synced lines when TIDAL sends LRC.

use syzygy_tidal::models::TidalLyrics;

#[derive(Debug, Clone, PartialEq)]
pub struct Lyrics {
    /// The words as plain text, for when there are no synced lines.
    pub plain: Option<String>,
    /// The lines with when each starts, sorted, when TIDAL's `subtitles`
    /// parse as LRC.
    pub synced: Option<Vec<Line>>,
    /// Laid out from the right: Arabic, Hebrew.
    pub right_to_left: bool,
    /// Who TIDAL got them from.
    pub provider: Option<String>,
}

/// One synced line.
#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    /// When it starts, in seconds.
    pub at: f32,
    pub text: String,
}

impl Lyrics {
    /// The synced line sung at `position` seconds: the last one to have
    /// started. `None` before the first, or without synced lines.
    pub fn active(&self, position: f32) -> Option<usize> {
        let lines = self.synced.as_ref()?;
        lines
            .partition_point(|line| line.at <= position)
            .checked_sub(1)
    }
}

impl From<TidalLyrics> for Lyrics {
    fn from(lyrics: TidalLyrics) -> Self {
        Lyrics {
            plain: lyrics.lyrics.filter(|plain| !plain.trim().is_empty()),
            synced: lyrics.subtitles.as_deref().and_then(lrc),
            right_to_left: lyrics.is_right_to_left,
            provider: lyrics.lyrics_provider,
        }
    }
}

/// The timed lines of LRC text, sorted. A line with several timestamps is
/// sung at each; lines without one, and tags like `[ar:…]`, are skipped.
/// `None` if no line is timed.
fn lrc(text: &str) -> Option<Vec<Line>> {
    let mut lines = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some((time, after)) = rest
            .strip_prefix('[')
            .and_then(|inner| inner.split_once(']'))
            .and_then(|(stamp, after)| Some((timestamp(stamp)?, after)))
        {
            times.push(time);
            rest = after;
        }
        let words = rest.trim();
        lines.extend(times.into_iter().map(|at| Line {
            at,
            text: words.to_string(),
        }));
    }
    // Stable, so lines at the same time keep their order.
    lines.sort_by(|a, b| a.at.total_cmp(&b.at));
    (!lines.is_empty()).then_some(lines)
}

/// `mm:ss`, `mm:ss.xx` or `mm:ss.xxx`, in seconds.
fn timestamp(stamp: &str) -> Option<f32> {
    let (minutes, seconds) = stamp.split_once(':')?;
    let minutes: u32 = minutes.trim().parse().ok()?;
    let seconds: f32 = seconds.trim().parse().ok()?;
    (seconds.is_finite() && seconds >= 0.0).then(|| minutes as f32 * 60.0 + seconds)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn lyrics(fields: serde_json::Value) -> Lyrics {
        let lyrics: TidalLyrics = serde_json::from_value(fields).expect("lyrics");
        Lyrics::from(lyrics)
    }

    fn line(at: f32, text: &str) -> Line {
        Line {
            at,
            text: text.to_string(),
        }
    }

    #[test]
    fn lyrics_keep_the_plain_text_the_direction_and_the_provider() {
        let lyrics = lyrics(json!({
            "trackId": 11,
            "lyricsProvider": "MusixMatch",
            "lyrics": "First\nSecond",
            "isRightToLeft": true,
        }));

        assert_eq!(lyrics.plain.as_deref(), Some("First\nSecond"));
        assert_eq!(lyrics.synced, None);
        assert!(lyrics.right_to_left);
        assert_eq!(lyrics.provider.as_deref(), Some("MusixMatch"));
    }

    #[test]
    fn subtitles_parse_as_lrc_lines() {
        let lyrics = lyrics(json!({
            "subtitles": "[00:12.34] First line\n[00:15.5]Second line\n[01:02.123] Third",
        }));

        assert_eq!(
            lyrics.synced,
            Some(vec![
                line(12.34, "First line"),
                line(15.5, "Second line"),
                line(62.123, "Third"),
            ])
        );
    }

    #[test]
    fn a_line_with_several_timestamps_is_sung_at_each_and_the_lines_are_sorted() {
        let lyrics = lyrics(json!({
            "subtitles": "[00:30.00]Verse\n[00:10.00][00:50.00]Chorus\n[00:40]Bridge",
        }));

        assert_eq!(
            lyrics.synced,
            Some(vec![
                line(10.0, "Chorus"),
                line(30.0, "Verse"),
                line(40.0, "Bridge"),
                line(50.0, "Chorus"),
            ])
        );
    }

    #[test]
    fn untimed_lines_and_tags_are_skipped_and_blank_lines_kept() {
        let lyrics = lyrics(json!({
            "subtitles": "[ar:Björk]\n[offset:+100]\nno timestamp\n\n[00:01.00]\n[00:02.00]Words",
        }));

        assert_eq!(lyrics.synced, Some(vec![line(1.0, ""), line(2.0, "Words")]));
    }

    #[test]
    fn subtitles_without_a_timed_line_are_not_synced() {
        for subtitles in ["", "just words\nno times", "[ar:Björk]"] {
            assert_eq!(lyrics(json!({ "subtitles": subtitles })).synced, None);
        }
        assert_eq!(lyrics(json!({})).synced, None);
    }

    #[test]
    fn empty_plain_lyrics_are_none() {
        assert_eq!(lyrics(json!({ "lyrics": "  \n " })).plain, None);
    }

    #[test]
    fn the_active_line_is_the_last_to_have_started() {
        let lyrics = lyrics(json!({
            "subtitles": "[00:10.00]One\n[00:20.00]Two\n[00:30.00]Three",
        }));

        assert_eq!(lyrics.active(0.0), None);
        assert_eq!(lyrics.active(9.99), None);
        assert_eq!(lyrics.active(10.0), Some(0));
        assert_eq!(lyrics.active(25.0), Some(1));
        assert_eq!(lyrics.active(30.0), Some(2));
        assert_eq!(lyrics.active(500.0), Some(2));
    }

    #[test]
    fn plain_lyrics_have_no_active_line() {
        assert_eq!(lyrics(json!({ "lyrics": "Words" })).active(10.0), None);
    }
}
