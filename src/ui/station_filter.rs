//! Локальные фильтры и сортировка станций выбранной страны.

use crate::i18n;
use crate::services::Station;
use crate::settings::Language;

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

    /// Подпись чипа для языка интерфейса (ADR-0030).
    pub(crate) fn label(self, language: Language) -> &'static str {
        let labels = &i18n::catalog(language).labels;
        match self {
            GenreTag::Rock => labels.genre_rock,
            GenreTag::Pop => labels.genre_pop,
            GenreTag::Jazz => labels.genre_jazz,
            GenreTag::Electronic => labels.genre_electronic,
            GenreTag::News => labels.genre_news,
            GenreTag::Metal => labels.genre_metal,
        }
    }

    /// Совпадение по подстроке в списке тегов без учёта регистра. Сравниваем с
    /// английским источником: теги Radio Browser — данные, они не переводятся.
    pub(crate) fn matches(self, tags: &str) -> bool {
        tags.to_lowercase()
            .contains(&self.label(Language::English).to_lowercase())
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

    /// Подпись пункта сортировки для языка интерфейса (ADR-0030).
    pub(crate) fn label(self, language: Language) -> &'static str {
        let labels = &i18n::catalog(language).labels;
        match self {
            StationSort::Popularity => labels.sort_popularity,
            StationSort::Bitrate => labels.sort_bitrate,
            StationSort::Name => labels.sort_name,
        }
    }
}

/// Состояние фильтров панели страны; живёт между кадрами в App Core.
#[derive(Debug, Clone)]
pub(crate) struct CountryFilterState {
    pub text: String,
    /// Мультивыбор жанров: пусто — без ограничения, иначе достаточно
    /// совпадения хотя бы одного тега.
    pub genres: Vec<GenreTag>,
    /// Показана ли строка жанров и бейджи выбранных фильтров. По умолчанию
    /// фильтры видны: это рабочая строка списка, а не скрытая настройка.
    pub filters_visible: bool,
    pub sort: StationSort,
    /// Компактные строки списка (переключатель плотности).
    pub compact_rows: bool,
}

impl Default for CountryFilterState {
    fn default() -> Self {
        Self {
            text: String::new(),
            genres: Vec::new(),
            filters_visible: true,
            sort: StationSort::default(),
            compact_rows: false,
        }
    }
}

impl CountryFilterState {
    /// Сбрасывает текстовый фильтр и жанры, сохраняя сортировку, плотность и
    /// видимость строки фильтров.
    pub(crate) fn reset(&mut self) {
        self.text.clear();
        self.genres.clear();
    }

    pub(crate) fn is_active(&self) -> bool {
        !self.text.trim().is_empty() || !self.genres.is_empty()
    }

    /// Добавляет или убирает жанр мультивыбора, сохраняя порядок выбора.
    pub(crate) fn toggle_genre(&mut self, genre: GenreTag) {
        if let Some(index) = self.genres.iter().position(|selected| *selected == genre) {
            self.genres.remove(index);
        } else {
            self.genres.push(genre);
        }
    }

    pub(crate) fn has_genre(&self, genre: GenreTag) -> bool {
        self.genres.contains(&genre)
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
            (filter.genres.is_empty()
                || filter
                    .genres
                    .iter()
                    .any(|genre| genre.matches(&station.tags)))
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
    fn labels_come_from_the_english_catalog() {
        let labels = &i18n::catalog(Language::English).labels;
        let genres = [
            (GenreTag::Rock, labels.genre_rock),
            (GenreTag::Pop, labels.genre_pop),
            (GenreTag::Jazz, labels.genre_jazz),
            (GenreTag::Electronic, labels.genre_electronic),
            (GenreTag::News, labels.genre_news),
            (GenreTag::Metal, labels.genre_metal),
        ];
        for (tag, expected) in genres {
            assert_eq!(tag.label(Language::English), expected);
        }
        assert_eq!(
            StationSort::Popularity.label(Language::English),
            labels.sort_popularity
        );
        assert_eq!(
            StationSort::Bitrate.label(Language::English),
            labels.sort_bitrate
        );
        assert_eq!(StationSort::Name.label(Language::English), labels.sort_name);
        // Английский источник каталога зафиксирован литералом.
        assert_eq!(GenreTag::Rock.label(Language::English), "Rock");
        assert_eq!(StationSort::Name.label(Language::English), "Name");
    }

    #[test]
    fn genre_chip_checks_tags_case_insensitively() {
        let stations = sample();
        let filter = CountryFilterState {
            genres: vec![GenreTag::Jazz],
            ..CountryFilterState::default()
        };
        let matches = filtered_results(&stations, &filter);
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "Berlin Jazz");

        let filter = CountryFilterState {
            genres: vec![GenreTag::Metal],
            ..CountryFilterState::default()
        };
        assert!(filtered_results(&stations, &filter).is_empty());
    }

    #[test]
    fn multiple_genres_match_any_tag() {
        let stations = sample();
        let filter = CountryFilterState {
            genres: vec![GenreTag::Jazz, GenreTag::News],
            ..CountryFilterState::default()
        };
        let matches = filtered_results(&stations, &filter);
        let names: Vec<&str> = matches
            .iter()
            .map(|station| station.name.as_str())
            .collect();
        // Сортировка по популярности сохраняется: у «Nacht-News» больше votes.
        assert_eq!(names, vec!["Nacht-News", "Berlin Jazz"], "OR-семантика");
    }

    #[test]
    fn toggling_genres_adds_and_removes_without_duplicates() {
        let mut filter = CountryFilterState::default();
        assert!(!filter.is_active());
        filter.toggle_genre(GenreTag::Rock);
        filter.toggle_genre(GenreTag::Jazz);
        assert!(filter.has_genre(GenreTag::Rock) && filter.has_genre(GenreTag::Jazz));
        assert!(filter.is_active());

        filter.toggle_genre(GenreTag::Rock);
        assert!(!filter.has_genre(GenreTag::Rock));
        assert_eq!(filter.genres, vec![GenreTag::Jazz], "порядок сохраняется");
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
            genres: vec![GenreTag::Jazz, GenreTag::Rock],
            sort: StationSort::Name,
            compact_rows: true,
            filters_visible: false,
        };
        assert!(filter.is_active());

        filter.reset();
        assert!(!filter.is_active());
        assert!(filter.genres.is_empty());
        assert_eq!(filter.sort, StationSort::Name);
        assert!(filter.compact_rows, "плотность — настройка вида, не фильтр");
        assert!(
            !filter.filters_visible,
            "видимость строки фильтров не сбрасывается"
        );
    }
}
