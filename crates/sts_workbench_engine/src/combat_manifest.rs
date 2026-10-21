use crate::domain_manifest::{
    append_action_availability_v1, manifest_scalar_v1, render_manifest_prelude_v1,
    require_internal_id, require_token, ActionAvailabilityV1, ActionLeaseEntryV1,
    ManifestPreludeV1, PublicPotionBeltV1, RenderedManifestV1, SharedExactSceneV1,
};
use std::collections::BTreeSet;
use std::fmt::Write;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCombatPowerV1 {
    pub id: String,
    pub amount: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCombatCardV1 {
    pub card_id: String,
    pub upgrades: u8,
    pub cost_for_turn: i64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCombatPlayerV1 {
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub energy: i32,
    pub powers: Vec<PublicCombatPowerV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCombatZoneSummaryV1 {
    pub draw: usize,
    pub discard: usize,
    pub exhaust: usize,
    pub limbo: usize,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) enum PublicCombatIntentV1 {
    Visible {
        kind: String,
        damage_per_hit: Option<i32>,
        hits: Option<u8>,
    },
    Hidden {
        reason: String,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCombatEnemyV1 {
    pub enemy_id: String,
    pub current_hp: i32,
    pub max_hp: i32,
    pub block: i32,
    pub dead_or_escaped: bool,
    pub intent: PublicCombatIntentV1,
    pub powers: Vec<PublicCombatPowerV1>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct PublicCombatV1 {
    pub turn_index: u64,
    pub player: PublicCombatPlayerV1,
    pub zones: PublicCombatZoneSummaryV1,
    pub hand: Vec<PublicCombatCardV1>,
    pub enemies: Vec<PublicCombatEnemyV1>,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) enum CombatActionKindV1 {
    Play {
        hand_index: usize,
        target: Option<usize>,
    },
    UsePotion {
        potion_index: usize,
        potency: i32,
        target: Option<usize>,
    },
    EndTurn,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CombatActionV1 {
    pub action_id: String,
    pub kind: CombatActionKindV1,
    pub availability: ActionAvailabilityV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ParityLabCombatInputV1 {
    pub shared: SharedExactSceneV1,
    pub combat: PublicCombatV1,
    pub actions: Vec<CombatActionV1>,
}

pub(super) fn render_parity_lab_combat_v1(
    input: &ParityLabCombatInputV1,
) -> Result<RenderedManifestV1, String> {
    if !matches!(input.shared.potions, PublicPotionBeltV1::Combat(_)) {
        return Err("Combat manifest requires a combat potion view".to_owned());
    }
    validate_combat_v1(&input.combat)?;
    let mut text = match render_manifest_prelude_v1(&input.shared, "combat", &[])? {
        ManifestPreludeV1::Exact(text) => text,
        ManifestPreludeV1::Blocked(manifest) => return Ok(manifest),
    };

    writeln!(text, "\n[combat turn={}]", input.combat.turn_index).unwrap();
    let player = &input.combat.player;
    writeln!(
        text,
        "PLAYER hp={}/{} block={} energy={}",
        player.current_hp, player.max_hp, player.block, player.energy,
    )
    .unwrap();
    append_powers_v1(&mut text, "PLAYER_POWER", &player.powers)?;
    writeln!(
        text,
        "zones draw={} discard={} exhaust={} limbo={}",
        input.combat.zones.draw,
        input.combat.zones.discard,
        input.combat.zones.exhaust,
        input.combat.zones.limbo,
    )
    .unwrap();

    writeln!(text, "\n[hand]").unwrap();
    if input.combat.hand.is_empty() {
        writeln!(text, "none").unwrap();
    }
    for (ordinal, card) in input.combat.hand.iter().enumerate() {
        writeln!(
            text,
            "CARD{ordinal} id={} upgrades={} cost={}",
            manifest_scalar_v1(&card.card_id)?,
            card.upgrades,
            card.cost_for_turn,
        )
        .unwrap();
    }

    writeln!(text, "\n[enemies]").unwrap();
    if input.combat.enemies.is_empty() {
        writeln!(text, "none").unwrap();
    }
    for (ordinal, enemy) in input.combat.enemies.iter().enumerate() {
        let status = if enemy.dead_or_escaped {
            "dead_or_escaped"
        } else {
            "active"
        };
        let intent = render_intent_v1(&enemy.intent)?;
        writeln!(
            text,
            "ENEMY{ordinal} id={} hp={}/{} block={} status={} {intent}",
            manifest_scalar_v1(&enemy.enemy_id)?,
            enemy.current_hp,
            enemy.max_hp,
            enemy.block,
            status,
        )
        .unwrap();
        append_powers_v1(&mut text, &format!("ENEMY{ordinal}_POWER"), &enemy.powers)?;
    }

    let ordered_actions = validate_actions_v1(input)?;
    writeln!(text, "\n[actions]").unwrap();
    if ordered_actions.is_empty() {
        writeln!(text, "none").unwrap();
    }
    let mut leases = Vec::with_capacity(ordered_actions.len());
    for (ordinal, action) in ordered_actions.into_iter().enumerate() {
        let alias = format!("A{ordinal}");
        let mut line = match action.kind {
            CombatActionKindV1::Play { hand_index, target } => {
                let mut line = format!("{alias} play CARD{hand_index}");
                if let Some(target) = target {
                    write!(line, " target=ENEMY{target}").unwrap();
                }
                line
            }
            CombatActionKindV1::UsePotion {
                potion_index,
                potency,
                target,
            } => {
                let mut line = format!("{alias} use POTION{potion_index} potency={potency}");
                if let Some(target) = target {
                    write!(line, " target=ENEMY{target}").unwrap();
                }
                line
            }
            CombatActionKindV1::EndTurn => format!("{alias} end_turn"),
        };
        let available = append_action_availability_v1(&mut line, &action.availability)?;
        writeln!(text, "{line}").unwrap();
        leases.push(ActionLeaseEntryV1 {
            alias,
            action_id: Some(action.action_id.clone()),
            available,
        });
    }
    Ok(RenderedManifestV1 {
        text,
        actions: leases,
        diagnostics: Vec::new(),
    })
}

fn validate_combat_v1(combat: &PublicCombatV1) -> Result<(), String> {
    let player = &combat.player;
    if player.max_hp <= 0
        || player.current_hp < 0
        || player.current_hp > player.max_hp
        || player.block < 0
        || player.energy < 0
    {
        return Err("Combat manifest player facts are invalid".to_owned());
    }
    validate_powers_v1(&player.powers)?;
    for card in &combat.hand {
        manifest_scalar_v1(&card.card_id)?;
    }
    for enemy in &combat.enemies {
        manifest_scalar_v1(&enemy.enemy_id)?;
        if enemy.max_hp <= 0
            || (!enemy.dead_or_escaped && enemy.current_hp < 0)
            || enemy.current_hp > enemy.max_hp
            || enemy.block < 0
        {
            return Err("Combat manifest enemy facts are invalid".to_owned());
        }
        match &enemy.intent {
            PublicCombatIntentV1::Visible {
                kind,
                damage_per_hit,
                hits,
            } => {
                manifest_scalar_v1(kind)?;
                if damage_per_hit.is_some() != hits.is_some()
                    || damage_per_hit.is_some_and(|damage| damage < 0)
                    || hits.is_some_and(|hits| hits == 0)
                {
                    return Err("Combat manifest visible intent is incoherent".to_owned());
                }
            }
            PublicCombatIntentV1::Hidden { reason } => {
                require_token(reason, "hidden intent reason")?;
            }
        }
        validate_powers_v1(&enemy.powers)?;
    }
    Ok(())
}

fn validate_powers_v1(powers: &[PublicCombatPowerV1]) -> Result<(), String> {
    for power in powers {
        manifest_scalar_v1(&power.id)?;
    }
    Ok(())
}

fn append_powers_v1(
    text: &mut String,
    prefix: &str,
    powers: &[PublicCombatPowerV1],
) -> Result<(), String> {
    require_token(prefix, "power handle prefix")?;
    if powers.is_empty() {
        writeln!(text, "{prefix} none").unwrap();
    }
    for (ordinal, power) in powers.iter().enumerate() {
        writeln!(
            text,
            "{prefix}{ordinal} id={} amount={}",
            manifest_scalar_v1(&power.id)?,
            power.amount,
        )
        .unwrap();
    }
    Ok(())
}

fn render_intent_v1(intent: &PublicCombatIntentV1) -> Result<String, String> {
    match intent {
        PublicCombatIntentV1::Visible {
            kind,
            damage_per_hit,
            hits,
        } => {
            let mut rendered = format!("intent={}", manifest_scalar_v1(kind)?);
            match (damage_per_hit, hits) {
                (Some(damage), Some(hits)) => {
                    write!(rendered, " damage={damage}x{hits}").unwrap();
                }
                (None, None) => {}
                _ => return Err("Combat manifest intent damage shape is incomplete".to_owned()),
            }
            Ok(rendered)
        }
        PublicCombatIntentV1::Hidden { reason } => {
            require_token(reason, "hidden intent reason")?;
            Ok(format!("intent=hidden reason={reason}"))
        }
    }
}

fn validate_actions_v1(input: &ParityLabCombatInputV1) -> Result<Vec<&CombatActionV1>, String> {
    let potion_count = match &input.shared.potions {
        PublicPotionBeltV1::Combat(potions) => potions.len(),
        PublicPotionBeltV1::Persistent(_) => {
            return Err("Combat actions cannot use persistent potion slots".to_owned())
        }
    };
    let mut action_ids = BTreeSet::new();
    let mut action_kinds = BTreeSet::new();
    let mut end_turns = 0;
    for action in &input.actions {
        require_internal_id(&action.action_id)?;
        if !action_ids.insert(action.action_id.as_str()) {
            return Err("Combat actions repeat an internal identity".to_owned());
        }
        if !action_kinds.insert(action.kind) {
            return Err("Combat actions repeat one semantic action".to_owned());
        }
        match action.kind {
            CombatActionKindV1::Play { hand_index, target } => {
                if hand_index >= input.combat.hand.len()
                    || target.is_some_and(|target| target >= input.combat.enemies.len())
                {
                    return Err("Combat play action references an absent entity".to_owned());
                }
            }
            CombatActionKindV1::UsePotion {
                potion_index,
                potency,
                target,
            } => {
                if potion_index >= potion_count
                    || potency < 0
                    || target.is_some_and(|target| target >= input.combat.enemies.len())
                {
                    return Err("Combat potion action references invalid public facts".to_owned());
                }
            }
            CombatActionKindV1::EndTurn => end_turns += 1,
        }
    }
    if end_turns != 1 {
        return Err("Combat action surface must contain exactly one end_turn".to_owned());
    }
    let mut ordered = input.actions.iter().collect::<Vec<_>>();
    ordered.sort_by_key(|action| action.kind);
    Ok(ordered)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain_manifest::{
        ExactParityV1, ManifestAuthorityV1, ParityStateV1, PublicCombatPotionV1,
        PublicMasterDeckCardV1, PublicRunV1,
    };
    use sts_paired_root::WorldProvenanceV1;

    fn fixture() -> ParityLabCombatInputV1 {
        ParityLabCombatInputV1 {
            shared: SharedExactSceneV1 {
                world: WorldProvenanceV1::Natural,
                authority: ManifestAuthorityV1::LabHead,
                parity: ParityStateV1::Exact(ExactParityV1 {
                    exact_domains: 9,
                    total_domains: 9,
                    coverage_gap_ids: Vec::new(),
                }),
                run: PublicRunV1 {
                    seed: "7".to_owned(),
                    ascension: 20,
                    character: "IRONCLAD".to_owned(),
                    act: 1,
                    floor: 1,
                    current_hp: 72,
                    max_hp: 80,
                    gold: 99,
                    final_act_available: true,
                    keys: [false, false, false],
                },
                master_deck: vec![PublicMasterDeckCardV1 {
                    ordinal: 0,
                    card_id: "Strike_R".to_owned(),
                    upgrades: 0,
                }],
                relics: Vec::new(),
                potions: PublicPotionBeltV1::Combat(vec![PublicCombatPotionV1 {
                    slot: 1,
                    id: "weakenpotion".to_owned(),
                    can_use: true,
                    can_discard: true,
                    target_required: true,
                }]),
            },
            combat: PublicCombatV1 {
                turn_index: 0,
                player: PublicCombatPlayerV1 {
                    current_hp: 72,
                    max_hp: 80,
                    block: 0,
                    energy: 3,
                    powers: Vec::new(),
                },
                zones: PublicCombatZoneSummaryV1 {
                    draw: 5,
                    discard: 0,
                    exhaust: 0,
                    limbo: 0,
                },
                hand: vec![PublicCombatCardV1 {
                    card_id: "Strike_R".to_owned(),
                    upgrades: 0,
                    cost_for_turn: 1,
                }],
                enemies: vec![PublicCombatEnemyV1 {
                    enemy_id: "JawWorm".to_owned(),
                    current_hp: 40,
                    max_hp: 40,
                    block: 0,
                    dead_or_escaped: false,
                    intent: PublicCombatIntentV1::Visible {
                        kind: "attack_buff".to_owned(),
                        damage_per_hit: Some(11),
                        hits: Some(1),
                    },
                    powers: vec![PublicCombatPowerV1 {
                        id: "vulnerable".to_owned(),
                        amount: 1,
                    }],
                }],
            },
            actions: vec![
                CombatActionV1 {
                    action_id: "action-end".to_owned(),
                    kind: CombatActionKindV1::EndTurn,
                    availability: ActionAvailabilityV1::Verified,
                },
                CombatActionV1 {
                    action_id: "action-potion".to_owned(),
                    kind: CombatActionKindV1::UsePotion {
                        potion_index: 0,
                        potency: 3,
                        target: Some(0),
                    },
                    availability: ActionAvailabilityV1::Unknown {
                        gap_id: "potion-gap".to_owned(),
                    },
                },
                CombatActionV1 {
                    action_id: "action-card".to_owned(),
                    kind: CombatActionKindV1::Play {
                        hand_index: 0,
                        target: Some(0),
                    },
                    availability: ActionAvailabilityV1::Verified,
                },
            ],
        }
    }

    #[test]
    fn combat_manifest_is_flat_ordered_and_public_intent_only() {
        let rendered = render_parity_lab_combat_v1(&fixture()).unwrap();
        assert!(rendered.text.contains("scene=combat"));
        assert!(rendered.text.contains("PLAYER hp=72/80 block=0 energy=3"));
        assert!(rendered.text.contains(
            "ENEMY0 id=JawWorm hp=40/40 block=0 status=active intent=attack_buff damage=11x1"
        ));
        assert!(rendered.text.contains("A0 play CARD0 target=ENEMY0"));
        assert!(rendered
            .text
            .contains("A1 use POTION0 potency=3 target=ENEMY0 unavailable=unknown gap=potion-gap"));
        assert!(rendered.text.contains("A2 end_turn"));
        assert!(!rendered.text.contains("next_move"));
        assert!(!rendered.text.contains("sha256"));
        assert_eq!(
            rendered.actions[0].action_id.as_deref(),
            Some("action-card")
        );
        assert!(!rendered.actions[1].available);
    }

    #[test]
    fn runic_dome_manifest_cannot_render_a_visible_intent() {
        let mut input = fixture();
        input.combat.enemies[0].intent = PublicCombatIntentV1::Hidden {
            reason: "runic_dome".to_owned(),
        };
        let rendered = render_parity_lab_combat_v1(&input).unwrap();
        assert!(rendered.text.contains("intent=hidden reason=runic_dome"));
        assert!(!rendered.text.contains("attack_buff"));
        assert!(!rendered.text.contains("damage=11x1"));
    }
}
