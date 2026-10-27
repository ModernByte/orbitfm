//! Метрики стран для тепловой карты глобуса.
//!
//! «Плотность» берётся из глобальных счётчиков Radio Browser, поэтому карта
//! показывает все страны сразу, даже когда в панели нет выборки. Остальные
//! метрики считаются по станциям — по выборке панели, а при её отсутствии по
//! мировому топу (`session::world`). Интенсивность нормируется в `0..=1` — цвет
//! выбирает presentation-слой (`ui::palette::heat_color`).

use std::collections::{HashMap, HashSet};

use crate::services::Station;

/// Теги, по которым станция считается разговорной для карты «Тип».
const TALK_TAGS: [&str; 8] = [
    "talk", "news", "sport", "sports", "podcast", "speech", "politics", "business",
];

/// Режим тепловой карты стран в легенде Focus-стола.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) enum CountryMetric {
    /// Базовый глобус без заливки стран.
    Off,
    /// Число станций страны.
    #[default]
    Density,
    /// Жанровое разнообразие: число различных тегов.
    Genre,
    /// Средний битрейт потока.
    Quality,
    /// Доля разговорных станций.
    Kind,
    /// Средний рейтинг: голоса Radio Browser.
    Rating,
}

impl CountryMetric {
    pub(crate) const ALL: [Self; 6] = [
        Self::Off,
        Self::Density,
        Self::Genre,
        Self::Quality,
        Self::Kind,
        Self::Rating,
    ];

    pub(crate) fn is_on(self) -> bool {
        self != Self::Off
    }
}

#[derive(Default)]
struct Tally {
    count: u32,
    votes: u64,
    bitrate_sum: u64,
    bitrate_count: u32,
    talk: u32,
    genres: HashSet<String>,
}

/// Интенсивность карты по странам (ISO 3166-1 alpha-2).
pub(crate) fn intensities(metric: CountryMetric, stations: &[Station]) -> HashMap<String, f32> {
    if !metric.is_on() {
        return HashMap::new();
    }

    let mut by_country: HashMap<String, Tally> = HashMap::new();
    let mut seen = HashSet::new();
    for station in stations {
        let Some(iso) = country_code(&station.countrycode) else {
            continue;
        };
        if !seen.insert(station.stationuuid.as_str()) {
            continue;
        }

        let tally = by_country.entry(iso).or_default();
        tally.count += 1;
        tally.votes += u64::from(station.votes);
        if station.bitrate > 0 {
            tally.bitrate_sum += u64::from(station.bitrate);
            tally.bitrate_count += 1;
        }

        let mut talk = false;
        for tag in station.tags.split(',') {
            let tag = tag.trim().to_lowercase();
            if tag.is_empty() {
                continue;
            }
            if TALK_TAGS.contains(&tag.as_str()) {
                talk = true;
            }
            tally.genres.insert(tag);
        }
        if talk {
            tally.talk += 1;
        }
    }

    by_country
        .into_iter()
        .filter_map(|(iso, tally)| intensity(metric, &tally).map(|value| (iso, value)))
        .collect()
}

/// Плотность по глобальным счётчикам стран. Шкала привязана к максимуму базы:
/// медиана страны — ~20 станций, жёсткая «красный от 100» окрасила бы половину
/// мира одним цветом. Нули не красятся — страны без станций остаются базовой
/// текстурой.
pub(crate) fn densities(counts: &HashMap<String, u32>) -> HashMap<String, f32> {
    let Some(top) = counts.values().max().copied() else {
        return HashMap::new();
    };
    let denominator = (top.max(10) as f32).log10();
    counts
        .iter()
        .filter_map(|(iso, count)| {
            let count = *count as f32;
            (count > 0.0).then(|| (iso.clone(), (count.log10() / denominator).clamp(0.15, 1.0)))
        })
        .collect()
}

fn country_code(raw: &str) -> Option<String> {
    let code = raw.trim().to_ascii_uppercase();
    (code.len() == 2 && code.bytes().all(|byte| byte.is_ascii_alphabetic())).then_some(code)
}

fn intensity(metric: CountryMetric, tally: &Tally) -> Option<f32> {
    let count = tally.count as f32;
    if count == 0.0 {
        return None;
    }

    let value = match metric {
        CountryMetric::Off => return None,
        // 1 станция → синий, 100 и больше → красный.
        CountryMetric::Density => count.log10() / 2.0,
        // 30 различных тегов дают полную шкалу.
        CountryMetric::Genre => (1.0 + tally.genres.len() as f32).log10() / 31.0f32.log10(),
        CountryMetric::Quality => {
            if tally.bitrate_count == 0 {
                return None;
            }
            let average = tally.bitrate_sum as f32 / tally.bitrate_count as f32;
            average / 320.0
        }
        // 10 % разговорных — уже половина шкалы, 50 % — максимум.
        CountryMetric::Kind => tally.talk as f32 / count * 2.0,
        // 10 000 голосов в среднем — максимум.
        CountryMetric::Rating => {
            let average = tally.votes as f32 / count;
            (1.0 + average).log10() / 4.0
        }
    };
    Some(value.clamp(0.15, 1.0))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::markers::sample_station;

    fn station(uuid: &str, code: &str) -> Station {
        let mut station = sample_station();
        station.stationuuid = uuid.to_owned();
        station.countrycode = code.to_owned();
        station
    }

    #[test]
    fn density_grows_with_station_count() {
        let stations = vec![
            station("a", "DE"),
            station("b", "DE"),
            station("c", "DE"),
            station("d", "FR"),
        ];
        let values = intensities(CountryMetric::Density, &stations);

        assert!(values["DE"] > values["FR"]);
        assert!((values["DE"] - 3.0f32.log10() / 2.0).abs() < 1e-6);
        assert_eq!(values.len(), 2);
    }

    #[test]
    fn invalid_and_missing_country_codes_are_skipped() {
        let mut empty = station("empty", "");
        empty.countrycode.clear();
        let stations = vec![
            empty,
            station("bad", "-99"),
            station("long", "DEU"),
            station("ok", " de "),
        ];
        let values = intensities(CountryMetric::Density, &stations);

        assert_eq!(values.len(), 1);
        assert!(values.contains_key("DE"));
    }

    #[test]
    fn quality_uses_average_bitrate_and_skips_zero() {
        let mut slow = station("slow", "DE");
        slow.bitrate = 64;
        let mut fast = station("fast", "DE");
        fast.bitrate = 256;
        let mut silent = station("silent", "FR");
        silent.bitrate = 0;

        let values = intensities(CountryMetric::Quality, &[slow, fast, silent]);

        assert!(((values["DE"] - 160.0 / 320.0).abs()) < 1e-6);
        assert!(
            !values.contains_key("FR"),
            "без битрейта страна не красится"
        );
    }

    #[test]
    fn kind_measures_the_talk_share() {
        let mut talk = station("talk", "DE");
        talk.tags = "News, politics".to_owned();
        let music = station("music", "DE");
        let other_music = station("other-music", "DE");
        let mut talk_only = station("talk-only", "FR");
        talk_only.tags = "podcast".to_owned();

        let values = intensities(CountryMetric::Kind, &[talk, music, other_music, talk_only]);

        assert!(
            (values["DE"] - 2.0 / 3.0).abs() < 1e-6,
            "треть разговорных даёт две трети шкалы"
        );
        assert!((values["FR"] - 1.0).abs() < 1e-6, "все станции разговорные");
    }

    #[test]
    fn genre_counts_distinct_tags_per_country() {
        let mut first = station("first", "DE");
        first.tags = "jazz, soul, jazz".to_owned();
        let mut second = station("second", "DE");
        second.tags = "Jazz, blues".to_owned();

        let values = intensities(CountryMetric::Genre, &[first, second]);
        let expected = (1.0 + 3.0f32).log10() / 31.0f32.log10();

        assert!(
            (values["DE"] - expected).abs() < 1e-6,
            "теги без учёта регистра"
        );
    }

    #[test]
    fn rating_averages_votes_and_saturates() {
        let mut modest = station("modest", "DE");
        modest.votes = 100;
        let mut popular = station("popular", "DE");
        popular.votes = 300;
        let mut huge = station("huge", "FR");
        huge.votes = 1_000_000;

        let values = intensities(CountryMetric::Rating, &[modest, popular, huge]);

        assert!((values["DE"] - 201.0f32.log10() / 4.0).abs() < 1e-6);
        assert!((values["FR"] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn off_mode_returns_no_countries() {
        assert!(intensities(CountryMetric::Off, &[station("a", "DE")]).is_empty());
    }

    #[test]
    fn densities_normalizes_by_the_largest_country() {
        let counts: HashMap<String, u32> = [
            ("US".to_owned(), 8_839),
            ("DE".to_owned(), 4_249),
            ("MD".to_owned(), 22),
            ("VA".to_owned(), 1),
            ("XX".to_owned(), 0),
        ]
        .into_iter()
        .collect();
        let denominator = 8_839.0f32.log10();

        let values = densities(&counts);

        assert_eq!(values.len(), 4, "страны без станций не красятся");
        assert!((values["US"] - 1.0).abs() < 1e-6, "максимум — верх шкалы");
        assert!(
            (values["DE"] - 4_249.0f32.log10() / denominator).abs() < 1e-6,
            "шкала логарифмическая: вдвое меньше станций — почти та же яркость"
        );
        assert!(
            values["MD"] < values["DE"],
            "медиана светлее крупной страны"
        );
        assert!(
            (values["VA"] - 0.15).abs() < 1e-6,
            "одна станция не тонет в текстуре"
        );
    }

    #[test]
    fn densities_of_a_small_snapshot_still_spreads() {
        let counts: HashMap<String, u32> = [("DE".to_owned(), 3), ("FR".to_owned(), 300)]
            .into_iter()
            .collect();

        let values = densities(&counts);

        assert!(values["FR"] > values["DE"]);
        assert!(values["DE"] >= 0.15, "нижний порог заметности держится");
    }
}
