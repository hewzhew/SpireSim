//! Decision-facing index of positive, shared card requests and their timing.
//! Full item rules remain alongside it; this neither scores the deck nor
//! treats an unprojected effect as absent. No state execution or RNG access.
use std::fmt::Write;
use sts_presentation::Locale;

use sts_oracle_learning::content::cards::{self, mechanics as m, CardId, CardType};

pub(crate) fn write_owned_card_links(
    out: &mut String,
    cards: impl IntoIterator<Item = (CardId, u8, i8, usize)>,
    locale: Locale,
) {
    let text = |key: &str, parameters: &[(&str, &str)]| {
        locale
            .text("mechanics", &format!("cardLinks.{key}"), parameters)
            .expect("card mechanism link message")
    };
    let mut strength = Vec::new();
    let mut conversion = Vec::new();
    let mut access = Vec::new();
    let mut triggers = Vec::new();
    let mut energy = Vec::new();
    let mut self_loss = Vec::new();
    let mut copying = Vec::new();
    for (id, upgrades, modifier, count) in cards {
        let definition = cards::get_card_definition(id);
        let magic = definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic;
        let source = text(
            "source",
            &[
                ("count", &count.to_string()),
                ("name", &locale.card_name(id.stable_id(), upgrades)),
                (
                    "attributes",
                    &super::card_attributes(id, upgrades, modifier, locale),
                ),
            ],
        );
        let rule_text = m::get(id)
            .map(|rule| rule.explanation(locale))
            .unwrap_or_else(|| text("ruleUnavailable", &[]));
        let rule = rule_text.as_str();
        let row = |effect: String| text("row", &[("source", &source), ("effect", &effect)]);
        if let Some(delivery) = m::card_player_strength_delivery_v1(id) {
            use m::CardPlayerStrengthDeliveryV1::*;
            let effect = match delivery {
                OnPlay => text("strengthOnPlay", &[("amount", &magic.to_string())]),
                IfTargetIntentBaseDamageNonnegativeAtResolution => {
                    text("strengthAttackIntent", &[("amount", &magic.to_string())])
                }
                OnPlayWithSeparateLoseStrength => {
                    text("strengthTemporary", &[("amount", &magic.to_string())])
                }
                EachPlayerTurnStartPostDraw => text(
                    "strengthLaterTurns",
                    &[
                        ("amount", &magic.to_string()),
                        ("second", &(magic * 2).to_string()),
                        ("third", &(magic * 3).to_string()),
                    ],
                ),
            };
            strength.push(row(effect));
        }
        if id == CardId::LimitBreak || m::card_hp_loss_strength_power_v1(id, magic).is_some() {
            conversion.push(row(rule.into()));
        }
        if let Some(loss) = m::card_self_hp_loss_on_play_v1(id, upgrades) {
            let destination = if definition.exhaust {
                text("selfLossExhaust", &[])
            } else {
                text("selfLossRepeat", &[])
            };
            self_loss.push(row(text(
                "selfLossOnPlay",
                &[
                    ("loss", &loss.base_hp_loss.to_string()),
                    (
                        "rupture",
                        locale.entity("card", CardId::Rupture.stable_id()),
                    ),
                    ("count", &count.to_string()),
                    ("destination", &destination),
                ],
            )));
        } else if let Some(loss) = m::card_end_of_turn_hp_loss_and_damage_v1(id, magic) {
            self_loss.push(row(text(
                "selfLossTurnEnd",
                &[
                    ("loss", &loss.hp_loss_per_application.to_string()),
                    (
                        "rupture",
                        locale.entity("card", CardId::Rupture.stable_id()),
                    ),
                    ("rule", rule),
                ],
            )));
        } else if id == CardId::Brutality {
            self_loss.push(row(rule.into()));
        }
        if m::card_hand_copy_v1(id, magic).is_some() {
            copying.push(row(text("copying", &[("rule", rule)])));
        }
        let roles = m::card_mechanic_profile(id, upgrades).roles;
        let draw = m::unconditional_card_draw_request_v1(id, magic);
        if let Some(n) = draw {
            access.push(row(text(
                "draw",
                &[("count", &n.to_string()), ("rule", rule)],
            )));
        } else if roles.contains(m::CardMechanicRole::DrawCards)
            || roles.contains(m::CardMechanicRole::DrawPileControl)
        {
            access.push(row(rule.into()));
        }
        if definition.card_type == CardType::Power
            && (roles.contains(m::CardMechanicRole::ExhaustPayoff)
                || roles.contains(m::CardMechanicRole::StatusDrawPayoff)
                || id == CardId::Corruption)
        {
            triggers.push(row(rule.into()));
        }
        if let Some(gain) = m::card_energy_gain_v1(id, upgrades, magic) {
            use m::CardEnergyDeliveryV1::*;
            let timing = match gain.delivery {
                OnPlay => "energyOnPlay",
                OnExhaust => "energyOnExhaust",
                EachPlayerTurnStart => "energyLaterTurn",
                NextPlayerEnergyRecharge => "energyNextRecharge",
                IfTargetVulnerableAtResolution => "energyVulnerable",
            };
            energy.push(row(text(
                timing,
                &[("amount", &gain.amount.to_string()), ("rule", rule)],
            )));
        }
    }
    let groups = [
        ("strength", strength),
        ("conversion", conversion),
        ("access", access),
        ("triggers", triggers),
        ("energy", energy),
        ("selfLoss", self_loss),
        ("copy", copying),
    ];
    if groups.iter().all(|(_, rows)| rows.is_empty()) {
        return;
    }
    writeln!(out, "{}", text("heading", &[])).unwrap();
    writeln!(out, "{}", text("deployment", &[])).unwrap();
    for (name, rows) in groups {
        if rows.is_empty() {
            continue;
        }
        writeln!(out, "  {}:", text(&format!("group.{name}"), &[])).unwrap();
        for row in rows {
            writeln!(out, "    {row}").unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chinese_links_keep_delayed_counts_conditional_energy_and_official_names() {
        let mut out = String::new();
        write_owned_card_links(
            &mut out,
            [
                (CardId::DemonForm, 0, 0, 1),
                (CardId::Sentinel, 1, 0, 2),
                (CardId::Offering, 0, 0, 2),
                (CardId::DualWield, 0, 0, 1),
            ],
            Locale::ZhCn,
        );
        assert!(out.contains("1× 恶魔形态"));
        assert!(out.contains("t+1 为 2，t+2 为 4，t+3 为 6"));
        assert!(out.contains("仅在此牌被消耗时请求获得 3 点能量"));
        assert!(out.contains("至多提供 2 次这样的打出事件"));
        assert!(out.contains(Locale::ZhCn.entity("card", CardId::Rupture.stable_id())));
        assert!(out.contains("复制前目标牌必须同处手牌"));
        assert!(!out.contains("Mechanic summary"));
    }

    #[test]
    fn sources_keep_counts_costs_and_delayed_growth_separate_from_amplification() {
        let mut out = String::new();
        write_owned_card_links(
            &mut out,
            [
                (CardId::DemonForm, 0, 0, 1),
                (CardId::DemonForm, 1, -1, 2),
                (CardId::Flex, 1, 0, 1),
                (CardId::LimitBreak, 0, 0, 1),
                (CardId::ShrugItOff, 0, 0, 1),
            ],
            Locale::En,
        );
        assert!(out.contains("1x Demon Form | Power | 3c"));
        assert!(out.contains("2x Demon Form+ | Power | 2c"));
        assert!(out.contains("0 on t, 2 on t+1, 4 on t+2, 6 on t+3"));
        assert!(out.contains("0 on t, 3 on t+1, 6 on t+2, 9 on t+3"));
        assert!(out.contains("A later deployment shifts this schedule"));
        assert!(out.contains("request 4 Strength on play, then a separate 4 Lose Strength"));
        let grants = out
            .split("Strength amplification and trigger dependencies:")
            .next()
            .unwrap();
        assert!(!grants.contains("Limit Break"));
        assert!(out.contains("Gives no Strength if you have none"));
        assert!(out.contains("another drawn copy pays its own current cost"));
        assert!(out.contains("1x Shrug It Off | Skill | 1c"));
        assert!(out.contains("after successful play request 1 draws"));
    }

    #[test]
    fn access_and_conditional_fuel_preserve_their_upstream_trigger_rules() {
        let mut out = String::new();
        write_owned_card_links(
            &mut out,
            [
                (CardId::Corruption, 1, 0, 1),
                (CardId::Sentinel, 1, 0, 2),
                (CardId::DarkEmbrace, 1, 0, 1),
                (CardId::Evolve, 1, 0, 1),
                (CardId::BattleTrance, 1, 0, 1),
                (CardId::Warcry, 1, 0, 1),
            ],
            Locale::En,
        );
        assert!(out.contains("after successful play request 4 draws"));
        assert!(out.contains(&cards::ironclad::battle_trance::MECHANICS.explanation(Locale::En)));
        assert!(out.contains("then put 1 card"));
        assert!(out.contains("request 3 Energy only when this card is Exhausted"));
        assert!(out.contains("Skills cost 0 and Exhaust when played"));
        assert!(out.contains("whenever a card is Exhausted, draw 1 card per stack"));
        assert!(out.contains("Curses do not trigger this effect"));
        assert!(out.contains("Power triggers require prior deployment"));
    }

    #[test]
    fn self_loss_counts_distinguish_single_pass_exhaust_and_deployed_repetition() {
        let mut out = String::new();
        write_owned_card_links(
            &mut out,
            [
                (CardId::Hemokinesis, 0, 0, 1),
                (CardId::Offering, 0, 0, 2),
                (CardId::Combust, 0, 0, 1),
                (CardId::DualWield, 0, 0, 1),
            ],
            Locale::En,
        );
        assert!(out.contains("one self HP-loss event of 2 HP"));
        assert!(out.contains("1 owned copies supply at most 1 such on-play events"));
        assert!(out.contains("2 owned copies supply at most 2 such on-play events"));
        assert!(out.contains("After the play it normally Exhausts"));
        assert!(out.contains("as one combined event"));
        assert!(out.contains("its particular target sharing Hand"));
    }
}
