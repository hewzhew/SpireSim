use crate::ai::noncombat_decision_v1::{
    validate_noncombat_decision_record_v1, DataRoleV1, DecisionSiteKindV1, PolicySelectionStatusV1,
};
use crate::content::potions::{Potion, PotionId};
use crate::content::relics::{RelicId, RelicState};
use crate::state::rewards::{RewardItem, RewardState};
use crate::state::run::RunState;

use super::{
    build_reward_decision_context_v1, plan_reward_decision_v1, RewardPolicyActionV1,
    RewardPolicyClassV1, RewardPolicyConfigV1,
};

#[test]
fn reward_policy_claims_visible_gold_with_policy_record() {
    let run = test_run();
    let reward = reward_state(vec![RewardItem::Gold { amount: 19 }]);

    let context = build_reward_decision_context_v1(&run, &reward);
    let decision = plan_reward_decision_v1(&context, &RewardPolicyConfigV1::default());

    assert_eq!(
        decision.action,
        RewardPolicyActionV1::Claim {
            index: 0,
            label: "19 gold".to_string(),
            confidence: 0.99,
            reason: "gold reward has no choice tradeoff after it is visible".to_string(),
        }
    );
    let record = decision.to_noncombat_decision_record_v1();
    validate_noncombat_decision_record_v1(&record).expect("reward record should validate");
    assert_eq!(record.site, DecisionSiteKindV1::Reward);
    assert_eq!(record.data_role, DataRoleV1::BehaviorPolicyNotTeacher);
    assert_eq!(record.selection.status, PolicySelectionStatusV1::Selected);
}

#[test]
fn reward_policy_claims_potion_only_when_empty_slot_is_available() {
    let mut full_run = test_run();
    full_run.potions = vec![
        Some(Potion::new(PotionId::FirePotion, 1)),
        Some(Potion::new(PotionId::DexterityPotion, 2)),
        Some(Potion::new(PotionId::StrengthPotion, 3)),
    ];
    let reward = reward_state(vec![RewardItem::Potion {
        potion_id: PotionId::EssenceOfSteel,
    }]);

    let blocked_context = build_reward_decision_context_v1(&full_run, &reward);
    assert_eq!(
        blocked_context.candidates[0].class,
        RewardPolicyClassV1::PotionNoEmptySlot
    );
    let blocked = plan_reward_decision_v1(&blocked_context, &RewardPolicyConfigV1::default());
    assert!(matches!(blocked.action, RewardPolicyActionV1::Stop { .. }));

    full_run.potions[1] = None;
    let claim_context = build_reward_decision_context_v1(&full_run, &reward);
    let claim = plan_reward_decision_v1(&claim_context, &RewardPolicyConfigV1::default());
    assert!(matches!(
        claim.action,
        RewardPolicyActionV1::Claim { index: 0, .. }
    ));
}

#[test]
fn reward_policy_claims_fruit_juice_before_other_potions_on_the_same_surface() {
    let mut run = test_run();
    run.potions = vec![Some(Potion::new(PotionId::FearPotion, 1)), None, None];
    let reward = reward_state(vec![
        RewardItem::Potion {
            potion_id: PotionId::FearPotion,
        },
        RewardItem::Potion {
            potion_id: PotionId::FirePotion,
        },
        RewardItem::Potion {
            potion_id: PotionId::FruitJuice,
        },
    ]);

    let context = build_reward_decision_context_v1(&run, &reward);
    let decision = plan_reward_decision_v1(&context, &RewardPolicyConfigV1::default());

    assert!(matches!(
        decision.action,
        RewardPolicyActionV1::Claim { index: 2, .. }
    ));
    assert_eq!(context.candidates[2].potion_id, Some(PotionId::FruitJuice));
}

#[test]
fn reward_policy_does_not_duplicate_potion_suffix_in_labels() {
    let run = test_run();
    let reward = reward_state(vec![RewardItem::Potion {
        potion_id: PotionId::ColorlessPotion,
    }]);

    let context = build_reward_decision_context_v1(&run, &reward);

    assert_eq!(context.candidates[0].label, "Colorless Potion");
}

#[test]
fn reward_policy_blocks_potion_when_sozu_is_owned() {
    let mut run = test_run();
    run.potions[0] = None;
    run.relics.push(RelicState::new(RelicId::Sozu));
    let reward = reward_state(vec![RewardItem::Potion {
        potion_id: PotionId::EssenceOfSteel,
    }]);

    let context = build_reward_decision_context_v1(&run, &reward);

    assert_eq!(
        context.candidates[0].class,
        RewardPolicyClassV1::PotionBlockedBySozu
    );
    let decision = plan_reward_decision_v1(&context, &RewardPolicyConfigV1::default());
    assert!(matches!(decision.action, RewardPolicyActionV1::Stop { .. }));
}

#[test]
fn reward_policy_leaves_relic_when_sapphire_key_competes() {
    let run = test_run();
    let reward = reward_state(vec![
        RewardItem::Relic {
            relic_id: RelicId::Anchor,
        },
        RewardItem::SapphireKey,
    ]);

    let context = build_reward_decision_context_v1(&run, &reward);
    let decision = plan_reward_decision_v1(&context, &RewardPolicyConfigV1::default());

    assert_eq!(
        context.candidates[0].class,
        RewardPolicyClassV1::RelicWithSapphireKeyConflict
    );
    assert!(matches!(decision.action, RewardPolicyActionV1::Stop { .. }));
}

fn reward_state(items: Vec<RewardItem>) -> RewardState {
    let mut reward = RewardState::new();
    reward.items = items;
    reward
}

fn test_run() -> RunState {
    RunState::new(1, 0, false, "Ironclad")
}
