//! Decision-facing index of positive, shared card requests and their timing.
//! Full item rules remain alongside it; this neither scores the deck nor
//! treats an unprojected effect as absent. No state execution or RNG access.
use std::fmt::Write;

use sts_oracle_learning::content::cards::{self, mechanics as m, CardId, CardType};

pub(crate) fn write_owned_card_links(
    out: &mut String,
    cards: impl IntoIterator<Item = (CardId, u8, i8, usize)>,
) {
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
        let source = format!(
            "{count}x {} | {}",
            crate::render::format_card(id, upgrades),
            super::card_attributes(id, upgrades, modifier, sts_presentation::Locale::En)
        );
        let rule = m::explanation(id).unwrap_or("See info card for the effect rule.");
        let row = |effect: String| format!("{source}: {effect}");
        if let Some(delivery) = m::card_player_strength_delivery_v1(id) {
            use m::CardPlayerStrengthDeliveryV1::*;
            let effect = match delivery {
                OnPlay => format!("request {magic} Strength on play, with no scheduled loss."),
                IfTargetIntentBaseDamageNonnegativeAtResolution => format!("request {magic} Strength if the target has an attack intent at resolution, including a zero-damage attack."),
                OnPlayWithSeparateLoseStrength => format!("request {magic} Strength on play, then a separate {magic} Lose Strength at turn end; preventing/removing that debuff changes persistence."),
                EachPlayerTurnStartPostDraw => format!("deployment grants 0 immediate Strength. Each later player turn start after normal draw requests {magic} Strength per application. One deployment on turn t contributes 0 on t, {magic} on t+1, {} on t+2, {} on t+3 before other effects. A later deployment shifts this schedule. A separate normal play of another drawn copy pays its own current cost and adds another application with its own deployment time.", magic * 2, magic * 3),
            };
            strength.push(row(effect));
        }
        if id == CardId::LimitBreak || m::card_hp_loss_strength_power_v1(id, magic).is_some() {
            conversion.push(row(rule.into()));
        }
        if let Some(loss) = m::card_self_hp_loss_on_play_v1(id, upgrades) {
            let destination = if definition.exhaust {
                "After the play it normally Exhausts; another use needs an actual recovery or copying effect."
            } else {
                "Another ordinary use needs reaching Hand again and paying its current cost."
            };
            self_loss.push(row(format!("each successful play requests one self HP-loss event of {} HP. With Rupture already deployed, actual positive self HP loss triggers its active Strength amount once for this event. {count} owned copies supply at most {count} such on-play events in one pass through those copies; repeated draws or copies have their own access and payment. {destination}", loss.base_hp_loss)));
        } else if let Some(loss) = m::card_end_of_turn_hp_loss_and_damage_v1(id, magic) {
            self_loss.push(row(format!("after deployment, at each player turn end while the combat is ongoing, request {} self HP loss per application as one combined event. An already deployed Rupture gains its active Strength amount once for that positive-loss event. Additional applications increase the loss amount of this event. {rule}", loss.hp_loss_per_application)));
        } else if id == CardId::Brutality {
            self_loss.push(row(rule.into()));
        }
        if m::card_hand_copy_v1(id, magic).is_some() {
            copying.push(row(format!("{rule} A useful copy needs its particular target sharing Hand before copying, then room and Energy for the resulting uses. Evaluate the intended target's added payoff and this entire payment sequence.")));
        }
        let roles = m::card_mechanic_profile(id, upgrades).roles;
        let draw = m::unconditional_card_draw_request_v1(id, magic);
        if let Some(n) = draw {
            access.push(row(format!("after successful play request {n} draws; draw prevention, hand capacity and follow-up effects still apply. {rule}")));
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
                OnPlay => "on play",
                OnExhaust => {
                    "only when this card is Exhausted; playing it alone does not grant this Energy"
                }
                EachPlayerTurnStart => "at each later player turn start after deployment",
                NextPlayerEnergyRecharge => "at the next player Energy recharge",
                IfTargetVulnerableAtResolution => "if the target is Vulnerable at resolution",
            };
            energy.push(row(format!(
                "request {} Energy {timing}. {rule}",
                gain.amount
            )));
        }
    }
    let groups = [
        ("Strength grants", strength),
        (
            "Strength amplification and trigger dependencies",
            conversion,
        ),
        ("Draw and pile access", access),
        ("Deployed trigger and Skill-cost rules", triggers),
        ("Energy grants", energy),
        ("Self HP-loss event sources and repetition", self_loss),
        ("Same-Hand copying", copying),
    ];
    if groups.iter().all(|(_, rows)| rows.is_empty()) {
        return;
    }
    writeln!(out, "Mechanic summary (same deck copies listed above):").unwrap();
    writeln!(out, "Normal card deployment requires reaching Hand and paying its current cost; free plays and on-Exhaust gains follow their own rules. Power triggers require prior deployment. Relic and potion rules are listed with their actual inventory below.").unwrap();
    for (name, rows) in groups {
        if rows.is_empty() {
            continue;
        }
        writeln!(out, "  {name}:").unwrap();
        for row in rows {
            writeln!(out, "    {row}").unwrap();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        );
        assert!(out.contains("after successful play request 4 draws"));
        assert!(out.contains(cards::ironclad::battle_trance::MECHANICS));
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
        );
        assert!(out.contains("one self HP-loss event of 2 HP"));
        assert!(out.contains("1 owned copies supply at most 1 such on-play events"));
        assert!(out.contains("2 owned copies supply at most 2 such on-play events"));
        assert!(out.contains("After the play it normally Exhausts"));
        assert!(out.contains("as one combined event"));
        assert!(out.contains("its particular target sharing Hand"));
    }
}
