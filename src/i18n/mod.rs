//! Компилируемый каталог строк интерфейса (ADR-0030).
//!
//! Модуль зависит только от `settings::Language` и std: ни egui, ни session
//! сюда не проникают. Каждый язык — литерал [`Catalog`], поэтому пропущенное
//! поле не собирается, а неизвестный язык честно падает в английский.

mod ar;
mod catalog;
mod de;
mod en;
mod es;
mod fr;
mod hi;
mod id;
mod it;
mod ja;
mod ko;
mod labels;
mod pl;
mod pt;
mod ru;
mod tr;
mod uk;
mod zh;

pub(crate) use catalog::{Catalog, IntegrationsText, LabelText, SettingsText, WelcomeText};
pub(crate) use labels::{
    accent_label, language_label, motion_label, network_profile_hint, network_profile_label,
    style_label, theme_label,
};

use crate::settings::Language;

/// Каталог уже разрешённого языка. `System` читает английский источник: `Session`
/// резолвит окружение при старте и при смене настройки (ADR-0030 §2), поэтому
/// `System` доходит сюда только в тестах и в треях без снимка.
pub(crate) fn catalog(language: Language) -> &'static Catalog {
    match language {
        Language::System | Language::English => &en::CATALOG,
        Language::Russian => &ru::CATALOG,
        Language::Ukrainian => &uk::CATALOG,
        Language::Polish => &pl::CATALOG,
        Language::French => &fr::CATALOG,
        Language::German => &de::CATALOG,
        Language::Spanish => &es::CATALOG,
        Language::Italian => &it::CATALOG,
        Language::Portuguese => &pt::CATALOG,
        Language::Turkish => &tr::CATALOG,
        Language::Arabic => &ar::CATALOG,
        Language::Hindi => &hi::CATALOG,
        Language::Indonesian => &id::CATALOG,
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

/// Подпись варианта таймера сна: `0` — выключен, иначе минуты из шаблона.
/// Общая для настроек, быстрых меню и трея (ADR-0046).
pub(crate) fn sleep_option_label(language: Language, minutes: u16) -> String {
    let text = &catalog(language).settings;
    if minutes == 0 {
        text.sleep_off.to_owned()
    } else {
        fill(text.sleep_minutes, &[("minutes", &minutes.to_string())])
    }
}

/// Оставшееся время таймера: `M:SS` или `H:MM:SS` (ADR-0057).
///
/// Секунды и минуты всегда двузначные, часы — без ведущего нуля. Потолок
/// таймера (`MAX_SLEEP_MINUTES = 720`) даёт ровно `12:00:00`. Язык пока не
/// влияет: строка состоит только из цифр и двоеточий.
pub(crate) fn sleep_remaining_label(_language: Language, seconds: u64) -> String {
    let hours = seconds / 3_600;
    let minutes = (seconds % 3_600) / 60;
    let secs = seconds % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{secs:02}")
    } else {
        format!("{minutes}:{secs:02}")
    }
}

/// Подпись варианта затухания: `0` — выключено, иначе секунды из шаблона.
/// Общая для настроек, быстрых меню и трея (ADR-0056).
#[allow(dead_code)]
pub(crate) fn sleep_fade_label(language: Language, seconds: u16) -> String {
    let text = &catalog(language).settings;
    if seconds == 0 {
        text.sleep_fade_off.to_owned()
    } else {
        fill(text.sleep_seconds, &[("seconds", &seconds.to_string())])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Имена `{placeholder}`; порядок в шаблоне зависит от грамматики языка,
    /// поэтому сравниваем наборы, а не последовательности.
    fn placeholders(template: &str) -> Vec<&str> {
        let mut names: Vec<&str> = template
            .split(['{', '}'])
            .enumerate()
            .filter_map(|(index, part)| (index % 2 == 1).then_some(part))
            .collect();
        names.sort_unstable();
        names
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
                favorites,
                history,
                play_this_page,
                no_audio,
                importing,
                stream_rejected,
                manual_link,
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
                all_genres,
                genres,
                genres_selected,
                remove_filter,
                filters,
                clear_filters,
                loaded_of,
                load_more,
                loading_more,
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
                integrations,
                about,
                theme,
                accent,
                visualizer,
                viz_bars,
                bars,
                allow_http_artwork,
                allow_http_artwork_hint,
                system_language,
                language_hint,
                volume,
                volume_hint,
                audio_hint,
                sleep_timer,
                sleep_timer_hint,
                sleep_off,
                sleep_minutes,
                sleep_action,
                sleep_action_hint,
                sleep_action_nothing,
                sleep_action_suspend,
                sleep_action_power_off,
                sleep_fade,
                sleep_fade_hint,
                sleep_fade_off,
                sleep_seconds,
                sleep_prevent_sleep,
                sleep_prevent_sleep_hint,
                sleep_warning,
                sleep_warning_hint,
                sleep_remaining,
                sleep_warning_title,
                sleep_warning_cancel,
                sleep_warning_stop,
                sleep_warning_suspend,
                sleep_warning_power_off,
                sleep_warning_cancel_hint,
                queue_wrap,
                queue_wrap_hint,
                motion_reactivity,
                motion_spin_hint,
                motion_style,
                reduce_motion,
                reduce_motion_hint,
                allow_flips,
                allow_flips_hint,
                flips_unavailable,
                network_profile,
                network_unlimited_hint,
                network_metered_hint,
                network_offline_first_hint,
                window_mode,
                window_hint,
                show_help,
                version,
                license,
                app_id,
                about_hint,
                saved_hint,
                appearance_page_hint,
                language_page_hint,
                audio_page_hint,
                motion_page_hint,
                window_page_hint,
                integrations_page_hint,
                about_page_hint,
                stats,
                stats_page_hint,
                stats_hint,
                stats_total,
                stats_week,
                stats_month,
                stats_stations,
                stats_countries,
                stats_tags,
                stats_enabled,
                stats_enabled_hint,
                stats_clear,
                stats_clear_confirm,
                stats_empty,
                stats_cleared,
                stats_duration,
                stats_duration_minutes,
                stats_duration_hours,
            }
        );
        assert_group_non_empty!(
            c,
            integrations {
                group_bars,
                group_environments,
                group_windows,
                group_browsers,
                not_installed,
                installed,
                update_available,
                problem,
                not_detected,
                install,
                uninstall,
                installing,
                enable_config,
                enabled_config,
                config_missing,
                reload,
                restart_browser,
                operation_done,
                operation_failed,
                reload_manual,
            }
        );
        assert_group_non_empty!(
            c,
            welcome {
                title,
                subtitle,
                language_theme,
                environment,
                environment_hint,
                detected,
                configure,
                applied,
                failed,
                niri_hint,
                copy,
                copied,
                skip,
                back,
                next,
                finish,
                integrations_hint,
                open_integrations,
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
                playback,
                volume,
                appearance,
                about_station,
                open_homepage,
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
                load_more_failed,
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
                sleep_expired,
                inhibition_lost,
                sleep_action_failed,
                stats_damaged,
                settings_damaged,
                library_damaged,
                audio_worker_failed,
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
                network_unlimited,
                network_metered,
                network_offline_first,
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
    fn sleep_remaining_label_formats_minutes_and_hours() {
        assert_eq!(sleep_remaining_label(Language::English, 0), "0:00");
        assert_eq!(sleep_remaining_label(Language::English, 59), "0:59");
        assert_eq!(sleep_remaining_label(Language::English, 3_599), "59:59");
        assert_eq!(sleep_remaining_label(Language::English, 3_600), "1:00:00");
        assert_eq!(sleep_remaining_label(Language::English, 43_200), "12:00:00");
    }

    #[test]
    fn sleep_fade_label_uses_off_and_seconds_template() {
        for language in [Language::English, Language::Russian] {
            assert_eq!(
                sleep_fade_label(language, 0),
                catalog(language).settings.sleep_fade_off
            );
            assert!(!sleep_fade_label(language, 300).contains("{seconds}"));
        }
        assert_eq!(sleep_fade_label(Language::English, 300), "300 s");
        assert_eq!(sleep_fade_label(Language::Russian, 300), "300 с");
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
