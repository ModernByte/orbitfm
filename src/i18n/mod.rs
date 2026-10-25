//! Компилируемый каталог строк интерфейса (ADR-0030).
//!
//! Модуль зависит только от `settings::Language` и std: ни egui, ни session
//! сюда не проникают. Каждый язык — литерал [`Catalog`], поэтому пропущенное
//! поле не собирается, а неизвестный язык честно падает в английский.

mod catalog;
mod en;
mod labels;

pub(crate) use catalog::{Catalog, LabelText};
pub(crate) use labels::{accent_label, language_label, motion_label, style_label, theme_label};

use crate::settings::Language;

/// Каталог уже разрешённого языка. `System` и языки без перевода читают
/// английский источник: пустого каталога UI не увидит никогда.
pub(crate) fn catalog(_language: Language) -> &'static Catalog {
    &en::CATALOG
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

    macro_rules! assert_group_non_empty {
        ($catalog:expr, $group:ident { $($field:ident),* $(,)? }) => {
            $(
                assert!(
                    !$catalog.$group.$field.trim().is_empty(),
                    concat!(stringify!($group), ".", stringify!($field))
                );
            )*
        };
    }

    #[test]
    fn english_catalog_has_no_blank_strings() {
        let c = catalog(Language::English);
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
                results_count,
                results_of,
                favorites,
                favorites_count,
                history,
                history_count,
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
                visualizer,
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
                station_density,
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
                audio_hint,
                motion_reactivity,
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
