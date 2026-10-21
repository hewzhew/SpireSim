//! Shared fixed-shuffle order proposals for full-state Oracle search and diagnostics.
//! Proposals execute real inputs; they never rewrite piles or merge permutations.
use serde_json::{json, Value};
use std::collections::BTreeSet;
use sts_core::{
    runtime::{
        combat::{CardLineageId, DrawPileChange, OracleShuffleReceipt},
        rng::shuffle_with_random_long,
    },
    sim::combat::{
        apply_combat_input_to_stable_observed_v1, apply_combat_input_to_stable_oracle_shuffles_v1,
        is_exact_combat_victory, CombatObservedStepResultV1, CombatOracleShuffleStepResultV1,
        CombatPosition, CombatStepLimits, CombatStepper, EngineCombatStepper,
    },
    state::core::{ClientInput, EngineState},
    EntityId,
};
#[derive(Clone)]
pub struct Play {
    pub lineage: CardLineageId,
    pub target: Option<EntityId>,
    /// Ordered append effect, including newly generated cards. The action is
    /// moved as a unit; these cards are never directly edited into a pile.
    pub discarded: Vec<CardLineageId>,
}

pub fn step(p: &CombatPosition, input: ClientInput) -> Result<CombatPosition, String> {
    if !EngineCombatStepper.is_legal_action(p, &input) {
        return Err(format!("illegal input: {input:?}"));
    }
    let s = EngineCombatStepper.apply_to_stable(
        p,
        input,
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    if s.timed_out || s.truncated {
        return Err("incomplete engine transition".into());
    }
    Ok(s.position)
}

pub fn replay(
    root: &CombatPosition,
    inputs: &[ClientInput],
) -> Result<Vec<CombatPosition>, String> {
    let mut states = vec![root.clone()];
    for input in inputs {
        states.push(step(states.last().unwrap(), input.clone())?);
    }
    Ok(states)
}

/// Extract a contiguous block whose plays append the played card and possibly
/// new cards, draw nothing and leave the other hand cards in place. Damage,
/// block, powers and relic triggers are deliberately allowed and later replayed.
pub fn discard_play(a: &CombatPosition, input: &ClientInput, b: &CombatPosition) -> Option<Play> {
    let ClientInput::PlayCard { card_index, target } = input else {
        return None;
    };
    if a.engine != EngineState::CombatPlayerTurn || b.engine != a.engine {
        return None;
    }
    let ac = &a.combat;
    let bc = &b.combat;
    let lineage = ac.zones.hand.get(*card_index)?.lineage;
    let expected_hand: Vec<_> = ac
        .zones
        .hand
        .iter()
        .enumerate()
        .filter(|(i, _)| i != card_index)
        .map(|(_, c)| c.lineage)
        .collect();
    let offset = ac.zones.discard_pile.len();
    let discarded: Vec<_> = bc
        .zones
        .discard_pile
        .iter()
        .skip(offset)
        .map(|c| c.lineage)
        .collect();
    let old_lineages: BTreeSet<_> = ac
        .zones
        .hand
        .iter()
        .chain(ac.zones.draw_pile.iter())
        .chain(ac.zones.discard_pile.iter())
        .chain(ac.zones.exhaust_pile.iter())
        .map(|c| c.lineage)
        .collect();
    (bc.turn.turn_count == ac.turn.turn_count
        && bc.rng.shuffle_rng == ac.rng.shuffle_rng
        && bc.zones.draw_pile == ac.zones.draw_pile
        && bc.zones.exhaust_pile == ac.zones.exhaust_pile
        && bc.zones.hand.iter().map(|c| c.lineage).eq(expected_hand)
        && bc
            .zones
            .discard_pile
            .iter()
            .take(offset)
            .eq(ac.zones.discard_pile.iter())
        && discarded.iter().filter(|&&id| id == lineage).count() == 1
        && discarded.iter().collect::<BTreeSet<_>>().len() == discarded.len()
        && discarded
            .iter()
            .all(|id| *id == lineage || !old_lineages.contains(id)))
    .then_some(Play {
        lineage,
        target: *target,
        discarded,
    })
}

/// A mapping from the real shuffle input, checked against both the owner's
/// output and the stable endpoint. Never reconstruct end-turn hook order here.
#[derive(Debug)]
pub struct Mapping {
    pub input: Vec<CardLineageId>,
    pub draw_to_input: Vec<usize>,
    pub drawn: usize,
    pub receipt: OracleShuffleReceipt,
}

pub fn mapping(
    before: &CombatPosition,
    action: &ClientInput,
    after: &CombatPosition,
) -> Result<Mapping, String> {
    let oracle = apply_combat_input_to_stable_oracle_shuffles_v1(
        before,
        action.clone(),
        CombatStepLimits {
            max_engine_steps: 512,
            deadline: None,
        },
    );
    checked_mapping(before, after, oracle)
}

/// Pure verification over an already executed transition; incremental search
/// supplies its budgeted observation without silently executing it again.
pub fn checked_mapping(
    before: &CombatPosition,
    after: &CombatPosition,
    oracle: CombatOracleShuffleStepResultV1,
) -> Result<Mapping, String> {
    let b = &before.combat;
    let a = &after.combat;
    let observed = oracle.observed;
    if observed.step.truncated || observed.step.timed_out || observed.step.position != *after {
        return Err("observed transition did not reproduce the exact endpoint".into());
    }
    if oracle.shuffles.len() != 1 {
        return Err(format!(
            "expected one shuffle, observed {}",
            oracle.shuffles.len()
        ));
    }
    let receipt = oracle.shuffles.into_iter().next().unwrap();
    let input: Vec<_> = receipt.input.iter().map(|c| c.lineage).collect();
    if input.is_empty() || input.iter().collect::<BTreeSet<_>>().len() != input.len() {
        return Err("shuffle input is empty or has duplicate lineage identities".into());
    }
    let mut rng = receipt.rng_before.clone();
    let mut draw_to_input: Vec<_> = (0..input.len()).collect();
    shuffle_with_random_long(&mut draw_to_input, &mut rng);
    draw_to_input.reverse();
    if receipt.rng_before != b.rng.shuffle_rng
        || rng != receipt.rng_after
        || rng != a.rng.shuffle_rng
    {
        return Err("shuffle RNG changed outside the single observed shuffle".into());
    }
    let expected: Vec<_> = draw_to_input.iter().map(|&i| input[i]).collect();
    if expected != receipt.output {
        return Err("position mapping disagrees with the owner's shuffled output".into());
    }
    let mut shuffles = 0;
    let mut old_drawn = 0;
    for change in &observed.draw_pile_changes {
        match change {
            DrawPileChange::PrependShuffled { count } if *count == input.len() => {
                if old_drawn != b.zones.draw_pile.len() {
                    return Err("shuffle before old draw pile is exhausted is not covered".into());
                }
                shuffles += 1;
            }
            DrawPileChange::TakeTop { .. } => {
                if shuffles == 0 {
                    old_drawn += 1;
                }
            }
            _ => return Err(format!("additional draw-pile mutation: {change:?}")),
        }
    }
    if shuffles != 1 {
        return Err(format!(
            "draw receipt contains {shuffles} matching shuffles"
        ));
    }
    let old: Vec<_> = b.zones.draw_pile.iter().map(|c| c.lineage).collect();
    let hand: Vec<_> = observed.drawn_cards.iter().map(|c| c.lineage).collect();
    if !hand.starts_with(&old) {
        return Err("drawn-card prefix differs from the original draw pile".into());
    }
    let drawn = hand.len() - old.len();
    if drawn == 0 || drawn > input.len() {
        return Err(format!(
            "observed {drawn} draws from {} shuffled cards",
            input.len()
        ));
    }
    let actual: Vec<_> = hand[old.len()..]
        .iter()
        .copied()
        .chain(a.zones.draw_pile.iter().map(|c| c.lineage))
        .collect();
    if expected != actual {
        return Err("drawn cards and remaining pile disagree with shuffled output".into());
    }
    Ok(Mapping {
        input,
        draw_to_input,
        drawn,
        receipt,
    })
}

#[derive(Clone)]
pub struct DrawSummary {
    pub count: usize,
    pub only_takes: bool,
}

impl From<&CombatObservedStepResultV1> for DrawSummary {
    fn from(observed: &CombatObservedStepResultV1) -> Self {
        Self {
            count: observed.drawn_cards.len(),
            only_takes: observed
                .draw_pile_changes
                .iter()
                .all(|c| matches!(c, DrawPileChange::TakeTop { .. })),
        }
    }
}

pub fn recorded_draw_cuts(
    map: &Mapping,
    draws: impl IntoIterator<Item = DrawSummary>,
) -> Vec<usize> {
    let mut cuts = vec![map.drawn];
    let mut consumed = map.drawn;
    for draw in draws {
        if consumed == map.input.len() || !draw.only_takes {
            break;
        }
        if draw.count > 0 {
            consumed += draw.count;
            if consumed > map.input.len() {
                break;
            }
            cuts.push(consumed);
        }
    }
    cuts
}

/// Draw batches along the supplied baseline, through the next pile mutation.
/// These are proposal goals, not predictions that the modified continuation
/// will draw the same numbers at the same times. No five-cards-per-turn guess.
pub fn draw_cuts(
    map: &Mapping,
    states: &[CombatPosition],
    inputs: &[ClientInput],
    end: usize,
) -> Vec<usize> {
    recorded_draw_cuts(
        map,
        (end + 1..inputs.len()).map(|i| {
            let obs = apply_combat_input_to_stable_observed_v1(
                &states[i],
                inputs[i].clone(),
                CombatStepLimits {
                    max_engine_steps: 512,
                    deadline: None,
                },
            );
            let mut summary = DrawSummary::from(&obs);
            summary.only_takes &=
                !obs.step.truncated && !obs.step.timed_out && obs.step.position == states[i + 1];
            summary
        }),
    )
}

/// One-action insertion moves across the baseline draw batches. The inverse map
/// identifies the input slots serving each batch. Every card is considered,
/// with no card value table. Other play order changes only by one insertion.
/// Within-batch permutations remain distinct real states; omission here is a
/// proposal restriction, never an assertion of fixed-RNG equivalence.
pub fn mapped_orders(
    widths: &[usize],
    offset: usize,
    map: &Mapping,
    cuts: &[usize],
) -> BTreeSet<Vec<usize>> {
    let n = widths.len();
    let base: Vec<_> = (0..n).collect();
    let mut batch = vec![cuts.len(); map.input.len()];
    for (draw, &input) in map.draw_to_input.iter().enumerate() {
        batch[input] = cuts.partition_point(|&cut| cut <= draw);
    }
    let mut orders = BTreeSet::from([base.clone()]);
    let layout = |order: &[usize]| {
        order
            .iter()
            .flat_map(|&i| (0..widths[i]).map(move |j| (i, j)))
            .collect::<Vec<_>>()
    };
    let original = layout(&base);
    for from in 0..n {
        for to in 0..n {
            let mut order = base.clone();
            let card = order.remove(from);
            order.insert(to, card);
            let changed = layout(&order);
            if original.iter().enumerate().any(|(slot, token)| {
                let new_slot = changed.iter().position(|x| x == token).unwrap();
                batch[offset + slot] != batch[offset + new_slot]
            }) {
                orders.insert(order);
            }
        }
    }
    orders
}

pub fn permutations(values: &mut [usize], i: usize, out: &mut BTreeSet<Vec<usize>>) {
    if i == values.len() {
        out.insert(values.to_vec());
        return;
    }
    for j in i..values.len() {
        values.swap(i, j);
        permutations(values, i + 1, out);
        values.swap(i, j);
    }
}

pub fn changed_draw_batches(original: &Mapping, changed: &Mapping, cuts: &[usize]) -> Vec<Value> {
    original.input.iter().enumerate().filter_map(|(old_slot, lineage)| {
        let new_slot = changed.input.iter().position(|id| id == lineage)?;
        let old_draw = original.draw_to_input.iter().position(|&i| i == old_slot)?;
        let new_draw = changed.draw_to_input.iter().position(|&i| i == new_slot)?;
        let old_batch = cuts.partition_point(|&cut| cut <= old_draw);
        let new_batch = cuts.partition_point(|&cut| cut <= new_draw);
        if old_batch == new_batch { return None; }
        let card = original.receipt.input.iter().find(|c| c.lineage == *lineage)?;
        Some(json!({"lineage":lineage,"card":card.id,"discard_from":old_slot,"discard_to":new_slot,
            "draw_from":old_draw,"draw_to":new_draw,"batch_from":old_batch,"batch_to":new_batch}))
    }).collect()
}

pub fn play_order(
    root: &CombatPosition,
    plays: &[Play],
    order: &[usize],
) -> Result<(CombatPosition, Vec<ClientInput>), String> {
    let mut p = root.clone();
    let mut inputs = Vec::new();
    for &i in order {
        let play = &plays[i];
        let matches: Vec<_> = p
            .combat
            .zones
            .hand
            .iter()
            .enumerate()
            .filter(|(_, c)| c.lineage == play.lineage)
            .map(|(i, _)| i)
            .collect();
        if matches.len() != 1 {
            return Err("played lineage is missing or ambiguous".into());
        }
        let input = ClientInput::PlayCard {
            card_index: matches[0],
            target: play.target,
        };
        let next = step(&p, input.clone())?;
        let effect = discard_play(&p, &input, &next)
            .ok_or("reordering changed the append-only movement boundary")?;
        if effect.discarded.len() != play.discarded.len() {
            return Err("reordering changed the number of appended cards".into());
        }
        inputs.push(input);
        p = next;
    }
    Ok((p, inputs))
}

pub fn rebind_follow_input(
    p: &CombatPosition,
    original: &CombatPosition,
    input: &ClientInput,
) -> Result<ClientInput, String> {
    Ok(match input {
        ClientInput::PlayCard { card_index, target } => {
            let lineage = original.combat.zones.hand[*card_index].lineage;
            let matches: Vec<_> = p
                .combat
                .zones
                .hand
                .iter()
                .enumerate()
                .filter(|(_, c)| c.lineage == lineage)
                .map(|(i, _)| i)
                .collect();
            if matches.len() != 1 {
                return Err("intervening play lost its lineage".into());
            }
            ClientInput::PlayCard {
                card_index: matches[0],
                target: *target,
            }
        }
        ClientInput::EndTurn => input.clone(),
        ClientInput::SubmitSelection(_)
            if EngineCombatStepper
                .legal_action_surface(&p)
                .selection_families
                == EngineCombatStepper
                    .legal_action_surface(original)
                    .selection_families =>
        {
            input.clone()
        }
        ClientInput::UsePotion { potion_index, .. }
            if p.combat.entities.potions.get(*potion_index)
                == original.combat.entities.potions.get(*potion_index) =>
        {
            input.clone()
        }
        _ => return Err("intervening positional input is outside this probe".into()),
    })
}

/// Rebind plays by lineage while retaining the intervening plan. Raw positional
/// selections require an unchanged selection domain. Reordering may invalidate
/// the plan or shuffle RNG. Potion inputs retain the exact original slot/item.
pub fn follow_to_shuffle(
    mut p: CombatPosition,
    states: &[CombatPosition],
    inputs: &[ClientInput],
    from: usize,
    end: usize,
) -> Result<(CombatPosition, CombatPosition, Vec<ClientInput>), String> {
    let mut resolved = Vec::new();
    for i in from..=end {
        let input = rebind_follow_input(&p, &states[i], &inputs[i])?;
        if p.combat.rng.shuffle_rng != states[i].combat.rng.shuffle_rng {
            return Err("intervening shuffle RNG changed".into());
        }
        let next = step(&p, input.clone())?;
        resolved.push(input);
        if i == end {
            return Ok((p, next, resolved));
        }
        p = next;
    }
    Err("missing shuffle boundary".into())
}

/// Try the supplied raw input suffix as one more proposal at each successor.
/// Indices may now play different cards; this is a new legal replay, never an
/// identity/equivalence claim or permission to reuse the old result. No potions.
pub fn replay_supplied_suffix(
    root: &CombatPosition,
    suffix: &[ClientInput],
) -> Result<(Vec<ClientInput>, CombatPosition), String> {
    let mut p = root.clone();
    let mut used = Vec::new();
    for input in suffix {
        if is_exact_combat_victory(&p) {
            break;
        }
        if !matches!(
            input,
            ClientInput::PlayCard { .. } | ClientInput::EndTurn | ClientInput::SubmitSelection(_)
        ) {
            return Err("supplied suffix requires an input outside potion-free combat play".into());
        }
        p = step(&p, input.clone())?;
        used.push(input.clone());
    }
    if !is_exact_combat_victory(&p) {
        return Err("supplied inputs did not finish a victory from this successor".into());
    }
    Ok((used, p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_core::{
        content::{cards::CardId, monsters::EnemyId},
        runtime::{combat::CombatCard, rng::StsRng},
    };

    fn root() -> CombatPosition {
        let mut c = sts_core::test_support::blank_test_combat();
        let mut enemy = sts_core::test_support::planned_monster(EnemyId::Cultist, 1);
        enemy.current_hp = 300;
        enemy.max_hp = 300;
        c.entities.monsters = vec![enemy];
        c.turn.energy = 4;
        c.zones.hand = [CardId::Bash, CardId::Strike, CardId::Defend]
            .into_iter()
            .enumerate()
            .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
            .collect();
        c.zones.draw_pile = (10..14)
            .map(|i| CombatCard::for_scenario(CardId::Wound, i))
            .collect::<Vec<_>>()
            .into();
        c.rng.shuffle_rng = StsRng::new(2);
        CombatPosition::new(EngineState::CombatPlayerTurn, c)
    }

    #[test]
    fn supplied_suffix_is_a_new_legal_victory_not_a_reused_outcome() {
        let mut root = root();
        root.combat.entities.monsters[0].current_hp = 6;
        root.combat.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1)];
        let attack = ClientInput::PlayCard {
            card_index: 0,
            target: Some(1),
        };
        let (_, original) = replay_supplied_suffix(&root, &[attack.clone()]).unwrap();
        let mut changed = root.clone();
        changed.combat.zones.hand[0].lineage = CardLineageId::constructed_scenario(50);
        let (used, alternative) =
            replay_supplied_suffix(&changed, &[attack.clone(), ClientInput::EndTurn]).unwrap();
        assert_eq!(used, [attack.clone()]);
        assert!(is_exact_combat_victory(&alternative));
        assert_ne!(original, alternative);
        assert_eq!(
            original.combat.entities.player.current_hp,
            alternative.combat.entities.player.current_hp
        );
        changed.combat.zones.hand[0].id = CardId::Defend;
        assert!(replay_supplied_suffix(&changed, &[attack]).is_err());
        assert!(replay_supplied_suffix(&root, &[ClientInput::EndTurn]).is_err());
        assert!(replay_supplied_suffix(
            &root,
            &[ClientInput::UsePotion {
                potion_index: 0,
                target: None
            }]
        )
        .unwrap_err()
        .contains("potion-free"));
    }

    #[test]
    fn end_turn_mapping_observes_exhaust_draws_and_enemy_generated_cards() {
        let mut root = root();
        root.combat.meta.ascension_level = 20;
        root.combat.entities.monsters =
            vec![sts_core::test_support::planned_monster(EnemyId::Sentry, 3)];
        root.combat.zones.hand = [CardId::DarkEmbrace, CardId::Dazed, CardId::Wound]
            .into_iter()
            .enumerate()
            .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
            .collect();
        root.combat.zones.discard_pile = [CardId::Strike, CardId::Defend, CardId::Bash]
            .into_iter()
            .enumerate()
            .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 10))
            .collect();
        root.combat.zones.draw_pile = [CardId::Cleave, CardId::PommelStrike]
            .into_iter()
            .enumerate()
            .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 20))
            .collect();
        let before = step(
            &root,
            ClientInput::PlayCard {
                card_index: 0,
                target: None,
            },
        )
        .unwrap();
        let next = step(&before, ClientInput::EndTurn).unwrap();
        let map = mapping(&before, &ClientInput::EndTurn, &next).unwrap();
        assert_eq!(
            map.receipt.input.iter().map(|c| c.id).collect::<Vec<_>>(),
            [
                CardId::Strike,
                CardId::Defend,
                CardId::Bash,
                CardId::Wound,
                CardId::Dazed,
                CardId::Dazed,
                CardId::Dazed
            ]
        );
        assert!(
            !map.input.contains(&CardLineageId::constructed_scenario(2)),
            "original Dazed exhausts before shuffle"
        );
        assert!(
            !map.input.contains(&CardLineageId::constructed_scenario(20)),
            "Dark Embrace's draw is retained through end-turn settlement"
        );
        assert_eq!(map.drawn, 4);
        let public = apply_combat_input_to_stable_observed_v1(
            &before,
            ClientInput::EndTurn,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        let oracle = apply_combat_input_to_stable_oracle_shuffles_v1(
            &before,
            ClientInput::EndTurn,
            CombatStepLimits {
                max_engine_steps: 512,
                deadline: None,
            },
        );
        assert_eq!(oracle.observed.step.position, next);
        assert_eq!(public.step.position, next);
        assert_eq!(
            serde_json::to_value(&public.step.position).unwrap(),
            serde_json::to_value(&oracle.observed.step.position).unwrap()
        );
        assert_eq!(public.draw_pile_changes, oracle.observed.draw_pile_changes);
        assert_eq!(public.drawn_cards, oracle.observed.drawn_cards);
        // Deep Breath also reshuffles the whole draw pile. The discard-group
        // receipt is deliberately insufficient for that extra RNG consumption.
        let mut early = root;
        early.combat.zones.hand = vec![CombatCard::for_scenario(CardId::DeepBreath, 30)];
        let input = ClientInput::PlayCard {
            card_index: 0,
            target: None,
        };
        let early_next = step(&early, input.clone()).unwrap();
        assert_eq!(
            mapping(&early, &input, &early_next).unwrap_err(),
            "shuffle RNG changed outside the single observed shuffle"
        );
    }

    #[test]
    fn generated_discard_pair_moves_with_its_action_through_real_shuffle() {
        let mut root = root();
        root.combat.zones.hand = [CardId::Clothesline, CardId::Anger]
            .into_iter()
            .enumerate()
            .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
            .collect();
        let mut base = root.clone();
        let mut plays = Vec::new();
        for _ in 0..2 {
            let input = ClientInput::PlayCard {
                card_index: 0,
                target: Some(1),
            };
            let next = step(&base, input.clone()).unwrap();
            plays.push(discard_play(&base, &input, &next).unwrap());
            base = next;
        }
        assert_eq!(
            plays.iter().map(|p| p.discarded.len()).collect::<Vec<_>>(),
            [1, 2]
        );
        let next = step(&base, ClientInput::EndTurn).unwrap();
        let map = mapping(&base, &ClientInput::EndTurn, &next).unwrap();
        assert!(mapped_orders(&[1, 2], 0, &map, &[map.drawn]).contains(&vec![1, 0]));
        let (changed, _) = play_order(&root, &plays, &[1, 0]).unwrap();
        assert_eq!(
            changed.combat.entities.monsters,
            base.combat.entities.monsters
        );
        let changed_next = step(&changed, ClientInput::EndTurn).unwrap();
        let changed_map = mapping(&changed, &ClientInput::EndTurn, &changed_next).unwrap();
        assert_eq!(map.draw_to_input, changed_map.draw_to_input);
        assert_eq!(next.combat.zones.hand.last().unwrap().id, CardId::Anger);
        assert_eq!(
            changed_next.combat.zones.hand.last().unwrap().id,
            CardId::Clothesline
        );
        // Duplicate copies are real separate lineage-bearing instances.
        assert_eq!(changed_map.input.iter().collect::<BTreeSet<_>>().len(), 3);
    }

    #[test]
    fn inverse_slots_survive_tactical_cost_but_not_a_changed_shuffle() {
        let root = root();
        let plays: Vec<_> = root
            .combat
            .zones
            .hand
            .iter()
            .enumerate()
            .map(|(i, c)| Play {
                lineage: c.lineage,
                target: (i < 2).then_some(1),
                discarded: vec![c.lineage],
            })
            .collect();
        let (base, _) = play_order(&root, &plays, &[0, 1, 2]).unwrap();
        let next = step(&base, ClientInput::EndTurn).unwrap();
        let map = mapping(&base, &ClientInput::EndTurn, &next).unwrap();
        assert_eq!(next.combat.zones.hand.last().unwrap().id, CardId::Defend);
        let proposals = mapped_orders(&[1, 1, 1], 0, &map, &[map.drawn]);
        assert!(proposals.contains(&vec![1, 2, 0]));
        let (changed, _) = play_order(&root, &plays, &[1, 2, 0]).unwrap();
        let next_changed = step(&changed, ClientInput::EndTurn).unwrap();
        let changed_map = mapping(&changed, &ClientInput::EndTurn, &next_changed).unwrap();
        assert_eq!(map.draw_to_input, changed_map.draw_to_input);
        assert_eq!(
            next_changed.combat.zones.hand.last().unwrap().id,
            CardId::Bash
        );
        // Strike before Bash sacrifices three immediate damage. The mapping
        // does not label this a free reorder or decide whether it is worthwhile.
        assert_eq!(
            changed.combat.entities.monsters[0].current_hp
                - base.combat.entities.monsters[0].current_hp,
            3
        );
        let mut bad = changed.clone();
        bad.combat.rng.shuffle_rng.random_long();
        assert!(mapping(&bad, &ClientInput::EndTurn, &next_changed).is_err());
        let mut bad = changed.clone();
        bad.combat
            .zones
            .discard_pile
            .push(CombatCard::for_scenario(CardId::Wound, 30));
        assert!(mapping(&bad, &ClientInput::EndTurn, &next_changed).is_err());
    }

    #[test]
    fn draw_card_is_not_inserted_in_its_own_shuffle_input() {
        let mut root = root();
        root.combat.zones.discard_pile = std::mem::take(&mut root.combat.zones.hand).into();
        root.combat.zones.draw_pile = Vec::new().into();
        root.combat.zones.hand = vec![CombatCard::for_scenario(CardId::ShrugItOff, 40)];
        let input = ClientInput::PlayCard {
            card_index: 0,
            target: None,
        };
        let next = step(&root, input.clone()).unwrap();
        let map = mapping(&root, &input, &next).unwrap();
        assert_eq!(map.input.len(), 3);
        assert_eq!(map.drawn, 1);
        assert_eq!(next.combat.zones.discard_pile[0].id, CardId::ShrugItOff);
        assert_eq!(next.combat.zones.hand[0].id, CardId::Defend);
    }
}
