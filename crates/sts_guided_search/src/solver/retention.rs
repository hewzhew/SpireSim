use super::{Node, SolverStats};
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use sts_core::content::cards::{mechanics::card_persistent_base_numbers_v1, CardId};
use sts_core::content::powers::PowerId;
use sts_core::runtime::combat::{CardLineageId, CombatCard, CombatState};
use sts_core::sim::combat_projection::{
    project_monster_attack_request_in_combat, project_player_turn_end_requests,
    PlayerTurnEndRequests,
};
use sts_core::state::core::EngineState;

pub(super) mod draw_triggers;
pub(super) mod recovery;
use recovery::{ProtectionResponse, ProtectionSource};

type CardNumbers = (i32, i32, i32);
pub(super) const SCORE_LANES: usize = 5;

fn persistent_numbers(card: &CombatCard) -> CardNumbers {
    let base = card_persistent_base_numbers_v1(card);
    (
        base.damage,
        base.block,
        card.combat_cost_without_turn_override_java(),
    )
}

/// Root-relative scheduling context; never state identity or game evidence.
pub(super) struct CardBaseline {
    numbers: HashMap<(CardLineageId, CardId), CardNumbers>,
}

impl CardBaseline {
    pub(super) fn new(combat: &CombatState) -> Self {
        let z = &combat.zones;
        let numbers: HashMap<_, _> = z
            .hand
            .iter()
            .chain(z.draw_pile.iter())
            .chain(z.discard_pile.iter())
            .chain(z.exhaust_pile.iter())
            .chain(z.limbo.iter())
            .map(|c| ((c.lineage, c.id), persistent_numbers(c)))
            .collect();
        Self { numbers }
    }
}

/// Credit one future use of improvements acquired since this search's root.
/// Root upgrades, temporary rendered values, new identities and exhausted cards
/// earn no credit. This partial potential is not a damage forecast: indirect
/// effects, draw timing and repeated/multi-target delivery are not priced here.
/// Setup uses its existing output/energy weights; survival ties reuse this
/// persistent capability without adding an immediate damage/HP exchange rate.
pub(super) fn card_improvements(node: &Node) -> i64 {
    let zones = &node.state.position().combat.zones;
    zones
        .hand
        .iter()
        .chain(zones.draw_pile.iter())
        .chain(zones.discard_pile.iter())
        .chain(zones.limbo.iter())
        .map(|card| {
            let Some(&(old_damage, old_block, old_cost)) =
                node.card_baseline.numbers.get(&(card.lineage, card.id))
            else {
                return 0;
            };
            let (damage, block, cost) = persistent_numbers(card);
            let saved_energy = if old_cost >= 0 && cost >= 0 {
                (old_cost - cost).max(0)
            } else {
                0
            };
            i64::from((damage - old_damage).max(0)) * 3
                + i64::from((block - old_block).max(0)) * 5
                + i64::from(saved_energy) * 4
        })
        .sum()
}

/// One use per available card, reading immediate healing requests from the
/// actual card effects. This conditional reserve is not today's HP, a survival
/// bound, or a forecast of access/energy. Discarded reusable cards participate;
/// exhausted/resolving cards do not. Later Strength and repeated uses are not
/// invented. Only the additional recovery view spends this scheduling credit.
pub(super) fn recovery_reserve(c: &CombatState) -> i64 {
    let z = &c.zones;
    recovery_from(
        c,
        z.hand
            .iter()
            .chain(z.draw_pile.iter())
            .chain(z.discard_pile.iter()),
    )
}

pub(super) fn permitted_revivals(
    node: &Node,
) -> impl Iterator<Item = sts_core::engine::action_handlers::RevivalRequest> + '_ {
    use sts_core::engine::action_handlers::RevivalSource;
    let mut remaining = node.remaining_potion_budget();
    sts_core::engine::action_handlers::available_revival_requests(&node.state.position().combat)
        .take_while(move |request| {
            if matches!(request.source, RevivalSource::FairyPotion { .. }) {
                if let Some(left) = &mut remaining {
                    if *left == 0 {
                        return false;
                    }
                    *left -= 1;
                }
            }
            // Revivals trigger in engine order. A forbidden Fairy also blocks
            // the Tail behind it; a Tail alone has no potion cost.
            true
        })
}

pub(super) fn revival_reserve(node: &Node) -> i64 {
    permitted_revivals(node)
        .map(|r| i64::from(r.healing_from_zero))
        .sum()
}

fn recovery_from<'a>(c: &CombatState, cards: impl Iterator<Item = &'a CombatCard>) -> i64 {
    recovery::reserve_from(c, cards)
}

/// Source-bound current-hand responses, including a fixed kill followed by a
/// payable block. This is a request relation, not a simulated successor. Shared
/// cards/energy and removed healing are accounted for before combining them.
#[cfg(test)]
fn available_protection(node: &Node, uncovered: i64, healing: i64) -> i64 {
    protection_response(node, uncovered, healing, 0).map_or(0, |response| response.credit())
}

fn protection_response(
    node: &Node,
    uncovered: i64,
    healing: i64,
    hp_loss: i64,
) -> Option<ProtectionResponse> {
    let pos = node.state.position();
    if uncovered <= 0 || !matches!(pos.engine, EngineState::CombatPlayerTurn) {
        return None;
    }
    let c = &pos.combat;
    let mut best = available_block(c, uncovered, healing, hp_loss);
    let mut rescue: Option<ProtectionResponse> = None;
    if best
        .as_ref()
        .is_some_and(|response| response.credit() >= uncovered)
    {
        return best;
    }
    for recipe in super::tactics::protection_recipes(node) {
        let target = recipe.target;
        let Some(monster) = c.entities.monsters.iter().find(|m| m.id == target) else {
            continue;
        };
        let pressure = i64::from(
            project_monster_attack_request_in_combat(c, monster).map_or(0, |r| r.total_damage),
        )
        .min(uncovered);
        let mut without_target = c.clone();
        without_target
            .entities
            .monsters
            .iter_mut()
            .find(|m| m.id == target)
            .unwrap()
            .current_hp = 0;
        // A kill can end access to healing, including the removed target's
        // Reaper HP. Future revival targets are not borrowed here, just as in
        // recovery_reserve. A remaining block may only spend the cards and
        // energy left by this specific recipe.
        let lost_healing = (healing - recovery_reserve(&without_target)).max(0);
        let response = ProtectionResponse {
            source: ProtectionSource::RemoveAttacker { target },
            gross_protection: pressure,
            recovery_reserve_lost: lost_healing,
            play_hp_cost: 0,
        };
        if response_value(c, uncovered, healing, Some(&response), hp_loss)
            > response_value(c, uncovered, healing, best.as_ref(), hp_loss)
        {
            best = Some(response);
        }
        if response_value(c, uncovered, healing, best.as_ref(), hp_loss).hp_margin > 0
            || !recipe.direct_only
            || recipe.payment > c.turn.energy
        {
            // Preserve ordinary endurance choices when their current response
            // margin is already positive. Only bridge a remaining physical
            // survival gap. Draws may expose new restrictions, so an attack
            // that draws does not receive this subsequent block request.
            continue;
        }
        // A request layout, never a CombatPosition or an engine successor.
        // Only reviewed direct packets precede the block. Existing counter
        // limits must still constrain its remaining block/funding response.
        // A draw inside the attack prefix may introduce hand restrictions;
        // this request layout cannot observe or settle that draw.
        if c.entities.player.relics.iter().any(|r| {
            r.id == sts_core::content::relics::RelicId::InkBottle
                && r.counter + recipe.cards.len() as i32 >= 10
        }) {
            continue;
        }
        without_target.turn.energy -= recipe.payment;
        without_target
            .zones
            .hand
            .retain(|card| !recipe.cards.contains(&(card.lineage, card.id)));
        let plays = recipe.cards.len() as u8;
        without_target.turn.counters.cards_played_this_turn = without_target
            .turn
            .counters
            .cards_played_this_turn
            .saturating_add(plays);
        for relic in &mut without_target.entities.player.relics {
            if matches!(
                relic.id,
                sts_core::content::relics::RelicId::VelvetChoker
                    | sts_core::content::relics::RelicId::InkBottle
            ) {
                relic.counter += plays as i32;
            }
        }
        for powers in without_target.entities.power_db.values_mut() {
            for power in powers {
                if power.power_type == PowerId::TimeWarp {
                    power.amount += plays as i32;
                }
            }
        }
        let remaining_healing = recovery_reserve(&without_target);
        let Some(block) = available_block(
            &without_target,
            uncovered - pressure,
            remaining_healing,
            hp_loss,
        ) else {
            continue;
        };
        let response = ProtectionResponse {
            source: ProtectionSource::RemoveAttackerAndBlock {
                target,
                attacks: recipe.cards,
                block: Box::new(block.source),
            },
            gross_protection: pressure + block.gross_protection,
            recovery_reserve_lost: lost_healing + block.recovery_reserve_lost,
            play_hp_cost: block.play_hp_cost,
        };
        if response_value(c, uncovered, healing, Some(&response), hp_loss).hp_margin > 0 {
            if rescue.as_ref().is_none_or(|before| {
                response_value(c, uncovered, healing, Some(&response), hp_loss)
                    > response_value(c, uncovered, healing, Some(before), hp_loss)
            }) {
                rescue = Some(response);
            }
        }
    }
    if response_value(c, uncovered, healing, best.as_ref(), hp_loss).hp_margin <= 0 {
        rescue.or(best)
    } else {
        best
    }
}

#[derive(PartialEq, Eq, PartialOrd, Ord)]
struct ResponseValue {
    endurance: i64,
    capacity_health: i64,
    hp_margin: i64,
}

/// Capacity applies after this horizon's pressure/funding requests, not before
/// the attack and not to the inventory itself. A signed non-surviving margin
/// remains visible: conditional healing is never labelled a resurrection.
fn response_value(
    c: &CombatState,
    uncovered: i64,
    healing: i64,
    response: Option<&ProtectionResponse>,
    hp_loss: i64,
) -> ResponseValue {
    let (protection, lost, funding) = response.map_or((0, 0, 0), |r| {
        (r.gross_protection, r.recovery_reserve_lost, r.play_hp_cost)
    });
    let margin =
        i64::from(c.entities.player.current_hp) - hp_loss - uncovered + protection - funding;
    let remaining = (healing - lost).max(0);
    let capacity = (i64::from(c.entities.player.max_hp) - margin.max(0)).max(0);
    // Endurance across later attacks may exceed current HP capacity. The first
    // component ranks that conditional pool; the other two expose the capped
    // health hint and the signed, unborrowed current margin separately.
    ResponseValue {
        endurance: margin + remaining,
        capacity_health: margin + remaining.min(capacity),
        hp_margin: margin,
    }
}

fn available_block(
    c: &CombatState,
    uncovered: i64,
    healing: i64,
    hp_loss: i64,
) -> Option<ProtectionResponse> {
    use sts_core::content::cards::{
        can_play_card, can_play_card_ignoring_energy, effective_target, get_card_definition,
        mechanics::card_base_block_delivery_v1, CardTarget,
    };

    if uncovered <= 0
        || c.entities.monsters.iter().any(|m| {
            m.current_hp > 0
                && (c.get_power(m.id, PowerId::Anger) > 0
                    || c.get_power(m.id, PowerId::TimeWarp) >= 11)
        })
    {
        // Skill-reactive Strength and automatic turn crossings need a response
        // model of their own. Do not price their block as a free protection.
        return None;
    }
    let hand = &c.zones.hand;
    let direct = hand
        .iter()
        .enumerate()
        .filter(|(_, card)| {
            effective_target(card) == CardTarget::SelfTarget
                && card_base_block_delivery_v1(card.id).is_some()
                && can_play_card(card, c).is_ok()
        })
        .filter_map(|(i, _)| protection_request(c, i, None, uncovered, healing))
        .filter(|response| {
            response_value(c, uncovered, healing, Some(response), hp_loss)
                > response_value(c, uncovered, healing, None, hp_loss)
        })
        .max_by_key(|response| response_value(c, uncovered, healing, Some(response), hp_loss));
    if direct
        .as_ref()
        .is_some_and(|response| response.credit() >= uncovered)
        || hand.iter().any(|card| card.id == CardId::Pain)
        || (hand.iter().any(|card| card.id == CardId::Normality)
            && c.turn.counters.cards_played_this_turn >= 2)
        || c.entities
            .player
            .relics
            .iter()
            .any(|r| r.id == sts_core::content::relics::RelicId::VelvetChoker && r.counter >= 5)
        || c.entities.monsters.iter().any(|m| {
            m.current_hp > 0
                && (c.get_power(m.id, PowerId::TimeWarp) >= 10
                    || c.get_power(m.id, PowerId::BeatOfDeath) > 0)
        })
    {
        // A two-card relation needs two remaining plays and does not settle
        // interactions between these hooks. The reviewed one-card costs remain.
        return direct;
    }
    let mut best = direct;
    if let Some(response) = joint_direct_block(c, uncovered) {
        if response_value(c, uncovered, healing, Some(&response), hp_loss)
            > response_value(c, uncovered, healing, best.as_ref(), hp_loss)
        {
            best = Some(response);
        }
        if best.as_ref().is_some_and(|r| r.credit() >= uncovered) {
            return best;
        }
    }
    for (i, source) in hand.iter().enumerate() {
        if !matches!(
            effective_target(source),
            CardTarget::SelfTarget | CardTarget::None
        ) || source.get_cost() < 0
            || can_play_card(source, c).is_err()
        {
            continue;
        }
        let Some((gain, hp_cost)) = protection_funding(c, source) else {
            continue;
        };
        let payment = if source.free_to_play_once
            || (get_card_definition(source.id).card_type
                == sts_core::content::cards::CardType::Skill
                && sts_core::content::powers::store::has_power(
                    c,
                    c.entities.player.id,
                    PowerId::Corruption,
                )) {
            0
        } else {
            i32::from(source.get_cost().max(0))
        };
        let funded = (i32::from(c.turn.energy) - payment + gain).max(0) as u8;
        if funded <= c.turn.energy || hp_cost >= i64::from(c.entities.player.current_hp) {
            continue;
        }
        for (j, card) in hand.iter().enumerate() {
            if i == j
                || effective_target(card) != CardTarget::SelfTarget
                || card_base_block_delivery_v1(card.id).is_none()
                || can_play_card_ignoring_energy(card, c).is_err()
                || can_play_card(card, c).is_ok()
                || card.get_cost() < 0
                || i32::from(card.get_cost()) > i32::from(funded)
            {
                continue;
            }
            let Some(mut response) = protection_request(c, j, Some(i), uncovered, healing) else {
                continue;
            };
            response.play_hp_cost += hp_cost;
            if response_value(c, uncovered, healing, Some(&response), hp_loss)
                > response_value(c, uncovered, healing, best.as_ref(), hp_loss)
            {
                best = Some(response);
            }
        }
    }
    best
}

/// Allocate current energy and remaining plays across independent block-only
/// requests. This is a conditional hand response, not a simulated successor.
/// It never borrows drawn cards, funding or one card more than once. Mixed
/// effects and copying/exhaust-draw hooks remain on the real search path.
fn joint_direct_block(c: &CombatState, uncovered: i64) -> Option<ProtectionResponse> {
    use sts_core::content::cards::{
        can_play_card, effective_target, exhausts_when_played, get_card_definition,
        mechanics::card_base_block_delivery_v1, resolve_card_play, CardTarget, CardType,
    };
    use sts_core::content::{powers::store::has_power, relics::RelicId};
    use sts_core::runtime::action::Action;

    let player = &c.entities.player;
    if [PowerId::DuplicationPower, PowerId::Burst, PowerId::EchoForm]
        .iter()
        .any(|&p| c.get_power(player.id, p) > 0)
    {
        return None;
    }
    let hand = &c.zones.hand;
    let mut plays = hand.len();
    if hand.iter().any(|card| card.id == CardId::Normality) {
        plays = plays.min(3usize.saturating_sub(c.turn.counters.cards_played_this_turn as usize));
    }
    for relic in &player.relics {
        match relic.id {
            RelicId::VelvetChoker => plays = plays.min((6 - relic.counter).max(0) as usize),
            // A last draw may follow the response, but cannot change legality
            // or hand properties between its planned block plays.
            RelicId::InkBottle => plays = plays.min((10 - relic.counter).max(0) as usize),
            _ => {}
        }
    }
    for m in c.entities.monsters.iter().filter(|m| m.current_hp > 0) {
        if has_power(c, m.id, PowerId::TimeWarp) {
            // Do not borrow a play that itself forces the enemy phase.
            plays = plays.min((11 - c.get_power(m.id, PowerId::TimeWarp)).max(0) as usize);
        }
    }
    if plays < 2 {
        return None;
    }
    let energy = usize::from(c.turn.energy);
    let mut requests = vec![];
    for (index, card) in hand.iter().enumerate() {
        if effective_target(card) != CardTarget::SelfTarget
            || !card_base_block_delivery_v1(card.id)
                .is_some_and(|d| d.has_unconditional_base_request())
            || card.get_cost() < 0
            || can_play_card(card, c).is_err()
        {
            continue;
        }
        let corruption = get_card_definition(card.id).card_type == CardType::Skill
            && has_power(c, player.id, PowerId::Corruption);
        if (c.get_power(player.id, PowerId::DarkEmbrace) > 0
            || player.has_relic(RelicId::DeadBranch))
            && (corruption
                || card
                    .exhaust_override
                    .unwrap_or_else(|| exhausts_when_played(card)))
        {
            continue;
        }
        let payment = if card.free_to_play_once || corruption {
            0
        } else {
            card.get_cost() as usize
        };
        let effects = resolve_card_play(card.id, c, card, None);
        let mut block = 0;
        let mut independent = true;
        for effect in effects {
            match effect.action {
                Action::GainBlock { target, amount } if target == player.id => {
                    block += i64::from(amount.max(0));
                }
                _ => {
                    independent = false;
                    break;
                }
            }
        }
        if independent && payment <= energy {
            let block = block + final_exhaust_block(c, card);
            if block > 0 {
                requests.push((index, payment, block));
            }
        }
    }
    plays = plays.min(requests.len());
    if plays < 2 {
        return None;
    }
    let stride = energy + 1;
    let mut dp = vec![None::<(i64, Vec<usize>)>; (plays + 1) * stride];
    dp[0] = Some((0, vec![]));
    for (index, payment, block) in requests {
        for count in (1..=plays).rev() {
            for paid in payment..=energy {
                let Some((previous, cards)) = &dp[(count - 1) * stride + paid - payment] else {
                    continue;
                };
                let total = (previous + block).min(uncovered);
                if dp[count * stride + paid]
                    .as_ref()
                    .is_none_or(|old| total > old.0)
                {
                    let mut cards = cards.clone();
                    cards.push(index);
                    dp[count * stride + paid] = Some((total, cards));
                }
            }
        }
    }
    let (paid, (block, mut cards)) = dp
        .into_iter()
        .enumerate()
        .filter_map(|(i, value)| value.map(|value| (i % stride, value)))
        .filter(|(_, (_, cards))| cards.len() >= 2)
        .max_by_key(|(paid, (block, cards))| {
            (
                *block,
                std::cmp::Reverse(cards.len()),
                std::cmp::Reverse(*paid),
            )
        })?;
    // Free payment does not bypass the engine's original-cost admission.
    // These requests were admitted with current energy; play them before
    // spending that energy on the other block cards.
    cards.sort_by_key(|&i| {
        !(hand[i].free_to_play_once
            || (get_card_definition(hand[i].id).card_type == CardType::Skill
                && has_power(c, player.id, PowerId::Corruption)))
    });
    Some(ProtectionResponse {
        source: ProtectionSource::HandBlockSet {
            cards: cards.into_iter().map(|i| hand[i].lineage).collect(),
            energy_payment: paid,
        },
        gross_protection: block,
        recovery_reserve_lost: 0,
        play_hp_cost: 0,
    })
}

/// Only an unconditional energy request with declared self-HP loss and optional
/// draw. Drawn identities/benefits are never consulted. Discard, status insertion,
/// orb/stance changes and other unpriced funding mechanisms earn no credit.
fn protection_funding(c: &CombatState, card: &CombatCard) -> Option<(i32, i64)> {
    use sts_core::content::cards::resolve_card_play;
    use sts_core::runtime::action::Action;

    let mut energy = 0;
    let mut hp = 0;
    for effect in resolve_card_play(card.id, c, card, None) {
        match effect.action {
            Action::GainEnergy { amount } => energy += amount,
            Action::LoseHp { target, amount, .. } if target == c.entities.player.id => {
                // Gross requested cost is conservative: prevention discounts,
                // self-damage triggers and revival are not borrowed as resources.
                hp += i64::from(amount.max(0));
            }
            Action::DrawCards(_) => {}
            _ => return None,
        }
    }
    (energy > 0).then_some((energy, hp))
}

/// Review only the phase-specific self-damage requests used by a direct
/// current-hand block response. Unknown hook layouts remain unpriced.
fn protection_play_costs(c: &CombatState, card: &CombatCard) -> Option<(i64, i64)> {
    use sts_core::runtime::action::{Action, AddTo, DamageType};

    let owner = c.entities.player.id;
    let mut hp_cost = 0;
    for request in sts_core::content::cards::on_play_card(card, c) {
        match request.action {
            Action::LoseHp { target, amount, .. }
                if target == owner && request.insertion_mode == AddTo::Top =>
            {
                hp_cost += i64::from(amount.max(0));
            }
            _ => return None,
        }
    }
    let mut after_use_damage = 0;
    for request in sts_core::engine::action_handlers::cards::after_use_damage_requests(c) {
        match request {
            Action::Damage(info)
                if info.target == owner && info.damage_type != DamageType::HpLoss =>
            {
                after_use_damage += i64::from(info.output.max(0));
            }
            _ => return None,
        }
    }
    Some((hp_cost, after_use_damage))
}

/// Only the played card's final Exhaust, using the current power hook owner.
/// Triggered draws are not delivered cards or another playable response.
fn final_exhaust_block(c: &CombatState, card: &CombatCard) -> i64 {
    use sts_core::content::cards::{exhausts_when_played, ironclad::corruption};
    use sts_core::content::powers::{resolve_power_on_exhaust, store};
    use sts_core::runtime::action::Action;

    let owner = c.entities.player.id;
    if [PowerId::DuplicationPower, PowerId::Burst, PowerId::EchoForm]
        .iter()
        .any(|&p| c.get_power(owner, p) > 0)
    {
        // Copies can change the ordering before this card's final Exhaust.
        return 0;
    }
    let mut forced = false;
    corruption::corruption_on_use_card(c, card, &mut forced);
    if !forced
        && !card
            .exhaust_override
            .unwrap_or_else(|| exhausts_when_played(card))
    {
        return 0;
    }
    store::powers_for(c, owner)
        .unwrap_or_default()
        .iter()
        .flat_map(|power| {
            resolve_power_on_exhaust(
                power.power_type,
                c,
                owner,
                power.amount,
                card.lineage,
                card.id,
            )
        })
        .filter_map(|action| match action {
            Action::GainBlock { target, amount } if target == owner => {
                Some(i64::from(amount.max(0)))
            }
            _ => None,
        })
        .sum()
}

fn protection_request(
    c: &CombatState,
    index: usize,
    funding: Option<usize>,
    uncovered: i64,
    healing: i64,
) -> Option<ProtectionResponse> {
    use sts_core::content::cards::{get_card_definition, resolve_card_play, CardType};
    use sts_core::runtime::action::Action;

    let hand = &c.zones.hand;
    let card = &hand[index];
    let (mut play_hp_cost, after_use_damage) = protection_play_costs(c, card)?;
    if play_hp_cost >= i64::from(c.entities.player.current_hp) {
        // The pre-effect payment can stop this play before its block exists.
        // Prevention/revival may rescue it in the engine but is not borrowed.
        return None;
    }
    let mut block = 0;
    let mut removed = HashSet::from([index]);
    if let Some(i) = funding {
        removed.insert(i);
    }
    for effect in resolve_card_play(card.id, c, card, None) {
        match effect.action {
            Action::GainBlock { target, amount } if target == c.entities.player.id => {
                block += i64::from(amount.max(0));
            }
            Action::BlockPerNonAttack { block_per_card } => {
                for (j, other) in hand.iter().enumerate() {
                    if !removed.contains(&j)
                        && get_card_definition(other.id).card_type != CardType::Attack
                    {
                        removed.insert(j);
                        block += i64::from(block_per_card.max(0));
                    }
                }
            }
            Action::ExhaustFromHand {
                amount,
                random,
                can_pick_zero: false,
                ..
            } => {
                let mut costs: Vec<_> = hand
                    .iter()
                    .enumerate()
                    .filter(|(j, _)| !removed.contains(j))
                    .map(|(j, other)| (recovery_from(c, std::iter::once(other)), j))
                    .collect();
                costs.sort_unstable();
                if random {
                    costs.reverse();
                }
                // A controlled choice may sacrifice the least healing;
                // a random exhaust receives the worst healing cost.
                for (_, j) in costs.into_iter().take(amount.max(0) as usize) {
                    removed.insert(j);
                }
            }
            _ => {}
        }
    }
    let current_block = i64::from(c.entities.player.block.max(0));
    play_hp_cost += (after_use_damage - current_block - block).max(0);
    if play_hp_cost >= i64::from(c.entities.player.current_hp) {
        // UseCardAfterUseHooks precedes this card's final Exhaust. Its block
        // cannot rescue a death at the preceding damage checkpoint.
        return None;
    }
    let remaining_block = (current_block + block - after_use_damage).max(0);
    let block_delta = remaining_block - current_block + final_exhaust_block(c, card);
    let remaining = recovery_from(
        c,
        hand.iter()
            .enumerate()
            .filter(|(j, _)| !removed.contains(j))
            .map(|(_, card)| card)
            .chain(c.zones.draw_pile.iter())
            .chain(c.zones.discard_pile.iter()),
    );
    Some(ProtectionResponse {
        source: ProtectionSource::HandBlock {
            card: card.lineage,
            funding: funding.map(|index| hand[index].lineage),
        },
        // Known after-use HP loss stays separate from this cap: later Exhaust
        // block cannot retroactively absorb it. Spent existing block is not new
        // protection; final Exhaust and end-turn power block follow the damage.
        gross_protection: block_delta.max(0).min(uncovered),
        recovery_reserve_lost: (healing - remaining).max(0),
        play_hp_cost,
    })
}

/// A scheduling hint, deliberately not an identity or a complete card encoding.
/// Order, lineage, temporary costs and several overrides are omitted here only
/// to diversify representatives. Full-state equality still owns deduplication.
type CardComposition = HashMap<(sts_core::content::cards::CardId, u8, i32, i32, i32, i32), usize>;

fn composition<'a>(
    cards: impl Iterator<Item = &'a sts_core::runtime::combat::CombatCard>,
) -> CardComposition {
    let mut result = HashMap::new();
    for c in cards {
        *result
            .entry((
                c.id,
                c.upgrades,
                c.misc_value,
                c.base_damage_mut,
                c.base_block_mut,
                c.base_magic_num_mut,
            ))
            .or_default() += 1;
    }
    result
}

/// Remaining damage work, including retained Block and mandatory rebirth.
fn remaining_enemy_damage_work(
    c: &CombatState,
    m: &sts_core::runtime::combat::MonsterEntity,
) -> i64 {
    use sts_core::content::{monsters::EnemyId, powers::store::has_power};

    let rebirth = m.enemy_id == EnemyId::AwakenedOne
        && !m.is_dying
        && !m.is_escaped
        && (m.half_dead
            || has_power(c, m.id, PowerId::Unawakened)
            || has_power(c, m.id, PowerId::Regrow));
    // The death owner requires another max-HP phase, even while HP is zero
    // and Unawakened has already been removed. Use power presence (its amount
    // is the -1 sentinel), not a positive stack or a fixed ascension HP value.
    // This measures remaining damage work, not future pressure or a kill bound.
    let retained_block = if m.current_hp > 0
        && !m.is_dying
        && !m.is_escaped
        && has_power(c, m.id, PowerId::Barricade)
    {
        i64::from(m.block.max(0))
    } else {
        0
    };
    // Ordinary monster Block clears before its next action. Barricade Block
    // remains work across turn boundaries, so removing it is actual progress.
    // This scheduling hint does not assume future attacks will hit Block:
    // direct HP loss, Poison and other bypass effects still execute normally.
    i64::from(m.current_hp.max(0))
        + retained_block
        + if rebirth {
            i64::from(m.max_hp.max(0))
        } else {
            0
        }
}

/// Scheduling hints in approximate HP units, not outcome estimates or bounds.
/// Separate offense/defense/setup lanes retain different ways to finish a fight.
fn setup_value(c: &CombatState) -> i64 {
    let player = &c.entities.player;
    let power = |p| i64::from(c.get_power(player.id, p).max(0));
    // Price the portion without a declared end-turn rollback. Immediate
    // attack/block requests still read the full current attributes. Artifact
    // blocking the rollback's application removes that debt in the engine;
    // later cleansing/prevention of an already present debt is not forecast.
    let strength = (power(PowerId::Strength) - power(PowerId::LoseStrength)).max(0);
    let dexterity = (power(PowerId::Dexterity) - power(PowerId::DexterityDown)).max(0);
    let setup = strength * 8
        + dexterity * 8
        + power(PowerId::DemonForm) * 16
        + power(PowerId::Ritual) * 12
        + power(PowerId::Metallicize) * 10
        + power(PowerId::PlatedArmor) * 8
        + power(PowerId::FeelNoPain) * 8
        // The existing draw-credit scale is 16 (Dark Embrace). Brutality
        // requests one extra draw and one HP loss per stack each future turn.
        // This is only a scheduling hint, never borrowed health or a bound.
        + power(PowerId::Brutality) * (16 - 1)
        + power(PowerId::DarkEmbrace) * 16;
    setup
        + if c.get_power(player.id, PowerId::Barricade) != 0 {
            30 + i64::from(player.block.max(0)) * 3
        } else {
            0
        }
}

pub(super) struct Evaluation {
    pub(super) lanes: [i64; SCORE_LANES],
    pub(super) durable: i64,
    /// Damage progress breaks survival/capability ties only. It cannot buy HP.
    pub(super) enemy_progress: i64,
    pub(super) recovery: recovery::RecoveryCredit,
}

pub(super) fn scores(node: &Node) -> [i64; SCORE_LANES] {
    score_components(node).lanes
}

/// One local evaluation shares the effect/card scans used by the numerical
/// lanes and recovery tie key. It is not cached across distinct search states.
fn score_components(node: &Node) -> Evaluation {
    use sts_core::sim::combat::{CombatStepper, CombatTerminal, EngineCombatStepper};
    let c = &node.state.position().combat;
    let player = &c.entities.player;
    let turn_end = if node.state.position().engine == EngineState::CombatPlayerTurn
        && EngineCombatStepper.terminal(node.state.position()) == CombatTerminal::Unresolved
    {
        project_player_turn_end_requests(c)
    } else {
        PlayerTurnEndRequests {
            block_after_pre_card_powers: player.block,
            ..Default::default()
        }
    };
    let hp = (i64::from(player.current_hp) - turn_end.hp_loss) * 10;
    let enemy_hp: i64 = c
        .entities
        .monsters
        .iter()
        .map(|m| remaining_enemy_damage_work(c, m))
        .sum();
    // Fading offers a second route to victory: survive until the enemy leaves.
    // Keep kill progress in the offense lane, but do not make survival lanes
    // race to remove HP that the engine will remove on its own. Damage that
    // reduces the visible attack still earns defense value below (Shifting).
    let persistent_enemy_hp: i64 = c
        .entities
        .monsters
        .iter()
        .filter(|m| c.get_power(m.id, PowerId::Fading) <= 0)
        .map(|m| remaining_enemy_damage_work(c, m))
        .sum();
    let incoming: i64 = c
        .entities
        .monsters
        .iter()
        .filter(|m| m.current_hp > 0)
        .map(|m| {
            i64::from(project_monster_attack_request_in_combat(c, m).map_or(0, |r| r.total_damage))
        })
        .sum();
    // Price visible uncovered damage, not only block already assembled. With
    // zero block the latter cannot distinguish a weakened attack from full
    // damage, nor reward removing an attacker until HP is actually lost.
    // End-turn power block precedes known self-damage packets. Unblockable
    // HP loss stays separate, so it cannot be funded by block or hand responses.
    let pressure = incoming + turn_end.blockable_self_damage;
    let defense = i64::from(turn_end.block_after_pre_card_powers.max(0)).min(pressure) - pressure;
    let energy = i64::from(c.turn.energy) * 4;
    let setup = setup_value(c);
    let persistent = setup * 3 + card_improvements(node);
    let preparation = hp - persistent_enemy_hp * 3 + defense * 5 + energy + persistent;
    let healing = recovery_reserve(c);
    let revival = revival_reserve(node);
    // Current-hand protection uses the same shared payments and restrictions
    // regardless of healing inventory. The response remains conditional: it
    // does not settle draw chains or certify a future safe turn.
    let protection_response = protection_response(node, -defense, healing, turn_end.hp_loss);
    let protection = protection_response
        .as_ref()
        .map_or(0, ProtectionResponse::credit);
    let response = response_value(
        c,
        -defense,
        healing,
        protection_response.as_ref(),
        turn_end.hp_loss,
    );
    let survival = (response.endurance + revival) * 10;
    let lanes = [
        hp - enemy_hp * 6 + defense * 5 + energy + setup,
        hp - persistent_enemy_hp * 2 + defense * 10 + energy + setup,
        preparation,
        hp - persistent_enemy_hp * 3 + defense * 7 + energy + setup
            - node.state.potion_uses() as i64 * 100,
        // Keep recoverable health separate from the other lanes' damage and
        // setup prices. Endurance includes remaining healing/revivals across
        // later damage, not simultaneous HP. Current-capacity health and the
        // physical margin are separately recorded. Access is not forecast.
        // Present-hand protection also matters with no recovery inventory.
        survival,
    ];
    Evaluation {
        lanes,
        durable: persistent,
        // Survival can finish Fading enemies without removing their HP. Only
        // required damage work breaks ties; Shifting still lowers pressure.
        enemy_progress: -persistent_enemy_hp,
        recovery: recovery::RecoveryCredit {
            conditional_reserve: healing,
            revival_reserve: revival,
            visible_attack: incoming,
            turn_end,
            uncovered_attack: -defense,
            conditional_protection: protection,
            requested_hp_after_response: response.hp_margin,
            conditional_health_hint: response.capacity_health,
            conditional_endurance_hint: response.endurance + revival,
            protection_response,
        },
    }
}

pub(super) fn evaluated_scores(
    node: &Node,
    model: Option<&crate::resource_value::ValueModel>,
) -> [i64; SCORE_LANES] {
    evaluate(node, model).lanes
}

pub(super) fn evaluate(
    node: &Node,
    model: Option<&crate::resource_value::ValueModel>,
) -> Evaluation {
    let mut values = score_components(node);
    if let Some(model) = model {
        values.lanes[3] = (model.predict(&node.state) * 1_000_000.).round() as i64;
    }
    values
}

/// Continuation estimates at one completed player-turn boundary. Its HP has
/// already paid the preceding turn's damage; pressure here concerns the next
/// unanswered turn. Observed HP is exported separately from these estimates.
pub(super) fn boundary_ranks(
    node: &Node,
    model: Option<&crate::resource_value::ValueModel>,
) -> [(i64, i64, i64); SCORE_LANES] {
    let values = evaluate(node, model);
    std::array::from_fn(|lane| {
        if lane == SCORE_LANES - 1 {
            (values.lanes[lane], values.durable, values.enemy_progress)
        } else {
            (values.lanes[lane], 0, 0)
        }
    })
}

/// Survival is an independent objective. Persistent capability then damage
/// progress break its ties; neither can compensate for losing health here.
pub(super) fn lane_rank_key(
    values: &[i64; SCORE_LANES],
    lane: usize,
    durable: i64,
    enemy_progress: i64,
    inputs: usize,
    ordinal: usize,
) -> (
    std::cmp::Reverse<i64>,
    std::cmp::Reverse<i64>,
    std::cmp::Reverse<i64>,
    usize,
    usize,
) {
    (
        std::cmp::Reverse(values[lane]),
        std::cmp::Reverse(if lane == SCORE_LANES - 1 { durable } else { 0 }),
        std::cmp::Reverse(if lane == SCORE_LANES - 1 {
            enemy_progress
        } else {
            0
        }),
        inputs,
        ordinal,
    )
}

/// Diversity only. Equal contexts are never treated as equal combat states.
/// Distinguish options that can change the next play: actual hand attributes,
/// energy, effects, enemy phases and relic counters. HP, block and enemy HP
/// still compete by scores; pile order and RNG variants share representative
/// slots, but keep their exact states and replayable traces.
fn same_context(a: &Node, b: &Node, effects: bool) -> bool {
    let a_pos = a.state.position();
    let b_pos = b.state.position();
    // Distinct enemy-removal progress deserves a continuation opportunity:
    // finishing an attacker can score below damage spread over living foes.
    // This is a bounded diversity preference, never state equivalence.
    if !a_pos
        .combat
        .entities
        .monsters
        .iter()
        .map(|m| (m.id, m.current_hp > 0, m.half_dead, m.is_escaped))
        .eq(b_pos
            .combat
            .entities
            .monsters
            .iter()
            .map(|m| (m.id, m.current_hp > 0, m.half_dead, m.is_escaped)))
    {
        return false;
    }
    if !a_pos
        .combat
        .entities
        .potions
        .iter()
        .map(|p| p.as_ref().map(|p| p.id))
        .eq(b_pos
            .combat
            .entities
            .potions
            .iter()
            .map(|p| p.as_ref().map(|p| p.id)))
    {
        return false;
    }
    if !effects {
        return true;
    }
    let same_powers = |a_id, b_id| {
        let a = sts_core::content::powers::store::powers_for(&a_pos.combat, a_id).unwrap_or(&[]);
        let b = sts_core::content::powers::store::powers_for(&b_pos.combat, b_id).unwrap_or(&[]);
        a.iter()
            .map(|p| {
                (
                    p.power_type,
                    p.amount,
                    p.extra_data,
                    p.just_applied,
                    &p.payload,
                )
            })
            .eq(b.iter().map(|p| {
                (
                    p.power_type,
                    p.amount,
                    p.extra_data,
                    p.just_applied,
                    &p.payload,
                )
            }))
    };
    let choice_kind = |e: &EngineState| match e {
        EngineState::PendingChoice(p) => Some(std::mem::discriminant(p)),
        _ => None,
    };
    same_powers(
        a_pos.combat.entities.player.id,
        b_pos.combat.entities.player.id,
    ) && a_pos
        .combat
        .entities
        .monsters
        .iter()
        .zip(&b_pos.combat.entities.monsters)
        .all(|(a, b)| a.move_state == b.move_state && same_powers(a.id, b.id))
        && a_pos.combat.entities.player.relics == b_pos.combat.entities.player.relics
        && a_pos.combat.entities.player.stance == b_pos.combat.entities.player.stance
        && a_pos.combat.entities.player.facing_left == b_pos.combat.entities.player.facing_left
        && a_pos.combat.turn.energy == b_pos.combat.turn.energy
        && choice_kind(&a_pos.engine) == choice_kind(&b_pos.engine)
        && a.state.selected_members().map(<[_]>::len) == b.state.selected_members().map(<[_]>::len)
}

/// A budget-sharing hint, never an equivalence or transposition key. Attribute
/// hashes omit lineage and hand order so repeated starter permutations cannot
/// occupy every representative slot before a different hand gets a followup.
/// Costs and all card mutations matter even when card IDs coincide. Hash
/// collisions only affect scheduling; exact-state deduplication remains above.
fn hand_context(node: &Node) -> Vec<u64> {
    use sts_core::runtime::combat::CombatCard;
    let mut result = node
        .state
        .position()
        .combat
        .zones
        .hand
        .iter()
        .map(|card| {
            let CombatCard {
                id,
                lineage: _,
                upgrades,
                misc_value,
                base_damage_override,
                base_block_override,
                cost_modifier,
                cost_for_turn,
                base_damage_mut,
                base_block_mut,
                base_magic_num_mut,
                multi_damage,
                exhaust_override,
                retain_override,
                free_to_play_once,
                energy_on_use,
            } = card;
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            (
                id,
                upgrades,
                misc_value,
                base_damage_override,
                base_block_override,
                cost_modifier,
                cost_for_turn,
                base_damage_mut,
            )
                .hash(&mut hash);
            (
                base_block_mut,
                base_magic_num_mut,
                multi_damage,
                exhaust_override,
                retain_override,
                free_to_play_once,
                energy_on_use,
            )
                .hash(&mut hash);
            hash.finish()
        })
        .collect::<Vec<_>>();
    result.sort_unstable();
    result
}

fn pile_composition(node: &Node) -> (CardComposition, CardComposition) {
    let zones = &node.state.position().combat.zones;
    (
        composition(
            zones
                .hand
                .iter()
                .chain(zones.draw_pile.iter())
                .chain(zones.discard_pile.iter())
                .chain(zones.limbo.iter()),
        ),
        composition(zones.exhaust_pile.iter()),
    )
}

/// Diagnostic view of the exact same representative grouping used below.
/// Sharing a group does not establish equal states or equal continuation value.
pub(super) fn same_retention_context(
    a: &Node,
    b: &Node,
    effects: bool,
    card_compositions: bool,
) -> bool {
    same_context(a, b, effects)
        && (!effects || hand_context(a) == hand_context(b))
        && (!card_compositions || pile_composition(a) == pile_composition(b))
}

#[cfg(test)]
pub(super) fn retain(
    pool: Vec<Node>,
    width: usize,
    stats: &mut SolverStats,
    effects: bool,
) -> Vec<Node> {
    partition(pool, width, stats, effects, false, None, None).0
}

pub(super) fn partition(
    pool: Vec<Node>,
    width: usize,
    stats: &mut SolverStats,
    effects: bool,
    card_compositions: bool,
    model: Option<&crate::resource_value::ValueModel>,
    guide: Option<super::graph::RetentionGuide<'_>>,
) -> (Vec<Node>, Vec<Node>) {
    let mut unique: Vec<Node> = Vec::with_capacity(pool.len());
    let mut index_keys = Vec::with_capacity(pool.len());
    let mut index: HashMap<u64, Vec<usize>> = HashMap::new();
    for node in pool {
        let key = node.index_key();
        let bucket = index.entry(key).or_default();
        if bucket.iter().any(|&i| unique[i].state == node.state) {
            stats.exact_duplicates += 1;
        } else {
            bucket.push(unique.len());
            index_keys.push(key);
            unique.push(node);
        }
    }
    // Scheduling keeps first occurrence and insertion order, independently of
    // hash-map iteration order. The index holds no additional combat states.
    drop(index);
    if unique.len() <= width {
        return (unique, vec![]);
    }
    let values = unique
        .iter()
        .map(|n| evaluate(n, model))
        .collect::<Vec<_>>();
    if model.is_some() {
        stats.resource_evaluations += unique.len();
    }
    let compositions =
        card_compositions.then(|| unique.iter().map(pile_composition).collect::<Vec<_>>());
    let hands = effects.then(|| unique.iter().map(hand_context).collect::<Vec<_>>());
    let mut lanes = Vec::new();
    let backed = guide.map_or_else(Vec::new, |guide| {
        guide.nominations(&unique, &index_keys, stats)
    });
    let guided = !backed.is_empty();
    if guided {
        lanes.push(backed);
    }
    for lane in 0..SCORE_LANES {
        let mut order = (0..unique.len()).collect::<Vec<_>>();
        order.sort_by_key(|&i| {
            lane_rank_key(
                &values[i].lanes,
                lane,
                values[i].durable,
                values[i].enemy_progress,
                unique[i].input_count,
                i,
            )
        });
        if let Some(guide) = guide {
            guide.order_by_boundaries(&unique, &index_keys, &mut order, stats);
        }
        lanes.push(order);
    }
    let mut selected = Vec::with_capacity(width);
    let mut seen = HashSet::new();
    // Give distinct decisions a representative before more order/RNG variants.
    // All score lanes participate; none alone owns the diversity budget.
    // Any remaining slots still explore variants with their original states.
    // The disabled diagnostic mode retains the earlier half-width allocation.
    let mut contexts = Vec::new();
    'contexts: for rank in 0..unique.len() {
        for (lane_index, lane) in lanes[..if effects {
            lanes.len()
        } else {
            1 + usize::from(guided)
        }]
            .iter()
            .enumerate()
        {
            let Some(&i) = lane.get(rank) else {
                continue;
            };
            if contexts.iter().all(|&j| {
                !same_context(&unique[i], &unique[j], effects)
                    || hands.as_ref().is_some_and(|h| h[i] != h[j])
                    || compositions.as_ref().is_some_and(|c| c[i] != c[j])
            }) {
                contexts.push(i);
                selected.push(i);
                seen.insert(i);
                stats.backed_retention_selected += usize::from(guided && lane_index == 0);
                if selected.len() >= if effects { width } else { width.div_ceil(2) } {
                    break 'contexts;
                }
            }
        }
    }
    for rank in 0..unique.len() {
        for (lane_index, lane) in lanes.iter().enumerate() {
            let Some(&i) = lane.get(rank) else {
                continue;
            };
            if selected.len() < width && seen.insert(i) {
                selected.push(i);
                stats.backed_retention_selected += usize::from(guided && lane_index == 0);
            }
        }
        if selected.len() == width {
            break;
        }
    }
    stats.beam_dropped += unique.len() - selected.len();
    let mut nodes = unique.into_iter().map(Some).collect::<Vec<_>>();
    let kept = selected
        .into_iter()
        .map(|i| nodes[i].take().unwrap())
        .collect();
    (kept, nodes.into_iter().flatten().collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::CombatSearchState;
    use sts_core::content::monsters::EnemyId;
    use sts_core::sim::combat::CombatPosition;

    #[test]
    fn recurring_draw_preparation_keeps_actual_curiosity_and_hp_costs() {
        use sts_core::content::cards::CardId;
        use sts_core::runtime::combat::{CombatCard, Power};
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.block = 50;
        c.entities.monsters = vec![sts_core::test_support::planned_monster(
            EnemyId::AwakenedOne,
            1,
        )];
        c.entities.monsters[0].current_hp = 320;
        let enemy = c.entities.monsters[0].id;
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            enemy,
            vec![Power {
                power_type: PowerId::Curiosity,
                amount: 2,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        c.zones.hand = vec![CombatCard::for_scenario(CardId::Brutality, 90)];
        c.zones.draw_pile = (100..108)
            .map(|id| CombatCard::for_scenario(CardId::Defend, id))
            .collect();
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, c);
        let limits = CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        };
        let played = EngineCombatStepper.apply_to_stable(
            &root,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            limits,
        );
        assert!(!played.truncated && !played.timed_out);
        assert_eq!(
            played.position.combat.get_power(enemy, PowerId::Strength),
            2
        );
        let before = score_components(&Node::root(CombatSearchState::new(root.clone()).unwrap()));
        let after = score_components(&Node::root(
            CombatSearchState::new(played.position.clone()).unwrap(),
        ));
        assert!(
            after.lanes[2] > before.lanes[2],
            "recurring draw is real preparation despite Curiosity"
        );
        assert!(
            after.recovery.requested_hp_after_response
                <= before.recovery.requested_hp_after_response,
            "draw credit cannot buy physical survival margin"
        );
        let ordinary = EngineCombatStepper.apply_to_stable(&root, ClientInput::EndTurn, limits);
        let extra_draw =
            EngineCombatStepper.apply_to_stable(&played.position, ClientInput::EndTurn, limits);
        assert!(!ordinary.truncated && !extra_draw.truncated);
        assert_eq!(
            extra_draw.position.combat.zones.hand.len(),
            ordinary.position.combat.zones.hand.len() + 1
        );
        assert_eq!(
            extra_draw.position.combat.entities.player.current_hp,
            ordinary.position.combat.entities.player.current_hp - 1
        );
    }

    #[test]
    fn removing_retained_monster_block_is_progress_before_hp_damage() {
        use sts_core::runtime::combat::Power;
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.monsters = vec![sts_core::test_support::planned_monster(
            EnemyId::SphericGuardian,
            1,
        )];
        c.entities.monsters[0].current_hp = 20;
        c.entities.monsters[0].block = 40;
        let target = c.entities.monsters[0].id;
        // A temporary shield can be waited out and earns no persistent work.
        sts_core::content::powers::store::set_powers_for(&mut c, target, vec![]);
        assert_eq!(remaining_enemy_damage_work(&c, &c.entities.monsters[0]), 20);
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            target,
            vec![Power {
                power_type: PowerId::Barricade,
                amount: -1,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        c.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 101)];
        assert_eq!(remaining_enemy_damage_work(&c, &c.entities.monsters[0]), 60);
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, c);
        let before = score_components(&Node::root(
            CombatSearchState::new(position.clone()).unwrap(),
        ));
        let step = EngineCombatStepper.apply_to_stable(
            &position,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(target),
            },
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!step.truncated && !step.timed_out);
        let mut after_combat = step.position.combat.clone();
        assert_eq!(after_combat.entities.monsters[0].current_hp, 20);
        assert_eq!(after_combat.entities.monsters[0].block, 34);
        assert_eq!(
            remaining_enemy_damage_work(&after_combat, &after_combat.entities.monsters[0]),
            54
        );
        let after = score_components(&Node::root(CombatSearchState::new(step.position).unwrap()));
        assert!(
            after.lanes[0] > before.lanes[0],
            "shield damage earns offense progress despite its Energy payment"
        );
        assert_eq!(
            after.lanes[4], before.lanes[4],
            "damage progress cannot buy health in the survival lane"
        );
        assert!(after.enemy_progress > before.enemy_progress);
        after_combat.entities.monsters[0].current_hp = 0;
        after_combat.entities.monsters[0].is_dying = true;
        assert_eq!(
            remaining_enemy_damage_work(&after_combat, &after_combat.entities.monsters[0]),
            0
        );
    }

    #[test]
    fn final_exhaust_protection_matches_native_phase_costs_without_borrowing_draws() {
        use sts_core::runtime::combat::{Power, PowerPayload};
        use sts_core::sim::combat::{
            CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
        };
        use sts_core::state::core::ClientInput;

        let limits = CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        };
        use CardId::{Defend, Impervious};
        for (deployment, powers, pressure, result) in [
            ((true, Defend, None), (3, 0, 0, 0), (7, 10), (10, 8, 10)),
            ((false, Defend, None), (3, 0, 0, 0), (7, 10), (10, 5, 8)),
            (
                (true, Defend, Some(false)),
                (3, 0, 2, 0),
                (7, 10),
                (10, 6, 9),
            ),
            ((true, Defend, None), (3, 0, 2, 0), (10, 10), (10, 6, 6)),
            ((true, Defend, None), (10, -5, 2, 0), (7, 10), (8, 10, 8)),
            ((true, Defend, None), (10, -5, 2, 1), (7, 10), (9, 10, 9)),
            ((true, Defend, None), (10, -5, 2, 0), (7, 1), (0, 0, 0)),
            (
                (false, Impervious, None),
                (3, 0, 2, 0),
                (32, 10),
                (10, 31, 9),
            ),
            (
                (false, Impervious, Some(false)),
                (3, 0, 2, 0),
                (32, 10),
                (10, 28, 6),
            ),
            (
                (false, Impervious, None),
                (0, 0, 2, 0),
                (32, 10),
                (10, 28, 6),
            ),
        ] {
            let (corruption, id, override_exhaust) = deployment;
            let (fnp, dex, beat, old_block) = powers;
            let (attack, hp) = pressure;
            let (played_hp, played_block, settled_hp) = result;
            let mut c = sts_core::test_support::blank_test_combat();
            c.entities.player.current_hp = hp;
            c.entities.player.max_hp = 75;
            c.entities.player.block = old_block;
            c.entities.monsters =
                vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
            set_joint_block_powers(
                &mut c,
                &[
                    (PowerId::FeelNoPain, fnp),
                    (PowerId::Dexterity, dex),
                    (PowerId::DarkEmbrace, 1),
                ],
            );
            let mut card = CombatCard::for_scenario(id, 900);
            card.exhaust_override = override_exhaust;
            c.zones.hand = vec![card];
            c.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Defend, 901)].into();
            if fnp == 0 {
                // Holding the capability is not deploying it.
                c.zones
                    .hand
                    .push(CombatCard::for_scenario(CardId::FeelNoPain, 902));
            }
            c.turn.energy = if corruption { 3 } else { 2 };
            let mut position = CombatPosition::new(EngineState::CombatPlayerTurn, c);
            if corruption {
                position
                    .combat
                    .zones
                    .hand
                    .insert(0, CombatCard::for_scenario(CardId::Corruption, 903));
                // Pay for actual deployment so Skill costs/overrides use the
                // native power application, not an injected Corruption flag.
                let deployed = EngineCombatStepper.apply_to_stable(
                    &position,
                    ClientInput::PlayCard {
                        card_index: 0,
                        target: None,
                    },
                    limits,
                );
                assert!(!deployed.truncated && !deployed.timed_out);
                position = deployed.position;
                assert_eq!(position.combat.turn.energy, 0);
            }
            // Explicit synthetic pressure, installed after the deployment.
            sts_core::content::powers::store::set_powers_for(
                &mut position.combat,
                1,
                [
                    (PowerId::Strength, attack - 11),
                    (PowerId::BeatOfDeath, beat),
                ]
                .into_iter()
                .map(|(power_type, amount)| Power {
                    power_type,
                    amount,
                    instance_id: None,
                    extra_data: 0,
                    just_applied: false,
                    payload: PowerPayload::None,
                })
                .collect(),
            );
            let node = Node::root(CombatSearchState::new(position.clone()).unwrap());
            let evaluation = score_components(&node);
            assert_eq!(node.state.position(), &position);
            assert_eq!(evaluation.recovery.visible_attack, i64::from(attack));
            let played = EngineCombatStepper.apply_to_stable(
                &position,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: None,
                },
                limits,
            );
            assert!(!played.truncated && !played.timed_out);
            assert_eq!(played.position.combat.entities.player.current_hp, played_hp);
            assert_eq!(played.position.combat.entities.player.block, played_block);
            let settled = if EngineCombatStepper.terminal(&played.position) == CombatTerminal::Loss
            {
                played
            } else {
                EngineCombatStepper.apply_to_stable(&played.position, ClientInput::EndTurn, limits)
            };
            assert!(!settled.truncated && !settled.timed_out);
            assert_eq!(
                settled.position.combat.entities.player.current_hp,
                settled_hp
            );
            if settled_hp > 0 {
                assert_eq!(
                    evaluation.recovery.requested_hp_after_response,
                    i64::from(settled_hp),
                    "{corruption:?}/{id:?}/{override_exhaust:?}: {:?}",
                    evaluation.recovery
                );
            } else {
                assert!(evaluation.recovery.requested_hp_after_response <= 0);
                assert_eq!(evaluation.recovery.conditional_protection, 0);
            }
        }

        // Without exhaust-draw/generation, two free Skills remain independent
        // requests. Count each card's own final hook once, even at zero raw block.
        for (dex, hp, attack, played_block) in [(0, 10, 13, 16), (-5, 1, 5, 6)] {
            let mut c = sts_core::test_support::blank_test_combat();
            c.entities.player.current_hp = hp;
            c.entities.player.max_hp = 75;
            c.entities.monsters =
                vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
            set_joint_block_powers(
                &mut c,
                &[(PowerId::FeelNoPain, 3), (PowerId::Dexterity, dex)],
            );
            sts_core::content::powers::store::set_powers_for(
                &mut c,
                1,
                vec![Power {
                    power_type: PowerId::Strength,
                    amount: attack - 11,
                    instance_id: None,
                    extra_data: 0,
                    just_applied: false,
                    payload: PowerPayload::None,
                }],
            );
            c.turn.energy = 3;
            c.zones.hand = [CardId::Corruption, CardId::Defend, CardId::Defend]
                .into_iter()
                .enumerate()
                .map(|(i, id)| CombatCard::for_scenario(id, 910 + i as u32))
                .collect();
            let initial = CombatPosition::new(EngineState::CombatPlayerTurn, c);
            let play = ClientInput::PlayCard {
                card_index: 0,
                target: None,
            };
            let deployed = EngineCombatStepper.apply_to_stable(&initial, play.clone(), limits);
            assert!(!deployed.truncated && !deployed.timed_out);
            let root = deployed.position;
            assert_eq!(root.combat.turn.energy, 0);
            let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
            let evaluation = score_components(&node);
            assert_eq!(node.state.position(), &root);
            assert_eq!(
                evaluation.recovery.requested_hp_after_response,
                i64::from(hp)
            );
            assert!(matches!(
                evaluation.recovery.protection_response.as_ref().unwrap().source,
                ProtectionSource::HandBlockSet { ref cards, energy_payment: 0 } if cards.len() == 2
            ));
            let mut position = root;
            for _ in 0..2 {
                let step = EngineCombatStepper.apply_to_stable(&position, play.clone(), limits);
                assert!(!step.truncated && !step.timed_out);
                position = step.position;
            }
            assert_eq!(position.combat.entities.player.block, played_block);
            let settled =
                EngineCombatStepper.apply_to_stable(&position, ClientInput::EndTurn, limits);
            assert!(!settled.truncated && !settled.timed_out);
            assert_eq!(settled.position.combat.entities.player.current_hp, hp);
        }
    }

    #[test]
    fn protection_prices_pre_effect_hp_loss_and_after_use_block_consumption() {
        use sts_core::runtime::combat::{Power, PowerPayload};
        use sts_core::sim::combat::{
            CombatStepLimits, CombatStepper, CombatTerminal, EngineCombatStepper,
        };
        use sts_core::state::core::ClientInput;

        for (hp, pain, beat, upgraded, metallicize, played_hp, played_block, settled_hp) in [
            (10, true, 0, false, 0, 9, 5, 9),
            (1, true, 0, false, 0, 0, 0, 0),
            (10, false, 2, false, 0, 10, 3, 8),
            (1, false, 2, false, 0, 1, 3, 0),
            (10, false, 2, true, 0, 10, 6, 10),
            (10, false, 2, false, 3, 10, 3, 10),
        ] {
            let mut c = sts_core::test_support::blank_test_combat();
            c.entities.player.current_hp = hp;
            c.entities.player.max_hp = 75;
            c.entities.monsters =
                vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
            sts_core::content::powers::store::set_powers_for(
                &mut c,
                1,
                [(PowerId::Strength, -6), (PowerId::BeatOfDeath, beat)]
                    .into_iter()
                    .map(|(power_type, amount)| Power {
                        power_type,
                        amount,
                        instance_id: None,
                        extra_data: 0,
                        just_applied: false,
                        payload: PowerPayload::None,
                    })
                    .collect(),
            );
            set_joint_block_powers(&mut c, &[(PowerId::Metallicize, metallicize)]);
            c.turn.energy = 1;
            let mut defend = CombatCard::for_scenario(CardId::Defend, 900);
            defend.upgrades = u8::from(upgraded);
            c.zones.hand = vec![defend];
            if pain {
                c.zones
                    .hand
                    .push(CombatCard::for_scenario(CardId::Pain, 901));
            }
            let position = CombatPosition::new(EngineState::CombatPlayerTurn, c);
            let node = Node::root(CombatSearchState::new(position.clone()).unwrap());
            let evaluation = score_components(&node);
            assert_eq!(
                node.state.position(),
                &position,
                "scoring cannot run the action"
            );
            assert_eq!(evaluation.recovery.visible_attack, 5);

            let limits = CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            };
            let played = EngineCombatStepper.apply_to_stable(
                &position,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: None,
                },
                limits,
            );
            assert!(!played.truncated && !played.timed_out);
            assert_eq!(played.position.combat.entities.player.current_hp, played_hp);
            assert_eq!(played.position.combat.entities.player.block, played_block);
            let settled = if EngineCombatStepper.terminal(&played.position) == CombatTerminal::Loss
            {
                played
            } else {
                EngineCombatStepper.apply_to_stable(&played.position, ClientInput::EndTurn, limits)
            };
            assert!(!settled.truncated && !settled.timed_out);
            assert_eq!(
                settled.position.combat.entities.player.current_hp,
                settled_hp
            );
            if settled_hp > 0 {
                assert_eq!(
                    evaluation.recovery.requested_hp_after_response,
                    i64::from(settled_hp)
                );
            } else {
                assert!(
                    evaluation.recovery.requested_hp_after_response <= 0,
                    "known cast costs cannot label a lethal response as positive physical margin"
                );
                if pain {
                    assert_eq!(evaluation.recovery.conditional_protection, 0);
                }
            }
        }
    }

    #[test]
    fn turn_end_power_block_changes_survival_and_matches_guardian_response() {
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 75;
        c.entities.player.max_hp = 75;
        c.entities.player.block = 13;
        c.entities.monsters = vec![sts_core::test_support::planned_monster(
            EnemyId::TheGuardian,
            4,
        )];
        for metallicize in [0, 4] {
            set_joint_block_powers(&mut c, &[(PowerId::Metallicize, metallicize)]);
            let position = CombatPosition::new(EngineState::CombatPlayerTurn, c.clone());
            let evaluation = score_components(&Node::root(
                CombatSearchState::new(position.clone()).unwrap(),
            ));
            assert_eq!(evaluation.recovery.visible_attack, 16);
            let expected_hp = if metallicize == 0 { 72 } else { 75 };
            assert_eq!(evaluation.recovery.requested_hp_after_response, expected_hp);
            assert_eq!(evaluation.lanes[4], expected_hp * 10);
            let ended = EngineCombatStepper.apply_to_stable(
                &position,
                ClientInput::EndTurn,
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!ended.truncated && !ended.timed_out);
            assert_eq!(
                i64::from(ended.position.combat.entities.player.current_hp),
                expected_hp
            );
        }
    }

    #[test]
    fn unblockable_turn_end_loss_cannot_be_paid_by_automatic_or_hand_block() {
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 75;
        c.entities.player.max_hp = 75;
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::Cultist, 3)];
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::Regret, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Wound, 3),
        ];
        c.zones
            .draw_pile
            .push(CombatCard::for_scenario(CardId::BandageUp, 4));
        for block in [0, 100] {
            set_joint_block_powers(&mut c, &[(PowerId::Metallicize, block)]);
            let node = Node::root(
                CombatSearchState::new(CombatPosition::new(
                    EngineState::CombatPlayerTurn,
                    c.clone(),
                ))
                .unwrap(),
            );
            let evaluation = score_components(&node);
            assert_eq!(evaluation.recovery.turn_end.hp_loss, 3);
            assert_eq!(evaluation.recovery.uncovered_attack, 0);
            assert_eq!(evaluation.recovery.conditional_protection, 0);
            assert!(evaluation.recovery.protection_response.is_none());
            assert_eq!(evaluation.recovery.requested_hp_after_response, 72);
            // The same unavoidable cost also stays outside a response to a
            // separate blockable request, even in recovery context.
            let response = protection_response(&node, 5, 0, 3).unwrap();
            assert_eq!(response_value(&c, 5, 0, Some(&response), 3).hp_margin, 72);
        }
    }

    #[test]
    fn settled_turn_end_choice_and_victory_do_not_repeat_turn_end_requests() {
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::Cultist, 3)];
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::Regret, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ];
        set_joint_block_powers(
            &mut c,
            &[(PowerId::Metallicize, 4), (PowerId::RetainCards, 1)],
        );
        let position = CombatPosition::new(EngineState::CombatPlayerTurn, c);
        let ended = EngineCombatStepper.apply_to_stable(
            &position,
            ClientInput::EndTurn,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!ended.truncated && !ended.timed_out);
        assert!(matches!(
            ended.position.engine,
            EngineState::PendingChoice(_)
        ));
        assert_eq!(ended.position.combat.entities.player.current_hp, 78);
        let evaluate =
            |position| score_components(&Node::root(CombatSearchState::new(position).unwrap()));
        let pending = evaluate(ended.position.clone());
        assert_eq!(pending.recovery.turn_end.hp_loss, 0);
        assert_eq!(pending.recovery.turn_end.block_after_pre_card_powers, 4);
        assert_eq!(pending.recovery.requested_hp_after_response, 78);

        let mut won = position;
        won.combat.entities.monsters[0].current_hp = 0;
        won.combat.entities.monsters[0].is_dying = true;
        won.engine = EngineState::CombatProcessing;
        assert_eq!(
            EngineCombatStepper.terminal(&won),
            sts_core::sim::combat::CombatTerminal::Unresolved
        );
        assert!(!sts_core::engine::core::tick_engine(
            &mut won.engine,
            &mut won.combat,
            None
        ));
        assert_eq!(
            EngineCombatStepper.terminal(&won),
            sts_core::sim::combat::CombatTerminal::Win
        );
        let victory = evaluate(won);
        assert_eq!(victory.recovery.turn_end.hp_loss, 0);
        assert_eq!(victory.recovery.turn_end.block_after_pre_card_powers, 0);
        assert_eq!(victory.recovery.requested_hp_after_response, 80);
    }

    #[test]
    fn survival_ties_do_not_race_fading_enemy_hp() {
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.entities.monsters = vec![sts_core::test_support::planned_monster(
            EnemyId::Transient,
            1,
        )];
        combat.entities.monsters[0].current_hp = 999;
        combat.entities.monsters[0].max_hp = 999;
        let target = combat.entities.monsters[0].id;
        sts_core::content::powers::store::set_powers_for(
            &mut combat,
            target,
            vec![sts_core::runtime::combat::Power {
                power_type: PowerId::Fading,
                amount: 5,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let evaluate_combat = |combat| {
            evaluate(
                &Node::root(
                    CombatSearchState::new(CombatPosition::new(
                        EngineState::CombatPlayerTurn,
                        combat,
                    ))
                    .unwrap(),
                ),
                None,
            )
        };
        let before = evaluate_combat(combat.clone());
        combat.entities.monsters[0].current_hp -= 100;
        let after = evaluate_combat(combat);
        assert!(
            after.lanes[0] > before.lanes[0],
            "offense keeps kill progress"
        );
        assert_eq!(
            after.recovery.visible_attack,
            before.recovery.visible_attack
        );
        assert_eq!(after.lanes[4], before.lanes[4]);
        assert_eq!(after.enemy_progress, before.enemy_progress);
    }

    fn joint_block_fixture() -> CombatState {
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        c.turn.energy = 2;
        c.zones.hand = (0..3)
            .map(|i| {
                let mut card = CombatCard::for_scenario(CardId::Defend, 800 + i);
                card.upgrades = 1;
                card
            })
            .collect();
        set_joint_block_powers(&mut c, &[(PowerId::Dexterity, 5)]);
        c
    }

    fn set_joint_block_powers(c: &mut CombatState, powers: &[(PowerId, i32)]) {
        use sts_core::runtime::combat::{Power, PowerPayload};
        sts_core::content::powers::store::set_powers_for(
            c,
            0,
            powers
                .iter()
                .map(|&(power_type, amount)| Power {
                    power_type,
                    amount,
                    extra_data: 0,
                    just_applied: false,
                    payload: PowerPayload::None,
                    instance_id: None,
                })
                .collect(),
        );
    }

    fn replay_joint_block(c: &CombatState, response: &ProtectionResponse) -> CombatState {
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let ProtectionSource::HandBlockSet {
            cards,
            energy_payment,
        } = &response.source
        else {
            panic!("expected shared hand response");
        };
        let mut position = CombatPosition::new(EngineState::CombatPlayerTurn, c.clone());
        for lineage in cards {
            let card_index = position
                .combat
                .zones
                .hand
                .iter()
                .position(|card| card.lineage == *lineage)
                .unwrap();
            let step = EngineCombatStepper.apply_to_stable(
                &position,
                ClientInput::PlayCard {
                    card_index,
                    target: None,
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            position = step.position;
            assert_eq!(position.engine, EngineState::CombatPlayerTurn);
        }
        assert_eq!(
            usize::from(c.turn.energy - position.combat.turn.energy),
            *energy_payment
        );
        assert!(
            i64::from(position.combat.entities.player.block - c.entities.player.block)
                >= response.gross_protection
        );
        position.combat
    }

    #[test]
    fn joint_block_shares_energy_and_matches_real_modified_block() {
        let mut c = joint_block_fixture();
        let unchanged = c.clone();
        let response = available_block(&c, 25, 0, 0).unwrap();
        assert_eq!(response.gross_protection, 25);
        assert_eq!(replay_joint_block(&c, &response).entities.player.block, 26);
        assert_eq!(c, unchanged, "request valuation does not execute cards");
        c.turn.energy = 1;
        assert!(joint_direct_block(&c, 100).is_none());
        assert_eq!(available_block(&c, 100, 0, 0).unwrap().gross_protection, 13);
        c.turn.energy = 2;
        set_joint_block_powers(&mut c, &[(PowerId::Dexterity, 5), (PowerId::Frail, 1)]);
        let response = joint_direct_block(&c, 100).unwrap();
        assert_eq!(response.gross_protection, 18);
        replay_joint_block(&c, &response);
        set_joint_block_powers(&mut c, &[(PowerId::NoBlock, 1)]);
        assert!(joint_direct_block(&c, 100).is_none());
    }

    #[test]
    fn joint_block_free_payment_never_reuses_a_card_or_borrows_admission() {
        let mut c = joint_block_fixture();
        c.turn.energy = 1;
        c.zones.hand[1].free_to_play_once = true;
        let response = joint_direct_block(&c, 100).unwrap();
        assert_eq!(response.gross_protection, 26);
        assert_eq!(replay_joint_block(&c, &response).turn.energy, 0);
        c.turn.energy = 0;
        assert!(
            joint_direct_block(&c, 100).is_none(),
            "engine still checks original cost"
        );
        for card in &mut c.zones.hand {
            card.cost_for_turn = Some(0);
        }
        let response = joint_direct_block(&c, 100).unwrap();
        assert_eq!(
            response.gross_protection, 39,
            "three distinct zero-cost cards, once each"
        );
        assert_eq!(
            replay_joint_block(&c, &response).zones.discard_pile.len(),
            3
        );
        c.turn.energy = 1;
        for card in &mut c.zones.hand {
            card.cost_for_turn = None;
        }
        set_joint_block_powers(&mut c, &[(PowerId::Dexterity, 5), (PowerId::Corruption, 1)]);
        let response = joint_direct_block(&c, 100).unwrap();
        let played = replay_joint_block(&c, &response);
        assert_eq!(played.turn.energy, 1);
        assert_eq!(played.zones.exhaust_pile.len(), 3);
    }

    #[test]
    fn joint_block_respects_shared_play_limits_and_draw_boundaries() {
        use sts_core::content::relics::{RelicId, RelicState};
        use sts_core::runtime::combat::{Power, PowerPayload};
        for constraint in 0..4 {
            let mut c = joint_block_fixture();
            c.turn.energy = 3;
            match constraint {
                0 => {
                    c.zones
                        .hand
                        .push(CombatCard::for_scenario(CardId::Normality, 900));
                    c.turn.counters.cards_played_this_turn = 1;
                }
                1 | 2 => {
                    let mut relic = RelicState::new(if constraint == 1 {
                        RelicId::VelvetChoker
                    } else {
                        RelicId::InkBottle
                    });
                    relic.counter = if constraint == 1 { 4 } else { 8 };
                    c.entities.player.add_relic(relic);
                    c.zones
                        .draw_pile
                        .push(CombatCard::for_scenario(CardId::Normality, 901));
                }
                _ => sts_core::content::powers::store::set_powers_for(
                    &mut c,
                    1,
                    vec![Power {
                        power_type: PowerId::TimeWarp,
                        amount: 9,
                        extra_data: 0,
                        just_applied: false,
                        payload: PowerPayload::None,
                        instance_id: None,
                    }],
                ),
            }
            let response = available_block(&c, 100, 0, 0).unwrap();
            assert_eq!(response.gross_protection, 26);
            replay_joint_block(&c, &response);
            match constraint {
                0 => c.turn.counters.cards_played_this_turn += 1,
                1 | 2 => c.entities.player.relics.last_mut().unwrap().counter += 1,
                _ => sts_core::content::powers::store::set_powers_for(
                    &mut c,
                    1,
                    vec![Power {
                        power_type: PowerId::TimeWarp,
                        amount: 10,
                        extra_data: 0,
                        just_applied: false,
                        payload: PowerPayload::None,
                        instance_id: None,
                    }],
                ),
            }
            assert!(joint_direct_block(&c, 100).is_none());
        }
    }

    #[test]
    fn joint_block_after_a_kill_debits_the_prefix_ink_bottle_plays() {
        use sts_core::content::relics::{RelicId, RelicState};
        use sts_core::runtime::combat::{Power, PowerPayload};
        let mut c = joint_block_fixture();
        c.entities.player.current_hp = 10;
        c.turn.energy = 3;
        c.zones.hand.truncate(2);
        c.zones
            .hand
            .push(CombatCard::for_scenario(CardId::Strike, 903));
        c.entities.monsters[0].current_hp = 6;
        let mut other = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        other.id = 2;
        other.current_hp = 100;
        c.entities.monsters.push(other);
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            2,
            vec![Power {
                power_type: PowerId::Strength,
                amount: 20,
                extra_data: 0,
                just_applied: false,
                payload: PowerPayload::None,
                instance_id: None,
            }],
        );
        let incoming: i64 = c
            .entities
            .monsters
            .iter()
            .map(|m| {
                i64::from(
                    project_monster_attack_request_in_combat(&c, m)
                        .unwrap()
                        .total_damage,
                )
            })
            .sum();
        let mut bottle = RelicState::new(RelicId::InkBottle);
        bottle.counter = 7;
        c.entities.player.add_relic(bottle);
        for (counter, has_rescue) in [(7, true), (8, false), (9, false)] {
            c.entities.player.relics.last_mut().unwrap().counter = counter;
            let node = Node::root(
                CombatSearchState::new(CombatPosition::new(
                    EngineState::CombatPlayerTurn,
                    c.clone(),
                ))
                .unwrap(),
            );
            let response = protection_response(&node, incoming, 0, 0).unwrap();
            assert_eq!(
                matches!(
                    response.source,
                    ProtectionSource::RemoveAttackerAndBlock { .. }
                ),
                has_rescue
            );
            if has_rescue {
                assert!(response_value(&c, incoming, 0, Some(&response), 0).hp_margin > 0);
            }
        }
    }

    #[test]
    fn joint_block_leaves_mixed_effects_and_exhaust_draw_to_real_search() {
        use sts_core::content::relics::{RelicId, RelicState};
        let mut c = joint_block_fixture();
        c.zones.hand.truncate(1);
        c.zones
            .hand
            .push(CombatCard::for_scenario(CardId::ShrugItOff, 901));
        c.zones
            .hand
            .push(CombatCard::for_scenario(CardId::TrueGrit, 902));
        c.turn.energy = 3;
        assert!(joint_direct_block(&c, 100).is_none());
        c = joint_block_fixture();
        for (power, amount) in [
            (PowerId::DarkEmbrace, 1),
            (PowerId::Burst, 1),
            (PowerId::DuplicationPower, 1),
            (PowerId::EchoForm, 1),
        ] {
            set_joint_block_powers(&mut c, &[(PowerId::Corruption, 1), (power, amount)]);
            assert!(joint_direct_block(&c, 100).is_none());
        }
        set_joint_block_powers(&mut c, &[(PowerId::Corruption, 1)]);
        c.entities
            .player
            .add_relic(RelicState::new(RelicId::DeadBranch));
        assert!(joint_direct_block(&c, 100).is_none());
    }

    #[test]
    fn temporary_attributes_keep_current_effects_without_permanent_setup_credit() {
        use sts_core::content::{
            potions::{Potion, PotionId},
            powers::store,
        };
        use sts_core::runtime::combat::{Power, PowerPayload};
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let limits = CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        };
        for (potion, stat, debt) in [
            (
                PotionId::SpeedPotion,
                PowerId::Dexterity,
                PowerId::DexterityDown,
            ),
            (
                PotionId::SteroidPotion,
                PowerId::Strength,
                PowerId::LoseStrength,
            ),
        ] {
            for artifact in [false, true] {
                let mut c = sts_core::test_support::blank_test_combat();
                c.entities.monsters =
                    vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
                c.entities.monsters[0].current_hp = 100;
                c.entities.monsters[0].max_hp = 100;
                c.entities.potions = vec![Some(Potion::new(potion, 1))];
                c.zones.hand = vec![
                    CombatCard::for_scenario(CardId::Defend, 1),
                    CombatCard::for_scenario(CardId::Strike, 2),
                ];
                if artifact {
                    store::set_powers_for(
                        &mut c,
                        0,
                        vec![Power {
                            power_type: PowerId::Artifact,
                            amount: 1,
                            extra_data: 0,
                            just_applied: false,
                            payload: PowerPayload::None,
                            instance_id: None,
                        }],
                    );
                }
                let before = CombatPosition::new(EngineState::CombatPlayerTurn, c);
                let drunk = EngineCombatStepper
                    .apply_to_stable(
                        &before,
                        ClientInput::UsePotion {
                            potion_index: 0,
                            target: None,
                        },
                        limits,
                    )
                    .position;
                assert_eq!(drunk.combat.get_power(0, stat), 5);
                assert_eq!(
                    drunk.combat.get_power(0, debt),
                    if artifact { 0 } else { 5 }
                );
                assert_eq!(setup_value(&drunk.combat), if artifact { 40 } else { 0 });
                let index = usize::from(stat == PowerId::Strength);
                let played = EngineCombatStepper
                    .apply_to_stable(
                        &drunk,
                        ClientInput::PlayCard {
                            card_index: index,
                            target: (stat == PowerId::Strength).then_some(1),
                        },
                        limits,
                    )
                    .position;
                if stat == PowerId::Dexterity {
                    assert_eq!(
                        played.combat.entities.player.block - drunk.combat.entities.player.block,
                        10
                    );
                } else {
                    assert_eq!(
                        drunk.combat.entities.monsters[0].current_hp
                            - played.combat.entities.monsters[0].current_hp,
                        11
                    );
                }
                let ended = EngineCombatStepper
                    .apply_to_stable(&played, ClientInput::EndTurn, limits)
                    .position;
                assert_eq!(
                    ended.combat.get_power(0, stat),
                    if artifact { 5 } else { 0 }
                );
            }
        }
    }

    #[test]
    fn retention_pressure_uses_current_facing_instead_of_cached_intent() {
        use sts_core::content::powers::store;
        use sts_core::runtime::combat::{Intent, Power, PowerPayload};
        let mut c = sts_core::test_support::blank_test_combat();
        let mut shield = sts_core::test_support::planned_monster(EnemyId::SpireShield, 1);
        shield.logical_position = -1;
        let target = shield.id;
        c.entities.monsters = vec![shield];
        store::set_powers_for(
            &mut c,
            0,
            vec![Power {
                power_type: PowerId::Surrounded,
                amount: -1,
                extra_data: 0,
                just_applied: false,
                payload: PowerPayload::None,
                instance_id: None,
            }],
        );
        c.set_monster_protocol_visible_intent(
            target,
            Intent::Attack {
                damage: 99,
                hits: 1,
            },
        );
        c.set_monster_protocol_preview_damage_per_hit(target, 99);
        let evaluate = |c: &CombatState| {
            score_components(&Node::root(
                CombatSearchState::new(CombatPosition::new(
                    EngineState::CombatPlayerTurn,
                    c.clone(),
                ))
                .unwrap(),
            ))
        };
        let back = evaluate(&c);
        c.entities.player.facing_left = true;
        let front = evaluate(&c);
        assert!(front.recovery.visible_attack < back.recovery.visible_attack);
        assert!(front.lanes[1] > back.lanes[1]);
        assert_eq!(
            front.recovery.visible_attack,
            i64::from(
                project_monster_attack_request_in_combat(&c, &c.entities.monsters[0])
                    .unwrap()
                    .total_damage
            )
        );
        assert_eq!(c.monster_protocol_preview_damage_per_hit(target), 99);
    }

    #[test]
    fn revival_credit_obeys_budget_and_engine_trigger_order() {
        use sts_core::content::{
            potions::{Potion, PotionId},
            relics::{RelicId, RelicState},
        };
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.max_hp = 68;
        c.entities.player.current_hp = 10;
        c.entities
            .player
            .add_relic(RelicState::new(RelicId::LizardTail));
        c.entities.potions = vec![
            Some(Potion::new(PotionId::FairyPotion, 101)),
            Some(Potion::new(PotionId::FairyPotion, 102)),
        ];
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        let root = Node::root(
            CombatSearchState::new(CombatPosition::new(EngineState::CombatPlayerTurn, c)).unwrap(),
        );
        for (budget, expected) in [(None, 74), (Some(0), 0), (Some(1), 20), (Some(2), 74)] {
            let node = root.clone().with_potion_limit(budget);
            let values = score_components(&node);
            assert_eq!(values.recovery.revival_reserve, expected);
            let profile = recovery::profile(&node, values.recovery);
            assert_eq!(
                profile
                    .revivals
                    .iter()
                    .map(|r| i64::from(r.healing_from_zero))
                    .sum::<i64>(),
                expected
            );
            assert_eq!(profile.potion_budget_remaining, budget);
        }
        let mut position = root.state.position().clone();
        position.combat.entities.potions.clear();
        let tail = Node::root(CombatSearchState::new(position).unwrap());
        assert_eq!(revival_reserve(&tail.with_potion_limit(Some(0))), 34);
    }

    #[test]
    fn passive_and_explicit_potion_loss_reduce_revival_credit_once() {
        use sts_core::content::potions::{Potion, PotionId};
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.max_hp = 68;
        c.entities.player.current_hp = 1;
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        c.entities.monsters[0].current_hp = 100;
        c.entities.potions = vec![
            Some(Potion::new(PotionId::FairyPotion, 101)),
            Some(Potion::new(PotionId::FairyPotion, 102)),
        ];
        let root = Node::root(
            CombatSearchState::new(CombatPosition::new(EngineState::CombatPlayerTurn, c)).unwrap(),
        )
        .with_potion_limit(Some(1));
        let mut position = root.state.position().clone();
        position.combat.entities.player.current_hp = 0;
        sts_core::engine::action_handlers::try_revive(&mut position.combat);
        assert_eq!(position.combat.entities.player.current_hp, 20);
        let mut spent = root.clone();
        spent.state = CombatSearchState::new(position).unwrap();
        assert_eq!(spent.remaining_potion_budget(), Some(0));
        assert_eq!(spent.expenditures().explicit_count(), 0);
        assert_eq!(revival_reserve(&spent), 0);

        let input = ClientInput::DiscardPotion(0);
        let step = EngineCombatStepper.apply_to_stable(
            root.state.position(),
            input.clone(),
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!step.truncated && !step.timed_out);
        let mut discarded = root.clone();
        discarded.state = CombatSearchState::new(step.position).unwrap();
        discarded.trace = Some(std::sync::Arc::new(crate::solver::Trace {
            previous: None, expenditures: sts_combat_outcome::combat_witness_materialization::CombatInputExpendituresV1::from_inputs(std::slice::from_ref(&input)), input,
        }));
        assert_eq!(discarded.remaining_potion_budget(), Some(0));
        assert_eq!(revival_reserve(&discarded), 0);
        assert_eq!(revival_reserve(&discarded.with_potion_limit(Some(2))), 20);
    }

    #[test]
    fn revival_inventory_keeps_endurance_without_inventing_current_hp() {
        use sts_core::content::relics::{RelicId, RelicState};
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.max_hp = 68;
        c.entities.player.current_hp = 10;
        c.entities
            .player
            .add_relic(RelicState::new(RelicId::LizardTail));
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        let mut node = Node::root(
            CombatSearchState::new(CombatPosition::new(
                EngineState::CombatPlayerTurn,
                c.clone(),
            ))
            .unwrap(),
        );
        let before = score_components(&node);
        assert_eq!(before.recovery.revival_reserve, 34);
        assert_eq!(
            before.recovery.requested_hp_after_response,
            10 - before.recovery.uncovered_attack
        );
        assert_eq!(
            before.recovery.conditional_health_hint,
            before.recovery.requested_hp_after_response
        );
        assert_eq!(
            before.recovery.conditional_endurance_hint,
            before.recovery.requested_hp_after_response + 34
        );

        c.entities.player.current_hp = 0;
        sts_core::engine::action_handlers::try_revive(&mut c);
        assert_eq!(c.entities.player.current_hp, 34);
        node.state =
            CombatSearchState::new(CombatPosition::new(EngineState::CombatPlayerTurn, c)).unwrap();
        let spent = score_components(&node);
        assert_eq!(spent.recovery.revival_reserve, 0);
        assert!(
            before.lanes[4] > spent.lanes[4],
            "revival's HP reset consumes endurance instead of creating free banked HP"
        );
    }

    #[test]
    fn recovery_reads_real_healing_effects_and_does_not_mutate_the_source() {
        use sts_core::content::relics::{RelicId, RelicState};
        use sts_core::runtime::combat::{CombatCard, Power, PowerPayload};
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;

        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 20;
        c.turn.energy = 3;
        let mut first = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        first.id = 1;
        first.current_hp = 40;
        first.block = 5;
        let mut second = first.clone();
        second.id = 2;
        second.current_hp = 3;
        second.block = 0;
        c.entities.monsters = vec![first, second];
        c.entities
            .player
            .add_relic(RelicState::new(RelicId::MagicFlower));
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            0,
            vec![Power {
                power_type: PowerId::Strength,
                amount: 4,
                extra_data: 0,
                just_applied: false,
                payload: PowerPayload::None,
                instance_id: None,
            }],
        );
        c.zones.hand = vec![CombatCard::for_scenario(CardId::Reaper, 100)];
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, c.clone());
        let reserve = recovery_reserve(&c);
        let played = EngineCombatStepper.apply_to_stable(
            &root,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!played.truncated && !played.timed_out);
        assert_eq!(reserve, 9, "actual damage caps, then Magic Flower");
        assert_eq!(
            reserve,
            i64::from(played.position.combat.entities.player.current_hp - 20)
        );
        assert_eq!(
            recovery_reserve(&played.position.combat),
            0,
            "Reaper was exhausted"
        );
        assert_eq!(
            root.combat, c,
            "valuation never applies an engine transition"
        );

        let before_bloom = c.clone();
        c.entities
            .player
            .add_relic(RelicState::new(RelicId::MarkOfTheBloom));
        assert_eq!(recovery_reserve(&c), 0);
        c = before_bloom;
        c.zones.hand.clear();
        c.zones
            .discard_pile
            .push(CombatCard::for_scenario(CardId::Bite, 101));
        c.zones
            .draw_pile
            .push(CombatCard::for_scenario(CardId::BandageUp, 102));
        assert_eq!(
            recovery_reserve(&c),
            9,
            "Bite and Bandage Up effects, once each"
        );
        c.zones.exhaust_pile.push(c.zones.draw_pile.pop().unwrap());
        assert_eq!(recovery_reserve(&c), 3);
        c.entities
            .monsters
            .iter_mut()
            .for_each(|m| m.current_hp = 0);
        assert_eq!(
            recovery_reserve(&c),
            0,
            "rebirth boundary has no current attack target"
        );

        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 20;
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        c.turn.energy = 1;
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::Defend, 201),
            CombatCard::for_scenario(CardId::BandageUp, 202),
        ];
        let protection = |combat: &CombatState| {
            let node = Node::root(
                CombatSearchState::new(CombatPosition::new(
                    EngineState::CombatPlayerTurn,
                    combat.clone(),
                ))
                .unwrap(),
            );
            available_protection(&node, 10, recovery_reserve(combat))
        };
        assert_eq!(protection(&c), 5);
        c.turn.energy = 0;
        assert_eq!(protection(&c), 0, "unaffordable block is not a reserve");
        c.turn.energy = 1;
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            0,
            vec![Power {
                power_type: PowerId::NoBlock,
                amount: 1,
                extra_data: 0,
                just_applied: false,
                payload: PowerPayload::None,
                instance_id: None,
            }],
        );
        assert_eq!(protection(&c), 0, "read the real NoBlock modifier");
        sts_core::content::powers::store::set_powers_for(&mut c, 0, vec![]);
        let mut reactive = Power {
            power_type: PowerId::Anger,
            amount: 1,
            extra_data: 0,
            just_applied: false,
            payload: PowerPayload::None,
            instance_id: None,
        };
        let monster = c.entities.monsters[0].id;
        sts_core::content::powers::store::set_powers_for(&mut c, monster, vec![reactive.clone()]);
        assert_eq!(protection(&c), 0, "skill-reactive Strength is not free");
        reactive.power_type = PowerId::TimeWarp;
        reactive.amount = 11;
        sts_core::content::powers::store::set_powers_for(&mut c, monster, vec![reactive.clone()]);
        assert_eq!(
            protection(&c),
            0,
            "a turn-crossing action is not this horizon"
        );
        reactive.amount = 10;
        sts_core::content::powers::store::set_powers_for(&mut c, monster, vec![reactive]);
        assert_eq!(protection(&c), 5);
        sts_core::content::powers::store::set_powers_for(&mut c, monster, vec![]);
        let mut wind = CombatCard::for_scenario(CardId::SecondWind, 203);
        wind.upgrades = 1;
        c.zones.hand = vec![
            wind,
            CombatCard::for_scenario(CardId::BandageUp, 204),
            CombatCard::for_scenario(CardId::Reaper, 205),
        ];
        let before = c.clone();
        let reserve = recovery_reserve(&c);
        let gain = protection(&c);
        let played = EngineCombatStepper.apply_to_stable(
            &CombatPosition::new(EngineState::CombatPlayerTurn, c.clone()),
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!played.truncated && !played.timed_out);
        assert_eq!(played.position.combat.entities.player.block, 7);
        assert_eq!(gain, 3, "7 block pays the exhausted Bandage Up's 4 HP");
        assert_eq!(
            gain,
            i64::from(played.position.combat.entities.player.block)
                + recovery_reserve(&played.position.combat)
                - reserve
        );
        assert_eq!(c, before, "hand potential does not mutate the position");
    }

    #[test]
    fn funded_protection_pays_hp_and_needs_a_replayable_two_play_opportunity() {
        use sts_core::content::relics::{RelicId, RelicState};
        use sts_core::runtime::combat::{Power, PowerPayload};
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;

        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 75;
        c.entities.player.max_hp = 75;
        c.entities.monsters = vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        c.turn.energy = 0;
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::Offering, 301),
            CombatCard::for_scenario(CardId::Impervious, 302),
        ];
        let protection = |combat: &CombatState| {
            let node = Node::root(
                CombatSearchState::new(CombatPosition::new(
                    EngineState::CombatPlayerTurn,
                    combat.clone(),
                ))
                .unwrap(),
            );
            available_protection(&node, 16, recovery_reserve(combat))
        };
        let before = c.clone();
        let gain = protection(&c);
        let response = protection_response(
            &Node::root(
                CombatSearchState::new(CombatPosition::new(
                    EngineState::CombatPlayerTurn,
                    c.clone(),
                ))
                .unwrap(),
            ),
            16,
            recovery_reserve(&c),
            0,
        )
        .unwrap();
        assert_eq!(response.gross_protection, 16);
        assert_eq!(response.play_hp_cost, 6);
        assert_eq!(response.recovery_reserve_lost, 0);
        assert!(
            matches!(response.source, ProtectionSource::HandBlock { card, funding: Some(funding) }
            if card == c.zones.hand[1].lineage && funding == c.zones.hand[0].lineage)
        );
        let mut played = CombatPosition::new(EngineState::CombatPlayerTurn, c.clone());
        for _ in 0..2 {
            let step = EngineCombatStepper.apply_to_stable(
                &played,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: None,
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            played = step.position;
        }
        assert_eq!(played.combat.turn.energy, 0);
        assert_eq!(played.combat.entities.player.current_hp, 69);
        assert_eq!(played.combat.entities.player.block, 30);
        let evaluation = score_components(&Node::root(
            CombatSearchState::new(CombatPosition::new(
                EngineState::CombatPlayerTurn,
                c.clone(),
            ))
            .unwrap(),
        ));
        assert_eq!(evaluation.recovery.conditional_reserve, 0);
        let ended = EngineCombatStepper.apply_to_stable(
            &played,
            ClientInput::EndTurn,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!ended.truncated && !ended.timed_out);
        assert_eq!(
            evaluation.recovery.requested_hp_after_response,
            i64::from(ended.position.combat.entities.player.current_hp),
            "funded protection pays its HP cost even without a healing card"
        );
        assert_eq!(evaluation.lanes[4], 690);
        assert_eq!(gain, 10, "16 covered pressure pays 6 HP to get the energy");
        assert_eq!(
            gain,
            i64::from(played.combat.entities.player.block.min(16))
                + i64::from(
                    played.combat.entities.player.current_hp - c.entities.player.current_hp
                )
        );
        assert_eq!(c, before, "no hidden successor or RNG work in the score");
        c.entities.player.current_hp = 6;
        assert_eq!(protection(&c), 0, "cannot borrow HP after lethal funding");
        let lethal = score_components(&Node::root(
            CombatSearchState::new(CombatPosition::new(
                EngineState::CombatPlayerTurn,
                c.clone(),
            ))
            .unwrap(),
        ));
        assert_eq!(lethal.recovery.conditional_protection, 0);
        assert!(lethal.recovery.requested_hp_after_response < 0);
        c.entities.player.current_hp = 75;
        c.zones.hand[1] = CombatCard::for_scenario(CardId::Defend, 303);
        assert_eq!(
            protection(&c),
            0,
            "paying 6 HP for 5 block has no net protection"
        );
        c.zones.hand[0] = CombatCard::for_scenario(CardId::Bloodletting, 304);
        assert_eq!(protection(&c), 2, "read another producer's 3 HP cost");
        c.zones.hand[0] = CombatCard::for_scenario(CardId::SeeingRed, 305);
        assert_eq!(
            protection(&c),
            0,
            "cannot finance a producer's own initial cost"
        );
        c.turn.energy = 1;
        c.zones.hand[1] = CombatCard::for_scenario(CardId::Impervious, 311);
        assert_eq!(
            protection(&c),
            16,
            "pay 1 to gain 2; exactly 2 remain for block"
        );
        c.turn.energy = 0;
        c.zones.hand[1] = CombatCard::for_scenario(CardId::Defend, 303);
        c.zones.hand[0].upgrades = 1;
        assert_eq!(
            protection(&c),
            5,
            "zero-cost producer funds current-hand defense"
        );
        c.zones.hand[1] = CombatCard::for_scenario(CardId::Strike, 306);
        c.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Impervious, 307)].into();
        c.zones.hand[0] = CombatCard::for_scenario(CardId::Offering, 308);
        assert_eq!(
            protection(&c),
            0,
            "never inspect identities that funding might draw"
        );
        c.zones.draw_pile.clear();
        c.zones.hand[1] = CombatCard::for_scenario(CardId::Impervious, 309);

        let monster = c.entities.monsters[0].id;
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            monster,
            vec![Power {
                power_type: PowerId::TimeWarp,
                amount: 10,
                extra_data: 0,
                just_applied: false,
                payload: PowerPayload::None,
                instance_id: None,
            }],
        );
        assert_eq!(protection(&c), 0, "two plays would cross the turn boundary");
        c.turn.energy = 2;
        assert_eq!(protection(&c), 16, "one ordinary play is still possible");
        c.turn.energy = 0;
        sts_core::content::powers::store::set_powers_for(&mut c, monster, vec![]);
        c.zones
            .hand
            .push(CombatCard::for_scenario(CardId::Normality, 310));
        c.turn.counters.cards_played_this_turn = 2;
        assert_eq!(protection(&c), 0, "Normality leaves only one play");
        c.turn.counters.cards_played_this_turn = 1;
        assert_eq!(protection(&c), 10);
        c.zones.hand.pop();
        let mut choker = RelicState::new(RelicId::VelvetChoker);
        choker.counter = 5;
        c.entities.player.add_relic(choker);
        assert_eq!(protection(&c), 0, "Choker leaves only one play");

        // The producer leaves hand before Second Wind counts non-Attacks.
        // Replay that interaction; do not spend its card twice as exhaust fuel.
        let mut c = before;
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::SeeingRed, 312),
            CombatCard::for_scenario(CardId::SecondWind, 313),
            CombatCard::for_scenario(CardId::BandageUp, 314),
        ];
        c.zones.hand[0].upgrades = 1;
        c.zones.hand[1].upgrades = 1;
        let initial_healing = recovery_reserve(&c);
        let gain = protection(&c);
        let mut played = CombatPosition::new(EngineState::CombatPlayerTurn, c.clone());
        for _ in 0..2 {
            let step = EngineCombatStepper.apply_to_stable(
                &played,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: None,
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            played = step.position;
        }
        assert_eq!(played.combat.entities.player.block, 7);
        assert_eq!(gain, 3, "7 block pays the consumed 4 HP healing reserve");
        assert_eq!(
            gain,
            i64::from(played.combat.entities.player.block) + recovery_reserve(&played.combat)
                - initial_healing
        );
    }

    #[test]
    fn protection_keeps_physical_pressure_and_lost_recovery_separate() {
        use sts_core::content::monsters::EnemyId;
        use sts_core::runtime::combat::{CombatCard, Power};
        use sts_core::sim::combat::{
            CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
        };
        use sts_core::state::core::ClientInput;

        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 18;
        c.turn.energy = 4;
        let mut attacker = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        attacker.id = 1;
        attacker.current_hp = 34;
        attacker.max_hp = 34;
        let mut other = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        other.id = 2;
        other.current_hp = 100;
        other.max_hp = 100;
        c.entities.monsters = vec![attacker, other];
        for (owner, strength) in [(0, 4), (1, 15), (2, 8)] {
            sts_core::content::powers::store::set_powers_for(
                &mut c,
                owner,
                vec![Power {
                    power_type: PowerId::Strength,
                    amount: strength,
                    instance_id: None,
                    extra_data: 0,
                    payload: Default::default(),
                    just_applied: false,
                }],
            );
        }
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::ShrugItOff, 701),
            CombatCard::for_scenario(CardId::HeavyBlade, 702),
            CombatCard::for_scenario(CardId::Strike, 703),
        ];
        c.zones.hand[0].upgrades = 1;
        c.zones.hand[1].upgrades = 1;
        c.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Reaper, 704)].into();
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, c);
        let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let credit = evaluate(&node, None).recovery;
        assert_eq!(credit.visible_attack, 45);
        assert_eq!(credit.conditional_reserve, 16);
        assert_eq!(credit.conditional_protection, 29);
        assert_eq!(credit.requested_hp_after_response, 10);
        assert_eq!(credit.conditional_health_hint, 18);
        let response = credit.protection_response.unwrap();
        assert_eq!(response.gross_protection, 37);
        assert_eq!(response.recovery_reserve_lost, 8);
        assert_eq!(response.play_hp_cost, 0);
        assert!(matches!(
            response.source,
            ProtectionSource::RemoveAttackerAndBlock { target: 1, .. }
        ));

        let mut actual = root.clone();
        for input in [
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
            ClientInput::EndTurn,
        ] {
            let step = EngineCombatStepper.apply_to_stable(
                &actual,
                input,
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            actual = step.position;
        }
        assert_eq!(actual.combat.entities.player.current_hp, 10);
        assert_eq!(recovery_reserve(&actual.combat), 8);
        assert_eq!(
            scores(&node)[4],
            180,
            "this jointly paid response matches replayed HP plus remaining request"
        );
        assert_eq!(
            node.state.position(),
            &root,
            "requests leave the source unchanged"
        );
        assert_eq!(18 - credit.uncovered_attack + response.gross_protection, 10);
        for (energy, margin, gross) in [(2, -1, 26), (3, 10, 37)] {
            let mut constrained = root.clone();
            constrained.combat.turn.energy = energy;
            let n = Node::root(CombatSearchState::new(constrained).unwrap());
            let credit = evaluate(&n, None).recovery;
            assert_eq!(credit.requested_hp_after_response, margin);
            assert_eq!(credit.protection_response.unwrap().gross_protection, gross);
        }
        for time_warp in [true, false] {
            let mut constrained = root.clone();
            if time_warp {
                sts_core::content::powers::store::set_powers_for(
                    &mut constrained.combat,
                    2,
                    vec![Power {
                        power_type: PowerId::TimeWarp,
                        amount: 10,
                        instance_id: None,
                        extra_data: 0,
                        payload: Default::default(),
                        just_applied: false,
                    }],
                );
            } else {
                constrained
                    .combat
                    .zones
                    .hand
                    .push(CombatCard::for_scenario(CardId::Normality, 705));
                constrained.combat.turn.counters.cards_played_this_turn = 2;
            }
            let n = Node::root(CombatSearchState::new(constrained).unwrap());
            assert_eq!(
                evaluate(&n, None)
                    .recovery
                    .protection_response
                    .unwrap()
                    .gross_protection,
                26,
                "a kill's play must leave enough counter allowance for block"
            );
        }
    }

    #[test]
    fn response_health_caps_current_capacity_without_erasing_healing_inventory() {
        use sts_core::runtime::combat::Power;
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let mut c = sts_core::test_support::blank_test_combat();
        c.entities.player.current_hp = 65;
        c.entities.player.max_hp = 75;
        let mut enemy = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        enemy.current_hp = 200;
        enemy.max_hp = 200;
        c.entities.monsters = vec![enemy];
        c.zones.hand = vec![CombatCard::for_scenario(CardId::Reaper, 901)];
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            0,
            vec![Power {
                power_type: PowerId::Strength,
                amount: 32,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let root = CombatPosition::new(EngineState::CombatPlayerTurn, c);
        let node = Node::root(CombatSearchState::new(root.clone()).unwrap());
        let values = evaluate(&node, None);
        assert_eq!(values.recovery.conditional_reserve, 36);
        assert_eq!(values.recovery.requested_hp_after_response, 54);
        assert_eq!(values.recovery.conditional_health_hint, 75);
        assert_eq!(values.recovery.conditional_endurance_hint, 90);
        assert_eq!(
            values.lanes[4], 900,
            "later damage can make remaining healing useful"
        );
        let healed = EngineCombatStepper.apply_to_stable(
            &root,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert!(!healed.truncated && !healed.timed_out);
        assert_eq!(healed.position.combat.entities.player.current_hp, 75);
        assert_eq!(healed.position.combat.entities.player.current_hp - 65, 10);
        let after = Node::root(CombatSearchState::new(healed.position).unwrap());
        let values = evaluate(&after, None);
        assert_eq!(values.recovery.conditional_reserve, 0);
        assert!(values.lanes[4] <= 750);
        assert_eq!(node.state.position(), &root);
    }

    #[test]
    fn card_composition_diversity_preserves_upgrades_and_exhaust_choices_without_merging_orders() {
        use sts_core::content::cards::CardId;
        use sts_core::runtime::combat::CombatCard;
        let mut combat = sts_core::test_support::blank_test_combat();
        combat.entities.monsters =
            vec![sts_core::test_support::planned_monster(EnemyId::JawWorm, 1)];
        combat.zones.hand = vec![CombatCard::for_scenario(CardId::Anger, 1)];
        combat.zones.draw_pile = vec![
            CombatCard::for_scenario(CardId::Strike, 2),
            CombatCard::for_scenario(CardId::Defend, 3),
        ]
        .into();
        let make = |combat| {
            Node::root(
                CombatSearchState::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
                    .unwrap(),
            )
        };
        let mut pool = vec![];
        for _ in 0..8 {
            combat.rng.shuffle_rng.random_long();
            pool.push(make(combat.clone()));
        }
        let mut swapped = combat.clone();
        swapped.zones.draw_pile.swap(0, 1);
        pool.push(make(swapped));
        let mut upgraded = combat.clone();
        upgraded.entities.player.current_hp -= 1;
        upgraded.zones.hand[0].upgrades = 1;
        let upgraded = make(upgraded);
        let mut exhausted = combat.clone();
        exhausted.entities.player.current_hp -= 2;
        exhausted
            .zones
            .exhaust_pile
            .push(exhausted.zones.draw_pile.pop().unwrap());
        let exhausted = make(exhausted);
        assert!(same_retention_context(&pool[0], &pool[8], true, true));
        assert!(same_retention_context(&pool[0], &upgraded, false, false));
        assert!(!same_retention_context(&pool[0], &upgraded, true, false));
        assert!(!same_retention_context(&pool[0], &exhausted, false, true));
        pool.extend([upgraded.clone(), exhausted.clone()]);
        let old = partition(
            pool.clone(),
            6,
            &mut SolverStats::default(),
            false,
            false,
            None,
            None,
        )
        .0;
        assert!(old
            .iter()
            .all(|n| n.state != upgraded.state && n.state != exhausted.state));
        let mut stats = SolverStats::default();
        let (kept, omitted) = partition(pool.clone(), 6, &mut stats, false, true, None, None);
        assert!(kept.iter().any(|n| n.state == upgraded.state));
        assert!(kept.iter().any(|n| n.state == exhausted.state));
        assert_eq!(kept.len(), 6);
        assert_eq!(stats.exact_duplicates, 0);
        assert_eq!(kept.len() + omitted.len(), pool.len());
        for original in pool {
            assert_eq!(
                kept.iter()
                    .chain(&omitted)
                    .filter(|n| n.state == original.state)
                    .count(),
                1
            );
        }
    }

    #[test]
    fn visible_pressure_and_enemy_progress_remain_distinct_retention_signals() {
        let mut combat = sts_core::test_support::blank_test_combat();
        let mut small = sts_core::test_support::planned_monster(EnemyId::SpikeSlimeS, 1);
        small.id = 1;
        small.current_hp = 1;
        let mut other = small.clone();
        other.id = 2;
        other.current_hp = 40;
        other.max_hp = 40;
        combat.entities.monsters = vec![small, other];
        let make_node = |combat| {
            Node::root(
                CombatSearchState::new(CombatPosition::new(EngineState::CombatPlayerTurn, combat))
                    .unwrap(),
            )
        };
        let pressured = make_node(combat.clone());
        // An exhausted healing card is not a reserve or a prerequisite for
        // valuing present-hand protection.
        let mut with_recovery = combat.clone();
        with_recovery
            .zones
            .exhaust_pile
            .push(CombatCard::for_scenario(CardId::BandageUp, 100));
        let with_recovery = make_node(with_recovery);
        assert_eq!(
            scores(&with_recovery)[4],
            scores(&pressured)[4],
            "no healing is required to retain survival"
        );
        let mut progress = combat.clone();
        progress.entities.monsters[1].current_hp -= 10;
        let progress_scores = scores(&make_node(progress.clone()));
        assert_eq!(progress_scores[4], scores(&pressured)[4]);
        assert!(progress_scores[0] > scores(&pressured)[0]);
        let mut strengthened = combat.clone();
        sts_core::content::powers::store::set_powers_for(
            &mut strengthened,
            combat.entities.player.id,
            vec![sts_core::runtime::combat::Power {
                power_type: PowerId::Strength,
                amount: 100,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let setup_scores = scores(&make_node(strengthened));
        assert_eq!(setup_scores[4], scores(&pressured)[4]);
        assert!(setup_scores[2] > scores(&pressured)[2]);
        assert_eq!(
            scores(&pressured)[4],
            i64::from(
                combat.entities.player.current_hp
                    - combat
                        .entities
                        .monsters
                        .iter()
                        .map(|m| {
                            project_monster_attack_request_in_combat(&combat, m)
                                .unwrap()
                                .total_damage
                        })
                        .sum::<i32>()
            ) * 10,
            "survival margin has no damage, Strength or unused-energy price"
        );
        // A health slot must distinguish equally protected preparations before
        // input production order. Other views still keep the low-HP offensive
        // alternative; preparation cannot buy a higher primary health rank.
        let mut prepared = combat.clone();
        prepared.entities.monsters[1].current_hp += 30;
        sts_core::content::powers::store::set_powers_for(
            &mut prepared,
            combat.entities.player.id,
            vec![sts_core::runtime::combat::Power {
                power_type: PowerId::Strength,
                amount: 8,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let prepared = make_node(prepared);
        let mut urgent = prepared.state.position().combat.clone();
        urgent.entities.player.current_hp = 10;
        urgent
            .entities
            .power_db
            .get_mut(&combat.entities.player.id)
            .unwrap()[0]
            .amount = 128;
        let urgent = make_node(urgent);
        assert_eq!(scores(&prepared)[4], scores(&pressured)[4]);
        assert!(scores(&prepared)[2] > scores(&pressured)[2]);
        assert!(scores(&urgent)[4] < scores(&prepared)[4]);
        assert!(scores(&urgent)[..4]
            .iter()
            .zip(scores(&prepared)[..4].iter())
            .all(|(a, b)| a > b));
        assert!(scores(&urgent)[..4]
            .iter()
            .zip(scores(&pressured)[..4].iter())
            .all(|(a, b)| a > b));
        for pool in [
            vec![pressured.clone(), urgent.clone(), prepared.clone()],
            vec![prepared.clone(), urgent.clone(), pressured.clone()],
        ] {
            let kept = retain(pool, 2, &mut SolverStats::default(), true);
            assert!(kept.iter().any(|n| n.state == prepared.state));
            assert!(kept.iter().any(|n| n.state == urgent.state));
            assert!(!kept.iter().any(|n| n.state == pressured.state));
        }
        // Among equal survival/capability states, a productive longer trace
        // must beat doing less. The offensive views still retain urgent work.
        let mut productive = make_node(progress);
        productive.input_count = 5;
        for pool in [
            vec![pressured.clone(), urgent.clone(), productive.clone()],
            vec![productive.clone(), urgent.clone(), pressured.clone()],
        ] {
            let kept = retain(pool, 2, &mut SolverStats::default(), true);
            assert!(kept.iter().any(|n| n.state == productive.state));
            assert!(kept.iter().any(|n| n.state == urgent.state));
            assert!(!kept.iter().any(|n| n.state == pressured.state));
        }
        let mut weakened = combat.clone();
        sts_core::content::powers::store::set_powers_for(
            &mut weakened,
            2,
            vec![sts_core::runtime::combat::Power {
                power_type: PowerId::Weak,
                amount: 1,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let weak_scores = scores(&make_node(weakened.clone()));
        assert!(
            weak_scores
                .iter()
                .zip(scores(&pressured))
                .all(|(weak, normal)| *weak > normal),
            "a visibly weakened attack matters even before any block is assembled"
        );
        weakened.entities.player.block = 100;
        let mut fully_blocked = combat.clone();
        fully_blocked.entities.player.block = 100;
        assert_eq!(
            scores(&make_node(weakened)),
            scores(&make_node(fully_blocked)),
            "already covered damage must not earn a second mitigation reward"
        );
        let mut pool = Vec::new();
        for block in 20..24 {
            combat.entities.player.block = block;
            pool.push(make_node(combat.clone()));
        }
        combat.entities.monsters[0].current_hp = 0;
        assert!(
            scores(&make_node(combat.clone()))[0] > scores(&pool[0])[0],
            "removing a fully blocked attacker must not erase block credit and lower the score"
        );
        combat.entities.player.current_hp -= 2;
        let defeated = make_node(combat.clone());
        // Spending some HP to finish an enemy is a real tradeoff. Diversity
        // keeps this completion even when the scalar lanes prefer saved HP.
        assert!(scores(&pool[0])[0] > scores(&defeated)[0]);
        pool.push(defeated.clone());
        let kept = retain(pool.clone(), 4, &mut SolverStats::default(), false);
        assert!(kept.iter().any(|n| n.state == defeated.state));
        assert!(kept.iter().any(|n| n.state != defeated.state));
        let wide = retain(pool, 8, &mut SolverStats::default(), false);
        assert_eq!(wide.len(), 5, "equal contexts must not merge exact states");

        combat.entities.monsters[0].half_dead = true;
        let rebirth = make_node(combat);
        assert!(!same_context(&rebirth, &defeated, false));

        // Actual lethal -> waiting rebirth -> full second phase must not look
        // like progress has been undone when the HP counter resets.
        use sts_core::runtime::combat::Power;
        use sts_core::sim::combat::{CombatStepLimits, CombatStepper, EngineCombatStepper};
        use sts_core::state::core::ClientInput;
        let mut c = sts_core::test_support::blank_test_combat();
        let mut awakened = sts_core::test_support::planned_monster(EnemyId::AwakenedOne, 1);
        awakened.id = 1;
        awakened.current_hp = 1;
        awakened.max_hp = 320;
        c.entities.monsters = vec![awakened];
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            1,
            vec![Power {
                power_type: PowerId::Unawakened,
                amount: -1,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let mut strike = CombatCard::for_scenario(CardId::Strike, 401);
        strike.base_damage_override = Some(400);
        c.zones.hand = vec![strike];
        assert_eq!(
            remaining_enemy_damage_work(&c, &c.entities.monsters[0]),
            321
        );
        let mut pos = CombatPosition::new(EngineState::CombatPlayerTurn, c);
        let mut totals = vec![];
        for input in [
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
            ClientInput::EndTurn,
            ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            },
        ] {
            let step = EngineCombatStepper.apply_to_stable(
                &pos,
                input,
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            pos = step.position;
            totals.push(remaining_enemy_damage_work(
                &pos.combat,
                &pos.combat.entities.monsters[0],
            ));
        }
        assert_eq!(totals, [320, 320, 0]);
        assert!(matches!(pos.engine, EngineState::RewardScreen(_)));

        // Damage is protection when a currently payable, reviewed attack
        // combination removes this turn's attacker. Replay both actual plays.
        let mut c = sts_core::test_support::blank_test_combat();
        let mut attacker = sts_core::test_support::planned_monster(EnemyId::JawWorm, 1);
        attacker.id = 1;
        attacker.current_hp = 70;
        attacker.max_hp = 70;
        c.entities.monsters = vec![attacker];
        c.turn.energy = 3;
        c.zones.hand = vec![
            CombatCard::for_scenario(CardId::HeavyBlade, 411),
            CombatCard::for_scenario(CardId::PommelStrike, 412),
        ];
        c.zones.hand[0].upgrades = 1;
        c.zones.hand[1].upgrades = 1;
        sts_core::content::powers::store::set_powers_for(
            &mut c,
            0,
            vec![Power {
                power_type: PowerId::Strength,
                amount: 10,
                instance_id: None,
                extra_data: 0,
                payload: Default::default(),
                just_applied: false,
            }],
        );
        let protected = make_node(c.clone());
        assert_eq!(available_protection(&protected, 10, 0), 10);
        let mut pos = protected.state.position().clone();
        for _ in 0..2 {
            let step = EngineCombatStepper.apply_to_stable(
                &pos,
                ClientInput::PlayCard {
                    card_index: 0,
                    target: Some(1),
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            pos = step.position;
        }
        assert!(matches!(pos.engine, EngineState::RewardScreen(_)));
        // Hand order is not play order: the direct attack can precede the
        // drawing attack, even if the drawing card is listed first. Put an
        // actual restrictive draw in the pile and replay that arrangement.
        c.zones.hand.swap(0, 1);
        c.zones.draw_pile = vec![CombatCard::for_scenario(CardId::Normality, 414)].into();
        let arranged = make_node(c.clone());
        assert_eq!(available_protection(&arranged, 10, 0), 10);
        let mut pos = arranged.state.position().clone();
        for card_index in [1, 0] {
            let step = EngineCombatStepper.apply_to_stable(
                &pos,
                ClientInput::PlayCard {
                    card_index,
                    target: Some(1),
                },
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            assert!(!step.truncated && !step.timed_out);
            pos = step.position;
        }
        assert!(matches!(pos.engine, EngineState::RewardScreen(_)));
        let power = |power_type, amount| Power {
            power_type,
            amount,
            instance_id: None,
            extra_data: 0,
            payload: Default::default(),
            just_applied: false,
        };
        // Do not credit a recipe that reaches the forced turn boundary.
        for (count, expected) in [(9, 10), (10, 0)] {
            sts_core::content::powers::store::set_powers_for(
                &mut c,
                1,
                vec![power(PowerId::TimeWarp, count)],
            );
            assert_eq!(available_protection(&make_node(c.clone()), 10, 0), expected);
        }
        sts_core::content::powers::store::set_powers_for(&mut c, 1, vec![]);
        c.zones
            .hand
            .push(CombatCard::for_scenario(CardId::Normality, 415));
        c.turn.counters.cards_played_this_turn = 2;
        assert_eq!(available_protection(&make_node(c.clone()), 10, 0), 0);
        c.zones.hand.pop();
        c.turn.counters.cards_played_this_turn = 0;
        c.turn.energy = 2;
        assert_eq!(available_protection(&make_node(c.clone()), 10, 0), 0);
        c.turn.energy = 3;
        c.zones
            .hand
            .push(CombatCard::for_scenario(CardId::Reaper, 413));
        let healing = recovery_reserve(&c);
        assert!(healing > 10);
        assert_eq!(
            available_protection(&make_node(c.clone()), 10, healing),
            0,
            "finishing the only target forfeits the same Reaper reserve"
        );
        c.zones.hand.pop();
        for power in [PowerId::Thorns, PowerId::Intangible, PowerId::ModeShift] {
            sts_core::content::powers::store::set_powers_for(
                &mut c,
                1,
                vec![Power {
                    power_type: power,
                    amount: 3,
                    instance_id: None,
                    extra_data: 0,
                    payload: Default::default(),
                    just_applied: false,
                }],
            );
            assert_eq!(available_protection(&make_node(c.clone()), 10, 0), 0);
        }
    }
}
