//! PROTOTYPE. Deterministic fake Catalog: one long playlist plus a Library.

pub const TRACK_COUNT: usize = 5000;
const TRACKS_PER_ALBUM: usize = 12;

pub struct Track {
    pub title: String,
    pub artist: String,
    pub album: String,
    /// Cover id, shared by every track on the album.
    pub cover: u32,
    pub duration: u32,
    pub explicit: bool,
    pub quality: &'static str,
}

pub struct LibraryItem {
    pub name: String,
    pub subtitle: String,
    pub cover: u32,
}

pub struct Catalog {
    pub tracks: Vec<Track>,
    pub library: Vec<LibraryItem>,
}

const WORDS: &[&str] = &[
    "Midnight", "Glass", "Echo", "Velvet", "Static", "Orbit", "Neon", "Silent", "Golden", "Hollow",
    "Paper", "Electric", "Lunar", "Wild", "Cold", "Fading", "Signal", "Ocean", "Ember", "Shadow",
];
const NOUNS: &[&str] = &[
    "Heart", "City", "Rain", "Lights", "Dream", "Wire", "Garden", "Tide", "Season", "Mirror",
    "Highway", "Ghost", "Fever", "Horizon", "Machine", "River", "Bloom", "Frequency",
];
const ARTISTS: &[&str] = &[
    "Aurora Vale", "The Lumen Set", "Kaito Mori", "Señorita Static", "Black Orchid", "Juno & the Tides",
    "Mira Okafor", "Parallel Lines", "Theo Lindqvist", "Ghost Atlas", "Nadia Rue", "Copperhead",
];
const QUALITIES: &[&str] = &["HI_RES_LOSSLESS", "LOSSLESS", "LOSSLESS", "HIGH"];

fn pick<'a>(list: &'a [&'a str], seed: usize) -> &'a str {
    list[seed % list.len()]
}

impl Catalog {
    pub fn fake() -> Self {
        let tracks = (0..TRACK_COUNT)
            .map(|i| {
                let album = i / TRACKS_PER_ALBUM;
                let long = i % 17 == 0;
                let mut title = format!("{} {}", pick(WORDS, i * 7 + 3), pick(NOUNS, i * 5 + 1));
                if long {
                    title.push_str(" (Extended Version) [feat. Someone With A Long Name]");
                }
                Track {
                    title,
                    artist: pick(ARTISTS, album * 3).to_string(),
                    album: format!("{} {}", pick(WORDS, album * 11), pick(NOUNS, album * 13)),
                    cover: album as u32,
                    duration: 95 + ((i * 37) % 260) as u32,
                    explicit: i % 9 == 0,
                    quality: pick(QUALITIES, album),
                }
            })
            .collect();
        let library = (0..40)
            .map(|i| LibraryItem {
                name: format!("{} {}", pick(WORDS, i * 3 + 5), pick(NOUNS, i * 7 + 2)),
                subtitle: if i % 4 == 0 { "Mix".into() } else { format!("Playlist · {}", pick(ARTISTS, i)) },
                cover: 10_000 + i as u32,
            })
            .collect();
        Self { tracks, library }
    }
}

pub fn fmt_time(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}
