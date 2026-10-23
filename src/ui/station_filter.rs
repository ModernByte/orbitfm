//! Локальные фильтры и сортировка станций выбранной страны.

use crate::services::Station;

/// Жанровые чипы панели страны.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GenreTag {
    Rock,
    Pop,
    Jazz,
    Electronic,
    News,
    Metal,
}

impl GenreTag {
    pub(crate) const ALL: [GenreTag; 6] = [
        GenreTag::Rock,
        GenreTag::Pop,
        GenreTag::Jazz,
        GenreTag::Electronic,
        GenreTag::News,
        GenreTag::Metal,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            GenreTag::Rock => "Rock",
            GenreTag::Pop => "Pop",
            GenreTag::Jazz => "Jazz",
            GenreTag::Electronic => "Electronic",
            GenreTag::News => "News",
            GenreTag::Metal => "Metal",
        }
    }

    /// Совпадение по подстроке в списке тегов без учёта регистра.
    pub(crate) fn matches(self, tags: &str) -> bool {
        tags.to_lowercase().contains(&self.label().to_lowercase())
    }
}

/// Порядок станций в панели страны; исходная выдача API — по votes.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum StationSort {
    #[default]
    Popularity,
    Bitrate,
    Name,
}

impl StationSort {
    pub(crate) const ALL: [StationSort; 3] = [
        StationSort::Popularity,
        StationSort::Bitrate,
        StationSort::Name,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            StationSort::Popularity => "Popularity",
            StationSort::Bitrate => "Bitrate",
            StationSort::Name => "Name",
        }
    }
}

/// Состояние фильтров панели страны; живёт между кадрами в App Core.
#[derive(Debug, Clone, Default)]
pub(crate) struct CountryFilterState {
    pub text: String,
    pub genre: Option<GenreTag>,
    pub sort: StationSort,
    /// Компактные строки списка (переключатель плотности).
    pub compact_rows: bool,
}

impl CountryFilterState {
    /// Сбрасывает текстовый фильтр и жанр, сохраняя сортировку и плотность.
    pub(crate) fn reset(&mut self) {
        self.text.clear();
        self.genre = None;
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.text.trim().is_empty() || self.genre.is_some()
    }
}

/// Применяет текстовый и жанровый фильтры, затем сортировку.
pub(crate) fn filtered_results<'a>(
    results: &'a [Station],
    filter: &CountryFilterState,
) -> Vec<&'a Station> {
    let text = filter.text.trim().to_lowercase();
    let mut stations: Vec<&Station> = results
        .iter()
        .filter(|station| {
            filter
                .genre
                .is_none_or(|genre| genre.matches(&station.tags))
                && (text.is_empty() || matches_text(station, &text))
        })
        .collect();

    match filter.sort {
        StationSort::Popularity => stations.sort_by(|left, right| {
            right
                .votes
                .cmp(&left.votes)
                .then_with(|| name_key(left).cmp(&name_key(right)))
        }),
        StationSort::Bitrate => stations.sort_by(|left, right| {
            right
                .bitrate
                .cmp(&left.bitrate)
                .then_with(|| name_key(left).cmp(&name_key(right)))
        }),
        StationSort::Name => stations.sort_by_cached_key(|station| name_key(station)),
    }
    stations
}

fn matches_text(station: &Station, query: &str) -> bool {
    [
        &station.name,
        &station.tags,
        &station.country,
        &station.state,
    ]
    .iter()
    .any(|field| field.to_lowercase().contains(query))
}

fn name_key(station: &Station) -> String {
    station.name.to_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn station(name: &str, tags: &str, votes: u32, bitrate: u32) -> Station {
        Station {
            stationuuid: name.to_owned(),
            name: name.to_owned(),
            url: "http://example.com/live".to_owned(),
            url_resolved: "https://example.com/live".to_owned(),
            homepage: None,
            favicon: None,
            tags: tags.to_owned(),
            countrycode: "DE".to_owned(),
            language: "german".to_owned(),
            votes,
            country: "Germany".to_owned(),
            state: "Berlin".to_owned(),
            bitrate,
            codec: "MP3".to_owned(),
            geo_lat: None,
            geo_long: None,
        }
    }

    fn sample() -> Vec<Station> {
        vec![
            station("Berlin Jazz", "jazz,smooth jazz", 50, 128),
            station("Rock Hauptstadt", "rock,alternative", 300, 96),
            station("Nacht-News", "news,talk", 120, 192),
        ]
    }

    #[test]
    fn text_filter_matches_name_tags_country_and_state() {
        let stations = sample();
        let mut filter = CountryFilterState {
            text: "jazz".to_owned(),
            ..CountryFilterState::default()
        };
        assert_eq!(filtered_results(&stations, &filter).len(), 1);

        filter.text = "berlin".to_owned();
        assert_eq!(
            filtered_results(&stations, &filter).len(),
            3,
            "страна и штат должны участвовать в поиске"
        );

        filter.text = "Germany".to_owned();
        assert_eq!(filtered_results(&stations, &filter).len(), 3);
    }

    #[test]
    fn genre_chip_checks_tags_case_insensitively() {
        let stations = sample();
        let filter = CountryFilterState {
            genre: Some(GenreTag::Jazz),
            ..CountryFilterState::default()
        };
        let matches = filtered_results(&stations, &filter);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Berlin Jazz");

        let filter = CountryFilterState {
            genre: Some(GenreTag::Metal),
            ..CountryFilterState::default()
        };
        assert!(filtered_results(&stations, &filter).is_empty());
    }

    #[test]
    fn sorts_by_popularity_bitrate_and_name() {
        let stations = sample();
        let filter = CountryFilterState::default();
        let popular = filtered_results(&stations, &filter);
        assert_eq!(popular[0].name, "Rock Hauptstadt");

        let filter = CountryFilterState {
            sort: StationSort::Bitrate,
            ..CountryFilterState::default()
        };
        let bitrate = filtered_results(&stations, &filter);
        assert_eq!(bitrate[0].name, "Nacht-News");

        let filter = CountryFilterState {
            sort: StationSort::Name,
            ..CountryFilterState::default()
        };
        let names: Vec<&str> = filtered_results(&stations, &filter)
            .iter()
            .map(|station| station.name.as_str())
            .collect();
        assert_eq!(names, vec!["Berlin Jazz", "Nacht-News", "Rock Hauptstadt"]);
    }

    #[test]
    fn reset_clears_text_and_genre_but_keeps_sort() {
        let mut filter = CountryFilterState {
            text: "jazz".to_owned(),
            genre: Some(GenreTag::Jazz),
            sort: StationSort::Name,
            compact_rows: true,
        };
        assert!(filter.is_active());

        filter.reset();
        assert!(!filter.is_active());
        assert_eq!(filter.sort, StationSort::Name);
        assert!(filter.compact_rows, "плотность — настройка вида, не фильтр");
    }
}
