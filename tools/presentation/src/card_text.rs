//! Plain-text formatting for verbatim original card templates. The caller
//! chooses the field and supplies numbers; this module owns no game rules.
use super::Locale;
use serde_json::Value;
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CardTextField {
    Description,
    UpgradeDescription,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CardTextValues {
    pub damage: i32,
    pub block: i32,
    pub magic: i32,
}

impl Locale {
    pub fn original_card_text(
        self,
        id: &str,
        field: CardTextField,
        values: CardTextValues,
    ) -> Result<Option<String>, String> {
        static SOURCE: OnceLock<Value> = OnceLock::new();
        let source = SOURCE.get_or_init(|| {
            serde_json::from_str(include_str!("../original-card-text.json"))
                .expect("embedded original card templates")
        });
        let Some(card) = source["cards"][self.to_string()].get(id) else {
            return Ok(None);
        };
        let key = match field {
            CardTextField::Description => "DESCRIPTION",
            CardTextField::UpgradeDescription => "UPGRADE_DESCRIPTION",
        };
        let template = card[key]
            .as_str()
            .filter(|text| !text.trim().is_empty() && !text.eq_ignore_ascii_case("deprecated"))
            .ok_or_else(|| format!("missing original card field: {id}/{key}"))?;
        plain_text(self, template, values).map(Some)
    }
}

fn plain_text(locale: Locale, template: &str, values: CardTextValues) -> Result<String, String> {
    let mut output = String::new();
    let mut rest = template;
    while !rest.is_empty() {
        let mut chars = rest.chars();
        let current = chars.next().unwrap();
        if current == '!' {
            if let Some(end) = rest[1..].find('!') {
                let token = &rest[1..end + 1];
                if !token.is_empty() && token.chars().all(|c| c.is_ascii_alphanumeric()) {
                    let number = match token {
                        "D" => values.damage,
                        "B" => values.block,
                        "M" => values.magic,
                        _ => return Err(format!("unknown original card variable: !{token}!")),
                    };
                    output.push_str(&number.to_string());
                    rest = &rest[end + 2..];
                    continue;
                }
            }
        }
        if current == '#' && chars.next().is_some_and(|c| "brgyp".contains(c)) {
            rest = &rest[2..];
            continue;
        }
        if current == '*' && rest[1..].chars().next().is_some_and(char::is_alphabetic) {
            rest = &rest[1..];
            continue;
        }
        if rest.starts_with('[')
            && rest.as_bytes().get(2) == Some(&b']')
            && rest.as_bytes().get(1).is_some_and(|c| b"RGBWE".contains(c))
        {
            output.push_str(
                locale
                    .entry("mechanics", "card.energy")
                    .expect("card energy symbol"),
            );
            rest = &rest[3..];
            continue;
        }
        output.push(current);
        rest = &rest[current.len_utf8()..];
    }
    let lines = output
        .split_whitespace()
        .fold(vec![Vec::new()], |mut lines, word| {
            if word == "NL" {
                lines.push(Vec::new());
            } else {
                lines.last_mut().unwrap().push(word);
            }
            lines
        });
    Ok(lines
        .into_iter()
        .map(|words| {
            let text = words.join(" ");
            if locale == Locale::En {
                return text;
            }
            let chars = text.chars().collect::<Vec<_>>();
            chars
                .iter()
                .enumerate()
                .filter_map(|(index, &c)| {
                    let previous = index.checked_sub(1).and_then(|i| chars.get(i)).copied();
                    let next = chars.get(index + 1).copied();
                    if c == ' '
                        && (next.is_some_and(|c| "。，、；：！？）".contains(c))
                            || (previous.is_some_and(is_cjk) && next.is_some_and(is_cjk)))
                    {
                        None
                    } else {
                        Some(c)
                    }
                })
                .collect::<String>()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn is_cjk(c: char) -> bool {
    ('\u{3400}'..='\u{9fff}').contains(&c)
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALUES: CardTextValues = CardTextValues {
        damage: 10,
        block: 8,
        magic: 3,
    };

    #[test]
    fn original_markup_preserves_variables_lines_keywords_and_energy_count() {
        assert_eq!(
            plain_text(
                Locale::En,
                "Deal !D! damage. NL Add !M! *Shivs. NL Gain [G] [G].",
                VALUES
            )
            .unwrap(),
            "Deal 10 damage.\nAdd 3 Shivs.\nGain [Energy] [Energy]."
        );
        assert_eq!(
            plain_text(
                Locale::ZhCn,
                "获得 !B! 点 #y格挡 。 NL 获得 [R] [R] 。",
                VALUES
            )
            .unwrap(),
            "获得 8 点格挡。\n获得 [能量] [能量]。"
        );
        assert_eq!(
            plain_text(Locale::En, "X+1 [G]. Keep #0 and 2 * 3!", VALUES).unwrap(),
            "X+1 [Energy]. Keep #0 and 2 * 3!"
        );
        assert!(plain_text(Locale::En, "Deal !Other! damage.", VALUES)
            .unwrap_err()
            .contains("unknown original card variable"));
    }

    #[test]
    fn field_selection_is_explicit_and_never_inferred_from_presence_or_emptiness() {
        let base = Locale::En
            .original_card_text("BodySlam", CardTextField::Description, VALUES)
            .unwrap()
            .unwrap();
        assert_eq!(base, "Deal damage equal to your Block.");
        assert!(Locale::En
            .original_card_text("Electrodynamics", CardTextField::UpgradeDescription, VALUES)
            .is_err());
        assert!(Locale::ZhCn
            .original_card_text("Impulse", CardTextField::Description, VALUES)
            .unwrap()
            .is_none());
    }
}
