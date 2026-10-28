//! Кэш тепловой карты стран для глобуса.
//!
//! Пересобирается только при смене режима или источников данных: растеризация
//! двух тысяч колонок не должна попадать в кадровый путь.

use std::collections::HashMap;
use std::sync::Arc;

use crate::geo::GeoService;
use crate::render::{CountryColor, HEAT_SIZE, HeatFrame, build_heat_map};
use crate::services::Station;
use crate::session::heat::{self, CountryMetric};

use super::palette::heat_color;

/// Прозрачность заливки: карта должна читатьсь, не затирая текстуру Земли.
/// Выше 0,4 рельеф уже не различим, ниже 0,15 цвет тонет в ночной стороне.
const HEAT_ALPHA_MIN: f32 = 0.16;
const HEAT_ALPHA_MAX: f32 = 0.40;

/// Что красит карту: глобальные счётчики стран или конкретные станции.
#[derive(Clone, Copy)]
pub(crate) enum HeatSource<'a> {
    /// Счётчики Radio Browser по всем странам (режим «Плотность»).
    Counts(&'a HashMap<String, u32>),
    /// Станции выборки: жанры, качество, тип, рейтинг.
    Stations(&'a [Station]),
}

/// Источник заливки для подписи в легенде: карта не должна обещать «весь мир»,
/// когда раскрашена выбранным жанром.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HeatScope {
    /// Счётчики всех стран базы.
    Global,
    /// Текущая выборка панели.
    Sample,
    /// Мировой топ станций, когда выборки нет.
    Top,
}

#[derive(Debug, Default)]
pub(crate) struct CountryHeatCache {
    signature: Option<(u64, u64, CountryMetric)>,
    frame: Option<Arc<HeatFrame>>,
}

impl CountryHeatCache {
    /// Обновляет карту под текущий источник; повторный вызов бесплатен.
    pub(crate) fn ensure(
        &mut self,
        geo: &GeoService,
        source: HeatSource<'_>,
        search_revision: u64,
        world_revision: u64,
        metric: CountryMetric,
    ) {
        let signature = (search_revision, world_revision, metric);
        if self.signature == Some(signature) {
            return;
        }
        self.signature = Some(signature);
        self.frame = build(geo, source, metric).map(Arc::new);
    }

    pub(crate) fn frame(&self) -> Option<&Arc<HeatFrame>> {
        self.frame.as_ref()
    }
}

fn build(geo: &GeoService, source: HeatSource<'_>, metric: CountryMetric) -> Option<HeatFrame> {
    let intensities = match source {
        HeatSource::Counts(counts) => heat::densities(counts),
        HeatSource::Stations(stations) => heat::intensities(metric, stations),
    };
    if intensities.is_empty() {
        return None;
    }

    let colors = intensities
        .into_iter()
        .map(|(iso, intensity)| {
            let color = heat_color(intensity);
            let alpha = HEAT_ALPHA_MIN + (HEAT_ALPHA_MAX - HEAT_ALPHA_MIN) * intensity;
            (iso, CountryColor { color, alpha })
        })
        .collect();

    build_heat_map(geo, &colors, HEAT_SIZE)
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
    fn off_mode_caches_no_frame() {
        let geo = GeoService::from_embedded().expect("встроенный GeoJSON");
        let mut cache = CountryHeatCache::default();
        cache.ensure(
            &geo,
            HeatSource::Stations(&[station("a", "DE")]),
            1,
            0,
            CountryMetric::Off,
        );
        assert!(cache.frame().is_none());
    }

    #[test]
    fn same_revision_and_mode_reuses_frame() {
        let geo = GeoService::from_embedded().expect("встроенный GeoJSON");
        let stations = vec![station("a", "DE")];
        let mut cache = CountryHeatCache::default();

        cache.ensure(
            &geo,
            HeatSource::Stations(&stations),
            7,
            0,
            CountryMetric::Density,
        );
        let first = cache.frame().cloned().expect("карта построена");
        cache.ensure(
            &geo,
            HeatSource::Stations(&stations),
            7,
            0,
            CountryMetric::Density,
        );
        let second = cache.frame().cloned().expect("карта построена");

        assert!(
            Arc::ptr_eq(&first, &second),
            "повторный кадр не пересобирает"
        );
    }

    #[test]
    fn new_revision_rebuilds_the_frame() {
        let geo = GeoService::from_embedded().expect("встроенный GeoJSON");
        let stations = vec![station("a", "DE")];
        let mut cache = CountryHeatCache::default();

        cache.ensure(
            &geo,
            HeatSource::Stations(&stations),
            1,
            0,
            CountryMetric::Density,
        );
        let first = cache.frame().cloned().expect("карта построена");
        cache.ensure(
            &geo,
            HeatSource::Stations(&stations),
            2,
            0,
            CountryMetric::Density,
        );
        let second = cache.frame().cloned().expect("карта построена");

        assert!(!Arc::ptr_eq(&first, &second));
    }

    #[test]
    fn global_counts_paint_countries_without_a_sample() {
        let geo = GeoService::from_embedded().expect("встроенный GeoJSON");
        let counts: HashMap<String, u32> = [("DE".to_owned(), 900)].into_iter().collect();
        let mut cache = CountryHeatCache::default();

        cache.ensure(
            &geo,
            HeatSource::Counts(&counts),
            0,
            1,
            CountryMetric::Density,
        );

        assert!(
            cache.frame().is_some(),
            "счётчики стран красят карту без выборки"
        );
    }
}
