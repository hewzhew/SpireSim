use crate::combat_manifest::{
    render_parity_lab_combat_v1, CombatActionKindV1, CombatActionV1, ParityLabCombatInputV1,
    PublicCombatCardV1, PublicCombatEnemyV1, PublicCombatIntentV1, PublicCombatPlayerV1,
    PublicCombatPowerV1, PublicCombatV1, PublicCombatZoneSummaryV1,
};
use crate::domain_manifest::{ManifestAuthorityV1, PublicPotionBeltV1, RenderedManifestV1};
use crate::observation::ActionObservationReportV6;
use crate::scene_manifest_adapter::{
    project_action_availability_v1, project_shared_scene_v1, validate_root_scope_v1,
};
use serde::Deserialize;
use serde_json::Value;
use std::collections::BTreeMap;
use sts_paired_root::{
    DecisionProfileV1, MonsterTargetV1, PairedActionV1, ProjectionDomainV1, StateRootArtifactV2,
    StateRootStoreV2,
};

const COMBAT_SCHEMA_V1: &str = "sts-paired-combat-root-v1";
const COMBAT_SCHEMA_V2: &str = "sts-paired-combat-root-v2";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCombatPowerV1 {
    id: String,
    amount: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCombatCardV1 {
    binding_id: String,
    card_id: String,
    upgrades: u8,
    cost_for_turn: i64,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCombatPlayerV2 {
    current_hp: i32,
    max_hp: i32,
    block: i32,
    energy: i32,
    powers: Vec<StoredCombatPowerV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCombatZonesV2 {
    draw_top_first: Vec<StoredCombatCardV1>,
    hand: Vec<StoredCombatCardV1>,
    discard_bottom_first: Vec<StoredCombatCardV1>,
    exhaust_bottom_first: Vec<StoredCombatCardV1>,
    limbo: Vec<StoredCombatCardV1>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(tag = "evidence", rename_all = "snake_case", deny_unknown_fields)]
enum StoredCombatIntentV2 {
    VisibleExact {
        visible_intent: String,
        #[serde(default)]
        damage_per_hit: Option<i32>,
        #[serde(default)]
        hits: Option<u8>,
    },
    Hidden {
        hidden_reason: StoredCombatHiddenReasonV2,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case")]
enum StoredCombatHiddenReasonV2 {
    RunicDome,
    IntentNotVisible,
}

impl StoredCombatHiddenReasonV2 {
    fn token(self) -> &'static str {
        match self {
            Self::RunicDome => "runic_dome",
            Self::IntentNotVisible => "intent_not_visible",
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCombatMonsterV2 {
    id: String,
    hp: i32,
    max_hp: i32,
    block: i32,
    dead_or_escaped: bool,
    next_move_id: i64,
    intent: String,
    move_history: Value,
    powers: Vec<StoredCombatPowerV1>,
    public_intent: StoredCombatIntentV2,
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
struct StoredCombatProjectionV2 {
    turn_index: u64,
    player: StoredCombatPlayerV2,
    zones: StoredCombatZonesV2,
    monsters: Vec<StoredCombatMonsterV2>,
}

pub(super) fn render_combat_observation_v1(
    store: &StateRootStoreV2,
    report: &ActionObservationReportV6,
    authority: ManifestAuthorityV1,
) -> Result<Option<RenderedManifestV1>, String> {
    if report.decision_context.profile != DecisionProfileV1::CombatPlayerDecision {
        return Err("Combat manifest adapter accepts CombatPlayerDecision only".to_owned());
    }
    let root = store.load_root(&report.root_id)?;
    validate_root_scope_v1(&root, report)?;
    match combat_projection_schema_v1(&root)? {
        COMBAT_SCHEMA_V1 => return Ok(None),
        COMBAT_SCHEMA_V2 => {}
        other => return Err(format!("unsupported Combat manifest projection {other:?}")),
    }
    let shared = project_shared_scene_v1(&root, report, authority)?;
    let stored: StoredCombatProjectionV2 = serde_json::from_value(
        store.load_exact_projection(&root.root_id, ProjectionDomainV1::Combat)?,
    )
    .map_err(|error| format!("Exact Combat v2 projection has an invalid schema: {error}"))?;
    let actions = project_actions_v1(report, &stored, &shared.potions)?;
    let combat = project_combat_v1(stored)?;
    render_parity_lab_combat_v1(&ParityLabCombatInputV1 {
        shared,
        combat,
        actions,
    })
    .map(Some)
}

fn combat_projection_schema_v1(root: &StateRootArtifactV2) -> Result<&str, String> {
    let component = root
        .root
        .preflight
        .components
        .iter()
        .find(|component| component.domain == ProjectionDomainV1::Combat)
        .ok_or_else(|| "Combat manifest root omitted its Combat projection".to_owned())?;
    Ok(component.projection_schema.as_str())
}

fn project_combat_v1(stored: StoredCombatProjectionV2) -> Result<PublicCombatV1, String> {
    Ok(PublicCombatV1 {
        turn_index: stored.turn_index,
        player: PublicCombatPlayerV1 {
            current_hp: stored.player.current_hp,
            max_hp: stored.player.max_hp,
            block: stored.player.block,
            energy: stored.player.energy,
            powers: project_powers_v1(stored.player.powers),
        },
        zones: PublicCombatZoneSummaryV1 {
            draw: stored.zones.draw_top_first.len(),
            discard: stored.zones.discard_bottom_first.len(),
            exhaust: stored.zones.exhaust_bottom_first.len(),
            limbo: stored.zones.limbo.len(),
        },
        hand: stored
            .zones
            .hand
            .into_iter()
            .map(|card| PublicCombatCardV1 {
                card_id: card.card_id,
                upgrades: card.upgrades,
                cost_for_turn: card.cost_for_turn,
            })
            .collect(),
        enemies: stored
            .monsters
            .into_iter()
            .map(|enemy| {
                let intent = match enemy.public_intent {
                    StoredCombatIntentV2::VisibleExact {
                        visible_intent,
                        damage_per_hit,
                        hits,
                    } => PublicCombatIntentV1::Visible {
                        kind: visible_intent,
                        damage_per_hit,
                        hits,
                    },
                    StoredCombatIntentV2::Hidden { hidden_reason } => {
                        PublicCombatIntentV1::Hidden {
                            reason: hidden_reason.token().to_owned(),
                        }
                    }
                };
                Ok(PublicCombatEnemyV1 {
                    enemy_id: enemy.id,
                    current_hp: enemy.hp,
                    max_hp: enemy.max_hp,
                    block: enemy.block,
                    dead_or_escaped: enemy.dead_or_escaped,
                    intent,
                    powers: project_powers_v1(enemy.powers),
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn project_powers_v1(powers: Vec<StoredCombatPowerV1>) -> Vec<PublicCombatPowerV1> {
    powers
        .into_iter()
        .map(|power| PublicCombatPowerV1 {
            id: power.id,
            amount: power.amount,
        })
        .collect()
}

fn project_actions_v1(
    report: &ActionObservationReportV6,
    combat: &StoredCombatProjectionV2,
    potions: &PublicPotionBeltV1,
) -> Result<Vec<CombatActionV1>, String> {
    if report.observed_actions.len() != report.authorizations.len() {
        return Err("Combat observation does not bind every action authorization".to_owned());
    }
    let mut hand_bindings = BTreeMap::new();
    for (hand_index, card) in combat.zones.hand.iter().enumerate() {
        if hand_bindings
            .insert(card.binding_id.as_str(), hand_index)
            .is_some()
        {
            return Err("Combat hand repeats one stable binding".to_owned());
        }
    }
    let combat_potions = match potions {
        PublicPotionBeltV1::Combat(potions) => potions,
        PublicPotionBeltV1::Persistent(_) => {
            return Err("Combat observation used a persistent potion view".to_owned())
        }
    };
    let potion_indices = combat_potions
        .iter()
        .enumerate()
        .map(|(index, potion)| (potion.slot, (index, potion)))
        .collect::<BTreeMap<_, _>>();

    report
        .observed_actions
        .iter()
        .map(|observed| {
            let kind = match &observed.handle.action {
                PairedActionV1::PlayCard { binding_id, target } => CombatActionKindV1::Play {
                    hand_index: *hand_bindings.get(binding_id.as_str()).ok_or_else(|| {
                        "Combat play action does not resolve to one hand card".to_owned()
                    })?,
                    target: resolve_target_v1(&combat.monsters, target.as_ref())?,
                },
                PairedActionV1::UsePotion {
                    slot,
                    potion_id,
                    potency,
                    target,
                } => {
                    let (potion_index, potion) =
                        potion_indices.get(&usize::from(*slot)).ok_or_else(|| {
                            "Combat potion action does not resolve to one occupied slot".to_owned()
                        })?;
                    if potion.id != *potion_id
                        || potion.target_required != target.is_some()
                        || *potency < 0
                    {
                        return Err(
                            "Combat potion action disagrees with its public potion facts"
                                .to_owned(),
                        );
                    }
                    CombatActionKindV1::UsePotion {
                        potion_index: *potion_index,
                        potency: *potency,
                        target: resolve_target_v1(&combat.monsters, target.as_ref())?,
                    }
                }
                PairedActionV1::EndTurn => CombatActionKindV1::EndTurn,
                _ => {
                    return Err("Combat observation exposed an action from another scene".to_owned())
                }
            };
            Ok(CombatActionV1 {
                action_id: observed.handle.handle_id.clone(),
                kind,
                availability: project_action_availability_v1(report, &observed.handle.handle_id)?,
            })
        })
        .collect()
}

fn resolve_target_v1(
    monsters: &[StoredCombatMonsterV2],
    target: Option<&MonsterTargetV1>,
) -> Result<Option<usize>, String> {
    let Some(target) = target else {
        return Ok(None);
    };
    let matches = monsters
        .iter()
        .enumerate()
        .filter(|(_, monster)| monster.id == target.enemy_id)
        .collect::<Vec<_>>();
    matches
        .get(target.same_enemy_ordinal)
        .map(|(index, _)| Some(*index))
        .ok_or_else(|| "Combat action target does not resolve to one enemy occurrence".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_manifest::ManifestAuthorityV1;
    use crate::observation::{ActionObservationReportV6, DecisionContextV1};
    use crate::test_support::paired_store_fixture;
    use serde_json::json;

    fn decode(value: Value) -> StoredCombatProjectionV2 {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn v2_projection_discards_private_intent_when_building_public_combat() {
        let stored = decode(json!({
            "turn_index": 0,
            "player": {"current_hp": 72, "max_hp": 80, "block": 0, "energy": 3, "powers": []},
            "zones": {
                "draw_top_first": [],
                "hand": [{"binding_id": "binding-0", "card_id": "Strike_R", "upgrades": 0, "cost_for_turn": 1}],
                "discard_bottom_first": [],
                "exhaust_bottom_first": [],
                "limbo": []
            },
            "monsters": [{
                "id": "JawWorm", "hp": 40, "max_hp": 40, "block": 0,
                "dead_or_escaped": false,
                "next_move_id": 99,
                "intent": "private_future_move",
                "move_history": [1, 2],
                "powers": [],
                "public_intent": {"evidence": "hidden", "hidden_reason": "runic_dome"}
            }]
        }));
        let public = project_combat_v1(stored).unwrap();
        assert_eq!(
            public.enemies[0].intent,
            PublicCombatIntentV1::Hidden {
                reason: "runic_dome".to_owned()
            }
        );
    }

    #[test]
    fn tagged_intent_rejects_nullable_legacy_shapes() {
        let error = serde_json::from_value::<StoredCombatIntentV2>(json!({
            "evidence": "hidden",
            "visible_intent": null,
            "damage_per_hit": null,
            "hits": null,
            "hidden_reason": "runic_dome"
        }))
        .unwrap_err();
        assert!(error.to_string().contains("unknown field"));
    }

    #[test]
    fn legacy_v1_root_remains_observable_without_a_manifest_lease() {
        let fixture = paired_store_fixture();
        let mut body = fixture.source.root.clone();
        body.decision_profile = DecisionProfileV1::CombatPlayerDecision;
        body.preflight.action_space_id = "legacy-combat-action-space".to_owned();
        body.preflight.components[0].domain = ProjectionDomainV1::Combat;
        body.preflight.components[0].projection_schema = COMBAT_SCHEMA_V1.to_owned();
        body.capabilities[0].required_domains = vec![ProjectionDomainV1::Combat];
        let root = fixture.store.create_root(body).unwrap();
        let report = ActionObservationReportV6 {
            schema: "sts-paired-root-action-observation-v6".to_owned(),
            root_id: root.root_id.clone(),
            action_space_id: root.root.preflight.action_space_id.clone(),
            decision_context: DecisionContextV1 {
                profile: DecisionProfileV1::CombatPlayerDecision,
                run: json!({}),
                master_deck: json!([]),
                relics: json!([]),
                potions: json!([]),
                map: None,
                room: None,
            },
            observed_actions: Vec::new(),
            authorizations: Vec::new(),
        };
        assert!(render_combat_observation_v1(
            &fixture.store,
            &report,
            ManifestAuthorityV1::ObserveOnly,
        )
        .unwrap()
        .is_none());
    }
}
