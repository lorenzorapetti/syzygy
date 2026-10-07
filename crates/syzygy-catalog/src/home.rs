//! The home feed and its cache encoding.

use syzygy_tidal::models::HomePageResponse;

/// A home feed with no sections is an upstream failure wearing a success mask,
/// never content — storing one blanks Home for the whole 4h fresh window.
pub(crate) fn encode(home: &HomePageResponse) -> Option<Vec<u8>> {
    if home.sections.is_empty() {
        return None;
    }
    serde_json::to_vec(home).ok()
}

/// The read-side half of [`encode`]: an entry with no sections is a miss, so
/// caches poisoned by earlier builds heal on the next launch instead of
/// waiting out the 24h stale window.
pub(crate) fn decode(bytes: &[u8]) -> Option<HomePageResponse> {
    let home = serde_json::from_slice::<HomePageResponse>(bytes).ok()?;
    if home.sections.is_empty() {
        return None;
    }
    Some(home)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use syzygy_tidal::models::HomePageSection;

    fn section(title: &str) -> HomePageSection {
        HomePageSection {
            title: title.to_string(),
            section_type: "HORIZONTAL_LIST".to_string(),
            items: Value::Array(vec![]),
            has_more: false,
            api_path: None,
        }
    }

    fn home(sections: Vec<HomePageSection>) -> HomePageResponse {
        HomePageResponse {
            tabs: vec![],
            sections,
            cursor: None,
        }
    }

    #[test]
    fn empty_home_is_never_written_to_cache() {
        assert!(encode(&home(vec![])).is_none());
    }

    #[test]
    fn home_with_sections_is_written_to_cache() {
        assert!(encode(&home(vec![section("Recently played")])).is_some());
    }

    #[test]
    fn an_already_cached_empty_home_reads_back_as_a_miss() {
        let bytes = serde_json::to_vec(&home(vec![])).expect("serialize");
        assert!(decode(&bytes).is_none());
    }

    #[test]
    fn a_cached_home_with_sections_reads_back() {
        let bytes = serde_json::to_vec(&home(vec![section("Mixes for you")])).expect("serialize");
        let decoded = decode(&bytes).expect("should decode");
        assert_eq!(decoded.sections.len(), 1);
        assert_eq!(decoded.sections[0].title, "Mixes for you");
    }

    #[test]
    fn unparseable_cache_bytes_read_back_as_a_miss() {
        assert!(decode(b"not json").is_none());
    }
}
