//! The texts of the built-in UI, in several languages.
//!
//! Every text has a stable key such as `slash.h1` or `find.no_results`.
//! [`EditorStrings`] picks the texts of one language and lets the host
//! override single keys, as Tiptap extensions take their texts through
//! `configure`:
//!
//! ```
//! use gpui_markdown_editor::EditorStrings;
//!
//! let strings = EditorStrings::for_locale("de_DE.UTF-8")
//!     .set("slash.wikilink", "Notiz verlinken");
//! assert_eq!(strings.get("slash.h1"), "Überschrift 1");
//! assert_eq!(strings.get("slash.wikilink"), "Notiz verlinken");
//! ```
//!
//! Hosts with their own translation system can fill every key from it with
//! [`EditorStrings::translate`]. [`EditorStrings::keys`] lists the keys with
//! their English text.

use std::collections::HashMap;
use std::fmt;

use gpui::SharedString;

mod de;
mod en;
mod es;
mod fr;
mod it;
mod ja;
mod pl;
mod pt_br;
mod ru;
mod zh_cn;

/// The built-in languages: tag and table. English is the fallback of every
/// other table.
const LOCALES: &[(&str, &[(&str, &str)])] = &[
    ("en", en::TABLE),
    ("de", de::TABLE),
    ("es", es::TABLE),
    ("fr", fr::TABLE),
    ("it", it::TABLE),
    ("ja", ja::TABLE),
    ("pl", pl::TABLE),
    ("pt_BR", pt_br::TABLE),
    ("ru", ru::TABLE),
    ("zh_CN", zh_cn::TABLE),
];

/// The texts of the built-in UI, see the [module docs](self).
///
/// The default is English. Set it as
/// [`EditorOptions::strings`](crate::EditorOptions::strings), or later with
/// [`MarkdownEditor::set_strings`](crate::MarkdownEditor::set_strings).
#[derive(Clone)]
pub struct EditorStrings {
    locale: &'static str,
    table: &'static [(&'static str, &'static str)],
    overrides: HashMap<SharedString, SharedString>,
}

impl Default for EditorStrings {
    fn default() -> Self {
        Self::english()
    }
}

impl fmt::Debug for EditorStrings {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EditorStrings")
            .field("locale", &self.locale)
            .field("overrides", &self.overrides.len())
            .finish()
    }
}

impl EditorStrings {
    /// The English texts.
    pub fn english() -> Self {
        Self {
            locale: "en",
            table: en::TABLE,
            overrides: HashMap::new(),
        }
    }

    /// The texts of the built-in language closest to `locale`, English when
    /// there is none.
    ///
    /// Takes POSIX locales (`de_DE.UTF-8`, `pt_BR`, `sr@latin`) and BCP 47
    /// tags (`de-AT`, `zh-CN`). The full tag is tried first, then the
    /// language alone: `de_AT` gets German, `pt_PT` gets Brazilian
    /// Portuguese, `zh_TW` gets simplified Chinese.
    pub fn for_locale(locale: &str) -> Self {
        Self::find(locale).map_or_else(Self::english, |(locale, table)| Self {
            locale,
            table,
            overrides: HashMap::new(),
        })
    }

    /// The texts for the language of the environment, read like gettext
    /// does: the first built-in language in `LANGUAGE` (a `:` separated
    /// list), else `LC_ALL`, `LC_MESSAGES` or `LANG`, the first one set.
    /// English when none of them names a built-in language.
    pub fn from_env() -> Self {
        let var = |name| std::env::var(name).ok().filter(|v| !v.is_empty());
        let fallback = ["LC_ALL", "LC_MESSAGES", "LANG"]
            .into_iter()
            .find_map(var)
            .unwrap_or_default();
        // gettext ignores LANGUAGE in the C locale.
        let languages = match var("LANGUAGE") {
            Some(list) if !matches!(fallback.as_str(), "" | "C" | "POSIX") => list,
            _ => String::new(),
        };
        languages
            .split(':')
            .chain([fallback.as_str()])
            .find_map(Self::find)
            .map_or_else(Self::english, |(locale, table)| Self {
                locale,
                table,
                overrides: HashMap::new(),
            })
    }

    /// The tags of the built-in languages, `en` first.
    pub fn locales() -> impl Iterator<Item = &'static str> {
        LOCALES.iter().map(|(tag, _)| *tag)
    }

    /// Every key with its English text, in UI order.
    pub fn keys() -> impl Iterator<Item = (&'static str, &'static str)> {
        en::TABLE.iter().copied()
    }

    /// The built-in language in use, such as `de` or `pt_BR`.
    pub fn locale(&self) -> &'static str {
        self.locale
    }

    /// Replaces the text of `key`. Texts with placeholders keep them:
    /// `find.position` takes `{current}` and `{total}`, `find.found` takes
    /// `{count}`. Unknown keys are kept, so that a host can name the slash
    /// entries of its own extensions (`slash.<id>`).
    #[must_use]
    pub fn set(mut self, key: impl Into<SharedString>, text: impl Into<SharedString>) -> Self {
        self.overrides.insert(key.into(), text.into());
        self
    }

    /// Replaces every text with what `translate` returns for its key and
    /// English text, for hosts with their own catalogs (gettext, Fluent).
    /// Return `None` to keep the current text.
    #[must_use]
    pub fn translate(mut self, mut translate: impl FnMut(&str, &str) -> Option<String>) -> Self {
        for (key, english) in Self::keys() {
            if let Some(text) = translate(key, english) {
                self.overrides.insert(key.into(), text.into());
            }
        }
        self
    }

    /// The text of `key`: an override, the language's text, the English
    /// text, or the key itself when it is unknown.
    pub fn get(&self, key: &str) -> SharedString {
        self.lookup(key).unwrap_or_else(|| key.to_string().into())
    }

    /// The text of `key`, or `None` when no table and no override has it.
    pub(crate) fn lookup(&self, key: &str) -> Option<SharedString> {
        if let Some(text) = self.overrides.get(key) {
            return Some(text.clone());
        }
        [self.table, en::TABLE].into_iter().find_map(|table| {
            table
                .iter()
                .find(|(k, _)| *k == key)
                .map(|(_, text)| SharedString::new_static(text))
        })
    }

    /// The text of `key` with `{name}` placeholders replaced.
    pub(crate) fn format(&self, key: &str, args: &[(&str, &dyn fmt::Display)]) -> SharedString {
        let mut text = self.get(key).to_string();
        for (name, value) in args {
            text = text.replace(&format!("{{{name}}}"), &value.to_string());
        }
        text.into()
    }

    fn find(locale: &str) -> Option<(&'static str, &'static [(&'static str, &'static str)])> {
        // Strip the encoding and the modifier: de_DE.UTF-8@euro -> de_DE.
        let tag = locale.split(['.', '@']).next()?.replace('-', "_");
        let language = tag.split('_').next()?;
        if language.is_empty() || matches!(tag.as_str(), "C" | "POSIX") {
            return None;
        }
        let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
        LOCALES
            .iter()
            .find(|(t, _)| same(t, &tag))
            .or_else(|| {
                LOCALES
                    .iter()
                    .find(|(t, _)| same(t.split('_').next().unwrap_or(t), language))
            })
            .copied()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn placeholders(text: &str) -> Vec<&str> {
        let mut found: Vec<&str> = text
            .match_indices('{')
            .filter_map(|(i, _)| text[i..].find('}').map(|end| &text[i..=i + end]))
            .collect();
        found.sort_unstable();
        found
    }

    #[test]
    fn every_language_has_every_key_once_with_its_placeholders() {
        let english: HashMap<_, _> = en::TABLE.iter().copied().collect();
        assert_eq!(english.len(), en::TABLE.len(), "duplicate English key");
        for (tag, table) in LOCALES {
            let mut seen = std::collections::HashSet::new();
            for (key, text) in *table {
                assert!(seen.insert(key), "{tag}: {key} twice");
                let Some(source) = english.get(key) else {
                    panic!("{tag}: unknown key {key}");
                };
                assert!(!text.is_empty(), "{tag}: {key} is empty");
                assert_eq!(placeholders(text), placeholders(source), "{tag}: {key}");
            }
            assert_eq!(seen.len(), english.len(), "{tag}: missing keys");
        }
    }

    #[test]
    fn locales_resolve_to_the_closest_language() {
        let tag = |l: &str| EditorStrings::for_locale(l).locale();
        assert_eq!(tag("de_DE.UTF-8"), "de");
        assert_eq!(tag("de-AT"), "de");
        assert_eq!(tag("pt_BR.utf8"), "pt_BR");
        assert_eq!(tag("pt_PT"), "pt_BR");
        assert_eq!(tag("zh-TW"), "zh_CN");
        assert_eq!(tag("fr_CA@euro"), "fr");
        assert_eq!(tag("C"), "en");
        assert_eq!(tag(""), "en");
        assert_eq!(tag("nl_NL"), "en");
    }

    #[test]
    fn overrides_win_and_unknown_keys_fall_back() {
        let strings = EditorStrings::for_locale("de").set("slash.h1", "Titel");
        assert_eq!(strings.get("slash.h1"), "Titel");
        assert_eq!(strings.get("slash.h2"), "Überschrift 2");
        assert_eq!(strings.get("no.such.key"), "no.such.key");
        assert_eq!(strings.lookup("no.such.key"), None);
    }

    #[test]
    fn translate_fills_from_a_host_catalog() {
        let strings = EditorStrings::english()
            .translate(|key, english| (key == "find.close").then(|| format!("[{english}]")));
        assert_eq!(strings.get("find.close"), "[Close]");
        assert_eq!(strings.get("find.next"), "Next match");
    }

    #[test]
    fn placeholders_are_replaced() {
        let strings = EditorStrings::for_locale("de");
        let text = strings.format("find.position", &[("current", &3), ("total", &12)]);
        assert_eq!(text, "3 von 12");
    }
}
