use super::*;
use crate::agent::information::state::public_combat_state_v1;
use crate::content::cards::CardId;
use crate::content::relics::{RelicId, RelicState};
use crate::runtime::combat::{CombatCard, DrawPileChange};

fn kind(card_id: CardId) -> DrawCardKind {
    DrawCardKind {
        card_id,
        upgrades: 0,
    }
}

fn partial(known: &[CardId], unknown: &[CardId]) -> DrawOrderKnowledge {
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.draw_pile = known
        .iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(*id, i as u32 + 1))
        .collect();
    combat
        .entities
        .player
        .add_relic(RelicState::new(RelicId::FrozenEye));
    let knowledge = DrawOrderKnowledge::from_observation(&public_combat_state_v1(&combat));
    combat.entities.player.relics.clear();
    for (i, id) in unknown.iter().enumerate() {
        combat
            .zones
            .draw_pile
            .push(CombatCard::for_scenario(*id, i as u32 + 100));
    }
    knowledge
        .after_changes(
            &unknown
                .iter()
                .map(|id| DrawPileChange::InsertUnpositioned { card: kind(*id) })
                .collect::<Vec<_>>(),
            &public_combat_state_v1(&combat),
        )
        .unwrap()
}

#[test]
fn duplicate_embeddings_do_not_multiply_the_probability_of_an_order() {
    use CardId::{Defend as B, Strike as A};
    let knowledge = partial(&[A, B], &[A, B]);
    let mut distribution = DrawOrderDistribution::new(&knowledge, 100).unwrap();
    let probability = distribution
        .completion_probability(&distribution.counts.clone(), 0)
        .unwrap();
    assert!((probability - 5.0 / 6.0).abs() < 1e-12);
    for sequence in [
        [A, A, B, B],
        [A, B, A, B],
        [A, B, B, A],
        [B, A, A, B],
        [B, A, B, A],
    ] {
        let mut counts = distribution.counts.clone();
        let mut matched = 0;
        let mut path_probability = 1.0;
        for id in sequence {
            let group = distribution
                .kinds
                .iter()
                .position(|card| *card == kind(id))
                .unwrap();
            let before = distribution
                .completion_probability(&counts, matched)
                .unwrap();
            let chance = counts[group] as f64 / counts.iter().sum::<usize>() as f64;
            counts[group] -= 1;
            matched += usize::from(distribution.required.get(matched) == Some(&group));
            path_probability *= chance
                * distribution
                    .completion_probability(&counts, matched)
                    .unwrap()
                / before;
        }
        assert!(
            (path_probability - 0.2).abs() < 1e-12,
            "{sequence:?}: {path_probability}"
        );
    }
    let mut frequencies = HashMap::new();
    for seed in 1..=5_000 {
        let order = distribution.sample(&mut StsRng::new(seed)).unwrap();
        assert!(knowledge.permits_kind_order(&order));
        *frequencies
            .entry(
                order
                    .iter()
                    .map(|c| (c.card_id.stable_ordinal(), c.upgrades))
                    .collect::<Vec<_>>(),
            )
            .or_insert(0) += 1;
    }
    assert_eq!(frequencies.len(), 5);
    assert!(frequencies
        .values()
        .all(|count| (800..=1_200).contains(count)));
}

#[test]
fn fixed_edges_and_overlapping_full_order_are_preserved() {
    use CardId::*;
    let mut knowledge = partial(&[Strike, Defend], &[Burn, Wound]);
    let mut combat = crate::test_support::blank_test_combat();
    combat.zones.draw_pile = [Bash, Strike, Defend, Burn, Wound, Carnage]
        .into_iter()
        .enumerate()
        .map(|(i, id)| CombatCard::for_scenario(id, i as u32 + 1))
        .collect();
    knowledge = knowledge
        .after_changes(
            &[
                DrawPileChange::PutTop { card: kind(Bash) },
                DrawPileChange::PutBottom {
                    card: kind(Carnage),
                },
            ],
            &public_combat_state_v1(&combat),
        )
        .unwrap();
    let mut distribution = DrawOrderDistribution::new(&knowledge, 100).unwrap();
    for seed in 1..=100 {
        let order = distribution.sample(&mut StsRng::new(seed)).unwrap();
        assert!(knowledge.permits_kind_order(&order));
        assert_eq!(order.first(), Some(&kind(Bash)));
        assert_eq!(order.last(), Some(&kind(Carnage)));
    }
    for known in [vec![], vec![Strike; 12], vec![Strike, Defend, Strike, Bash]] {
        let knowledge = partial(&known, &[]);
        let mut distribution = DrawOrderDistribution::new(&knowledge, 0).unwrap();
        assert_eq!(
            distribution.sample(&mut StsRng::new(1)).unwrap(),
            known.into_iter().map(kind).collect::<Vec<_>>()
        );
    }
}

#[test]
fn bounded_conditioning_shares_unconstrained_kinds_instead_of_permuting_them() {
    use CardId::*;
    let known = [Strike, Defend].repeat(15);
    let unknown = [
        Bash,
        Carnage,
        Headbutt,
        Anger,
        Burn,
        Wound,
        Dazed,
        Slimed,
        Offering,
        ShrugItOff,
        Corruption,
        BattleTrance,
    ];
    let knowledge = partial(&known, &unknown);
    let mut distribution = DrawOrderDistribution::new(&knowledge, 500).unwrap();
    assert!(distribution.probabilities.len() < 500);
    for seed in 1..=20 {
        let order = distribution.sample(&mut StsRng::new(seed)).unwrap();
        assert!(knowledge.permits_kind_order(&order));
    }
    assert!(matches!(
        DrawOrderDistribution::new(&knowledge, 1),
        Err(Error::DrawOrderConditioningBudgetExceeded { max_states: 1 })
    ));
}

#[test]
fn unconstrained_orders_are_exchangeable_and_large_piles_fail_explicitly() {
    use CardId::{Defend as B, Strike as A};
    let knowledge = partial(&[], &[A, A, B]);
    let mut distribution = DrawOrderDistribution::new(&knowledge, 0).unwrap();
    let mut frequencies = HashMap::new();
    for seed in 1..=3_000 {
        let order = distribution.sample(&mut StsRng::new(seed)).unwrap();
        *frequencies
            .entry(
                order
                    .iter()
                    .map(|c| (c.card_id.stable_ordinal(), c.upgrades))
                    .collect::<Vec<_>>(),
            )
            .or_insert(0) += 1;
    }
    assert_eq!(frequencies.len(), 3);
    assert!(frequencies
        .values()
        .all(|count| (800..=1_200).contains(count)));
    let oversized = partial(&[A; 129], &[]);
    assert!(matches!(
        DrawOrderDistribution::new(&oversized, 100),
        Err(Error::DrawPileTooLarge)
    ));
}
