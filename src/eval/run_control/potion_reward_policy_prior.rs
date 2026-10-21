use serde::{Deserialize, Serialize};

use crate::ai::strategy::potion_portfolio::{
    assess_potion_retention_v1, PotionRetentionEvidenceV1,
};
use crate::content::potions::PotionId;
use crate::state::core::EngineState;
use crate::state::rewards::RewardItem;

use super::RunControlSession;

pub const POTION_REWARD_POLICY_AUDIT_SCHEMA_NAME: &str = "PotionRewardPolicyAudit";
pub const POTION_REWARD_POLICY_AUDIT_SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRewardHeldEvidenceV1 {
    pub slot: usize,
    pub discardable: bool,
    pub retention: PotionRetentionEvidenceV1,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PotionRewardPolicyDecisionV1 {
    ReplaceUniqueWeakest { slot: usize, outgoing: PotionId },
    PreservePortfolio,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct PotionRewardPolicyAuditV1 {
    pub schema_name: &'static str,
    pub schema_version: u32,
    pub incoming: PotionRetentionEvidenceV1,
    pub held: Vec<PotionRewardHeldEvidenceV1>,
    pub decision: PotionRewardPolicyDecisionV1,
}

impl PotionRewardPolicyAuditV1 {
    pub fn recommended_discard_slot(&self) -> Option<usize> {
        match self.decision {
            PotionRewardPolicyDecisionV1::ReplaceUniqueWeakest { slot, .. } => Some(slot),
            PotionRewardPolicyDecisionV1::PreservePortfolio => None,
        }
    }
}

/// Compares a single visible incoming potion against a full held portfolio.
///
/// The policy is deliberately partial: it recommends a replacement only when
/// the incoming retention band is strictly stronger and there is exactly one
/// discardable weakest slot. Equal-band and multi-way choices remain unknown
/// instead of being resolved by a hidden total score.
pub fn potion_reward_policy_audit_v1(
    session: &RunControlSession,
) -> Option<PotionRewardPolicyAuditV1> {
    if session.run_state.find_empty_potion_slot().is_some()
        || session
            .run_state
            .relics
            .iter()
            .any(|relic| relic.id == crate::content::relics::RelicId::Sozu)
    {
        return None;
    }
    let reward = match &session.engine_state {
        EngineState::RewardScreen(reward) => reward,
        EngineState::RewardOverlay { reward_state, .. } => reward_state,
        _ => return None,
    };
    let incoming = reward
        .items
        .iter()
        .filter_map(|item| match item {
            RewardItem::Potion { potion_id } => Some(*potion_id),
            _ => None,
        })
        .collect::<Vec<_>>();
    let [incoming] = incoming.as_slice() else {
        return None;
    };
    let incoming = assess_potion_retention_v1(&session.run_state, *incoming);
    let held = session
        .run_state
        .potions
        .iter()
        .enumerate()
        .filter_map(|(slot, potion)| {
            let potion = potion.as_ref()?;
            Some(PotionRewardHeldEvidenceV1 {
                slot,
                discardable: potion.can_discard,
                retention: assess_potion_retention_v1(&session.run_state, potion.id),
            })
        })
        .collect::<Vec<_>>();
    let weakest_band = held
        .iter()
        .filter(|held| held.discardable)
        .map(|held| held.retention.band)
        .min()?;
    let weakest = held
        .iter()
        .filter(|held| held.discardable && held.retention.band == weakest_band)
        .collect::<Vec<_>>();
    let decision = match weakest.as_slice() {
        [outgoing] if incoming.band > outgoing.retention.band => {
            PotionRewardPolicyDecisionV1::ReplaceUniqueWeakest {
                slot: outgoing.slot,
                outgoing: outgoing.retention.potion,
            }
        }
        _ => PotionRewardPolicyDecisionV1::PreservePortfolio,
    };
    Some(PotionRewardPolicyAuditV1 {
        schema_name: POTION_REWARD_POLICY_AUDIT_SCHEMA_NAME,
        schema_version: POTION_REWARD_POLICY_AUDIT_SCHEMA_VERSION,
        incoming,
        held,
        decision,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;
    use crate::content::potions::Potion;
    use crate::runtime::combat::CombatCard;
    use crate::state::rewards::RewardState;

    #[test]
    fn grounded_energy_replaces_unique_flexible_discovery_but_keeps_grounded_tools() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.master_deck = [
            CardId::DarkEmbrace,
            CardId::Impervious,
            CardId::FiendFire,
            CardId::Clothesline,
            CardId::Whirlwind,
        ]
        .into_iter()
        .enumerate()
        .map(|(uuid, card)| CombatCard::for_scenario(card, uuid as u32))
        .collect();
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::ColorlessPotion, 10)),
            Some(Potion::new(PotionId::LiquidMemories, 20)),
            Some(Potion::new(PotionId::Elixir, 30)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::EnergyPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let audit = potion_reward_policy_audit_v1(&session).expect("potion audit");

        assert_eq!(audit.recommended_discard_slot(), Some(0));
        assert_eq!(
            audit.decision,
            PotionRewardPolicyDecisionV1::ReplaceUniqueWeakest {
                slot: 0,
                outgoing: PotionId::ColorlessPotion,
            }
        );
    }

    #[test]
    fn equal_band_portfolio_remains_unknown_instead_of_using_slot_order() {
        let mut session = RunControlSession::new(Default::default());
        session.run_state.potions = vec![
            Some(Potion::new(PotionId::ColorlessPotion, 10)),
            Some(Potion::new(PotionId::AttackPotion, 20)),
            Some(Potion::new(PotionId::SkillPotion, 30)),
        ];
        let mut reward = RewardState::new();
        reward.items = vec![RewardItem::Potion {
            potion_id: PotionId::PowerPotion,
        }];
        session.engine_state = EngineState::RewardScreen(reward);

        let audit = potion_reward_policy_audit_v1(&session).expect("potion audit");

        assert_eq!(audit.recommended_discard_slot(), None);
        assert_eq!(
            audit.decision,
            PotionRewardPolicyDecisionV1::PreservePortfolio
        );
    }
}
