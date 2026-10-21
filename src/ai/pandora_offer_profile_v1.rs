use serde::Serialize;

use crate::ai::card_semantics_v1::CardRewardSemanticRoleV1;
use crate::state::run::RunState;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PandoraOfferHorizonV1 {
    AfterAct1,
    AfterAct2,
    Other,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PandoraNonStarterSupportV1 {
    Frontload,
    Block,
    Access,
    Scaling,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct PandoraOfferProfileV1 {
    pub starter_strikes: usize,
    pub starter_defends: usize,
    pub transform_targets: usize,
    pub deck_size: usize,
    pub transform_share_percent: u8,
    pub nonstarter_support: Vec<PandoraNonStarterSupportV1>,
    pub horizon: PandoraOfferHorizonV1,
    pub high_variance: bool,
}

pub fn pandora_offer_profile_v1(run_state: &RunState) -> PandoraOfferProfileV1 {
    use crate::ai::card_semantics_v1::card_reward_semantic_profile_v1;
    use crate::content::cards::{is_starter_basic, is_starter_defend, is_starter_strike};
    use crate::state::rewards::RewardCard;

    let starter_strikes = run_state
        .master_deck
        .iter()
        .filter(|card| is_starter_strike(card.id))
        .count();
    let starter_defends = run_state
        .master_deck
        .iter()
        .filter(|card| is_starter_defend(card.id))
        .count();
    let transform_targets = run_state
        .master_deck
        .iter()
        .filter(|card| is_starter_basic(card.id))
        .count();
    let deck_size = run_state.master_deck.len();
    let transform_share_percent = if deck_size == 0 {
        0
    } else {
        ((transform_targets.saturating_mul(100) / deck_size).min(100)) as u8
    };
    let mut nonstarter_support = Vec::new();
    for card in run_state
        .master_deck
        .iter()
        .filter(|card| !is_starter_basic(card.id))
    {
        let roles = card_reward_semantic_profile_v1(&RewardCard::new(card.id, card.upgrades)).roles;
        push_support_for_roles(&mut nonstarter_support, &roles);
    }
    nonstarter_support.sort();
    nonstarter_support.dedup();

    PandoraOfferProfileV1 {
        starter_strikes,
        starter_defends,
        transform_targets,
        deck_size,
        transform_share_percent,
        nonstarter_support,
        horizon: match run_state.act_num {
            1 => PandoraOfferHorizonV1::AfterAct1,
            2 => PandoraOfferHorizonV1::AfterAct2,
            _ => PandoraOfferHorizonV1::Other,
        },
        high_variance: true,
    }
}

fn push_support_for_roles(
    support: &mut Vec<PandoraNonStarterSupportV1>,
    roles: &[CardRewardSemanticRoleV1],
) {
    let mappings = [
        (
            PandoraNonStarterSupportV1::Frontload,
            &[CardRewardSemanticRoleV1::FrontloadDamage][..],
        ),
        (
            PandoraNonStarterSupportV1::Block,
            &[
                CardRewardSemanticRoleV1::Block,
                CardRewardSemanticRoleV1::Weak,
                CardRewardSemanticRoleV1::EnemyStrengthDown,
            ][..],
        ),
        (
            PandoraNonStarterSupportV1::Access,
            &[
                CardRewardSemanticRoleV1::CardDraw,
                CardRewardSemanticRoleV1::CycleAccess,
                CardRewardSemanticRoleV1::DiscardPileTopdeckAccess,
                CardRewardSemanticRoleV1::HandTopdeckSelection,
            ][..],
        ),
        (
            PandoraNonStarterSupportV1::Scaling,
            &[
                CardRewardSemanticRoleV1::ScalingSource,
                CardRewardSemanticRoleV1::StrengthPayoff,
                CardRewardSemanticRoleV1::BlockPayoff,
            ][..],
        ),
    ];
    for (item, accepted_roles) in mappings {
        if roles.iter().any(|role| accepted_roles.contains(role)) && !support.contains(&item) {
            support.push(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{pandora_offer_profile_v1, PandoraOfferHorizonV1};
    use crate::content::cards::CardId;
    use crate::runtime::combat::CombatCard;
    use crate::state::run::RunState;

    fn card(id: CardId, uuid: u32) -> CombatCard {
        CombatCard::for_scenario(id, uuid)
    }

    #[test]
    fn eight_starters_expose_more_transform_opportunity_than_two() {
        let mut many = RunState::new(1, 0, false, "Ironclad");
        many.act_num = 1;
        many.master_deck = (0..4)
            .map(|index| card(CardId::Strike, index + 1))
            .chain((0..4).map(|index| card(CardId::Defend, index + 10)))
            .chain([card(CardId::Bash, 30)])
            .collect();
        let mut few = many.clone();
        few.master_deck = vec![
            card(CardId::Strike, 1),
            card(CardId::Defend, 2),
            card(CardId::Bash, 3),
        ];

        let many_profile = pandora_offer_profile_v1(&many);
        let few_profile = pandora_offer_profile_v1(&few);

        assert_eq!(many_profile.transform_targets, 8);
        assert_eq!(many_profile.starter_strikes, 4);
        assert_eq!(many_profile.starter_defends, 4);
        assert!(many_profile.transform_share_percent > few_profile.transform_share_percent);
        assert_eq!(many_profile.horizon, PandoraOfferHorizonV1::AfterAct1);
        assert!(many_profile.high_variance);
    }
}
