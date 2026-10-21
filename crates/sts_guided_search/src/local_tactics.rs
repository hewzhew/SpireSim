//! Schema 2: bounded entity/action observations for the draw/select task.
//! Rows and references are local to one source. Nothing here executes or scores.
use sts_core::content::{cards::CardId, monsters::EnemyId, powers::PowerId, relics::RelicId};
use sts_core::sim::combat_action_surface::CombatSelectionReasonV2;
use sts_core::state::core::{GridSelectReason, HandSelectReason};

use crate::observation::{combat_observation, model_choices, CardAddress, CardCost, ModelChoice};
use crate::{CombatSearchSpace, NodeRef, SearchSession};

pub const SCHEMA: u32 = 2;
pub const CONTEXT_WIDTH: usize = 32;
pub const CARD_WIDTH: usize = 24;
pub const MONSTER_WIDTH: usize = 50;
pub const ACTION_WIDTH: usize = 6;
pub const MAX_CARDS: usize = 16;
pub const MAX_MONSTERS: usize = 3;
pub const MAX_ACTIONS: usize = 32;

pub(crate) fn card_index(card: CardId) -> Result<usize, String> {
    match card {
        CardId::Strike => Ok(0),
        CardId::Defend => Ok(1),
        CardId::Bash => Ok(2),
        CardId::AscendersBane => Ok(3),
        CardId::BurningPact => Ok(4),
        CardId::Headbutt => Ok(5),
        CardId::PommelStrike => Ok(6),
        CardId::ShrugItOff => Ok(7),
        _ => Err("card is outside the numerical draw/select task".into()),
    }
}

/// Explicit constructed deck, in master-deck order; distinct copies stay distinct.
pub fn validate_deck(deck: &[(CardId, u8)]) -> Result<(), String> {
    if !(1..=MAX_CARDS).contains(&deck.len()) {
        return Err("constructed deck must contain 1..16 cards".into());
    }
    for &(card, upgrades) in deck {
        card_index(card)?;
        if upgrades > 1 || (card == CardId::AscendersBane && upgrades != 0) {
            return Err("unsupported constructed card upgrade".into());
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct TacticalPosition {
    pub context: [f32; CONTEXT_WIDTH],
    pub cards: Vec<[f32; CARD_WIDTH]>,
    pub monsters: Vec<[f32; MONSTER_WIDTH]>,
    /// Play, EndTurn, BeginSelection, SelectMember, FinishSelection; target damage.
    pub actions: Vec<[f32; ACTION_WIDTH]>,
    /// Card row and monster row, -1 when absent; each legal edge stays distinct.
    pub action_refs: Vec<[i64; 2]>,
}

pub fn encode(
    session: &SearchSession<CombatSearchSpace>,
    source: NodeRef,
) -> Result<TacticalPosition, String> {
    let view = session.view(source).map_err(|e| format!("{e:?}"))?;
    let obs = combat_observation(view.state)?;
    if obs.player_class != "Ironclad"
        || obs.ascension != 20
        || obs.monsters.is_empty()
        || obs.monsters.len() > MAX_MONSTERS
        || obs.relics.len() != 1
        || obs.relics[0].relic != RelicId::BurningBlood
        || obs.potions.iter().any(Option::is_some)
        || !obs.orbs.is_empty()
        || obs.indexed_choice_reason.is_some()
        || obs.selections.len() > 1
    {
        return Err("position is outside the numerical draw/select task".into());
    }
    let mut context = [0.0; CONTEXT_WIDTH];
    context[..12].copy_from_slice(&[
        obs.hp[0] as f32 / 75.0,
        obs.hp[1] as f32 / 75.0,
        obs.block as f32 / 50.0,
        obs.energy as f32 / 3.0,
        obs.energy_per_turn as f32 / 3.0,
        obs.turn as f32 / 20.0,
        obs.gold as f32 / 200.0,
        obs.counters.played as f32 / 10.0,
        obs.counters.attacks as f32 / 10.0,
        obs.counters.discarded as f32 / 10.0,
        obs.counters.draw_modifier_next_turn as f32 / 5.0,
        obs.counters.damaged_this_combat as f32 / 10.0,
    ]);
    for p in &obs.powers {
        if p.power != PowerId::Weak {
            return Err("player power is outside the numerical draw/select task".into());
        }
        context[12..16].copy_from_slice(&[
            1.0,
            p.amount.unwrap_or(0) as f32 / 10.0,
            p.extra as f32 / 10.0,
            f32::from(p.just_applied),
        ]);
    }
    context[31] = f32::from(obs.facing_left);
    let piles = [
        &obs.hand,
        &obs.draw_top_first,
        &obs.discard,
        &obs.exhaust,
        &obs.limbo,
    ];
    let mut offsets = [0usize; 5];
    for i in 1..5 {
        offsets[i] = offsets[i - 1] + piles[i - 1].len();
    }
    let address = |a: &CardAddress| -> Result<usize, String> {
        let (zone, index) = match a {
            CardAddress::Hand(i) => (0, *i),
            CardAddress::Draw(i) => (1, *i),
            CardAddress::Discard(i) => (2, *i),
            CardAddress::Exhaust(i) => (3, *i),
            CardAddress::Limbo(i) => (4, *i),
        };
        if index >= piles[zone].len() {
            return Err("invalid selection card address".into());
        }
        Ok(offsets[zone] + index)
    };
    let mut cards = Vec::new();
    for (zone, pile) in piles.iter().enumerate() {
        for (position, c) in pile.iter().enumerate() {
            if c.base_damage_override.is_some()
                || c.base_block_override.is_some()
                || c.exhaust_override.is_some()
                || c.retain_override.is_some()
            {
                return Err("card override is outside the numerical draw/select task".into());
            }
            let mut row = [0.0; CARD_WIDTH];
            row[card_index(c.card)?] = 1.0;
            row[8 + zone] = 1.0;
            let cost = match c.cost {
                CardCost::Energy(n) => n,
                CardCost::Special("unplayable") => -2,
                _ => return Err("cost is outside the numerical draw/select task".into()),
            };
            row[13..22].copy_from_slice(&[
                position as f32 / MAX_CARDS as f32,
                c.upgrades as f32,
                cost as f32 / 3.0,
                c.combat_cost.unwrap_or(cost) as f32 / 3.0,
                c.damage as f32 / 20.0,
                c.block as f32 / 20.0,
                c.magic as f32 / 5.0,
                f32::from(c.free_once),
                c.misc as f32 / 20.0,
            ]);
            cards.push(row);
        }
    }
    if !(1..=MAX_CARDS).contains(&cards.len()) {
        return Err("draw/select card capacity exceeded".into());
    }
    let mut members = Vec::new();
    if let Some(s) = obs.selections.first() {
        context[16] = 1.0;
        context[match s.reason {
            CombatSelectionReasonV2::Hand(HandSelectReason::Exhaust) => 17,
            CombatSelectionReasonV2::Grid(GridSelectReason::MoveToDrawPile) => 18,
            _ => return Err("selection is outside the numerical draw/select task".into()),
        }] = 1.0;
        context[19] = s.min as f32 / MAX_CARDS as f32;
        context[20] = s.max as f32 / MAX_CARDS as f32;
        for a in &s.members {
            let row = address(a)?;
            cards[row][22] = 1.0;
            members.push(row);
        }
        if let Some(assembly) = &obs.assembling {
            if assembly.family != 0 {
                return Err("invalid numerical selection family".into());
            }
            context[21] = 1.0;
            context[22] = assembly.selected.len() as f32 / MAX_CARDS as f32;
            for (order, &member) in assembly.selected.iter().enumerate() {
                let row = *members.get(member).ok_or("invalid selection prefix")?;
                cards[row][23] = (order + 1) as f32 / MAX_CARDS as f32;
            }
        }
    } else if obs.assembling.is_some() {
        return Err("selection prefix lacks a family".into());
    }
    let mut monsters = Vec::new();
    for (index, m) in obs.monsters.iter().enumerate() {
        let mut row = [0.0; MONSTER_WIDTH];
        row[match m.enemy {
            EnemyId::JawWorm => 0,
            EnemyId::Cultist => 1,
            EnemyId::LouseNormal => 2,
            EnemyId::LouseDefensive => 3,
            _ => return Err("enemy is outside the numerical draw/select task".into()),
        }] = 1.0;
        row[4..11].copy_from_slice(&[
            index as f32 / MAX_MONSTERS as f32,
            m.hp[0] as f32 / 60.0,
            m.hp[1] as f32 / 60.0,
            m.block as f32 / 50.0,
            f32::from(m.alive),
            f32::from(m.half_dead),
            f32::from(m.escaped),
        ]);
        if let Some(intent) = &m.intent {
            row[11..15].copy_from_slice(&[
                1.0,
                intent.damage_per_hit.unwrap_or(0) as f32 / 30.0,
                intent.hits as f32 / 5.0,
                intent.total_damage.unwrap_or(0) as f32 / 30.0,
            ]);
            row[15 + intent.visible_intent as usize] = 1.0;
        }
        for p in &m.powers {
            let base = match p.power {
                PowerId::Strength => 33,
                PowerId::Ritual => 37,
                PowerId::Vulnerable => 41,
                PowerId::CurlUp => 45,
                _ => return Err("enemy power is outside the numerical draw/select task".into()),
            };
            row[base..base + 4].copy_from_slice(&[
                1.0,
                p.amount.unwrap_or(0) as f32 / 10.0,
                p.extra as f32 / 10.0,
                f32::from(p.just_applied),
            ]);
        }
        monsters.push(row);
    }
    let mut actions = Vec::new();
    let mut action_refs = Vec::new();
    for a in model_choices(view.state, &view.actions)? {
        let mut row = [0.0; ACTION_WIDTH];
        let mut refs = [-1; 2];
        let kind = match a {
            ModelChoice::Play { hand, target } => {
                let c = obs.hand.get(hand).ok_or("invalid play card address")?;
                refs[0] = hand as i64;
                if let Some(target) = target {
                    if target >= monsters.len() {
                        return Err("invalid monster target".into());
                    }
                    refs[1] = target as i64;
                    row[5] = *c
                        .damage_by_target
                        .get(target)
                        .ok_or("missing target damage")? as f32
                        / 20.0;
                }
                0
            }
            ModelChoice::EndTurn {} => 1,
            ModelChoice::BeginSelection { family: 0 } if !obs.selections.is_empty() => 2,
            ModelChoice::SelectMember { member } if obs.assembling.is_some() => {
                refs[0] = *members.get(member).ok_or("invalid member reference")? as i64;
                3
            }
            ModelChoice::FinishSelection {} if obs.assembling.is_some() => 4,
            _ => return Err("action is outside the numerical draw/select task".into()),
        };
        row[kind] = 1.0;
        actions.push(row);
        action_refs.push(refs);
    }
    if actions.len() > MAX_ACTIONS {
        return Err("draw/select action capacity exceeded".into());
    }
    Ok(TacticalPosition {
        context,
        cards,
        monsters,
        actions,
        action_refs,
    })
}
