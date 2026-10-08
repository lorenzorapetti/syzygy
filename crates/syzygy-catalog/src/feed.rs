//! The Feed: new releases from the artists the user follows, and their
//! monthly history mixes, newest first and grouped by month as in sone.

use std::time::{SystemTime, UNIX_EPOCH};
use syzygy_tidal::models::{FeedItem, FeedItemKind, FeedResponse};

use crate::home_feed::{self, Card, Target, items, string};

#[derive(Debug, Clone, Default)]
pub struct Feed {
    pub entries: Vec<Entry>,
    /// How many the user hasn't seen.
    pub unseen: u32,
}

/// One release or mix.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub card: Card,
    /// When it came out, by calendar month in UTC.
    pub month: Month,
}

/// A calendar month.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Month {
    pub year: i32,
    /// 1 to 12.
    pub month: u32,
}

/// Which group an entry is shown in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Period {
    ThisMonth,
    LastMonth,
    Older,
}

impl Period {
    pub fn label(self) -> &'static str {
        match self {
            Period::ThisMonth => "This month",
            Period::LastMonth => "Last month",
            Period::Older => "Older",
        }
    }
}

impl Month {
    /// This month, in UTC.
    pub fn now() -> Self {
        let seconds = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |since| since.as_secs());
        Self::of_day((seconds / 86_400) as i64)
    }

    /// The month of an ISO 8601 timestamp, as `2026-10-08T00:00:00.000Z`.
    fn of(timestamp: &str) -> Option<Self> {
        let (year, rest) = timestamp.split_once('-')?;
        let month = rest.get(..2)?;
        let month = Month {
            year: year.parse().ok()?,
            month: month.parse().ok()?,
        };
        (1..=12).contains(&month.month).then_some(month)
    }

    /// The month of a day counted from 1970-01-01 (Howard Hinnant's
    /// `civil_from_days`).
    fn of_day(days: i64) -> Self {
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let day_of_era = z - era * 146_097;
        let year_of_era =
            (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
        let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
        // Counted from March, so the leap day comes last.
        let shifted = (5 * day_of_year + 2) / 153;
        let month = if shifted < 10 {
            shifted + 3
        } else {
            shifted - 9
        };
        let year = year_of_era + era * 400 + i64::from(month <= 2);
        Month {
            year: year as i32,
            month: month as u32,
        }
    }

    /// Months since year 0, so two months can be subtracted.
    fn index(self) -> i64 {
        i64::from(self.year) * 12 + i64::from(self.month) - 1
    }
}

impl Feed {
    /// The entries by how long ago they came out, in their own order, as
    /// sone's `groupFeedByPeriod`: by calendar month, so a December entry
    /// seen in January is last month's. Empty groups are left out.
    pub fn grouped(&self, now: Month) -> Vec<(Period, Vec<&Entry>)> {
        let period = |entry: &Entry| match now.index() - entry.month.index() {
            ..=0 => Period::ThisMonth,
            1 => Period::LastMonth,
            _ => Period::Older,
        };
        [Period::ThisMonth, Period::LastMonth, Period::Older]
            .into_iter()
            .map(|wanted| {
                let entries = self
                    .entries
                    .iter()
                    .filter(|entry| period(entry) == wanted)
                    .collect();
                (wanted, entries)
            })
            .filter(|(_, entries): &(Period, Vec<&Entry>)| !entries.is_empty())
            .collect()
    }
}

impl From<FeedResponse> for Feed {
    fn from(response: FeedResponse) -> Self {
        Feed {
            entries: response.items.iter().filter_map(entry).collect(),
            unseen: response.unseen_count,
        }
    }
}

fn entry(item: &FeedItem) -> Option<Entry> {
    let month = Month::of(&item.occurred_at)?;
    let card = match item.kind {
        FeedItemKind::Album => Card {
            subtitle: release_subtitle(&item.item),
            ..home_feed::card(&item.item, Some("ALBUM_LIST"))
        },
        FeedItemKind::Mix => home_feed::card(&item.item, Some("MIX_LIST")),
        FeedItemKind::Unknown => Card {
            target: Target::None,
            ..home_feed::card(&item.item, None)
        },
    };
    Some(Entry { card, month })
}

/// "Single by Ado, KUROMI", as sone's `feedSubtitle`.
fn release_subtitle(album: &serde_json::Value) -> String {
    let artists: Vec<&str> = items(&album["artists"])
        .iter()
        .filter_map(|artist| string(artist, "name"))
        .collect();
    let artists = artists.join(", ");
    let kind = match string(album, "type") {
        Some("EP") => "EP".to_string(),
        Some(kind) => {
            let lower = kind.to_lowercase();
            let mut chars = lower.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        None => String::new(),
    };
    match (kind.is_empty(), artists.is_empty()) {
        (false, false) => format!("{kind} by {artists}"),
        _ => artists,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{Value, json};

    fn item(kind: FeedItemKind, occurred_at: &str, item: Value) -> FeedItem {
        FeedItem {
            kind,
            activity_type: String::new(),
            occurred_at: occurred_at.to_string(),
            seen: false,
            item,
        }
    }

    fn album(id: u64, album_type: &str, occurred_at: &str) -> FeedItem {
        item(
            FeedItemKind::Album,
            occurred_at,
            json!({
                "id": id,
                "title": format!("Album {id}"),
                "type": album_type,
                "cover": "aa-bb",
                "artists": [{ "name": "Ado" }, { "name": "KUROMI" }],
            }),
        )
    }

    fn feed(items: Vec<FeedItem>) -> Feed {
        Feed::from(FeedResponse {
            items,
            unseen_count: 3,
        })
    }

    #[test]
    fn a_release_leads_to_its_album_and_says_what_it_is_and_by_whom() {
        let feed = feed(vec![
            album(1, "SINGLE", "2026-10-08T00:00:00.000Z"),
            album(2, "EP", "2026-10-01T00:00:00.000Z"),
            album(3, "ALBUM", "2026-10-01T00:00:00.000Z"),
        ]);
        assert_eq!(feed.unseen, 3);
        let subtitles: Vec<&str> = feed
            .entries
            .iter()
            .map(|entry| entry.card.subtitle.as_str())
            .collect();
        assert_eq!(
            subtitles,
            [
                "Single by Ado, KUROMI",
                "EP by Ado, KUROMI",
                "Album by Ado, KUROMI"
            ]
        );
        assert_eq!(feed.entries[0].card.target, Target::Album(1));
        assert_eq!(feed.entries[0].card.title, "Album 1");
        assert_eq!(
            feed.entries[0].month,
            Month {
                year: 2026,
                month: 10
            }
        );
    }

    #[test]
    fn a_release_of_no_known_type_is_just_by_its_artists() {
        let feed = feed(vec![album(1, "", "2026-10-08T00:00:00.000Z")]);
        assert_eq!(feed.entries[0].card.subtitle, "Ado, KUROMI");
    }

    #[test]
    fn a_history_mix_leads_to_the_mix() {
        let feed = feed(vec![item(
            FeedItemKind::Mix,
            "2026-10-01T00:00:00.000Z",
            json!({
                "id": "008bf7c3",
                "title": "September 2026",
                "subTitle": "Your monthly history",
                "mixType": "HISTORY_MONTHLY_MIX",
                "images": { "SMALL": { "url": "https://img/small.jpg" } },
            }),
        )]);
        let card = &feed.entries[0].card;
        assert_eq!(card.target, Target::Mix("008bf7c3".to_string()));
        assert_eq!(card.title, "September 2026");
        assert_eq!(card.subtitle, "Your monthly history");
    }

    #[test]
    fn something_syzygy_doesnt_know_shows_but_leads_nowhere() {
        let feed = feed(vec![item(
            FeedItemKind::Unknown,
            "2026-10-01T00:00:00.000Z",
            json!({ "id": 9, "title": "A video" }),
        )]);
        assert_eq!(feed.entries[0].card.title, "A video");
        assert_eq!(feed.entries[0].card.target, Target::None);
    }

    #[test]
    fn an_entry_with_no_readable_date_is_left_out() {
        let feed = feed(vec![
            album(1, "SINGLE", "yesterday"),
            album(2, "SINGLE", ""),
            album(3, "SINGLE", "2026-13-01T00:00:00.000Z"),
        ]);
        assert!(feed.entries.is_empty());
    }

    fn months(feed: &Feed, now: Month) -> Vec<(Period, Vec<u64>)> {
        feed.grouped(now)
            .into_iter()
            .map(|(period, entries)| {
                let ids = entries
                    .iter()
                    .map(|entry| match entry.card.target {
                        Target::Album(id) => id,
                        _ => 0,
                    })
                    .collect();
                (period, ids)
            })
            .collect()
    }

    #[test]
    fn entries_group_by_calendar_month_keeping_their_order() {
        let feed = feed(vec![
            album(1, "SINGLE", "2026-10-08T00:00:00.000Z"),
            album(2, "SINGLE", "2026-10-01T00:00:00.000Z"),
            album(3, "SINGLE", "2026-09-30T00:00:00.000Z"),
            album(4, "SINGLE", "2026-08-31T00:00:00.000Z"),
            album(5, "SINGLE", "2025-10-08T00:00:00.000Z"),
        ]);
        let now = Month {
            year: 2026,
            month: 10,
        };
        assert_eq!(
            months(&feed, now),
            [
                (Period::ThisMonth, vec![1, 2]),
                (Period::LastMonth, vec![3]),
                (Period::Older, vec![4, 5]),
            ]
        );
    }

    #[test]
    fn last_month_reaches_back_over_new_year_and_empty_groups_are_left_out() {
        let feed = feed(vec![album(1, "SINGLE", "2025-12-31T00:00:00.000Z")]);
        let january = Month {
            year: 2026,
            month: 1,
        };
        assert_eq!(months(&feed, january), [(Period::LastMonth, vec![1])]);
    }

    #[test]
    fn an_entry_from_a_month_still_to_come_counts_as_this_month() {
        let feed = feed(vec![album(1, "SINGLE", "2026-11-01T00:00:00.000Z")]);
        let october = Month {
            year: 2026,
            month: 10,
        };
        assert_eq!(months(&feed, october), [(Period::ThisMonth, vec![1])]);
    }

    #[test]
    fn days_since_1970_land_in_their_month() {
        let month = |year, month| Month { year, month };
        assert_eq!(Month::of_day(0), month(1970, 1));
        assert_eq!(Month::of_day(19_782), month(2024, 2)); // 2024-02-29
        assert_eq!(Month::of_day(19_783), month(2024, 3));
        assert_eq!(Month::of_day(20_453), month(2025, 12));
        assert_eq!(Month::of_day(20_454), month(2026, 1));
        assert!(Month::now() >= month(2026, 1));
    }
}
