//! Offline draw-trigger relations. Requests are conditioned on reaching one
//! existing source, never added together into a score or a predicted hand.
use serde::Serialize;
use sts_core::content::cards::{is_ethereal, CardId};
use sts_core::content::powers::{card_draw_trigger_requests, store, PowerId};
use sts_core::runtime::action::Action;
use sts_core::runtime::combat::{CardLineageId, CombatState};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DrawTriggerZone {
    Hand,
    Draw,
    Discard,
}

#[derive(Clone, Debug, Serialize)]
pub struct DrawTriggerPayoff {
    pub power: PowerId,
    pub stack: i32,
    /// Shared content-owner requests at the current attributes. No damage
    /// mitigation, hand admission, chained draw or subsequent play is settled.
    pub requests: Vec<Action>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DrawTriggerSource {
    pub card: CardId,
    pub lineage: CardLineageId,
    /// A hand source must return to draw; it has already run its draw hooks.
    /// A discard source requires a shuffle. Neither return is promised here.
    pub zone: DrawTriggerZone,
    pub index: usize,
    pub payoffs: Vec<DrawTriggerPayoff>,
}

#[derive(Clone, Debug, Serialize)]
pub struct DrawTriggerProfile {
    pub draw_blocked_now: bool,
    pub hand_slots_now: usize,
    pub sources: Vec<DrawTriggerSource>,
}

/// Only the audit/inspection calls this. Ordinary candidate evaluation does
/// not collect these relations or turn latent requests into current resources.
pub(in crate::solver) fn profile(c: &CombatState) -> Option<DrawTriggerProfile> {
    let player = &c.entities.player;
    let powers = store::powers_for(c, player.id).unwrap_or(&[]);
    if !powers
        .iter()
        .any(|p| p.amount > 0 && matches!(p.power_type, PowerId::Evolve | PowerId::FireBreathing))
    {
        return None;
    }
    let mut sources = vec![];
    for (zone, index, card) in c
        .zones
        .hand
        .iter()
        .enumerate()
        .filter(|(_, card)| !is_ethereal(card))
        .map(|(i, card)| (DrawTriggerZone::Hand, i, card))
        .chain(
            c.zones
                .draw_pile
                .iter()
                .enumerate()
                .map(|(i, card)| (DrawTriggerZone::Draw, i, card)),
        )
        .chain(
            c.zones
                .discard_pile
                .iter()
                .enumerate()
                .map(|(i, card)| (DrawTriggerZone::Discard, i, card)),
        )
    {
        let payoffs: Vec<_> = powers
            .iter()
            .filter(|p| p.amount > 0)
            .filter_map(|p| {
                let requests =
                    card_draw_trigger_requests(p.power_type, c, player.id, p.amount, card.id);
                (!requests.is_empty()).then(|| DrawTriggerPayoff {
                    power: p.power_type,
                    stack: p.amount,
                    requests: requests.into_vec(),
                })
            })
            .collect();
        if !payoffs.is_empty() {
            sources.push(DrawTriggerSource {
                card: card.id,
                lineage: card.lineage,
                zone,
                index,
                payoffs,
            });
        }
    }
    Some(DrawTriggerProfile {
        draw_blocked_now: store::has_power(c, player.id, PowerId::NoDraw),
        hand_slots_now: 10usize.saturating_sub(c.zones.hand.len()),
        sources,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::solver::tests::root;
    use sts_core::runtime::combat::{CombatCard, Power, PowerPayload};
    use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
    use sts_core::state::core::ClientInput;

    fn power(power_type: PowerId, amount: i32) -> Power {
        Power {
            power_type,
            amount,
            instance_id: None,
            extra_data: 0,
            just_applied: false,
            payload: PowerPayload::None,
        }
    }

    #[test]
    fn conditional_draw_requests_match_real_status_and_curse_delivery() {
        for (source, extra) in [(CardId::Wound, 1), (CardId::Injury, 0)] {
            let mut r = root(&[CardId::PommelStrike], 100);
            r.combat.zones.hand[0].upgrades = 1;
            r.combat.zones.draw_pile = [source, CardId::Strike, CardId::Defend, CardId::Bash]
                .into_iter()
                .enumerate()
                .map(|(i, id)| CombatCard::for_scenario(id, 10 + i as u32))
                .collect();
            store::set_powers_for(
                &mut r.combat,
                0,
                vec![power(PowerId::Evolve, 1), power(PowerId::FireBreathing, 6)],
            );
            let original = r.clone();
            let relation = profile(&r.combat).unwrap();
            let requests: Vec<_> = relation.sources[0]
                .payoffs
                .iter()
                .flat_map(|p| &p.requests)
                .collect();
            assert_eq!(
                requests
                    .iter()
                    .filter(|a| matches!(a, Action::DrawCards(1)))
                    .count(),
                extra
            );
            assert!(requests.iter().any(|a| matches!(a,
                Action::DamageAllEnemies { damages, .. } if damages.as_slice() == [6])));
            assert_eq!(r, original, "analysis must preserve state, order and RNG");
            let result = EngineCombatStepper.apply_to_stable(
                &r,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(1),
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!result.truncated && !result.timed_out);
            assert_eq!(result.position.combat.zones.hand.len(), 2 + extra);
            assert_eq!(result.position.combat.entities.monsters[0].current_hp, 84);
        }
    }

    #[test]
    fn no_draw_and_mitigation_separate_requests_from_actual_resources() {
        for no_draw in [false, true] {
            let mut r = root(&[CardId::PommelStrike], 100);
            r.combat.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Wound, 10)].into();
            let mut powers = vec![power(PowerId::Evolve, 1), power(PowerId::FireBreathing, 6)];
            if no_draw {
                powers.push(power(PowerId::NoDraw, -1));
            }
            store::set_powers_for(&mut r.combat, 0, powers);
            store::set_powers_for(&mut r.combat, 1, vec![power(PowerId::Intangible, 1)]);
            let relation = profile(&r.combat).unwrap();
            assert_eq!(relation.draw_blocked_now, no_draw);
            assert_eq!(
                relation.sources[0]
                    .payoffs
                    .iter()
                    .any(|p| p.power == PowerId::Evolve),
                !no_draw
            );
            // Fire Breathing's six-point request still needs a draw and then
            // mitigation. It is never six points of delivered damage here.
            let result = EngineCombatStepper.apply_to_stable(
                &r,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(1),
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!result.truncated && !result.timed_out);
            assert_eq!(
                result.position.combat.entities.monsters[0].current_hp,
                if no_draw { 99 } else { 98 }
            );
        }
    }

    #[test]
    fn draw_trigger_sources_preserve_occurrences_and_return_requirements() {
        let mut r = root(&[CardId::Wound, CardId::Dazed], 100);
        r.combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Wound, 10),
            CombatCard::for_scenario(CardId::Wound, 11),
            CombatCard::for_scenario(CardId::Dazed, 12),
        ]
        .into();
        r.combat.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Wound, 13)].into();
        r.combat.zones.exhaust_pile = vec![CombatCard::for_scenario(CardId::Wound, 14)].into();
        r.combat.zones.limbo = vec![CombatCard::for_scenario(CardId::Wound, 15)];
        assert!(profile(&r.combat).is_none());
        store::set_powers_for(&mut r.combat, 0, vec![power(PowerId::Evolve, 1)]);
        let relation = profile(&r.combat).unwrap();
        assert_eq!(relation.hand_slots_now, 8);
        let locations: Vec<_> = relation.sources.iter().map(|s| (s.zone, s.index)).collect();
        assert_eq!(
            locations,
            vec![
                (DrawTriggerZone::Hand, 0),
                (DrawTriggerZone::Draw, 0),
                (DrawTriggerZone::Draw, 1),
                (DrawTriggerZone::Draw, 2),
                (DrawTriggerZone::Discard, 0),
            ]
        );
        assert_ne!(relation.sources[1].lineage, relation.sources[2].lineage);
    }
}
