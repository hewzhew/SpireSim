//! Read-only text resources shared with JavaScript. No simulator state or RNG.
use std::{fmt, str::FromStr, sync::OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

mod card_text;
pub use card_text::{CardTextField, CardTextValues};

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub enum Locale {
    #[default]
    #[serde(rename = "en")]
    En,
    #[serde(rename = "zh-CN")]
    ZhCn,
}

impl fmt::Display for Locale {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
        })
    }
}

impl FromStr for Locale {
    type Err = String;
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "en" => Ok(Self::En),
            "zh-CN" => Ok(Self::ZhCn),
            _ => Err(format!("unsupported text locale: {value}; use en or zh-CN")),
        }
    }
}

fn catalog(locale: Locale) -> &'static Value {
    static EN: OnceLock<Value> = OnceLock::new();
    static ZH: OnceLock<Value> = OnceLock::new();
    let (cell, source) = match locale {
        Locale::En => (&EN, include_str!("../locales/en.json")),
        Locale::ZhCn => (&ZH, include_str!("../locales/zh-CN.json")),
    };
    cell.get_or_init(|| serde_json::from_str(source).expect("embedded presentation catalog"))
}

impl Locale {
    fn original_event_text(self, id: &str, field: &str, index: usize) -> Option<&'static str> {
        static EVENTS: OnceLock<Value> = OnceLock::new();
        let source = EVENTS.get_or_init(|| {
            serde_json::from_str(include_str!("../original-event-text.json"))
                .expect("embedded original event text")
        });
        let language = match self {
            Self::En => "en",
            Self::ZhCn => "zh-CN",
        };
        source
            .get("events")?
            .get(language)?
            .get(id)?
            .get(field)?
            .get(index)?
            .as_str()
    }

    /// Verbatim original event scenes whose layout has an explicit consumer.
    /// This reads embedded resources; it never opens a local game installation.
    pub fn original_event_description(self, id: &str, index: usize) -> Option<&'static str> {
        self.original_event_text(id, "DESCRIPTIONS", index)
    }

    /// Raw original button fragment. The event renderer owns its assembly and
    /// supplies public numeric effects; this resource layer owns no game rules.
    pub fn original_event_option(self, id: &str, index: usize) -> Option<&'static str> {
        self.original_event_text(id, "OPTIONS", index)
    }

    pub fn original_neow_reward(self, index: usize) -> Option<&'static str> {
        self.original_event_text("Neow", "REWARD_TEXT", index)
    }

    pub fn original_neow_unique_reward(self, index: usize) -> Option<&'static str> {
        self.original_event_text("Neow", "UNIQUE_REWARDS", index)
    }

    /// Static rule text may explicitly reference another entity by stable ID.
    /// These slots carry names only; no rule values or game state are inferred.
    pub fn definition_text(self, namespace: &str, key: &str) -> Result<String, String> {
        let template = self
            .entry(namespace, key)
            .ok_or_else(|| format!("unknown presentation message: {namespace}:{key}"))?;
        self.definition_template(template)
    }

    fn definition_template(self, template: &str) -> Result<String, String> {
        let mut result = String::new();
        let mut remaining = template;
        while let Some(start) = remaining.find("{{") {
            result.push_str(&remaining[..start]);
            let tail = &remaining[start + 2..];
            let end = tail.find("}}").ok_or("invalid definition template")?;
            let name = tail[..end].trim();
            let (kind, id) = name
                .strip_prefix("entity_")
                .and_then(|value| value.split_once('_'))
                .filter(|(kind, _)| {
                    matches!(
                        *kind,
                        "card"
                            | "relic"
                            | "potion"
                            | "enemy"
                            | "power"
                            | "stance"
                            | "event"
                            | "character"
                            | "encounter"
                    )
                })
                .ok_or_else(|| format!("invalid definition reference: {name}"))?;
            let label = self
                .entry("game", &format!("{kind}.{id}"))
                .ok_or_else(|| format!("unknown definition entity: {kind}:{id}"))?;
            result.push_str(label);
            remaining = &tail[end + 2..];
        }
        result.push_str(remaining);
        Ok(result)
    }

    /// Native scalar text only. Plural selection remains with each renderer;
    /// this does not interpret i18next options or reparse substituted values.
    pub fn entry(self, namespace: &str, key: &str) -> Option<&'static str> {
        let lookup = |locale| {
            key.split('.')
                .try_fold(catalog(locale).get(namespace)?, |value, part| {
                    value.get(part)
                })?
                .as_str()
        };
        lookup(self).or_else(|| lookup(Self::En))
    }

    pub fn entity<'a>(self, kind: &str, id: &'a str) -> &'a str {
        self.entry("game", &format!("{kind}.{id}")).unwrap_or(id)
    }

    pub fn card_name(self, id: &str, upgrades: u8) -> String {
        let name = self.entity("card", id);
        match upgrades {
            0 => name.to_owned(),
            1 if id != "SearingBlow" => format!("{name}+"),
            _ => format!("{name}+{upgrades}"),
        }
    }

    pub fn text(
        self,
        namespace: &str,
        key: &str,
        parameters: &[(&str, &str)],
    ) -> Result<String, String> {
        let template = self
            .entry(namespace, key)
            .ok_or_else(|| format!("unknown presentation message: {namespace}:{key}"))?;
        let mut result = String::new();
        let mut remaining = template;
        while let Some(start) = remaining.find("{{") {
            result.push_str(&remaining[..start]);
            let tail = &remaining[start + 2..];
            let end = tail
                .find("}}")
                .ok_or_else(|| format!("invalid presentation template: {namespace}:{key}"))?;
            let name = tail[..end].trim();
            let value = parameters
                .iter()
                .find(|(parameter, _)| *parameter == name)
                .ok_or_else(|| format!("missing presentation parameter: {namespace}:{key}:{name}"))?
                .1;
            result.push_str(value);
            remaining = &tail[end + 2..];
        }
        result.push_str(remaining);
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn definition_references_resolve_exact_entities_and_reject_unbound_parameters() {
        for (locale, expected) in [(Locale::En, "Heavy Blade"), (Locale::ZhCn, "重刃")] {
            assert!(locale
                .definition_text("game", "powerRule.Strength.effect")
                .unwrap()
                .contains(expected));
            assert_eq!(
                locale
                    .definition_template("{{entity_card_Strike}} {{entity_card_StrikeG}}")
                    .unwrap(),
                format!(
                    "{} {}",
                    locale.entity("card", "Strike"),
                    locale.entity("card", "StrikeG")
                )
            );
            assert!(locale
                .definition_template("{{entity_card_UnknownCard}}")
                .unwrap_err()
                .contains("unknown definition entity"));
            for template in [
                "{{amount}}",
                "{{entity_effect_damage}}",
                "{{entity_card_Strike",
                "{{entity card Strike}}",
            ] {
                assert!(locale.definition_template(template).is_err(), "{template}");
            }
            assert!(locale.definition_text("game", "effect.damage").is_err());
            assert!(locale
                .definition_text("game", "powerRule.Unknown.effect")
                .is_err());
        }
    }

    #[test]
    fn original_names_retain_stable_ids_when_display_names_are_shared_or_changed() {
        assert_eq!(Locale::ZhCn.entity("event", "DrugDealer"), "增益研究者");
        assert_eq!(Locale::En.entity("event", "Ssssserpent"), "The Ssssserpent");
        assert_eq!(
            Locale::ZhCn.entity("event", "FountainOfCurseCleansing"),
            "神圣泉水"
        );
        assert_eq!(Locale::En.card_name("SearingBlow", 1), "Searing Blow+1");
        assert_eq!(Locale::ZhCn.card_name("SearingBlow", 1), "灼热攻击+1");
        for id in ["Strike", "StrikeG", "StrikeB", "StrikeP"] {
            assert_eq!(Locale::ZhCn.entity("card", id), "打击");
            assert_eq!(Locale::En.entity("card", id), "Strike");
        }
        assert_eq!(Locale::ZhCn.entity("card", "SwordBoomerang"), "飞剑回旋镖");
        assert_eq!(Locale::ZhCn.entity("encounter", "TheChamp"), "第一勇士");
        assert_eq!(
            Locale::ZhCn.entity("encounter", "Automaton"),
            "铜制机械人偶"
        );
        assert_eq!(Locale::En.entity("power", "Weak"), "Weakened");
        assert_eq!(Locale::ZhCn.entity("potion", "Elixir"), "万灵药水");
        assert_eq!(Locale::ZhCn.entity("relic", "HappyFlower"), "开心小花");
        assert_eq!(Locale::ZhCn.entity("card", "Impulse"), "Impulse");
    }

    #[test]
    fn event_scene_templates_are_embedded_and_retain_the_original_control_markup() {
        assert!(Locale::En
            .original_event_description("FaceTrader", 0)
            .unwrap()
            .contains("~\"Stop.\"~"));
        assert!(Locale::ZhCn
            .original_event_description("FaceTrader", 0)
            .unwrap()
            .contains("NL"));
        assert!(Locale::En
            .original_event_description("FaceTrader", 5)
            .is_none());
        assert!(Locale::ZhCn.original_event_description("Neow", 0).is_none());
    }

    #[test]
    fn locales_are_explicit_and_instances_do_not_share_a_language_preference() {
        assert_eq!("en".parse(), Ok(Locale::En));
        assert_eq!("zh-CN".parse(), Ok(Locale::ZhCn));
        assert!("zh".parse::<Locale>().is_err());
        assert!("fr".parse::<Locale>().is_err());
        assert_eq!(serde_json::to_string(&Locale::ZhCn).unwrap(), "\"zh-CN\"");
        assert_eq!(Locale::ZhCn.entity("potion", "SteroidPotion"), "类固醇药水");
        assert_eq!(Locale::En.entity("potion", "SteroidPotion"), "Flex Potion");
        assert_eq!(
            Locale::ZhCn.entity("potion", "FuturePotion"),
            "FuturePotion"
        );
    }

    #[test]
    fn scalar_templates_reject_missing_arguments_and_preserve_literal_parameter_text() {
        assert!(Locale::En
            .text("game", "effect.damage", &[])
            .unwrap_err()
            .contains("missing presentation parameter"));
        assert!(Locale::En
            .text("game", "missing", &[])
            .unwrap_err()
            .contains("unknown presentation message"));
        assert_eq!(
            Locale::ZhCn
                .text("game", "effect.damage", &[("damage", "{{keep}} 原文")])
                .unwrap(),
            "造成 {{keep}} 原文 点伤害"
        );
    }
}
