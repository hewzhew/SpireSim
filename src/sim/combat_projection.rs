use crate::runtime::combat::{CombatState, MonsterEntity};
use crate::runtime::monster_move::{DamageKind, EffectStrength, MonsterMoveSpec, MonsterTurnPlan};
use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum VisibleIntentKind {
    Attack,
    AttackBuff,
    AttackDebuff,
    AttackDefend,
    Buff,
    Heal,
    Debuff,
    StrongDebuff,
    Defend,
    DefendDebuff,
    DefendBuff,
    Escape,
    Magic,
    Sleep,
    Stun,
    Debug,
    None,
    Unknown,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MonsterMovePreview {
    pub damage_per_hit: Option<i32>,
    pub hits: u8,
    pub total_damage: Option<i32>,
    pub visible_intent: VisibleIntentKind,
}

/// Current-state attack request before block and damage-received hooks. Unlike
/// the public intent preview, this never reads the protocol's display cache.
/// It is not a forecast of the whole enemy turn: earlier packets and turn hooks
/// may change the state before this attack actually executes.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
pub struct MonsterAttackRequest {
    pub damage_per_hit: i32,
    pub hits: u8,
    pub total_damage: i32,
}

/// Current-state requests before the enemy acts. These are separate phase
/// components, not already gained block or a whole-turn damage forecast.
/// Relics, orbs, exhaust/retention hooks and changes caused by these requests
/// are not settled; damage-received prevention is not borrowed here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct PlayerTurnEndRequests {
    pub block_after_pre_card_powers: i32,
    pub blockable_self_damage: i64,
    pub hp_loss: i64,
}

pub fn project_player_turn_end_requests(combat: &CombatState) -> PlayerTurnEndRequests {
    use crate::content::powers::{self, store, PowerId};
    use crate::engine::action_handlers::{cards, damage::block_after_gain};
    use crate::runtime::action::{Action, DamageType};
    let owner = combat.entities.player.id;
    let mut requests = PlayerTurnEndRequests {
        block_after_pre_card_powers: combat.entities.player.block,
        ..Default::default()
    };
    let powers = store::powers_for(combat, owner).unwrap_or(&[]);
    for power in powers {
        if !powers::end_turn_power_precedes_cards(power.power_type) {
            continue;
        }
        for action in powers::resolve_power_pre_end_turn_cards(power, combat, owner) {
            if let Action::GainBlock { target, amount } = action {
                if target == owner {
                    requests.block_after_pre_card_powers =
                        block_after_gain(requests.block_after_pre_card_powers, amount);
                }
            }
        }
    }
    let mut debt = |action: &Action| match action {
        Action::Damage(info) if info.target == owner => {
            if info.damage_type == DamageType::HpLoss {
                requests.hp_loss += i64::from(info.output.max(0));
            } else {
                requests.blockable_self_damage += i64::from(info.output.max(0));
            }
        }
        Action::LoseHp { target, amount, .. } if *target == owner => {
            requests.hp_loss += i64::from((*amount).max(0));
        }
        _ => {}
    };
    for card in &combat.zones.hand {
        if let Some(queued) = cards::end_turn_autoplay_card(combat, card) {
            for request in cards::resolve_end_turn_triggered_autoplay(combat, &queued) {
                debt(&request.action);
            }
        }
    }
    for power in powers {
        let actions = match power.power_type {
            PowerId::Combust => {
                powers::ironclad::combust::at_end_of_turn(combat, owner, power.amount)
            }
            PowerId::Constricted => powers::core::constricted::at_end_of_turn(owner, power),
            _ => continue,
        };
        for action in actions {
            debt(&action);
        }
    }
    requests
}

pub fn project_monster_attack_request_in_combat(
    combat: &CombatState,
    monster: &MonsterEntity,
) -> Option<MonsterAttackRequest> {
    let plan = crate::content::monsters::resolve_monster_turn_plan(combat, monster);
    attack_request_from_plan(combat, monster, &plan)
}

fn attack_request_from_plan(
    combat: &CombatState,
    monster: &MonsterEntity,
    plan: &MonsterTurnPlan,
) -> Option<MonsterAttackRequest> {
    let attack = plan.attack()?;
    let damage_per_hit = if attack.damage_kind == DamageKind::Normal {
        crate::content::powers::calculate_monster_damage(
            attack.base_damage,
            monster.id,
            combat.entities.player.id,
            combat,
        )
    } else {
        attack.base_damage.max(0)
    };
    let hits = attack.hits.max(1);
    Some(MonsterAttackRequest {
        damage_per_hit,
        hits,
        total_damage: damage_per_hit.saturating_mul(i32::from(hits)),
    })
}

impl MonsterMovePreview {
    pub fn from_plan(plan: &MonsterTurnPlan, damage_per_hit: Option<i32>) -> Self {
        let hits = plan.attack().map(|attack| attack.hits.max(1)).unwrap_or(0);
        let total_damage = damage_per_hit.map(|damage| damage.saturating_mul(hits as i32));

        Self {
            damage_per_hit,
            hits,
            total_damage,
            visible_intent: visible_intent_kind(plan),
        }
    }
}

pub fn project_monster_move_preview(monster: &MonsterEntity) -> MonsterMovePreview {
    let plan = monster.turn_plan();
    MonsterMovePreview::from_plan(&plan, plan.attack().map(|attack| attack.base_damage.max(0)))
}

pub fn project_monster_move_preview_in_combat(
    combat: &CombatState,
    monster: &MonsterEntity,
) -> MonsterMovePreview {
    let plan = crate::content::monsters::resolve_monster_turn_plan(combat, monster);
    project_monster_move_preview_from_plan(combat, monster, &plan)
}

pub fn project_monster_move_preview_from_plan(
    combat: &CombatState,
    monster: &MonsterEntity,
    plan: &MonsterTurnPlan,
) -> MonsterMovePreview {
    let protocol_damage =
        if plan.attack().is_some() && combat.monster_has_protocol_visible_intent(monster.id) {
            let observed = combat
                .monster_protocol_preview_damage_per_hit(monster.id)
                .max(0);
            (observed > 0).then_some(observed)
        } else {
            None
        };
    let semantic_damage =
        attack_request_from_plan(combat, monster, plan).map(|request| request.damage_per_hit);
    let damage_per_hit = protocol_damage.or(semantic_damage);
    MonsterMovePreview::from_plan(plan, damage_per_hit)
}

pub fn monster_has_visible_attack(monster: &MonsterEntity) -> bool {
    project_monster_move_preview(monster)
        .damage_per_hit
        .is_some()
}

pub fn monster_has_visible_attack_in_combat(combat: &CombatState, monster: &MonsterEntity) -> bool {
    project_monster_move_preview_in_combat(combat, monster)
        .damage_per_hit
        .is_some()
}

pub fn monster_preview_total_damage(monster: &MonsterEntity) -> i32 {
    project_monster_move_preview(monster)
        .total_damage
        .unwrap_or(0)
}

pub fn monster_preview_total_damage_in_combat(
    combat: &CombatState,
    monster: &MonsterEntity,
) -> i32 {
    project_monster_move_preview_in_combat(combat, monster)
        .total_damage
        .unwrap_or(0)
}

fn visible_intent_kind(plan: &MonsterTurnPlan) -> VisibleIntentKind {
    match plan.summary_spec() {
        MonsterMoveSpec::Attack(_) => VisibleIntentKind::Attack,
        MonsterMoveSpec::AttackAddCard(_, _) => VisibleIntentKind::AttackDebuff,
        MonsterMoveSpec::AttackUpgradeCards(_, _) => VisibleIntentKind::AttackDebuff,
        MonsterMoveSpec::AttackBuff(_, _) | MonsterMoveSpec::AttackBuffIntent(_) => {
            VisibleIntentKind::AttackBuff
        }
        MonsterMoveSpec::AttackDebuff(_, _) => VisibleIntentKind::AttackDebuff,
        MonsterMoveSpec::AttackDefend(_, _) => VisibleIntentKind::AttackDefend,
        MonsterMoveSpec::AddCard(add_card) => match add_card.visible_strength {
            EffectStrength::Strong => VisibleIntentKind::StrongDebuff,
            EffectStrength::Normal => VisibleIntentKind::Debuff,
        },
        MonsterMoveSpec::Buff(_) | MonsterMoveSpec::BuffIntent => VisibleIntentKind::Buff,
        MonsterMoveSpec::Heal(_) => VisibleIntentKind::Heal,
        MonsterMoveSpec::Debuff(_) => VisibleIntentKind::Debuff,
        MonsterMoveSpec::StrongDebuff(_) | MonsterMoveSpec::StrongDebuffIntent => {
            VisibleIntentKind::StrongDebuff
        }
        MonsterMoveSpec::Defend(_) => VisibleIntentKind::Defend,
        MonsterMoveSpec::DefendDebuff(_, _) => VisibleIntentKind::DefendDebuff,
        MonsterMoveSpec::DefendBuff(_, _) => VisibleIntentKind::DefendBuff,
        MonsterMoveSpec::Escape => VisibleIntentKind::Escape,
        MonsterMoveSpec::Magic => VisibleIntentKind::Magic,
        MonsterMoveSpec::Sleep => VisibleIntentKind::Sleep,
        MonsterMoveSpec::Stun => VisibleIntentKind::Stun,
        MonsterMoveSpec::Debug => VisibleIntentKind::Debug,
        MonsterMoveSpec::None => VisibleIntentKind::None,
        MonsterMoveSpec::Unknown => VisibleIntentKind::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::EnemyId;
    use crate::content::powers::{store, PowerId};
    use crate::runtime::combat::{Power, PowerPayload};
    use crate::test_support::{blank_test_combat, planned_monster};

    fn power(power_type: PowerId, amount: i32) -> Power {
        Power {
            power_type,
            instance_id: None,
            amount,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }
    }

    fn end_turn_fixture() -> CombatState {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;
        let mut combat = blank_test_combat();
        combat.entities.monsters = vec![planned_monster(EnemyId::Cultist, 1)];
        combat.turn.mark_skip_monster_turn_pending();
        combat.zones.draw_pile = (100..105)
            .map(|id| CombatCard::for_scenario(CardId::Defend, id))
            .collect::<Vec<_>>()
            .into();
        combat
    }

    fn end_turn_hp(mut combat: CombatState) -> i32 {
        use crate::state::core::{ClientInput, EngineState};
        let mut engine = EngineState::CombatPlayerTurn;
        assert!(crate::engine::core::tick_until_stable_turn(
            &mut engine,
            &mut combat,
            ClientInput::EndTurn,
        ));
        assert_eq!(engine, EngineState::CombatPlayerTurn);
        combat.entities.player.current_hp
    }

    #[test]
    fn automatic_block_precedes_cards_without_card_block_modifiers() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::{CombatCard, StanceId};
        for stance in [StanceId::Neutral, StanceId::Calm] {
            let mut combat = end_turn_fixture();
            combat.entities.player.stance = stance;
            let mut burn = CombatCard::for_scenario(CardId::Burn, 1);
            burn.upgrades = 1;
            combat.zones.hand = vec![
                burn,
                CombatCard::for_scenario(CardId::Burn, 2),
                CombatCard::for_scenario(CardId::Decay, 3),
            ];
            store::set_powers_for(
                &mut combat,
                0,
                vec![
                    power(PowerId::Metallicize, 4),
                    power(PowerId::PlatedArmor, 3),
                    power(PowerId::LikeWaterPower, 5),
                    power(PowerId::Dexterity, 8),
                    power(PowerId::Frail, 2),
                    power(PowerId::NoBlock, 1),
                ],
            );
            let before = combat.clone();
            let request = project_player_turn_end_requests(&combat);
            assert_eq!(combat, before, "projection must not mutate state or RNG");
            assert_eq!(
                request.block_after_pre_card_powers,
                if stance == StanceId::Calm { 12 } else { 7 }
            );
            assert_eq!((request.blockable_self_damage, request.hp_loss), (8, 0));
            let requested_loss = (request.blockable_self_damage
                - i64::from(request.block_after_pre_card_powers))
            .max(0);
            assert_eq!(
                end_turn_hp(combat),
                before.entities.player.current_hp - requested_loss as i32
            );
        }
    }

    #[test]
    fn end_turn_debt_preserves_regret_snapshot_and_unblockable_power_cost() {
        use crate::content::cards::CardId;
        use crate::runtime::combat::CombatCard;
        let mut combat = end_turn_fixture();
        combat.entities.player.block = 3;
        let mut burn = CombatCard::for_scenario(CardId::Burn, 2);
        burn.upgrades = 1;
        combat.zones.hand = vec![
            CombatCard::for_scenario(CardId::Burn, 1),
            burn,
            CombatCard::for_scenario(CardId::Decay, 3),
            CombatCard::for_scenario(CardId::Regret, 4),
            CombatCard::for_scenario(CardId::Wound, 5),
        ];
        let mut combust = power(PowerId::Combust, 5);
        combust.extra_data = 2;
        let mut constricted = power(PowerId::Constricted, 10);
        constricted.payload = PowerPayload::SourceEntity(1);
        store::set_powers_for(
            &mut combat,
            0,
            vec![power(PowerId::Metallicize, 4), combust, constricted],
        );
        let before = combat.clone();
        let request = project_player_turn_end_requests(&combat);
        assert_eq!(combat, before);
        assert_eq!(request.block_after_pre_card_powers, 7);
        assert_eq!(request.blockable_self_damage, 18);
        assert_eq!(
            request.hp_loss, 7,
            "Regret snapshots all five cards before they leave; Combust costs two HP"
        );
        assert_eq!(
            end_turn_hp(combat.clone()),
            before.entities.player.current_hp - 18
        );
        combat.entities.player.block = 100;
        assert_eq!(
            end_turn_hp(combat),
            before.entities.player.current_hp - 7,
            "extra block cannot pay HP-loss requests"
        );
    }

    #[test]
    fn turn_end_requests_share_block_cap_without_borrowing_damage_prevention() {
        use crate::content::cards::CardId;
        use crate::engine::action_handlers::execute_action;
        use crate::runtime::action::Action;
        use crate::runtime::combat::CombatCard;
        let mut combat = end_turn_fixture();
        combat.entities.player.block = 998;
        store::set_powers_for(&mut combat, 0, vec![power(PowerId::Metallicize, 4)]);
        let request = project_player_turn_end_requests(&combat);
        let mut executed = combat.clone();
        execute_action(
            Action::GainBlock {
                target: 0,
                amount: 4,
            },
            &mut executed,
        );
        assert_eq!(
            request.block_after_pre_card_powers,
            executed.entities.player.block
        );
        assert_eq!(request.block_after_pre_card_powers, 999);

        combat.entities.player.block = 0;
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Burn, 1)];
        store::set_powers_for(&mut combat, 0, vec![power(PowerId::Buffer, 1)]);
        let request = project_player_turn_end_requests(&combat);
        assert_eq!(request.blockable_self_damage, 2);
        assert_eq!(
            end_turn_hp(combat.clone()),
            combat.entities.player.current_hp,
            "requests are not settled damage predictions"
        );
    }

    #[test]
    fn semantic_monster_preview_uses_the_execution_damage_pipeline() {
        let mut combat = blank_test_combat();
        let monster = planned_monster(EnemyId::TimeEater, 2);
        let monster_id = monster.id;
        combat.entities.monsters = vec![monster.clone()];
        store::set_powers_for(&mut combat, monster_id, vec![power(PowerId::Strength, 5)]);
        let player_id = combat.entities.player.id;
        store::set_powers_for(&mut combat, player_id, vec![power(PowerId::Vulnerable, 1)]);

        let plan = crate::content::monsters::resolve_monster_turn_plan(&combat, &monster);
        let attack = plan.attack().expect("test move should attack");
        let expected = crate::content::powers::calculate_monster_damage(
            attack.base_damage,
            monster_id,
            player_id,
            &combat,
        );
        let preview = project_monster_move_preview_in_combat(&combat, &monster);

        assert!(expected > attack.base_damage);
        assert_eq!(preview.damage_per_hit, Some(expected));
        assert_eq!(
            preview.total_damage,
            Some(expected.saturating_mul(attack.hits.max(1) as i32))
        );
    }

    #[test]
    fn attack_request_tracks_facing_and_powers_without_rewriting_visible_cache() {
        let mut combat = blank_test_combat();
        let mut monster = planned_monster(EnemyId::SpireShield, 1);
        monster.logical_position = -1;
        let monster_id = monster.id;
        combat.entities.monsters = vec![monster.clone()];
        let player_id = combat.entities.player.id;
        store::set_powers_for(&mut combat, player_id, vec![power(PowerId::Surrounded, -1)]);
        combat.set_monster_protocol_visible_intent(
            monster_id,
            crate::runtime::combat::Intent::Attack {
                damage: 99,
                hits: 1,
            },
        );
        combat.set_monster_protocol_preview_damage_per_hit(monster_id, 99);

        let back = project_monster_attack_request_in_combat(&combat, &monster).unwrap();
        combat.entities.player.facing_left = true;
        let front = project_monster_attack_request_in_combat(&combat, &monster).unwrap();
        assert!(back.total_damage > front.total_damage);
        assert_eq!(
            project_monster_move_preview_in_combat(&combat, &monster).damage_per_hit,
            Some(99),
            "the observation cache keeps its own display semantics"
        );

        store::set_powers_for(&mut combat, monster_id, vec![power(PowerId::Strength, 8)]);
        let strong = project_monster_attack_request_in_combat(&combat, &monster).unwrap();
        assert!(strong.total_damage > front.total_damage);
        store::ensure_powers_for_mut(&mut combat, monster_id).push(power(PowerId::Weak, 1));
        let weak = project_monster_attack_request_in_combat(&combat, &monster).unwrap();
        assert!(weak.total_damage < strong.total_damage);
        let plan = crate::content::monsters::resolve_monster_turn_plan(&combat, &monster);
        assert_eq!(
            weak.damage_per_hit,
            crate::content::powers::calculate_monster_damage(
                plan.attack().unwrap().base_damage,
                monster_id,
                player_id,
                &combat
            )
        );
        assert_eq!(
            combat.monster_protocol_preview_damage_per_hit(monster_id),
            99
        );
    }
}
