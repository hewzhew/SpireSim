//! Visible shop stock is broader than the executable purchase menu.
use sts_oracle_run_control::{
    ai::planner_core::{
        PlannerShopInventoryV1, PlannerShopPurchaseBlockReasonV1, PlannerShopPurchaseStatusV1,
    },
    content::identity::gameplay_display_label_v1 as label,
    eval::run_control::render_shop_purchase_block_reason_v1,
};

pub(crate) fn render(shop: &PlannerShopInventoryV1) -> Vec<String> {
    let slots_full = shop.potions.iter().any(|p| {
        matches!(
            p.purchase_status,
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot
            }
        )
    });
    let offer = |name: String, price: i32, status: &PlannerShopPurchaseStatusV1| {
        let suffix = match status {
            PlannerShopPurchaseStatusV1::Purchasable => String::new(),
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot,
            } => String::new(),
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NotEnoughGold { gold, price },
            } => format!(" (need {}g)", price - gold),
            PlannerShopPurchaseStatusV1::Blocked { reason } => {
                format!(" ({})", render_shop_purchase_block_reason_v1(reason))
            }
        };
        format!("{name} {price}g{suffix}")
    };
    let cards = shop
        .cards
        .iter()
        .map(|card| {
            offer(
                crate::manual::format_card(label(card.card), u64::from(card.upgrades), None),
                card.price,
                &card.purchase_status,
            )
        })
        .collect::<Vec<_>>();
    let relics = shop
        .relics
        .iter()
        .map(|relic| {
            offer(
                label(relic.relic).into(),
                relic.price,
                &relic.purchase_status,
            )
        })
        .collect::<Vec<_>>();
    let potions = shop
        .potions
        .iter()
        .map(|potion| {
            offer(
                label(potion.potion).into(),
                potion.price,
                &potion.purchase_status,
            )
        })
        .collect::<Vec<_>>();
    let mut lines = Vec::new();
    for (title, offers) in [
        ("Shop cards", cards),
        ("Shop relics", relics),
        (
            if slots_full {
                "Shop potions (slots full)"
            } else {
                "Shop potions"
            },
            potions,
        ),
    ] {
        if !offers.is_empty() {
            lines.push(format!("{title}: {}", offers.join(", ")));
        }
    }
    if let Some(purge) = &shop.purge {
        lines.push(offer(
            "Card removal".into(),
            purge.price,
            &purge.purchase_status,
        ));
    }
    lines
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DecisionStateV1;
    use sts_oracle_run_control::{
        ai::planner_core::PlannerShopPurchaseBlockReasonV1,
        content::{
            cards::CardId,
            potions::{Potion, PotionId},
            relics::{RelicId, RelicState},
        },
        eval::run_control::{RunControlConfig, RunControlSession, RunDecisionAction},
        state::{
            core::{ClientInput, EngineState},
            shop::{ShopCard, ShopPotion, ShopRelic, ShopState},
        },
    };
    use sts_parity_lockstep::{LockstepAdapterV1, SemanticActionSpaceV1, SemanticActionV1};
    use sts_parity_lockstep_rust::{natural_run_bootstrap_v1, RustLockstepAdapterV1};

    fn inspect(session: &RunControlSession) -> (DecisionStateV1, SemanticActionSpaceV1) {
        let bootstrap = natural_run_bootstrap_v1(&RunControlConfig::default()).unwrap();
        let mut adapter = RustLockstepAdapterV1::new(
            session.clone(),
            bootstrap,
            "constructed",
            "shop-observation-test",
        )
        .unwrap();
        let before = adapter.observe().unwrap();
        let state = DecisionStateV1::capture(adapter.session(), &before).unwrap();
        let actions = SemanticActionSpaceV1::from_snapshot(&before).unwrap();
        state.render(None, true).unwrap();
        state.render_with_action_menu(None, false).unwrap();
        assert_eq!(
            adapter.observe().unwrap(),
            before,
            "reading leaves state and RNG unchanged"
        );
        (state, actions)
    }

    fn stocked_shop() -> RunControlSession {
        let mut session = RunControlSession::new(RunControlConfig::default());
        session.run_state.event_state = None;
        session.run_state.map.current_x = 0;
        session.run_state.map.current_y = 0;
        session.run_state.gold = 100;
        for (i, slot) in session.run_state.potions.iter_mut().enumerate() {
            *slot = Some(Potion::new(PotionId::BlockPotion, i as u32));
        }
        session.engine_state = EngineState::Shop(ShopState {
            cards: vec![ShopCard {
                card_id: CardId::Armaments,
                upgrades: 1,
                price: 40,
                can_buy: true,
                blocked_reason: None,
            }],
            relics: vec![ShopRelic {
                relic_id: RelicId::BagOfPreparation,
                price: 157,
                can_buy: true,
                blocked_reason: None,
            }],
            potions: vec![ShopPotion {
                potion_id: PotionId::StrengthPotion,
                price: 50,
                can_buy: true,
                blocked_reason: None,
            }],
            ..ShopState::new()
        });
        session
    }

    fn has_purchase(actions: &SemanticActionSpaceV1, kind: &str) -> bool {
        actions.actions.iter().any(|a| {
            matches!(&a.action,
            SemanticActionV1::Select { choice, .. } if choice.kind == kind)
        })
    }

    #[test]
    fn decision_shop_reads_blocked_stock_without_granting_purchase_actions() {
        let mut session = stocked_shop();
        let (full, actions) = inspect(&session);
        let stock = full.shop.as_ref().unwrap();
        assert_eq!(stock.potions[0].potion, PotionId::StrengthPotion);
        assert_eq!(stock.potions[0].price, 50);
        assert_eq!(
            stock.potions[0].purchase_status,
            PlannerShopPurchaseStatusV1::Blocked {
                reason: PlannerShopPurchaseBlockReasonV1::NoEmptyPotionSlot,
            }
        );
        assert!(!has_purchase(&actions, "shop_potion"));
        assert!(!has_purchase(&actions, "shop_relic"));
        let display = full.render_with_action_menu(None, false).unwrap();
        assert!(display.contains("Shop potions (slots full): Strength Potion 50g"));
        assert!(display.contains("Bag of Preparation 157g (need 57g)"));
        assert!(display.contains("Armaments+ 40g"));
        assert!(full.render(None, false).unwrap().contains("Armaments+ 40g"));
        assert!(serde_json::to_value(&full).unwrap()["shop"]["potions"].is_array());

        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::DiscardPotion(0)))
            .unwrap();
        let (open, actions) = inspect(&session);
        assert!(has_purchase(&actions, "shop_potion"));
        assert_eq!(
            open.shop.as_ref().unwrap().potions[0].purchase_status,
            PlannerShopPurchaseStatusV1::Purchasable
        );
        assert!(!open
            .render_with_action_menu(Some(&full), false)
            .unwrap()
            .contains("slots full"));
        session
            .apply_decision_action(RunDecisionAction::Input(ClientInput::BuyPotion(0)))
            .unwrap();
        let (bought, actions) = inspect(&session);
        assert!(bought.shop.as_ref().unwrap().potions.is_empty());
        assert!(!has_purchase(&actions, "shop_potion"));
        assert_eq!(bought.run.gold, 50);

        session.engine_state = EngineState::MapNavigation;
        let (left, _) = inspect(&session);
        assert!(left.shop.is_none());
        assert!(!left
            .render(Some(&bought), false)
            .unwrap()
            .contains("Shop potions"));
    }

    #[test]
    fn decision_shop_purchase_restrictions_follow_current_resources_and_relics() {
        let mut session = stocked_shop();
        session.run_state.gold = 200;
        session.run_state.potions[0] = None;
        let (funded, actions) = inspect(&session);
        assert!(has_purchase(&actions, "shop_relic"));
        assert!(has_purchase(&actions, "shop_potion"));
        assert!(funded.shop.as_ref().unwrap().relics[0].affordable);
        session
            .run_state
            .relics
            .push(RelicState::new(RelicId::Sozu));
        let (sozu, actions) = inspect(&session);
        assert!(!has_purchase(&actions, "shop_potion"));
        assert!(sozu
            .render_with_action_menu(Some(&funded), false)
            .unwrap()
            .contains("blocked by Sozu"));
        session.run_state.gold = 0;
        let (poor, actions) = inspect(&session);
        assert!(!has_purchase(&actions, "shop_card"));
        assert!(!poor.shop.as_ref().unwrap().cards[0].affordable);
        assert!(poor
            .render_with_action_menu(Some(&sozu), false)
            .unwrap()
            .contains("Armaments+ 40g (need 40g)"));
    }
}
