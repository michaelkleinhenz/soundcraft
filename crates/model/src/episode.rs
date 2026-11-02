//! Podcast episode details kept with the session: what an MP3 export writes into its tags and
//! what show notes and feeds need.

/// The longest text field kept (characters); longer input is cut.
pub const MAX_EPISODE_TEXT: usize = 16_384;

/// The episode a session produces. Every field is optional; empty text means unset.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct EpisodeInfo {
    /// The show (podcast) name.
    pub show: String,
    /// The episode title.
    pub title: String,
    /// The host or author credited.
    pub author: String,
    pub season: Option<u32>,
    pub episode: Option<u32>,
    /// The publication date as the user writes it (`2026-11-02`); its first four digits are the
    /// year tags carry.
    pub date: String,
    /// Description or show notes.
    pub description: String,
    /// Cover art: a JPEG or PNG file, absolute or relative to the session folder.
    pub artwork: String,
    /// Contains explicit content.
    pub explicit: bool,
}

impl EpisodeInfo {
    /// Whether nothing is set.
    pub fn is_empty(&self) -> bool {
        *self == EpisodeInfo::default()
    }

    /// The year at the start of [`EpisodeInfo::date`], if it starts with four digits.
    pub fn year(&self) -> Option<&str> {
        let y = self.date.trim().get(..4)?;
        y.bytes().all(|b| b.is_ascii_digit()).then_some(y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_year_comes_from_the_date() {
        let mut e = EpisodeInfo { date: "2026-11-02".into(), ..EpisodeInfo::default() };
        assert_eq!(e.year(), Some("2026"));
        e.date = "Nov 2".into();
        assert_eq!(e.year(), None);
        e.date = "20".into();
        assert_eq!(e.year(), None);
    }

    #[test]
    fn a_session_saved_without_episode_details_reads_them_as_empty() {
        let mut v = serde_json::to_value(crate::Session::default()).unwrap();
        v.as_object_mut().unwrap().remove("episode");
        let s: crate::Session = serde_json::from_value(v).unwrap();
        assert!(s.episode.is_empty());
    }
}
