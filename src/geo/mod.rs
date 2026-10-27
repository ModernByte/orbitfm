//! Определение страны по географическим координатам или точке на сфере.
//!
//! Границы стран загружаются из GeoJSON (Natural Earth 110m) и индексируются
//! R-Tree по bounding box. Точная проверка выполняется через Point-in-Polygon.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use geo::{BoundingRect, Contains, Coord, LineString, MultiPolygon, Point, Polygon};
use rstar::{AABB, RTree, RTreeObject};

const EMBEDDED_WORLD: &str = include_str!("../../assets/geojson/world-110m.json");
const INVALID_PROPERTY: &str = "-99";

/// Информация о стране.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Country {
    /// Название страны (ADMIN из Natural Earth).
    pub name: String,
    /// ISO 3166-1 alpha-2.
    pub iso_a2: String,
    /// ISO 3166-1 alpha-3, если известен.
    pub iso_a3: Option<String>,
}

/// Кольца границ страны в градусах `[lon, lat]`.
///
/// Включает внешние контуры и отверстия (озёра), без доменных типов —
/// пригодно для проекции и отрисовки в слое render.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CountryShape {
    pub rings: Vec<Vec<[f64; 2]>>,
}

impl CountryShape {
    /// Центр крупнейшего кольца — точка для мягкого доворота камеры.
    ///
    /// Долготы соседних точек раскрываются относительно первой точки кольца,
    /// поэтому страны на антимеридиане не дают центр в Африке.
    pub fn focus_point(&self) -> Option<(f64, f64)> {
        let ring = self.rings.iter().max_by_key(|ring| ring.len())?;
        let first = *ring.first()?;
        let mut lon_sum = 0.0;
        let mut lat_sum = 0.0;
        for [lon, lat] in ring {
            let mut lon = *lon;
            while lon - first[0] > 180.0 {
                lon -= 360.0;
            }
            while lon - first[0] < -180.0 {
                lon += 360.0;
            }
            lon_sum += lon;
            lat_sum += lat;
        }
        let count = ring.len() as f64;
        Some((lat_sum / count, normalize_longitude(lon_sum / count)))
    }
}

fn normalize_longitude(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

/// Ошибки геосервиса.
#[derive(Debug, thiserror::Error)]
pub enum GeoError {
    /// Не удалось прочитать или разобрать GeoJSON.
    #[error("не удалось загрузить GeoJSON: {0}")]
    Load(String),
    /// В данных не оказалось пригодных границ.
    #[error("некорректные данные границ")]
    InvalidGeometry,
}

struct CountryEntry {
    country: Country,
    geometry: MultiPolygon<f64>,
    shape: CountryShape,
}

struct IndexedCountry {
    entry: usize,
    envelope: AABB<[f64; 2]>,
}

impl RTreeObject for IndexedCountry {
    type Envelope = AABB<[f64; 2]>;

    fn envelope(&self) -> Self::Envelope {
        self.envelope
    }
}

/// Геосервис с предварительно построенным R-Tree.
pub struct GeoService {
    countries: Vec<CountryEntry>,
    index: RTree<IndexedCountry>,
    by_iso: HashMap<String, usize>,
}

impl GeoService {
    /// Загружает границы стран из GeoJSON-файла.
    ///
    /// Часть контракта модуля для альтернативных датасетов; приложение
    /// использует [`GeoService::from_embedded`].
    #[allow(dead_code)]
    pub fn load(path: &Path) -> Result<Self, GeoError> {
        let raw = fs::read_to_string(path).map_err(|err| GeoError::Load(err.to_string()))?;
        Self::parse(&raw)
    }

    /// Загружает встроенный датасет Natural Earth 110m.
    pub fn from_embedded() -> Result<Self, GeoError> {
        Self::parse(EMBEDDED_WORLD)
    }

    /// Находит страну по географическим координатам (широта, долгота).
    pub fn country_at(&self, lat: f64, lon: f64) -> Option<&Country> {
        if !(-90.0..=90.0).contains(&lat) || !(-180.0..=180.0).contains(&lon) {
            return None;
        }

        let point = Point::new(lon, lat);
        self.index
            .locate_in_envelope_intersecting(AABB::from_point([lon, lat]))
            .find(|candidate| self.countries[candidate.entry].geometry.contains(&point))
            .map(|candidate| &self.countries[candidate.entry].country)
    }

    /// Находит страну по точке на сфере (x, y, z), не обязательно единичной.
    ///
    /// Сферические координаты: `x = cos φ · sin λ`, `y = sin φ`, `z = cos φ · cos λ`,
    /// где `φ` — широта, `λ` — долгота. Направление +Z соответствует Гринвичу.
    pub fn country_at_sphere_point(&self, x: f64, y: f64, z: f64) -> Option<&Country> {
        let radius = (x * x + y * y + z * z).sqrt();
        if !radius.is_finite() || radius < f64::EPSILON {
            return None;
        }

        let lat = (y / radius).clamp(-1.0, 1.0).asin().to_degrees();
        let lon = x.atan2(z).to_degrees();
        self.country_at(lat, lon)
    }

    /// Ищет страны по названию или ISO-коду без учёта регистра.
    ///
    /// Пустой запрос даёт пустой результат: подсказки появляются только
    /// после ввода. Результаты отсортированы по названию.
    pub fn search_countries(&self, query: &str, limit: usize) -> Vec<&Country> {
        let query = query.trim().to_lowercase();
        if query.is_empty() || limit == 0 {
            return Vec::new();
        }

        let mut matches: Vec<&Country> = self
            .countries
            .iter()
            .map(|entry| &entry.country)
            .filter(|country| {
                country.name.to_lowercase().contains(&query)
                    || country.iso_a2.eq_ignore_ascii_case(&query)
                    || country
                        .iso_a3
                        .as_deref()
                        .is_some_and(|iso| iso.eq_ignore_ascii_case(&query))
            })
            .collect();
        matches.sort_by_cached_key(|country| country.name.to_lowercase());
        matches.truncate(limit);
        matches
    }

    /// Кольца границ страны по ISO 3166-1 alpha-2.
    pub fn country_shape(&self, iso_a2: &str) -> Option<&CountryShape> {
        let index = *self.by_iso.get(&iso_a2.trim().to_uppercase())?;
        Some(&self.countries[index].shape)
    }

    /// Все страны датасета с контурами: источник тепловой карты глобуса.
    ///
    /// Порядок — порядок загрузки GeoJSON; дубликаты ISO уже отброшены.
    pub fn countries(&self) -> impl Iterator<Item = (&Country, &CountryShape)> {
        self.countries
            .iter()
            .map(|entry| (&entry.country, &entry.shape))
    }

    fn parse(raw: &str) -> Result<Self, GeoError> {
        let collection: geojson::FeatureCollection = raw
            .parse()
            .map_err(|err: geojson::Error| GeoError::Load(err.to_string()))?;

        let mut countries = Vec::new();
        let mut indexed = Vec::new();
        let mut by_iso = HashMap::new();

        for feature in collection.features {
            let Some((country, geometry)) = convert_feature(feature) else {
                continue;
            };
            let Some(bounds) = geometry.bounding_rect() else {
                continue;
            };
            let envelope = AABB::from_corners(
                [bounds.min().x, bounds.min().y],
                [bounds.max().x, bounds.max().y],
            );
            indexed.push(IndexedCountry {
                entry: countries.len(),
                envelope,
            });
            by_iso
                .entry(country.iso_a2.clone())
                .or_insert(countries.len());
            let shape = shape_from_geometry(&geometry);
            countries.push(CountryEntry {
                country,
                geometry,
                shape,
            });
        }

        if countries.is_empty() {
            return Err(GeoError::InvalidGeometry);
        }

        Ok(Self {
            countries,
            index: RTree::bulk_load(indexed),
            by_iso,
        })
    }
}

fn shape_from_geometry(geometry: &MultiPolygon<f64>) -> CountryShape {
    let mut rings = Vec::new();
    for polygon in &geometry.0 {
        for ring in std::iter::once(polygon.exterior()).chain(polygon.interiors()) {
            let mut coords: Vec<[f64; 2]> = ring.coords().map(|coord| [coord.x, coord.y]).collect();
            if coords.len() > 1 && coords.first() == coords.last() {
                coords.pop();
            }
            coords.dedup();
            if coords.len() >= 3 {
                rings.push(coords);
            }
        }
    }
    CountryShape { rings }
}

fn convert_feature(feature: geojson::Feature) -> Option<(Country, MultiPolygon<f64>)> {
    let properties = feature.properties?;
    let name = property(&properties, "ADMIN").or_else(|| property(&properties, "NAME"))?;
    let iso_a2 = property(&properties, "ISO_A2")
        .or_else(|| property(&properties, "ISO_A2_EH"))?
        .to_uppercase();
    let iso_a3 = property(&properties, "ISO_A3").map(|code| code.to_uppercase());
    let geometry = convert_geometry(feature.geometry?)?;

    Some((
        Country {
            name,
            iso_a2,
            iso_a3,
        },
        geometry,
    ))
}

fn property(properties: &geojson::JsonObject, key: &str) -> Option<String> {
    let value = properties.get(key)?.as_str()?.trim();
    if value.is_empty() || value == INVALID_PROPERTY {
        None
    } else {
        Some(value.to_owned())
    }
}

fn convert_geometry(geometry: geojson::Geometry) -> Option<MultiPolygon<f64>> {
    match geometry.value {
        geojson::GeometryValue::Polygon { coordinates } => {
            convert_polygon(coordinates).map(|polygon| MultiPolygon::new(vec![polygon]))
        }
        geojson::GeometryValue::MultiPolygon { coordinates } => {
            let polygons: Vec<_> = coordinates
                .into_iter()
                .filter_map(convert_polygon)
                .collect();
            (!polygons.is_empty()).then(|| MultiPolygon::new(polygons))
        }
        _ => None,
    }
}

fn convert_polygon(rings: Vec<Vec<geojson::Position>>) -> Option<Polygon<f64>> {
    let mut rings = rings.into_iter().filter_map(convert_ring);
    let exterior = rings.next()?;
    Some(Polygon::new(exterior, rings.collect()))
}

fn convert_ring(positions: Vec<geojson::Position>) -> Option<LineString<f64>> {
    let coords: Vec<Coord<f64>> = positions
        .into_iter()
        .filter_map(|position| {
            let values = position.as_slice();
            let x = *values.first()?;
            let y = *values.get(1)?;
            (x.is_finite() && y.is_finite()).then_some(Coord { x, y })
        })
        .collect();

    (coords.len() >= 4).then(|| LineString::new(coords))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn service() -> GeoService {
        GeoService::from_embedded().expect("встроенный датасет должен загружаться")
    }

    fn sphere_point(lat: f64, lon: f64) -> (f64, f64, f64) {
        let lat = lat.to_radians();
        let lon = lon.to_radians();
        (lat.cos() * lon.sin(), lat.sin(), lat.cos() * lon.cos())
    }

    #[test]
    fn loads_geojson_from_file() {
        let geojson = r#"{
            "type": "FeatureCollection",
            "features": [{
                "type": "Feature",
                "properties": {"ADMIN": "Testland", "ISO_A2": "TS", "ISO_A3": "TSL"},
                "geometry": {
                    "type": "Polygon",
                    "coordinates": [[[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.0, 0.0]]]
                }
            }]
        }"#;
        let directory = tempfile::tempdir().expect("временная директория");
        let path = directory.path().join("world.json");
        fs::write(&path, geojson).expect("запись тестового GeoJSON");

        let service = GeoService::load(&path).expect("загрузка из файла");
        let country = service.country_at(0.5, 0.5).expect("точка внутри полигона");
        assert_eq!(country.iso_a2, "TS");
        assert_eq!(country.iso_a3.as_deref(), Some("TSL"));
        assert!(service.country_at(10.0, 10.0).is_none());
    }

    #[test]
    fn loads_embedded_dataset() {
        let service = service();
        assert!(service.countries.len() > 170);
        assert!(
            service
                .countries
                .iter()
                .all(|entry| !entry.country.iso_a2.is_empty())
        );
    }

    #[test]
    fn resolves_country_by_coordinates() {
        let service = service();

        assert_eq!(service.country_at(52.52, 13.405).unwrap().iso_a2, "DE");
        assert_eq!(service.country_at(55.7558, 37.6173).unwrap().iso_a2, "RU");
        assert_eq!(service.country_at(-33.8688, 151.2093).unwrap().iso_a2, "AU");
        assert_eq!(service.country_at(-80.0, 0.0).unwrap().iso_a2, "AQ");
    }

    #[test]
    fn uses_iso_a2_eh_fallback() {
        let service = service();

        assert_eq!(service.country_at(48.8566, 2.3522).unwrap().iso_a2, "FR");
        assert_eq!(service.country_at(59.9139, 10.7522).unwrap().iso_a2, "NO");
    }

    #[test]
    fn resolves_antimeridian_country() {
        let service = service();

        assert_eq!(service.country_at(-17.7134, 178.065).unwrap().iso_a2, "FJ");
    }

    #[test]
    fn returns_none_for_ocean_and_invalid_coordinates() {
        let service = service();

        assert!(service.country_at(0.0, -140.0).is_none());
        assert!(service.country_at(90.0, 0.0).is_none());
        assert!(service.country_at(91.0, 0.0).is_none());
        assert!(service.country_at(0.0, 181.0).is_none());
        assert!(service.country_at(f64::NAN, 0.0).is_none());
    }

    #[test]
    fn resolves_sphere_point() {
        let service = service();
        let (x, y, z) = sphere_point(52.52, 13.405);

        assert_eq!(
            service.country_at_sphere_point(x, y, z).unwrap().iso_a2,
            "DE"
        );
        assert!(service.country_at_sphere_point(0.0, 0.0, 0.0).is_none());
    }

    #[test]
    fn searches_countries_by_name_and_iso() {
        let service = service();

        let by_name = service.search_countries("  germ  ", 5);
        assert_eq!(
            by_name.first().map(|country| country.iso_a2.as_str()),
            Some("DE")
        );

        let by_iso = service.search_countries("de", 5);
        assert!(
            by_iso.iter().any(|country| country.iso_a2 == "DE"),
            "ISO-код должен находить Германию"
        );
        assert_eq!(service.search_countries("   ", 5).len(), 0);
        assert_eq!(service.search_countries("germ", 0).len(), 0);
    }

    #[test]
    fn search_results_are_sorted_and_limited() {
        let service = service();

        let matches = service.search_countries("a", 7);
        assert_eq!(matches.len(), 7);
        let names: Vec<String> = matches
            .iter()
            .map(|country| country.name.to_lowercase())
            .collect();
        let mut sorted = names.clone();
        sorted.sort();
        assert_eq!(names, sorted);
    }

    #[test]
    fn exposes_country_shape_rings() {
        let service = service();

        let shape = service
            .country_shape("de")
            .expect("Германия должна иметь контур");
        assert!(!shape.rings.is_empty());
        assert!(
            shape.rings.iter().all(|ring| ring.len() >= 3
                && ring.iter().all(|[lon, lat]| {
                    (-180.0..=180.0).contains(lon) && (-90.0..=90.0).contains(lat)
                })),
            "кольца должны содержать валидные координаты"
        );
        assert!(service.country_shape("XX").is_none());
    }

    #[test]
    fn iterates_all_countries_with_shapes() {
        let service = service();
        let countries: Vec<_> = service.countries().collect();

        assert!(countries.len() >= 150, "датасет Natural Earth 110m");
        assert!(
            countries
                .iter()
                .any(|(country, shape)| country.iso_a2 == "DE" && !shape.rings.is_empty())
        );
        assert!(
            countries
                .iter()
                .all(|(country, shape)| !country.name.is_empty() && !shape.rings.is_empty())
        );
    }

    #[test]
    fn focus_point_lies_inside_country() {
        let service = service();

        let shape = service.country_shape("DE").expect("контур Германии");
        let (lat, lon) = shape.focus_point().expect("точка фокуса");
        assert!(lat > 47.0 && lat < 56.0, "широта Германии: {lat}");
        assert!(lon > 5.0 && lon < 16.0, "долгота Германии: {lon}");
        assert!(
            service
                .country_at(lat, lon)
                .is_some_and(|country| country.iso_a2 == "DE")
        );
    }

    #[test]
    fn focus_point_keeps_antimeridian_country_together() {
        let service = service();

        let shape = service.country_shape("FJ").expect("контур Фиджи");
        let (_, lon) = shape.focus_point().expect("точка фокуса");
        assert!(
            lon.abs() > 170.0,
            "Фиджи должна остаться у антимеридиана: {lon}"
        );
    }

    #[test]
    fn focus_point_of_empty_shape_is_none() {
        assert!(CountryShape::default().focus_point().is_none());
    }
}
