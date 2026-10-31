//! Язык интерфейса и его определение из окружения (ADR-0030).

use std::env;

use serde::{Deserialize, Deserializer, Serialize};

/// Язык интерфейса. `System` — не язык каталога, а способ его выбрать.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "system")]
    System,
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ru")]
    Russian,
    #[serde(rename = "uk")]
    Ukrainian,
    #[serde(rename = "pl")]
    Polish,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "it")]
    Italian,
    #[serde(rename = "pt")]
    Portuguese,
    #[serde(rename = "tr")]
    Turkish,
    #[serde(rename = "ar")]
    Arabic,
    #[serde(rename = "hi")]
    Hindi,
    #[serde(rename = "id")]
    Indonesian,
    #[serde(rename = "zh")]
    Chinese,
    #[serde(rename = "ko")]
    Korean,
    #[serde(rename = "ja")]
    Japanese,
}

impl Language {
    pub const ALL: [Self; 17] = [
        Self::System,
        Self::English,
        Self::Russian,
        Self::Ukrainian,
        Self::Polish,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
        Self::Portuguese,
        Self::Turkish,
        Self::Arabic,
        Self::Hindi,
        Self::Indonesian,
        Self::Chinese,
        Self::Korean,
        Self::Japanese,
    ];

    pub fn code(self) -> &'static str {
        match self {
            Self::System => "system",
            Self::English => "en",
            Self::Russian => "ru",
            Self::Ukrainian => "uk",
            Self::Polish => "pl",
            Self::French => "fr",
            Self::German => "de",
            Self::Spanish => "es",
            Self::Italian => "it",
            Self::Portuguese => "pt",
            Self::Turkish => "tr",
            Self::Arabic => "ar",
            Self::Hindi => "hi",
            Self::Indonesian => "id",
            Self::Chinese => "zh",
            Self::Korean => "ko",
            Self::Japanese => "ja",
        }
    }

    /// Название языка в нём самом: список выбора не зависит от текущего языка.
    pub fn native_name(self) -> &'static str {
        match self {
            Self::System => "System",
            Self::English => "English",
            Self::Russian => "Русский",
            Self::Ukrainian => "Українська",
            Self::Polish => "Polski",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
            Self::Portuguese => "Português",
            Self::Turkish => "Türkçe",
            Self::Arabic => "العربية",
            Self::Hindi => "हिन्दी",
            Self::Indonesian => "Bahasa Indonesia",
            Self::Chinese => "中文",
            Self::Korean => "한국어",
            Self::Japanese => "日本語",
        }
    }

    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|language| language.code().eq_ignore_ascii_case(code.trim()))
    }

    /// Тег локали (`ru_RU.UTF-8`, `de_DE@euro`, `pt-BR`) → язык.
    /// `None` — тег не распознан; вызывающий решает, что это значит.
    pub fn from_tag(tag: &str) -> Option<Self> {
        let tag = tag.trim();
        if tag.is_empty() {
            return None;
        }
        let base = tag
            .split('.')
            .next()
            .unwrap_or(tag)
            .split('@')
            .next()
            .unwrap_or(tag);
        let primary = base.split(['_', '-']).next().unwrap_or(base);
        match primary.to_ascii_lowercase().as_str() {
            "c" | "posix" => Some(Self::English),
            code => Self::from_code(code).filter(|language| *language != Self::System),
        }
    }

    /// `System` превращается в конкретный язык, который есть в каталоге.
    pub fn resolve(self) -> Self {
        let lc_all = env::var("LC_ALL").ok();
        let lc_messages = env::var("LC_MESSAGES").ok();
        let lang = env::var("LANG").ok();
        self.resolve_from(lc_all.as_deref(), lc_messages.as_deref(), lang.as_deref())
    }

    fn resolve_from(
        self,
        lc_all: Option<&str>,
        lc_messages: Option<&str>,
        lang: Option<&str>,
    ) -> Self {
        if self != Self::System {
            return self;
        }
        resolve_tag(lc_all)
            .or_else(|| resolve_tag(lc_messages))
            .or_else(|| resolve_tag(lang))
            .unwrap_or(Self::English)
    }
}

/// Первая непустая локаль из цепочки; неизвестная или пустая — английский.
fn resolve_tag(tag: Option<&str>) -> Option<Language> {
    tag.map(str::trim)
        .filter(|tag| !tag.is_empty())
        .map(|tag| Language::from_tag(tag).unwrap_or(Language::English))
}

/// Неизвестный код в файле настроек не должен стоить остальных настроек.
pub(crate) fn deserialize_language<'de, D>(deserializer: D) -> Result<Language, D::Error>
where
    D: Deserializer<'de>,
{
    let code = String::deserialize(deserializer)?;
    Ok(Language::from_code(&code).unwrap_or_default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_language_has_a_unique_code_and_native_name() {
        let mut codes: Vec<&str> = Language::ALL
            .iter()
            .map(|language| language.code())
            .collect();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), Language::ALL.len());

        let mut names: Vec<&str> = Language::ALL
            .iter()
            .map(|language| language.native_name())
            .collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), Language::ALL.len());
    }

    #[test]
    fn codes_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(language));
            assert_eq!(
                Language::from_code(&language.code().to_uppercase()),
                Some(language)
            );
        }
        assert_eq!(Language::from_code("klingon"), None);
    }

    #[test]
    fn locale_tags_map_to_primary_languages() {
        assert_eq!(Language::from_tag("ru_RU.UTF-8"), Some(Language::Russian));
        assert_eq!(Language::from_tag("uk_UA.UTF-8"), Some(Language::Ukrainian));
        assert_eq!(Language::from_tag("pl_PL"), Some(Language::Polish));
        assert_eq!(Language::from_tag("tr_TR.UTF-8"), Some(Language::Turkish));
        assert_eq!(Language::from_tag("ar_SA"), Some(Language::Arabic));
        assert_eq!(Language::from_tag("hi_IN"), Some(Language::Hindi));
        assert_eq!(Language::from_tag("id_ID"), Some(Language::Indonesian));
        assert_eq!(Language::from_tag("de_DE@euro"), Some(Language::German));
        assert_eq!(Language::from_tag("fr-FR"), Some(Language::French));
        assert_eq!(Language::from_tag("zh_CN.UTF-8"), Some(Language::Chinese));
        assert_eq!(Language::from_tag("ja_JP"), Some(Language::Japanese));
        assert_eq!(Language::from_tag("ko_KR.UTF-8"), Some(Language::Korean));
        assert_eq!(Language::from_tag("pt_BR"), Some(Language::Portuguese));
        assert_eq!(Language::from_tag("en_GB"), Some(Language::English));
        assert_eq!(Language::from_tag("C"), Some(Language::English));
        assert_eq!(Language::from_tag("POSIX"), Some(Language::English));
        assert_eq!(Language::from_tag("sr_RS"), None);
        assert_eq!(Language::from_tag(""), None);
    }

    #[test]
    fn system_resolution_uses_lc_all_then_messages_then_lang() {
        let resolve = |lc_all: Option<&str>, lc_messages: Option<&str>, lang: Option<&str>| {
            Language::System.resolve_from(lc_all, lc_messages, lang)
        };
        assert_eq!(
            resolve(Some("ru_RU.UTF-8"), Some("de_DE"), Some("fr_FR")),
            Language::Russian
        );
        assert_eq!(
            resolve(None, Some("de_DE"), Some("fr_FR")),
            Language::German
        );
        assert_eq!(
            resolve(Some(""), Some(""), Some("ja_JP")),
            Language::Japanese
        );
        assert_eq!(resolve(None, None, None), Language::English);
        assert_eq!(
            resolve(Some("sr_RS"), None, Some("ru_RU")),
            Language::English
        );
        assert_eq!(resolve(Some("C"), None, Some("ru_RU")), Language::English);
    }

    #[test]
    fn explicit_language_is_not_resolved_from_environment() {
        assert_eq!(
            Language::Russian.resolve_from(None, None, Some("de_DE")),
            Language::Russian
        );
    }
}
