use crate::domain_manifest::{
    validate_shared_scene_v1, ActionAvailabilityV1, ExactParityV1, ManifestAuthorityV1,
    ParityStateV1, PublicCombatPotionV1, PublicMasterDeckCardV1, PublicPersistentPotionSlotV1,
    PublicPotionBeltV1, PublicRelicV1, PublicRunV1, SharedExactSceneV1,
};
use crate::observation::ActionObservationReportV6;
use sts_paired_root::{
    CapabilityStatusV1, ComparisonStatusV1, DecisionProfileV1, ProjectionDomainV1,
    StateRootArtifactV2,
};

pub(super) const RUN_SCALARS_SCHEMA_V1: &str = "sts-paired-run-scalars-v1";
pub(super) const MASTER_DECK_SCHEMA_V1: &str = "sts-paired-master-deck-order-v1";
pub(super) const RELICS_SCHEMA_V1: &str = "sts-paired-relic-order-counter-v1";
pub(super) const COMBAT_POTIONS_SCHEMA_V1: &str = "sts-paired-potion-slots-v1";
pub(super) const PERSISTENT_POTIONS_SCHEMA_V1: &str = "sts-paired-persistent-potion-slots-v1";

pub(super) fn project_shared_scene_v1(
    root: &StateRootArtifactV2,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<SharedExactSceneV1, String> {
    validate_root_scope_v1(root, report)?;
    require_projection_schema_v1(root, ProjectionDomainV1::RunScalars, RUN_SCALARS_SCHEMA_V1)?;
    require_projection_schema_v1(root, ProjectionDomainV1::MasterDeck, MASTER_DECK_SCHEMA_V1)?;
    require_projection_schema_v1(root, ProjectionDomainV1::Relics, RELICS_SCHEMA_V1)?;

    let run: PublicRunV1 = serde_json::from_value(report.decision_context.run.clone())
        .map_err(|error| format!("Exact run projection has an invalid schema: {error}"))?;
    let master_deck: Vec<PublicMasterDeckCardV1> =
        serde_json::from_value(report.decision_context.master_deck.clone()).map_err(|error| {
            format!("Exact master-deck projection has an invalid schema: {error}")
        })?;
    let relics: Vec<PublicRelicV1> = serde_json::from_value(report.decision_context.relics.clone())
        .map_err(|error| format!("Exact relic projection has an invalid schema: {error}"))?;
    let potions = match report.decision_context.profile {
        DecisionProfileV1::CombatPlayerDecision => {
            require_projection_schema_v1(
                root,
                ProjectionDomainV1::Potions,
                COMBAT_POTIONS_SCHEMA_V1,
            )?;
            PublicPotionBeltV1::Combat(
                serde_json::from_value::<Vec<PublicCombatPotionV1>>(
                    report.decision_context.potions.clone(),
                )
                .map_err(|error| {
                    format!("Exact combat-potion projection has an invalid schema: {error}")
                })?,
            )
        }
        DecisionProfileV1::CombatReward
        | DecisionProfileV1::CardReward
        | DecisionProfileV1::Event
        | DecisionProfileV1::MapNavigation
        | DecisionProfileV1::Neow => {
            require_projection_schema_v1(
                root,
                ProjectionDomainV1::Potions,
                PERSISTENT_POTIONS_SCHEMA_V1,
            )?;
            PublicPotionBeltV1::Persistent(
                serde_json::from_value::<Vec<PublicPersistentPotionSlotV1>>(
                    report.decision_context.potions.clone(),
                )
                .map_err(|error| {
                    format!("Exact persistent-potion projection has an invalid schema: {error}")
                })?,
            )
        }
    };
    let shared = SharedExactSceneV1 {
        world: root.root.world_provenance,
        authority,
        parity: ParityStateV1::Exact(ExactParityV1 {
            exact_domains: root
                .root
                .preflight
                .components
                .iter()
                .filter(|component| component.status == ComparisonStatusV1::Exact)
                .count(),
            total_domains: root.root.preflight.components.len(),
            coverage_gap_ids: root
                .root
                .preflight
                .uncovered
                .iter()
                .map(|gap| gap.gap_id.clone())
                .collect(),
        }),
        run,
        master_deck,
        relics,
        potions,
    };
    validate_shared_scene_v1(&shared)?;
    Ok(shared)
}

pub(super) fn validate_root_scope_v1(
    root: &StateRootArtifactV2,
    report: &ActionObservationReportV6,
) -> Result<(), String> {
    if root.root_id != report.root_id
        || root.root.preflight.action_space_id != report.action_space_id
        || root.root.decision_profile != report.decision_context.profile
    {
        return Err("manifest observation is not bound to its PairedRoot scope".to_owned());
    }
    Ok(())
}

pub(super) fn require_projection_schema_v1(
    root: &StateRootArtifactV2,
    domain: ProjectionDomainV1,
    expected: &str,
) -> Result<(), String> {
    let component = root
        .root
        .preflight
        .components
        .iter()
        .find(|component| component.domain == domain)
        .ok_or_else(|| format!("manifest root omitted required {domain:?} projection"))?;
    if component.status != ComparisonStatusV1::Exact || component.projection_schema != expected {
        return Err(format!(
            "manifest root has no Exact {domain:?} projection in schema {expected}"
        ));
    }
    Ok(())
}

pub(super) fn project_action_availability_v1(
    report: &ActionObservationReportV6,
    action_id: &str,
) -> Result<ActionAvailabilityV1, String> {
    let matches = report
        .authorizations
        .iter()
        .filter(|authorization| authorization.handle_id == action_id)
        .collect::<Vec<_>>();
    let [authorization] = matches.as_slice() else {
        return Err(format!(
            "manifest action {action_id:?} has {} authorizations instead of one",
            matches.len()
        ));
    };
    Ok(match &authorization.capability.status {
        CapabilityStatusV1::Verified { .. } => ActionAvailabilityV1::Verified,
        CapabilityStatusV1::Unknown { gap_id } => ActionAvailabilityV1::Unknown {
            gap_id: gap_id.clone(),
        },
        CapabilityStatusV1::Unsupported { .. } => ActionAvailabilityV1::Unsupported {
            reason: "capability_unsupported".to_owned(),
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::observation::{ActionObservationReportV6, DecisionContextV1};
    use crate::test_support::paired_store_fixture;
    use serde_json::{json, Value};
    use sts_paired_root::{ComparisonStatusV1, ProjectionComparisonV1};

    fn project(profile: DecisionProfileV1, potions: Value) -> Result<SharedExactSceneV1, String> {
        let fixture = paired_store_fixture();
        let run = json!({
            "seed": "18446744073709551615",
            "ascension": 20,
            "character": "IRONCLAD",
            "act": 1,
            "floor": 3,
            "current_hp": 68,
            "max_hp": 75,
            "gold": 99,
            "final_act_available": true,
            "keys": [false, false, false]
        });
        let deck = json!([{"ordinal": 0, "card_id": "Perfected Strike", "upgrades": 0}]);
        let relics = json!([{"ordinal": 0, "id": "burningblood", "counter": -1}]);
        let values = [
            (
                ProjectionDomainV1::RunScalars,
                RUN_SCALARS_SCHEMA_V1,
                run.clone(),
            ),
            (
                ProjectionDomainV1::MasterDeck,
                MASTER_DECK_SCHEMA_V1,
                deck.clone(),
            ),
            (ProjectionDomainV1::Relics, RELICS_SCHEMA_V1, relics.clone()),
            (
                ProjectionDomainV1::Potions,
                if profile == DecisionProfileV1::CombatPlayerDecision {
                    COMBAT_POTIONS_SCHEMA_V1
                } else {
                    PERSISTENT_POTIONS_SCHEMA_V1
                },
                potions.clone(),
            ),
        ];
        let mut root_body = fixture.source.root.clone();
        root_body.decision_profile = profile;
        root_body.preflight.action_space_id = "action-space:shared-scene-fixture".to_owned();
        root_body.preflight.components = values
            .iter()
            .map(|(domain, schema, value)| {
                let blob = fixture
                    .store
                    .put_bytes("application/json", &serde_json::to_vec(value).unwrap())
                    .unwrap();
                ProjectionComparisonV1 {
                    domain: *domain,
                    projection_schema: (*schema).to_owned(),
                    java: blob.clone(),
                    rust: blob,
                    status: ComparisonStatusV1::Exact,
                    first_difference: None,
                }
            })
            .collect();
        let root = fixture.store.create_root(root_body).unwrap();
        let report = ActionObservationReportV6 {
            schema: "sts-paired-root-action-observation-v6".to_owned(),
            root_id: root.root_id.clone(),
            action_space_id: root.root.preflight.action_space_id.clone(),
            decision_context: DecisionContextV1 {
                profile,
                run,
                master_deck: deck,
                relics,
                potions,
                map: None,
                room: None,
            },
            observed_actions: Vec::new(),
            authorizations: Vec::new(),
        };
        project_shared_scene_v1(&root, &report, ManifestAuthorityV1::ObserveOnly)
    }

    #[test]
    fn potion_projection_is_typed_by_decision_boundary() {
        let persistent = project(
            DecisionProfileV1::CardReward,
            json!([
                {"slot": 0, "id": null},
                {"slot": 1, "id": "weakenpotion"}
            ]),
        )
        .unwrap();
        assert!(matches!(
            persistent.potions,
            PublicPotionBeltV1::Persistent(ref slots)
                if slots.len() == 2 && slots[0].id.is_none()
        ));

        let combat = project(
            DecisionProfileV1::CombatPlayerDecision,
            json!([{
                "slot": 1,
                "id": "weakenpotion",
                "can_use": true,
                "can_discard": true,
                "target_required": true
            }]),
        )
        .unwrap();
        assert!(matches!(
            combat.potions,
            PublicPotionBeltV1::Combat(ref potions)
                if potions.len() == 1 && potions[0].target_required
        ));

        let error = project(
            DecisionProfileV1::CombatPlayerDecision,
            json!([{"slot": 0, "id": null}]),
        )
        .unwrap_err();
        assert!(error.contains("combat-potion"));
    }
}
