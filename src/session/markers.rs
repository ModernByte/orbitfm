//! Агрегация станций в маркеры глобуса.
//!
//! Это данные Application Core: представление о гео-группировке не знает ни про egui,
//! ни про то, как маркер будет нарисован (`src/ui/globe_view.rs` потребляет результат).

use std::collections::{HashMap, HashSet};

use crate::services::Station;

/// Шаг сетки группировки: 0.01° — станции одного «пятнистого» пикселя сливаются.
const GROUP_SCALE: f64 = 100.0;

/// Станция (или группа станций), отображаемая на глобусе.
#[derive(Debug, Clone)]
pub(crate) struct GlobeMarker {
    pub station: Station,
    pub lat: f32,
    pub lon: f32,
    pub count: usize,
    /// Максимум голосов по группе: рейтинг в подписи при наведении.
    pub popularity: f32,
    /// Лучший битрейт по группе: качество в подписи при наведении.
    pub bitrate: u32,
    /// UUID всех станций группы — играющей считается любая из них.
    pub members: Vec<String>,
}

/// Рейтинг станции: голоса Radio Browser. На вид маркера не влияет — только
/// на подпись при наведении.
fn station_popularity(station: &Station) -> f32 {
    station.votes as f32
}

pub(crate) fn build_markers(stations: Vec<Station>) -> Vec<GlobeMarker> {
    let mut markers: Vec<GlobeMarker> = Vec::new();
    let mut groups: HashMap<(i64, i64), usize> = HashMap::new();
    let mut seen = HashSet::new();

    for station in stations {
        if !seen.insert(station.stationuuid.clone()) {
            continue;
        }
        let Some((lat, lon)) = station_geo(&station) else {
            continue;
        };
        let popularity = station_popularity(&station);
        let bitrate = station.bitrate;
        let key = (
            (lat * GROUP_SCALE).round() as i64,
            (lon * GROUP_SCALE).round() as i64,
        );
        if let Some(&index) = groups.get(&key) {
            let marker = &mut markers[index];
            marker.count += 1;
            marker.popularity = marker.popularity.max(popularity);
            marker.bitrate = marker.bitrate.max(bitrate);
            marker.members.push(station.stationuuid);
        } else {
            groups.insert(key, markers.len());
            markers.push(GlobeMarker {
                members: vec![station.stationuuid.clone()],
                station,
                lat: lat as f32,
                lon: lon as f32,
                count: 1,
                popularity,
                bitrate,
            });
        }
    }

    markers
}

/// Координаты станции в допустимом диапазоне. `(0, 0)` — заглушка Radio Browser,
/// её не показываем.
pub(crate) fn station_geo(station: &Station) -> Option<(f64, f64)> {
    let lat = station.geo_lat?;
    let lon = station.geo_long?;
    if !lat.is_finite()
        || !lon.is_finite()
        || !(-90.0..=90.0).contains(&lat)
        || !(-180.0..=180.0).contains(&lon)
    {
        return None;
    }
    if lat == 0.0 && lon == 0.0 {
        return None;
    }
    Some((lat, lon))
}

#[cfg(test)]
pub(crate) fn sample_station() -> Station {
    Station {
        stationuuid: "c685c086-b716-4edd-b313-d4a7baf1084c".to_owned(),
        name: "Test Radio".to_owned(),
        url: "http://example.com/live".to_owned(),
        url_resolved: "https://example.com/live".to_owned(),
        homepage: None,
        favicon: None,
        tags: "jazz".to_owned(),
        countrycode: "US".to_owned(),
        language: "english".to_owned(),
        votes: 120,
        country: "United States".to_owned(),
        state: "New York".to_owned(),
        bitrate: 128,
        codec: "MP3".to_owned(),
        geo_lat: None,
        geo_long: None,
        source: crate::services::StationSource::Radio,
    }
}

#[cfg(test)]
pub(crate) fn geo_station(uuid: &str, lat: f64, lon: f64) -> Station {
    let mut station = sample_station();
    station.stationuuid = uuid.to_owned();
    station.geo_lat = Some(lat);
    station.geo_long = Some(lon);
    station
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn marker_groups_stations_with_shared_coordinates() {
        let markers = build_markers(vec![
            geo_station("a", 52.5200, 13.4050),
            geo_station("b", 52.5201, 13.4051),
            geo_station("c", 48.8566, 2.3522),
        ]);

        assert_eq!(markers.len(), 2);
        assert_eq!(markers[0].count, 2);
        assert_eq!(markers[1].count, 1);
        assert_eq!(markers[0].station.stationuuid, "a");
    }

    #[test]
    fn markers_skip_missing_invalid_and_zero_coordinates() {
        let mut missing = sample_station();
        missing.stationuuid = "missing".to_owned();

        let zero = geo_station("zero", 0.0, 0.0);

        let markers = build_markers(vec![
            missing,
            zero,
            geo_station("nan", f64::NAN, 13.0),
            geo_station("range", 91.0, 13.0),
            geo_station("valid", 52.52, 13.405),
        ]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].station.stationuuid, "valid");
    }

    #[test]
    fn markers_deduplicate_by_uuid() {
        let markers = build_markers(vec![
            geo_station("same", 52.52, 13.405),
            geo_station("same", 48.85, 2.35),
        ]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].station.stationuuid, "same");
    }

    #[test]
    fn popularity_signal_is_votes_only() {
        let mut silent = sample_station();
        silent.votes = 0;
        silent.bitrate = 320;
        assert_eq!(
            station_popularity(&silent),
            0.0,
            "качество потока не делает маркер крупнее"
        );

        let mut loud = sample_station();
        loud.votes = 4_000;
        loud.bitrate = 32;
        assert_eq!(station_popularity(&loud), 4_000.0);
    }

    #[test]
    fn grouped_markers_keep_the_loudest_station() {
        let mut light = geo_station("light", 52.5200, 13.4050);
        light.votes = 10;
        light.bitrate = 64;
        let mut heavy = geo_station("heavy", 52.5201, 13.4051);
        heavy.votes = 4_000;
        heavy.bitrate = 256;

        let markers = build_markers(vec![light, heavy]);

        assert_eq!(markers.len(), 1);
        assert_eq!(markers[0].count, 2);
        assert_eq!(
            markers[0].popularity, 4_000.0,
            "голоса группы — по максимуму"
        );
        assert_eq!(markers[0].bitrate, 256, "цвет группы — по лучшему потоку");
        assert_eq!(
            markers[0].members,
            ["light".to_owned(), "heavy".to_owned()],
            "любая станция группы считается играющей"
        );
    }
}
