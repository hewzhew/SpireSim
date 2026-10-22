//! Read-only text resources shared with JavaScript. No simulator state or RNG.
use std::{fmt, str::FromStr, sync::OnceLock};

use serde::{Deserialize, Serialize};
use serde_json::Value;

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
    fn original_names_retain_stable_ids_when_display_names_are_shared_or_changed() {
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
