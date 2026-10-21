//! Shop policy owned by the strategic-owner crate.

use std::collections::BTreeMap;

use sts_simulator::eval::run_control::{
    exact_shop_policy_prior_v1, DecisionSurface, RunControlSession, RunPolicyCandidateV1,
};

use super::owner_commands::executable_choices;
use super::owner_model::{OwnerChoice, OwnerChoiceExpansion};

pub(super) fn shop_tiny_owner_choices(
    session: &RunControlSession,
    surface: &DecisionSurface,
) -> Result<Vec<OwnerChoice>, String> {
    let mut choices = executable_choices(surface);
    if choices.is_empty() {
        return Err("shop owner found no executable exact candidate".to_string());
    }

    let prior = {
        let legal = choices
            .iter()
            .map(|choice| RunPolicyCandidateV1 {
                candidate_id: &choice.candidate_id,
                label: &choice.label,
                action: &choice.action,
            })
            .collect::<Vec<_>>();
        exact_shop_policy_prior_v1(session, &legal)?
    };
    let ranks = prior
        .entries
        .iter()
        .enumerate()
        .map(|(rank, entry)| (entry.candidate_id.as_str(), rank))
        .collect::<BTreeMap<_, _>>();
    choices.sort_by_key(|choice| {
        ranks
            .get(choice.candidate_id.as_str())
            .copied()
            .unwrap_or(usize::MAX)
    });
    for choice in &mut choices {
        choice.expansion = OwnerChoiceExpansion::AutoAllowed;
    }
    Ok(choices)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_simulator::content::cards::CardId;
    use sts_simulator::content::relics::{RelicId, RelicState};
    use sts_simulator::eval::run_control::{
        build_decision_surface, DecisionCandidateKey, RunControlConfig, RunDecisionAction,
    };
    use sts_simulator::runtime::combat::CombatCard;
    use sts_simulator::state::core::{ClientInput, EngineState};
    use sts_simulator::state::shop::{ShopCard, ShopRelic, ShopState};

    #[test]
    fn production_shop_owner_uses_exact_waffle_successor() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.current_hp = 30;
        session.run_state.max_hp = 80;
        session.run_state.gold = 300;
        let mut shop = ShopState::new();
        shop.purge_cost = 75;
        shop.relics.push(ShopRelic {
            relic_id: RelicId::Waffle,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let choices = shop_tiny_owner_choices(&session, &surface).expect("exact shop owner");

        assert!(matches!(
            choices.first().and_then(|choice| choice.key.as_ref()),
            Some(DecisionCandidateKey::ShopBuyRelic {
                relic: RelicId::Waffle,
                ..
            })
        ));
        assert!(choices.iter().all(OwnerChoice::auto_expand_allowed));
    }

    #[test]
    fn production_shop_owner_preserves_gold_over_redundant_armaments_scope() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 100;
        session
            .run_state
            .master_deck
            .push(CombatCard::for_scenario(CardId::Armaments, 10_001));
        for uuid in 20_000..20_003 {
            session
                .run_state
                .master_deck
                .push(CombatCard::for_scenario(CardId::Impervious, uuid));
        }
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.push(ShopCard {
            card_id: CardId::Armaments,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);

        let surface = build_decision_surface(&session);
        let choices = shop_tiny_owner_choices(&session, &surface).expect("exact shop owner");

        assert!(matches!(
            choices.first().and_then(|choice| choice.key.as_ref()),
            Some(DecisionCandidateKey::ShopLeave)
        ));
        assert!(choices.iter().any(|choice| matches!(
            choice.key,
            Some(DecisionCandidateKey::ShopBuyCard {
                card: CardId::Armaments,
                ..
            })
        )));
    }

    #[test]
    fn membership_followup_is_rebuilt_from_the_discounted_shop() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 300;
        let mut shop = ShopState::new();
        shop.purge_available = false;
        shop.cards.push(ShopCard {
            card_id: CardId::Armaments,
            upgrades: 0,
            price: 80,
            can_buy: true,
            blocked_reason: None,
        });
        shop.relics.push(ShopRelic {
            relic_id: RelicId::MembershipCard,
            price: 150,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::BuyRelic(0)))
            .expect("Membership Card purchase");

        let surface = build_decision_surface(&session);
        let choices = shop_tiny_owner_choices(&session, &surface).expect("discounted shop owner");

        assert!(matches!(
            choices.first().and_then(|choice| choice.key.as_ref()),
            Some(DecisionCandidateKey::ShopBuyCard {
                card: CardId::Armaments,
                price: 40,
                ..
            })
        ));
        assert_eq!(session.run_state.gold, 150);
        assert!(session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == RelicId::MembershipCard));
    }

    #[test]
    fn every_exact_shop_action_remains_expandable() {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.gold = 300;
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::RunicPyramid));
        let mut shop = ShopState::new();
        shop.cards.push(ShopCard {
            card_id: CardId::WildStrike,
            upgrades: 0,
            price: 50,
            can_buy: true,
            blocked_reason: None,
        });
        session.engine_state = EngineState::Shop(shop);
        let surface = build_decision_surface(&session);

        let choices = shop_tiny_owner_choices(&session, &surface).expect("exact shop owner");

        assert_eq!(
            choices.len(),
            surface
                .view
                .candidates
                .iter()
                .filter(|candidate| candidate.action.executable_action_ref().is_some())
                .count()
        );
        assert!(choices.iter().all(OwnerChoice::auto_expand_allowed));
    }
}
