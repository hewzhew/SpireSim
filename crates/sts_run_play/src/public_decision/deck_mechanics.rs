//! Instance readings from public deck ordinals; no runtime identities or actions.
use super::*;
use sts_oracle_learning::ai::card_semantics_v1::card_owned_reading_v1;
use sts_oracle_learning::runtime::combat::CardBlueprint;

pub(super) fn project(decision: &TaskPublicDecisionV1) -> Result<Value, String> {
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Err("deck reading requires a strategic observation".into());
    };
    let cards = observation
        .cards
        .iter()
        .map(|card| {
            let blueprint = CardBlueprint {
                upgrades: card.upgrades,
                misc_value: card.misc_value,
                base_damage_override: card.base_damage_override,
                base_block_override: card.base_block_override,
                cost_modifier: card.cost_modifier,
                ..CardBlueprint::fresh(card.card)
            };
            let input = serde_json::to_value(card).map_err(|error| error.to_string())?;
            let bytes = serde_json::to_vec(&input).map_err(|error| error.to_string())?;
            Ok(json!({"ordinal": card.ordinal,
            "card_sha256": format!("{:x}", Sha256::digest(bytes)),
            "facts": card_owned_reading_v1(&blueprint)}))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(json!({
        "schema": "sts-owned-card-reading-v1",
        "public_snapshot_id": decision.information.snapshot_id,
        "basis": "public_instance_and_shared_card_mechanics",
        "coverage": "partial",
        "cards": cards,
    }))
}
