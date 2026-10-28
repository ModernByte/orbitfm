//! Компилируемый каталог строк интерфейса (ADR-0030).
//!
//! Модуль зависит только от `settings::Language` и std: ни egui, ни session
//! сюда не проникают. Каждый язык — литерал [`Catalog`], поэтому пропущенное
//! поле не собирается, а неизвестный язык честно падает в английский.

mod catalog;
mod de;
mod en;
mod es;
mod fr;
mod it;
mod ja;
mod ko;
mod labels;
mod pt;
mod ru;
mod zh;

pub(crate) use catalog::{Catalog, LabelText};
pub(crate) use labels::{accent_label, language_label, motion_label, style_label, theme_label};

use crate::settings::Language;

/// Каталог уже разрешённого языка. `System` читает английский источник: `Session`
/// резолвит окружение при старте и при смене настройки (ADR-0030 §2), поэтому
/// `System` доходит сюда только в тестах и в треях без снимка.
pub(crate) fn catalog(language: Language) -> &'static Catalog {
    match language {
        Language::System | Language::English => &en::CATALOG,
        Language::Russian => &ru::CATALOG,
        Language::French => &fr::CATALOG,
        Language::German => &de::CATALOG,
        Language::Spanish => &es::CATALOG,
        Language::Italian => &it::CATALOG,
        Language::Portuguese => &pt::CATALOG,
        Language::Chinese => &zh::CATALOG,
        Language::Korean => &ko::CATALOG,
        Language::Japanese => &ja::CATALOG,
    }
}

/// Подставляет именованные плейсхолдеры `{name}`. Неизвестные остаются как есть:
/// шаблон и аргументы пишутся рядом, поэтому расхождение видно в ревью.
pub(crate) fn fill(template: &str, args: &[(&str, &str)]) -> String {
    let mut result = template.to_owned();
    for (name, value) in args {
        result = result.replace(&format!("{{{name}}}"), value);
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Имена `{placeholder}` в порядке появления.
    fn placeholders(template: &str) -> Vec<&str> {
        template
            .split(['{', '}'])
            .enumerate()
            .filter_map(|(index, part)| (index % 2 == 1).then_some(part))
            .collect()
    }

    macro_rules! assert_group_non_empty {
        ($catalog:expr, $group:ident { $($field:ident),* $(,)? }) => {
            $(
                assert!(
                    !$catalog.$group.$field.trim().is_empty(),
                    concat!(stringify!($group), ".", stringify!($field))
                );
                assert_eq!(
                    placeholders(catalog(Language::English).$group.$field),
                    placeholders($catalog.$group.$field),
                    concat!(
                        stringify!($group),
                        ".",
                        stringify!($field),
                        ": наборы плейсхолдеров разошлись с английским"
                    )
                );
            )*
        };
    }

    /// Каталог обязан быть заполненным и сохранять `{placeholder}` английского
    /// источника — иначе `fill` молча оставит `{count}` в интерфейсе.
    fn assert_catalog_complete(c: &Catalog) {
        assert_group_non_empty!(
            c,
            common {
                nothing_playing,
                untitled,
                add_favorite,
                remove_favorite,
                hide_window,
                hide_window_hint,
                show_station_list,
                hide_station_list,
                open_focus,
                mouse_controls,
                settings,
                close,
                search,
                no_match,
                light_theme,
                dark_theme,
                pause,
                resume,
                stop,
                previous_station,
                next_station,
                live,
                quit,
                retry,
            }
        );
        assert_group_non_empty!(
            c,
            hud {
                subtitle,
                search_stations,
                filter_stations,
                search_country,
                country,
                genre,
                pick_genre,
                global_genre,
                custom_tag_hint,
                genre_search_hint,
                idle,
            }
        );
        assert_group_non_empty!(
            c,
            panel {
                stations,
                country_prefix,
                genre_prefix,
                on_map,
                searching,
                no_stations,
                intro,
                results,
                favorites,
                history,
                station_count,
                search_failed,
                results_of,
                pick_country_or_genre,
                quiet_sector,
                no_filter_match,
                reset_filters,
                favorites_empty,
                history_empty,
                clear_history,
                search_in_country,
                search_here,
                all,
                sort_by,
                compact,
            }
        );
        assert_group_non_empty!(
            c,
            transport {
                radio_roulette,
                rolling,
                random_station,
                now_playing,
                now_playing_tags,
                no_station,
                volume,
                kbps,
            }
        );
        assert_group_non_empty!(
            c,
            capsule {
                favorites,
                count_of,
                no_favorites,
                favorite_hint,
            }
        );
        assert_group_non_empty!(
            c,
            overlay {
                right_drag,
                orbit,
                left_click,
                country_station,
                scroll,
                zoom,
                map_off,
                map_density,
                map_genre,
                map_quality,
                map_kind,
                map_rating,
                map_scope_global,
                map_scope_sample,
                map_scope_top,
                low,
                high,
            }
        );
        assert_group_non_empty!(
            c,
            settings {
                title,
                appearance,
                language,
                audio,
                motion,
                window,
                about,
                theme,
                accent,
                visualizer,
                viz_bars,
                bars,
                system_language,
                language_hint,
                volume,
                volume_hint,
                audio_hint,
                motion_reactivity,
                motion_spin_hint,
                motion_style,
                reduce_motion,
                reduce_motion_hint,
                allow_flips,
                allow_flips_hint,
                flips_unavailable,
                window_mode,
                window_hint,
                show_help,
                version,
                license,
                app_id,
                about_hint,
                saved_hint,
            }
        );
        assert_group_non_empty!(
            c,
            tray {
                show_window,
                hide_window,
                now_playing,
                paused,
                nothing_playing,
                play_pause,
                stop,
                previous,
                next,
                radio_roulette,
                volume_up,
                volume_down,
                settings,
                quit,
            }
        );
        assert_group_non_empty!(
            c,
            notices {
                history_not_saved,
                favorites_not_saved,
                history_cleared,
                clear_history_failed,
                playback_failed,
                station_without_url,
                search_task_ended,
                roulette_started,
                roulette_failed,
                roulette_task_ended,
                theme_not_saved,
                accent_not_saved,
                motion_not_saved,
                setting_not_saved,
                volume_not_saved,
                language_not_saved,
                visualizer_not_saved,
                fft_unavailable,
            }
        );
        assert_group_non_empty!(
            c,
            labels {
                theme_system,
                theme_dark,
                theme_light,
                accent_orbit,
                accent_emerald,
                accent_amber,
                accent_violet,
                accent_rose,
                accent_ocean,
                motion_off,
                motion_low,
                motion_normal,
                motion_high,
                style_auto,
                style_soft_pulse,
                style_groove_sway,
                style_bass_drop,
                style_ambient_drift,
                style_sparkle_twitch,
                style_beat_march,
                mode_focus,
                mode_overlay,
                mode_listening,
                genre_rock,
                genre_pop,
                genre_jazz,
                genre_electronic,
                genre_news,
                genre_metal,
                genre_classical,
                genre_synthwave,
                sort_popularity,
                sort_bitrate,
                sort_name,
                playback_stopped,
                playback_on_air,
                playback_paused,
                playback_buffering,
                playback_reconnecting,
                playback_error,
            }
        );
    }

    #[test]
    fn english_catalog_is_complete() {
        assert_catalog_complete(catalog(Language::English));
    }

    /// Каталог обязан покрыть каждый вариант enum: `System` — не язык каталога,
    /// он разрешается в английский. Проверка на указателе ловит язык, который
    /// молча вернулся к английскому.
    #[test]
    fn every_translation_language_has_its_own_catalog() {
        let english = catalog(Language::English);
        for language in Language::ALL
            .into_iter()
            .filter(|language| *language != Language::System)
        {
            let c = catalog(language);
            if language != Language::English {
                assert!(
                    !std::ptr::eq(c, english),
                    "{language:?} отдаёт английский каталог"
                );
            }
            assert_catalog_complete(c);
        }
    }

    #[test]
    fn system_falls_back_to_the_english_catalog() {
        let system = catalog(Language::System);
        let english = catalog(Language::English);
        assert_eq!(system.settings.title, english.settings.title);
        assert_eq!(system.common.nothing_playing, "Nothing playing");
    }

    #[test]
    fn fill_replaces_every_occurrence() {
        assert_eq!(
            fill(
                "Results ({count}) of {total}",
                &[("count", "3"), ("total", "10")]
            ),
            "Results (3) of 10"
        );
        assert_eq!(fill("{count} + {count}", &[("count", "2")]), "2 + 2");
        assert_eq!(fill("{missing}", &[("other", "x")]), "{missing}");
        assert_eq!(fill("plain", &[]), "plain");
    }
}
