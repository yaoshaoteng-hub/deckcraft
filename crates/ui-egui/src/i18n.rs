//! Interface language: the centralized switch that turns the UI's English source strings into
//! another language. The first edition ships English and Simplified Chinese; adding a language is
//! a new [`Lang`] variant, an entry in [`CATALOGS`], and an entry in [`LANGUAGES`].
//!
//! The translations are held by the workspace's existing `craft-i18n` system, not re-implemented
//! here: an embedded TSV catalog (`catalogs/zh-hans.tsv`) is loaded once and looked up with the
//! English source string as the key. A key with no translation falls back to the key itself, which
//! is English, so a missing Chinese string never shows an empty label. This is a catalog and a
//! switch — never a full-project replacement of the literals.
//!
//! Only the interface is translated. Slide content, file names, command ids, proofing languages and
//! file-format fields are data, not interface, and pass through untouched.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize};

/// A language the interface can be shown in.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Lang {
    /// English (the source language; every string already exists in it).
    #[default]
    En,
    /// Simplified Chinese (Mandarin).
    ZhHans,
}

impl Lang {
    /// The two languages offered today. Adding one later starts here.
    pub const ALL: [Lang; 2] = [Lang::En, Lang::ZhHans];

    /// The stable code stored in `ui.json`.
    pub fn as_code(self) -> &'static str {
        match self {
            Lang::En => "en",
            Lang::ZhHans => "zh-Hans",
        }
    }

    /// The language's own name, for the selector. It is deliberately *not* translated: a user who
    /// switched to a language they cannot read still recognises "English" and "简体中文" and can
    /// switch back. The name comes from the catalog (`@id language.name`), so it travels with the
    /// translations.
    pub fn display(self) -> &'static str {
        I18N.name(craft_lang(self))
    }
}

/// Serialize as the code (`"en"`, `"zh-Hans"`).
impl Serialize for Lang {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_code())
    }
}

/// Deserialize leniently: a missing or unknown value is English, never an error, so an old or
/// hand-edited `ui.json` keeps loading with the rest of its settings intact.
impl<'de> Deserialize<'de> for Lang {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let code = String::deserialize(d)?;
        Ok(match code.as_str() {
            "zh-Hans" | "zh-Hans-CN" | "zh-CN" | "zh_CN" | "zh" => Lang::ZhHans,
            _ => Lang::En,
        })
    }
}

/// The Simplified Chinese catalog text — the one copy, parsed by `craft-i18n` for lookups and
/// scanned here for the strings the font machinery must cover.
static ZH_CATALOG: &str = include_str!("i18n/catalogs/zh-hans.tsv");

/// The catalogs embedded at build time: one entry per locale, `(locale, shared, app)`.
static CATALOGS: &craft_i18n::Embedded = &[("zh-hans", "", ZH_CATALOG)];

/// The non-English languages the interface offers, as `craft-i18n` locale codes.
pub const LANGUAGES: &[&str] = &["zh-hans"];

/// The loaded catalogs. Built once; a catalog problem is logged, never a failure.
static I18N: LazyLock<craft_i18n::I18n> = LazyLock::new(|| {
    let (i18n, problems) = craft_i18n::I18n::new(CATALOGS, LANGUAGES);
    for problem in &problems {
        log::warn!("interface language: {problem}");
    }
    i18n
});

/// The `craft-i18n` language for [`Lang::ZhHans`], looked up once.
static ZH: LazyLock<craft_i18n::Lang> =
    LazyLock::new(|| I18N.from_code("zh-hans").unwrap_or(craft_i18n::Lang::EN));

fn craft_lang(lang: Lang) -> craft_i18n::Lang {
    match lang {
        Lang::En => craft_i18n::Lang::EN,
        Lang::ZhHans => *ZH,
    }
}

/// Translate `key` into `lang`, returning `key` (English) when there is no translation.
pub fn tr_for<'a>(lang: Lang, key: &'a str) -> &'a str {
    I18N.t(craft_lang(lang), key)
}

/// The Chinese translations the catalog holds, read from the catalog text itself so the font
/// machinery covers exactly what a translated interface can draw. The translation column is taken
/// straight (the catalog's translations keep no escapes), so a multi-line *source* key stays out
/// of this list while its translation — the text actually drawn — stays in.
fn catalog_translations() -> impl Iterator<Item = &'static str> {
    ZH_CATALOG.lines().filter_map(|line| {
        let line = line.strip_suffix('\r').unwrap_or(line);
        if line.is_empty() || line.starts_with('#') {
            return None;
        }
        let mut columns = line.split('\t');
        let _context = columns.next()?;
        let _source = columns.next()?;
        let translation = columns.next()?;
        (!translation.is_empty()).then_some(translation)
    })
}

/// The interface text the font machinery must be able to draw, whatever the current language: the
/// selector's own names (always shown, so a user who switched can still find the way back) and,
/// when `lang` is not English, its translated strings. The interface faces lack Simplified Chinese
/// glyphs, so [`crate::uifonts`] borrows a face for these once per session. Adding a language
/// extends the `match` here and its catalog entry.
pub fn interface_text(lang: Lang) -> &'static str {
    match lang {
        Lang::En => LANGUAGE_NAMES.as_str(),
        Lang::ZhHans => ZH_INTERFACE.as_str(),
    }
}

/// The selector's own names in every language: `English`, `简体中文`, … — always visible, so a
/// switched user recognises the way back.
static LANGUAGE_NAMES: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::new();
    for lang in Lang::ALL {
        text.push_str(lang.display());
        text.push('\n');
    }
    text
});

/// The Simplified Chinese interface text, joined once: the selector's names and every translation
/// in the catalog.
static ZH_INTERFACE: LazyLock<String> = LazyLock::new(|| {
    let mut text = String::with_capacity(1024);
    text.push_str(LANGUAGE_NAMES.as_str());
    for translation in catalog_translations() {
        text.push_str(translation);
        text.push('\n');
    }
    text
});

// The current interface language is carried in the egui context for one frame, so free helpers
// (`buttons`, dialog titles, the ribbon) translate without threading a language through every call.

fn lang_id() -> egui::Id {
    egui::Id::unique("deckcraft.interface-language")
}

/// Record the language the frame draws in; call once at the top of the root UI pass.
pub fn set_lang(ctx: &egui::Context, lang: Lang) {
    ctx.data_mut(|d| d.insert_temp(lang_id(), lang));
}

/// The language the frame draws in (English when none was set).
pub fn lang(ctx: &egui::Context) -> Lang {
    ctx.data(|d| d.get_temp(lang_id())).unwrap_or(Lang::En)
}

/// Translate `key` into the frame's language, returning `key` (English) when untranslated.
pub fn tr<'a>(ctx: &egui::Context, key: &'a str) -> &'a str {
    tr_for(lang(ctx), key)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used)]

    use super::*;

    /// The embedded catalog is well-formed and offers Simplified Chinese with its own name.
    #[test]
    fn catalog_is_well_formed_and_offers_simplified_chinese() {
        let (i18n, problems) = craft_i18n::I18n::new(CATALOGS, LANGUAGES);
        assert!(problems.is_empty(), "{problems:?}");
        assert!(i18n.check_all().is_empty(), "no row is skipped: {:?}", i18n.check_all());
        let zh = i18n.from_code("zh-hans").expect("zh-hans is offered");
        assert_eq!(i18n.code(zh), "zh-hans");
        assert_eq!(i18n.name(zh), "简体中文");
        assert_eq!(i18n.t(zh, "File"), "文件");
        assert_eq!(i18n.t(zh, "Cancel"), "取消");
        assert_eq!(i18n.t(zh, "Untranslated"), "Untranslated", "missing is English");
    }

    /// A known key translates and an unknown key falls back to the English source string.
    #[test]
    fn missing_translations_fall_back_to_english() {
        assert_eq!(tr_for(Lang::ZhHans, "File"), "文件");
        assert_eq!(tr_for(Lang::ZhHans, "Home"), "开始");
        assert_eq!(tr_for(Lang::ZhHans, "OK"), "确定");
        assert_eq!(tr_for(Lang::ZhHans, "Cancel"), "取消");
        // Not yet translated: the key itself (English) is shown, never empty.
        assert_eq!(tr_for(Lang::ZhHans, "Some Untranslated Label"), "Some Untranslated Label");
        // English is the identity.
        assert_eq!(tr_for(Lang::En, "File"), "File");
        assert_eq!(tr_for(Lang::En, "Some Untranslated Label"), "Some Untranslated Label");
    }

    /// The selector's display names are never translated, so the way back is always visible.
    #[test]
    fn selector_names_are_stable_and_bilingual() {
        assert_eq!(Lang::En.display(), "English");
        assert_eq!(Lang::ZhHans.display(), "简体中文");
    }

    /// Codes round-trip, and an unknown or missing code is English, not an error.
    #[test]
    fn language_codes_round_trip_and_unknown_is_english() {
        for lang in Lang::ALL {
            let code = lang.as_code();
            let back: Lang = serde_json::from_value(serde_json::Value::String(code.into())).unwrap();
            assert_eq!(back, lang);
        }
        // Alternate Simplified-Chinese spellings all resolve to ZhHans.
        for code in ["zh-Hans", "zh-Hans-CN", "zh-CN", "zh_CN", "zh"] {
            let l: Lang = serde_json::from_value(serde_json::Value::String(code.into())).unwrap();
            assert_eq!(l, Lang::ZhHans);
        }
        // Unknown values are English.
        for code in ["fr", "de", "ja", "English", ""] {
            let l: Lang = serde_json::from_value(serde_json::Value::String(code.into())).unwrap();
            assert_eq!(l, Lang::En);
        }
    }

    /// The interface text handed to the font machinery covers the selector's own names in every
    /// language (the way back) and, in Chinese, the translated strings.
    #[test]
    fn interface_text_covers_the_translated_strings() {
        assert!(interface_text(Lang::En).contains("简体中文"), "the selector name is covered even in English");
        let text = interface_text(Lang::ZhHans);
        assert!(text.contains("文件"), "{text}");
        assert!(text.contains("简体中文"), "{text}");
    }

    /// With the language set to Simplified Chinese, the File tab is announced and labelled in
    /// Chinese, and the English label is gone.
    #[test]
    fn switching_the_interface_language_relabels_the_ribbon() {
        use egui_kittest::kittest::Queryable;
        let mut h = egui_kittest::Harness::builder()
            .with_size(egui::vec2(1200.0, 800.0))
            // The interface faces lack Simplified Chinese glyphs on a machine without craft-fonts;
            // this test checks the relabeling, not glyph coverage.
            .allow_missing_glyphs()
            .build_ui_state(
                |ui, app: &mut crate::SlideApp| {
                    app.logic(ui.ctx());
                    app.ui(ui);
                },
                crate::SlideApp::new(deckcraft_engine::Session::with_new(), crate::Services::default()),
            );
        h.run_steps(3);
        assert!(h.query_by_label("File").is_some(), "English by default");
        h.state_mut().ui.lang = Lang::ZhHans;
        h.run_steps(3);
        assert!(h.query_by_label("文件").is_some(), "the File tab is now labelled in Chinese");
        assert!(h.query_by_label("File").is_none(), "the English File label is gone");
        // The ribbon content below the tab row translates too: a couple of Home-tab buttons.
        assert!(h.query_by_label("粘贴").is_some(), "the Paste button is labelled in Chinese");
        assert!(h.query_by_label("新建幻灯片").is_some(), "the New Slide button is labelled in Chinese");
        assert!(h.query_by_label("Paste").is_none(), "the English Paste label is gone");
    }
}
