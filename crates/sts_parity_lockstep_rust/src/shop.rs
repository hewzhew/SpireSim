use crate::{action::ResolvedRustActionV1, bootstrap::BoundCardSourceV1};
use std::collections::BTreeMap;
use sts_java_parity_protocol::canonical_game_id;
use sts_oracle_run_control::{
    eval::run_control::{build_decision_surface, RunControlSession},
    runtime::combat::CardLineageId,
    state::core::{ClientInput, EngineState},
};
use sts_parity_lockstep::{
    natural_deck_card_choice_v1, DecisionSurfaceV1, NaturalShopCardV1, NaturalShopItemV1,
    NaturalShopStockV1, SemanticActionV1, SemanticScalarV1,
};

pub(crate) fn state(
    session: &RunControlSession,
) -> Result<&sts_oracle_run_control::state::shop::ShopState, String> {
    match &session.engine_state {
        EngineState::Shop(shop) => Ok(shop),
        EngineState::RewardOverlay { return_state, .. } => {
            let EngineState::Shop(shop) = return_state.as_ref() else {
                return Err("Rust reward overlay requires a declared shop return state".to_owned());
            };
            if shop.pending_reward_overlay.is_some() {
                return Err("Rust open shop overlay also carries closed pending rewards".to_owned());
            }
            Ok(shop)
        }
        _ => Err("Rust shop inventory requires a shop or its reward overlay".to_owned()),
    }
}

pub(crate) fn stock(session: &RunControlSession) -> Result<NaturalShopStockV1, String> {
    let shop = state(session)?;
    let stock = NaturalShopStockV1 {
        cards: shop
            .cards
            .iter()
            .map(|item| NaturalShopCardV1 {
                card_id: canonical_game_id(item.card_id.stable_id()),
                upgrades: item.upgrades,
                price: item.price,
            })
            .collect(),
        relics: shop
            .relics
            .iter()
            .map(|item| NaturalShopItemV1 {
                id: canonical_game_id(item.relic_id.stable_id()),
                price: item.price,
            })
            .collect(),
        potions: shop
            .potions
            .iter()
            .map(|item| NaturalShopItemV1 {
                id: canonical_game_id(item.potion_id.stable_id()),
                price: item.price,
            })
            .collect(),
        purge_cost: shop.purge_cost,
        purge_available: shop.purge_available,
    };
    stock.validate()?;
    Ok(stock)
}

pub(crate) fn actions(
    session: &RunControlSession,
    bindings: &BTreeMap<CardLineageId, BoundCardSourceV1>,
) -> Result<Vec<ResolvedRustActionV1>, String> {
    let stock = stock(session)?;
    let cards = stock.card_choices();
    let relics = stock.relic_choices();
    let potions = stock.potion_choices();
    let mut actions = Vec::new();
    for candidate in build_decision_surface(session).view.candidates {
        let Some(input) = candidate.action.executable_input() else {
            continue;
        };
        if let Some(semantic) =
            crate::action::run_potion_semantic(session, candidate.key.as_ref(), &input)?
        {
            actions.push(ResolvedRustActionV1::candidate(
                semantic,
                candidate.id,
                input,
            ));
            continue;
        }
        if input == ClientInput::OpenRewardOverlay {
            actions.push(ResolvedRustActionV1::candidate(
                SemanticActionV1::Proceed {
                    surface: DecisionSurfaceV1::CombatReward,
                },
                candidate.id,
                input,
            ));
            continue;
        }
        let choice = match &input {
            ClientInput::BuyCard(index) => Some(
                cards
                    .get(*index)
                    .ok_or("Rust shop card index exceeds inventory")?
                    .clone(),
            ),
            ClientInput::BuyRelic(index) => Some(
                relics
                    .get(*index)
                    .ok_or("Rust shop relic index exceeds inventory")?
                    .clone(),
            ),
            ClientInput::BuyPotion(index) => Some(
                potions
                    .get(*index)
                    .ok_or("Rust shop potion index exceeds inventory")?
                    .clone(),
            ),
            ClientInput::PurgeCard(index) => {
                let card = session
                    .run_state
                    .master_deck
                    .get(*index)
                    .ok_or("Rust shop purge index exceeds deck")?;
                let bound = bindings
                    .get(&card.lineage)
                    .ok_or("Rust shop purge card has no source")?;
                let mut choice = natural_deck_card_choice_v1(
                    &bound.binding,
                    &canonical_game_id(card.id.stable_id()),
                    card.upgrades,
                )?;
                choice.attributes.insert(
                    "price".to_owned(),
                    SemanticScalarV1::Integer(i64::from(stock.purge_cost)),
                );
                Some(choice)
            }
            ClientInput::Proceed | ClientInput::Cancel => None,
            _ => {
                return Err(format!(
                    "Rust shop has an unsupported input {}",
                    input.payload_key_v1()
                ))
            }
        };
        let semantic = choice
            .map(|choice| SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Shop,
                choice,
            })
            .unwrap_or(SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::Shop,
            });
        actions.push(ResolvedRustActionV1::candidate(
            semantic,
            candidate.id,
            input,
        ));
    }
    Ok(actions)
}
