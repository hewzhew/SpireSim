use crate::ai::planner_core::PlannerShopPurchaseBlockReasonV1;
use crate::state::run::RunState;
use crate::state::shop::{ShopPotion, ShopState};

pub(crate) fn shop_merchandise_purchase_block_reason_v1(
    run_state: &RunState,
    can_buy: bool,
    blocked_reason: Option<&str>,
    price: i32,
) -> Option<PlannerShopPurchaseBlockReasonV1> {
    if !can_buy {
        return Some(PlannerShopPurchaseBlockReasonV1::MerchantUnavailable {
            detail: blocked_reason.map(str::to_string),
        });
    }
    if run_state.gold < price {
        return Some(PlannerShopPurchaseBlockReasonV1::NotEnoughGold {
            gold: run_state.gold,
            price,
        });
    }
    None
}

pub(crate) fn shop_potion_purchase_block_reason_v1(
    run_state: &RunState,
    potion: &ShopPotion,
) -> Option<PlannerShopPurchaseBlockReasonV1> {
    if let Some(reason) = shop_merchandise_purchase_block_reason_v1(
        run_state,
        potion.can_buy,
        potion.blocked_reason.as_deref(),
        potion.price,
    ) {
        return Some(reason);
    }
    if run_state
        .relics
        .iter()
        .any(|relic| relic.id == crate::content::relics::RelicId::Sozu)
    {
        return Some(PlannerShopPurchaseBlockReasonV1::BlockedBySozu);
    }
    if run_state.find_empty_potion_slot().is_none() {
        return Some(PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot);
    }
    None
}

pub(crate) fn shop_purge_purchase_block_reason_v1(
    run_state: &RunState,
    shop: &ShopState,
) -> Option<PlannerShopPurchaseBlockReasonV1> {
    if !shop.purge_available {
        return Some(PlannerShopPurchaseBlockReasonV1::PurgeAlreadyUsed);
    }
    if run_state.gold < shop.purge_cost {
        return Some(PlannerShopPurchaseBlockReasonV1::NotEnoughGold {
            gold: run_state.gold,
            price: shop.purge_cost,
        });
    }
    let has_eligible_card = run_state.master_deck.iter().any(|card| {
        crate::state::core::master_deck_card_is_purgeable(card)
            && !crate::state::core::master_deck_card_is_bottled(card, &run_state.relics)
    });
    (!has_eligible_card).then_some(PlannerShopPurchaseBlockReasonV1::NoEligiblePurgeTarget)
}

pub fn render_shop_purchase_block_reason_v1(reason: &PlannerShopPurchaseBlockReasonV1) -> String {
    match reason {
        PlannerShopPurchaseBlockReasonV1::MerchantUnavailable { detail } => {
            detail.clone().unwrap_or_else(|| "cannot buy".to_string())
        }
        PlannerShopPurchaseBlockReasonV1::NotEnoughGold { .. } => "not enough gold".to_string(),
        PlannerShopPurchaseBlockReasonV1::BlockedBySozu => "blocked by Sozu".to_string(),
        PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot => "no empty potion slot".to_string(),
        PlannerShopPurchaseBlockReasonV1::PurgeAlreadyUsed => "already used".to_string(),
        PlannerShopPurchaseBlockReasonV1::NoEligiblePurgeTarget => "no eligible cards".to_string(),
    }
}
