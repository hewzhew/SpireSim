//! Schema 9: variable-length numerical objects from the shared oracle projection.
//! Local row references are control-free relations, not persistent game identities.
use crate::observation::{
    combat_observation, model_choices, CardAddress, CardCost, CardObservation, Creature,
    ModelChoice, PowerObservation,
};
use crate::{CombatBranch, CombatSearchSpace, CombatSearchState, NodeRef, SearchSession};
use serde::{Deserialize, Serialize};
use sts_core::runtime::combat::StanceId;
use sts_core::sim::combat_action_surface::{
    CombatIndexedChoiceCandidateV2, CombatIndexedChoiceReasonV2, CombatSelectionReasonV2,
};
use sts_core::state::core::{GridSelectReason, HandSelectReason};
mod cards;
mod effects;
mod potions;
mod powers;

pub const SCHEMA: u32 = 9;
pub const WIDTH: usize = 32;
pub const ACTION_WIDTH: usize = 4;
pub const KIND_COUNT: usize = 26;
pub const CATEGORY_COUNT: usize = 4096;
// Entity kinds: player, card, monster, power, relic, potion slot, selection,
// selection member, offered card, turn card history, combat card history, choice context.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct EntityPosition {
    pub ids: Vec<[i64; 2]>,
    pub values: Vec<[f32; WIDTH]>,
    pub refs: Vec<[i64; 3]>,
    pub action_ids: Vec<[i64; 2]>,
    pub actions: Vec<[f32; ACTION_WIDTH]>,
    pub action_refs: Vec<[i64; 3]>,
}

impl EntityPosition {
    fn push(&mut self, kind: i64, category: i64, values: [f32; WIDTH], refs: [i64; 3]) -> i64 {
        let row = self.ids.len() as i64;
        self.ids.push([kind, category]);
        self.values.push(values);
        self.refs.push(refs);
        row
    }
}

fn optional(row: &mut [f32], index: usize, value: Option<i32>, scale: f32) {
    row[index] = f32::from(value.is_some());
    row[index + 1] = value.unwrap_or(0) as f32 / scale;
}

fn card_values(c: &CardObservation, zone: usize, position: usize) -> Result<[f32; WIDTH], String> {
    let mut v = [0.; WIDTH];
    v[zone] = 1.; // 0..4 piles, 5 power payload; candidates use a separate kind.
    v[6] = position as f32 / 32.;
    v[7] = c.upgrades as f32;
    match c.cost {
        CardCost::Energy(n) => {
            v[8] = 1.;
            v[9] = n as f32 / 3.;
        }
        CardCost::Special("X") => v[10] = 1.,
        CardCost::Special("unplayable") => v[11] = 1.,
        _ => return Err("unknown entity card cost".into()),
    }
    optional(&mut v, 12, c.combat_cost, 3.);
    optional(&mut v, 14, c.base_damage_override, 20.);
    optional(&mut v, 16, c.base_block_override, 20.);
    v[18..23].copy_from_slice(&[
        f32::from(c.free_once),
        c.misc as f32 / 20.,
        c.damage as f32 / 20.,
        c.block as f32 / 20.,
        c.magic as f32 / 5.,
    ]);
    optional(&mut v, 23, c.exhaust_override.map(i32::from), 1.);
    optional(&mut v, 25, c.retain_override.map(i32::from), 1.);
    cards::card_type(&mut v, c.card);
    Ok(v)
}

fn address(a: &CardAddress, piles: &[Vec<i64>]) -> Result<i64, String> {
    let (zone, index) = match *a {
        CardAddress::Hand(i) => (0, i),
        CardAddress::Draw(i) => (1, i),
        CardAddress::Discard(i) => (2, i),
        CardAddress::Exhaust(i) => (3, i),
        CardAddress::Limbo(i) => (4, i),
    };
    piles[zone]
        .get(index)
        .copied()
        .ok_or_else(|| "absent entity card reference".into())
}

fn creature(c: &Creature, monsters: &[i64]) -> Result<i64, String> {
    match c {
        Creature::Player => Ok(0),
        Creature::Monster(i) => monsters
            .get(*i)
            .copied()
            .ok_or_else(|| "absent entity creature".into()),
        Creature::Unobserved => Err("unobserved power source in entity task".into()),
    }
}

fn powers(
    p: &mut EntityPosition,
    powers: &[PowerObservation],
    owner: i64,
    monsters: &[i64],
    piles: &[Vec<i64>],
) -> Result<(), String> {
    for (index, power) in powers.iter().enumerate() {
        let mut v = [0.; WIDTH];
        optional(&mut v, 0, power.amount, 10.);
        v[2] = power.extra as f32 / 10.;
        v[3] = f32::from(power.just_applied);
        v[4] = index as f32 / 32.;
        let mut refs = [owner, -1, -1];
        if let Some(card) = &power.card {
            refs[1] = p.push(
                2,
                card.card.stable_ordinal() as i64,
                card_values(card, 5, 0)?,
                [owner, -1, -1],
            );
            cards::encode(p, card, refs[1]);
        }
        if let Some(held) = &power.held_card {
            if refs[1] != -1 {
                return Err("ambiguous power card payload".into());
            }
            refs[1] = address(held, piles)?;
        }
        if let Some(source) = &power.source {
            refs[2] = creature(source, monsters)?;
        }
        let row = p.push(4, power.power.stable_ordinal() as i64, v, refs);
        powers::encode(p, power.power, row, owner);
    }
    Ok(())
}

fn selection_reason(reason: &CombatSelectionReasonV2) -> (i64, f32) {
    use GridSelectReason as G;
    use HandSelectReason as H;
    match reason {
        CombatSelectionReasonV2::Hand(h) => match h {
            H::Exhaust => (0, 0.),
            H::PotionExhaust {
                has_pending_use_effect,
            } => (1, f32::from(*has_pending_use_effect)),
            H::Discard => (2, 0.),
            H::Retain => (3, 0.),
            H::PutOnDrawPile => (4, 0.),
            H::PutToBottomOfDraw => (5, 0.),
            H::Setup => (6, 0.),
            H::Copy { amount } => (7, *amount as f32),
            H::Nightmare { amount } => (8, *amount as f32),
            H::Upgrade => (9, 0.),
            H::GamblingChip => (10, 0.),
            H::Recycle => (11, 0.),
        },
        CombatSelectionReasonV2::Grid(g) => match g {
            G::MoveToDrawPile => (12, 0.),
            G::Exhume { upgrade } => (13, f32::from(*upgrade)),
            G::DrawPileToHand => (14, 0.),
            G::SkillFromDeckToHand => (15, 0.),
            G::AttackFromDeckToHand => (16, 0.),
            G::DiscardToHand => (17, 0.),
            G::DiscardToHandNoCostChange => (18, 0.),
            G::DiscardToHandRetain => (19, 0.),
            G::Omniscience { play_amount } => (20, *play_amount as f32),
        },
        CombatSelectionReasonV2::ScryDiscard => (21, 0.),
    }
}

pub fn encode(
    session: &SearchSession<CombatSearchSpace>,
    source: NodeRef,
) -> Result<EntityPosition, String> {
    let view = session.view(source).map_err(|e| format!("{e:?}"))?;
    encode_choices(view.state, &view.actions, session.space.max_potion_uses())
}

/// Read-only projection of an exact simulator decision. No reconstructed loadout,
/// search tree, RNG advancement or gameplay action is involved. Callers retain
/// the position's producer/source and declare uses already spent in this task.
pub fn encode_position(
    position: sts_core::sim::combat::CombatPosition,
    potion_uses: usize,
    max_potion_uses: Option<usize>,
) -> Result<EntityPosition, String> {
    if max_potion_uses.is_some_and(|max| potion_uses > max) {
        return Err("spent potion uses exceed task allowance".into());
    }
    let mut state = CombatSearchState::new(position)?;
    state.potion_uses = potion_uses;
    let space = max_potion_uses.map_or_else(
        CombatSearchSpace::default,
        CombatSearchSpace::with_potion_limit,
    );
    let choices = space.prepare_choices(&state)?;
    if choices.branches().is_empty() {
        return Err("entity input needs a nonterminal combat decision".into());
    }
    encode_choices(
        &state,
        &choices.branches().iter().collect::<Vec<_>>(),
        max_potion_uses,
    )
}

/// Encode a resident decision without creating or running a search tree.
pub(crate) fn encode_choices(
    state: &CombatSearchState,
    actions: &[&CombatBranch],
    max_potion_uses: Option<usize>,
) -> Result<EntityPosition, String> {
    let o = combat_observation(state)?;
    if o.player_class != "Ironclad"
        || o.stance != StanceId::Neutral
        || !o.orbs.is_empty()
        || o.max_orbs != 0
        || !o.counters.orbs_channeled_this_turn.is_empty()
        || !o.counters.orbs_channeled_this_combat.is_empty()
        || o.counters.mantra_gained_this_combat != 0
    {
        return Err("entity task requires Ironclad without stance or orb mechanics".into());
    }
    let mut p = EntityPosition {
        ids: vec![],
        values: vec![],
        refs: vec![],
        action_ids: vec![],
        actions: vec![],
        action_refs: vec![],
    };
    let mut player = [0.; WIDTH];
    player[..15].copy_from_slice(&[
        o.hp[0] as f32 / 75.,
        o.hp[1] as f32 / 75.,
        o.block as f32 / 50.,
        o.energy as f32 / 3.,
        o.energy_per_turn as f32 / 3.,
        o.turn as f32 / 20.,
        o.gold as f32 / 200.,
        f32::from(o.facing_left),
        o.potion_uses as f32,
        o.ascension as f32 / 20.,
        o.counters.played as f32 / 10.,
        o.counters.attacks as f32 / 10.,
        o.counters.discarded as f32 / 10.,
        o.counters.draw_modifier_next_turn as f32 / 5.,
        o.counters.damaged_this_combat as f32 / 10.,
    ]);
    optional(
        &mut player,
        15,
        o.counters.discovery_cost_for_turn.map(i32::from),
        3.,
    );
    player[17] = f32::from(max_potion_uses.is_some());
    player[18] = max_potion_uses.unwrap_or(0) as f32;
    p.push(1, o.phase as i64, player, [-1; 3]);
    let mut piles = Vec::new();
    for (zone, cards) in [&o.hand, &o.draw_top_first, &o.discard, &o.exhaust, &o.limbo]
        .into_iter()
        .enumerate()
    {
        let mut rows = vec![];
        for (index, c) in cards.iter().enumerate() {
            let row = p.push(
                2,
                c.card.stable_ordinal() as i64,
                card_values(c, zone, index)?,
                [-1; 3],
            );
            cards::encode(&mut p, c, row);
            rows.push(row);
        }
        piles.push(rows);
    }
    let mut monsters = vec![];
    for (index, m) in o.monsters.iter().enumerate() {
        let mut v = [0.; WIDTH];
        v[..8].copy_from_slice(&[
            index as f32 / 5.,
            m.logical_position as f32 / 5.,
            m.hp[0] as f32 / 100.,
            m.hp[1] as f32 / 100.,
            m.block as f32 / 50.,
            f32::from(m.alive),
            f32::from(m.half_dead),
            f32::from(m.escaped),
        ]);
        if let Some(intent) = &m.intent {
            v[8] = 1.;
            optional(&mut v, 9, intent.damage_per_hit, 30.);
            v[11] = intent.hits as f32 / 5.;
            optional(&mut v, 12, intent.total_damage, 50.);
            v[14 + intent.visible_intent as usize] = 1.;
        }
        monsters.push(p.push(3, m.enemy.stable_ordinal() as i64, v, [-1; 3]));
    }
    powers(&mut p, &o.powers, 0, &monsters, &piles)?;
    for (hand, c) in o.hand.iter().enumerate() {
        for (target, damage) in c.damage_by_target.iter().enumerate() {
            let mut v = [0.; WIDTH];
            v[0] = *damage as f32 / 20.;
            p.push(
                13,
                0,
                v,
                [
                    piles[0][hand],
                    *monsters.get(target).ok_or("absent damage preview target")?,
                    -1,
                ],
            );
        }
    }
    for (index, m) in o.monsters.iter().enumerate() {
        powers(&mut p, &m.powers, monsters[index], &monsters, &piles)?;
        effects::encode(&mut p, &m.intent_steps, monsters[index]);
        let decision = m
            .decision
            .as_ref()
            .ok_or("enemy decision state outside entity task")?;
        let owner = [monsters[index], -1, -1];
        for fact in &decision.facts {
            let mut v = [0.; WIDTH];
            v[0] = fact.value as f32 / fact.fact.scale();
            let mut refs = owner;
            if let Some(target) = fact.related_monster {
                refs[1] = *monsters
                    .get(target)
                    .ok_or("absent controller-bound monster")?;
            }
            p.push(14, fact.fact as i64, v, refs);
        }
        let mut current = [0.; WIDTH];
        current[0] = 1.;
        p.push(15, i64::from(decision.planned_move), current, owner);
        for (order, move_id) in decision.roll_history.iter().enumerate() {
            let mut v = [0.; WIDTH];
            v[1] = order as f32 / 32.;
            p.push(15, i64::from(*move_id), v, owner);
        }
    }
    for (index, r) in o.relics.iter().enumerate() {
        let mut v = [0.; WIDTH];
        optional(&mut v, 0, r.counter, 10.);
        v[2] = f32::from(r.used_up);
        v[3] = r.amount as f32 / 10.;
        v[4] = index as f32 / 32.;
        let bound = if let Some(bound) = &r.bound_card {
            let live = bound
                .location
                .as_ref()
                .map(|location| address(location, &piles))
                .transpose()?
                .unwrap_or(-1);
            let row = p.push(
                2,
                bound.card.card.stable_ordinal() as i64,
                card_values(&bound.card, 5, 0)?,
                [0, live, -1],
            );
            cards::encode(&mut p, &bound.card, row);
            row
        } else {
            -1
        };
        p.push(5, r.relic.stable_ordinal() as i64, v, [0, bound, -1]);
    }
    let potions = potions::encode(&mut p, &state.position().combat);
    for (kind, cards) in [
        (10, &o.counters.cards_played_this_turn),
        (11, &o.counters.cards_played_this_combat),
    ] {
        for (i, card) in cards.iter().enumerate() {
            let mut v = [0.; WIDTH];
            v[0] = i as f32 / 32.;
            p.push(kind, card.stable_ordinal() as i64, v, [0, -1, -1]);
        }
    }
    let mut families = vec![];
    let mut members = vec![];
    for (i, s) in o.selections.iter().enumerate() {
        let (category, parameter) = selection_reason(&s.reason);
        let mut v = [0.; WIDTH];
        v[..4].copy_from_slice(&[
            s.min as f32 / 32.,
            s.max as f32 / 32.,
            parameter,
            f32::from(o.assembling.as_ref().is_some_and(|a| a.family == i)),
        ]);
        optional(&mut v, 4, s.source_pile.map(|s| s as i32), 1.);
        let family = p.push(7, category, v, [-1; 3]);
        families.push(family);
        let mut rows = vec![];
        for (j, a) in s.members.iter().enumerate() {
            let mut v = [0.; WIDTH];
            v[0] = j as f32 / 32.;
            if let Some(order) = o
                .assembling
                .as_ref()
                .filter(|a| a.family == i)
                .and_then(|a| a.selected.iter().position(|m| *m == j))
            {
                v[1] = 1.;
                v[2] = order as f32 / 32.;
            }
            rows.push(p.push(8, 0, v, [family, address(a, &piles)?, -1]));
        }
        members.push(rows);
    }
    let mut choice_context = -1;
    if let Some(reason) = &o.indexed_choice_reason {
        let mut v = [0.; WIDTH];
        let kind = match reason {
            CombatIndexedChoiceReasonV2::Discovery {
                colorless,
                card_type,
                amount,
            } => {
                v[0] = f32::from(*colorless);
                optional(&mut v, 1, card_type.map(|t| t as i32), 1.);
                v[3] = *amount as f32;
                0
            }
            CombatIndexedChoiceReasonV2::ChooseOne => 1,
            _ => return Err("indexed choice reason outside entity task".into()),
        };
        choice_context = p.push(12, kind, v, [-1; 3]);
    }
    for choice in model_choices(state, actions)? {
        let mut refs = [-1; 3];
        let mut v = [0.; ACTION_WIDTH];
        let kind = match choice {
            ModelChoice::Play { hand, target } => {
                refs[0] = *piles[0].get(hand).ok_or("absent hand reference")?;
                if let Some(t) = target {
                    refs[1] = *monsters.get(t).ok_or("absent target")?;
                    optional(
                        &mut v,
                        0,
                        o.hand[hand].damage_by_target.get(t).copied(),
                        20.,
                    );
                }
                1
            }
            ModelChoice::EndTurn {} => 2,
            ModelChoice::UsePotion { slot, target } => {
                refs[0] = *potions.get(slot).ok_or("absent potion slot")?;
                if let Some(t) = target {
                    refs[1] = *monsters.get(t).ok_or("absent potion target")?;
                }
                3
            }
            ModelChoice::DiscardPotion { slot } => {
                refs[0] = *potions.get(slot).ok_or("absent potion slot")?;
                4
            }
            ModelChoice::BeginSelection { family } => {
                refs[0] = *families.get(family).ok_or("absent selection family")?;
                5
            }
            ModelChoice::SelectMember { member } => {
                let family = o.assembling.as_ref().ok_or("missing assembly")?.family;
                refs[0] = *members
                    .get(family)
                    .and_then(|m| m.get(member))
                    .ok_or("absent selection member")?;
                6
            }
            ModelChoice::FinishSelection {} => {
                refs[0] = *families
                    .get(o.assembling.as_ref().ok_or("missing assembly")?.family)
                    .ok_or("absent family")?;
                7
            }
            ModelChoice::Choose {
                candidate: CombatIndexedChoiceCandidateV2::Card { card_id, upgrades },
                ..
            } => {
                if choice_context < 0 {
                    return Err("offered card without choice context".into());
                }
                let mut values = [0.; WIDTH];
                values[0] = upgrades as f32;
                cards::card_type(&mut values, card_id);
                refs[0] = p.push(
                    9,
                    card_id.stable_ordinal() as i64,
                    values,
                    [choice_context, -1, -1],
                );
                cards::encode_description(&mut p, card_id, upgrades, refs[0]);
                8
            }
            ModelChoice::Cancel {} => 9,
            ModelChoice::Proceed {} => 10,
            _ => return Err("action outside entity task".into()),
        };
        p.action_ids.push([kind, 0]);
        p.actions.push(v);
        p.action_refs.push(refs);
    }
    if p.ids.iter().chain(&p.action_ids).any(|id| {
        id[0] < 0 || id[0] >= KIND_COUNT as i64 || id[1] < 0 || id[1] >= CATEGORY_COUNT as i64
    }) {
        return Err("entity vocabulary capacity exceeded".into());
    }
    Ok(p)
}
