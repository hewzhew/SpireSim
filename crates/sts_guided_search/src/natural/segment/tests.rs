use super::*;
use crate::observation::ModelChoice;

fn recipe() -> SegmentRecipe {
    // Small diagnostic deck makes each persistent mechanism directly observable.
    // This is not training data or a plausible naturally acquired loadout.
    SegmentRecipe {
        hp: 40,
        deck: vec![
            (CardId::Feed, 1),
            (CardId::RitualDagger, 1),
            (CardId::LessonLearned, 1),
            (CardId::Strike, 0),
            (CardId::Defend, 0),
        ],
        relics: vec![
            RelicId::HappyFlower,
            RelicId::PenNib,
            RelicId::Akabeko,
            RelicId::MutagenicStrength,
        ],
        potions: vec![PotionId::FruitJuice, PotionId::FirePotion],
        route: vec![
            SegmentRoom {
                encounter: EncounterId::TwoLouse,
                room: RoomType::MonsterRoom,
            },
            SegmentRoom {
                encounter: EncounterId::Cultist,
                room: RoomType::MonsterRoom,
            },
        ],
    }
}

fn run(e: &mut NaturalEpisode, i: ClientInput) {
    e.choose(e.source(), &NaturalAction::Run(i)).unwrap();
}
fn choose(e: &mut NaturalEpisode, f: impl Fn(&ModelChoice) -> bool) {
    let a = e
        .options()
        .unwrap()
        .into_iter()
        .find(|o| o.combat_choice.as_ref().is_some_and(&f))
        .unwrap()
        .action;
    e.choose(e.source(), &a).unwrap();
}
fn play(e: &mut NaturalEpisode, card: CardId, target: usize) {
    let o = e.combat_observation().unwrap().unwrap();
    choose(
        e,
        |c| matches!(c, ModelChoice::Play {hand, target: Some(t)} if o.hand[*hand].card == card && *t == target),
    );
}
fn counter(e: &NaturalEpisode, id: RelicId) -> i32 {
    e.resources()
        .relics
        .iter()
        .find(|r| r.id == id)
        .unwrap()
        .counter
}

#[test]
fn settlement_carries_hp_slots_relic_counters_misc_and_claimed_cards() {
    let mut e = NaturalEpisode::from_segment(711, 100, &recipe()).unwrap();
    assert_eq!(e.engine(), &EngineState::MapNavigation);
    assert_eq!(e.run.current_hp, 40);
    run(&mut e, ClientInput::SelectMapNode(0));
    choose(&mut e, |c| {
        matches!(c, ModelChoice::UsePotion { slot: 0, .. })
    });
    play(&mut e, CardId::Feed, 0);
    play(&mut e, CardId::RitualDagger, 1);
    assert_eq!(e.results.len(), 1, "{}", e.view().unwrap());
    assert!(e.results[0].defeated);
    assert!(e.active.is_none());
    assert!(e.stop.is_none());
    assert_eq!((e.run.current_hp, e.run.max_hp), (55, 84)); // Fruit + Feed + one Burning Blood.
    assert_eq!(e.results[0].potion_uses, 1);
    assert!(e.run.potions[0].is_none());
    let kept = e.run.potions[1].clone();
    assert_eq!(kept.as_ref().unwrap().id, PotionId::FirePotion);
    assert_eq!(counter(&e, RelicId::HappyFlower), 1);
    assert_eq!(counter(&e, RelicId::PenNib), 2);
    assert_eq!(
        e.run
            .master_deck
            .iter()
            .find(|c| c.id == CardId::RitualDagger)
            .unwrap()
            .misc_value,
        20
    );
    assert!(matches!(e.engine(), EngineState::RewardScreen(_)));
    let before_reward = e.run.clone();
    let rewards = match e.engine() {
        EngineState::RewardScreen(r) => r.clone(),
        _ => unreachable!(),
    };
    let gold_index = rewards
        .items
        .iter()
        .position(|r| matches!(r, sts_core::state::rewards::RewardItem::Gold { .. }))
        .unwrap();
    run(&mut e, ClientInput::ClaimReward(gold_index));
    assert!(e.run.gold > before_reward.gold);
    let index = match e.engine() {
        EngineState::RewardScreen(r) => r
            .items
            .iter()
            .position(|r| matches!(r, sts_core::state::rewards::RewardItem::Card { .. }))
            .unwrap(),
        _ => unreachable!(),
    };
    run(&mut e, ClientInput::ClaimReward(index));
    run(&mut e, ClientInput::SelectCard(0));
    assert_eq!(e.run.master_deck.len(), 6);
    let deck = e.run.master_deck.clone();
    let post_reward_rng = e.run.rng_pool.clone();
    run(&mut e, ClientInput::Proceed);
    assert_eq!(e.run.rng_pool, post_reward_rng);
    run(&mut e, ClientInput::SelectMapNode(0));
    assert_eq!(
        e.entry.as_ref().unwrap().encounter,
        Some(EncounterId::Cultist)
    );
    assert_eq!(e.run.floor_num, 2);
    assert_eq!(e.resources().hp, 55);
    assert_eq!(e.resources().max_hp, 84);
    assert_eq!(e.resources().potions, [None, kept]);
    assert_eq!(counter(&e, RelicId::HappyFlower), 2);
    assert_eq!(counter(&e, RelicId::PenNib), 2);
    assert_eq!(
        &*e.active
            .as_ref()
            .unwrap()
            .combat_state
            .meta
            .master_deck_snapshot,
        deck.as_slice()
    );
    assert_eq!(e.cursor.as_ref().unwrap().potion_uses(), 0);
}

#[test]
fn lesson_learned_persists_once_before_next_shuffle() {
    let mut e = NaturalEpisode::from_segment(711, 100, &recipe()).unwrap();
    run(&mut e, ClientInput::SelectMapNode(0));
    play(&mut e, CardId::LessonLearned, 0); // Akabeko makes this a guaranteed lethal.
    play(&mut e, CardId::RitualDagger, 1);
    assert_eq!(e.results.len(), 1);
    let upgraded = e.run.master_deck.iter().filter(|c| c.upgrades == 1).count();
    assert_eq!(upgraded, 4);
    let deck = e.run.master_deck.clone();
    run(&mut e, ClientInput::Proceed);
    run(&mut e, ClientInput::SelectMapNode(0));
    assert_eq!(
        &*e.active
            .as_ref()
            .unwrap()
            .combat_state
            .meta
            .master_deck_snapshot,
        deck.as_slice()
    );
}

#[test]
fn death_escape_and_budget_never_start_another_combat() {
    let mut low = recipe();
    low.hp = 1;
    low.relics.clear();
    low.potions.clear();
    low.route[0].encounter = EncounterId::JawWorm;
    let mut e = NaturalEpisode::from_segment(712, 100, &low).unwrap();
    run(&mut e, ClientInput::SelectMapNode(0));
    while e.stop.is_none() {
        choose(&mut e, |c| matches!(c, ModelChoice::EndTurn {}));
    }
    assert_eq!(e.stop(), Some(&NaturalStop::Death));
    assert_eq!(e.run.floor_num, 1);
    assert!(e.options().unwrap().is_empty());
    let mut smoke = recipe();
    smoke.potions = vec![PotionId::SmokeBomb];
    let mut e = NaturalEpisode::from_segment(712, 100, &smoke).unwrap();
    run(&mut e, ClientInput::SelectMapNode(0));
    choose(&mut e, |c| {
        matches!(c, ModelChoice::UsePotion { slot: 0, .. })
    });
    assert_eq!(e.stop(), Some(&NaturalStop::SegmentEscaped));
    assert!(e.results[0].escaped && !e.results[0].defeated);
    assert!(e.options().unwrap().is_empty());
    let mut e = NaturalEpisode::from_segment(712, 1, &recipe()).unwrap();
    run(&mut e, ClientInput::SelectMapNode(0));
    assert_eq!(e.stop(), Some(&NaturalStop::ActionBudget));
    assert!(e.results.is_empty());
    assert!(e.options().unwrap().is_empty());
}

#[test]
fn complete_segment_replays_all_inputs_numbers_and_states() {
    let mut p = recipe();
    p.route[1] = p.route[0].clone();
    let mut e = NaturalEpisode::from_segment(713, 100, &p).unwrap();
    let mut replay = NaturalEpisode::from_segment(713, 100, &p).unwrap();
    let mut inputs = vec![];
    for _ in 0..2 {
        let start = NaturalAction::Run(ClientInput::SelectMapNode(0));
        inputs.push(start.clone());
        e.choose(e.source(), &start).unwrap();
        for (id, t) in [(CardId::Feed, 0), (CardId::RitualDagger, 1)] {
            let o = e.combat_observation().unwrap().unwrap();
            let a = e.options().unwrap().into_iter().find(|a| matches!(a.combat_choice, Some(ModelChoice::Play {hand, target: Some(target)}) if o.hand[hand].card == id && target == t)).unwrap().action;
            inputs.push(a.clone());
            e.choose(e.source(), &a).unwrap();
        }
        if e.stop.is_none() {
            inputs.push(NaturalAction::Run(ClientInput::Proceed));
            run(&mut e, ClientInput::Proceed);
        }
    }
    let mut fresh = NaturalEpisode::from_segment(713, 100, &p).unwrap();
    for a in inputs {
        assert_eq!(
            fresh.combat_entities().unwrap(),
            replay.combat_entities().unwrap()
        );
        assert_eq!(
            fresh.choose(fresh.source(), &a).unwrap(),
            replay.choose(replay.source(), &a).unwrap()
        );
        assert_eq!(fresh.snapshot(), replay.snapshot());
    }
    assert_eq!(e.snapshot(), replay.snapshot());
    assert_eq!(e.stop(), Some(&NaturalStop::SegmentCompleted));
    assert_eq!(e.results.len(), 2);
    assert_eq!(e.run.current_hp, 60);
    assert!(e.options().unwrap().is_empty());
}

#[test]
fn invalid_segment_cannot_silently_change_room_semantics() {
    let mut p = recipe();
    p.route[1].encounter = EncounterId::GremlinNob;
    assert!(NaturalEpisode::from_segment(1, 20, &p).is_err());
    p.route[1].room = RoomType::MonsterRoomElite;
    assert!(NaturalEpisode::from_segment(1, 20, &p).is_ok());
    p.route.truncate(1);
    assert!(NaturalEpisode::from_segment(1, 20, &p).is_err());
}

#[test]
fn automatic_fairy_consumption_is_carried_without_an_explicit_use_input() {
    let mut p = recipe();
    p.hp = 1;
    p.deck = vec![(CardId::SearingBlow, 10); 5]; // Mechanism sentinel, not learning data.
    p.relics.clear();
    p.potions = vec![PotionId::FairyPotion];
    p.route[0].encounter = EncounterId::JawWorm;
    let mut e = NaturalEpisode::from_segment(712, 100, &p).unwrap();
    run(&mut e, ClientInput::SelectMapNode(0));
    choose(&mut e, |c| matches!(c, ModelChoice::EndTurn {}));
    assert!(e.resources().potions.iter().all(Option::is_none));
    assert!(e.resources().hp > 1);
    play(&mut e, CardId::SearingBlow, 0);
    assert!(e.results[0].defeated);
    assert_eq!(e.results[0].potion_uses, 0);
    let hp = e.run.current_hp;
    run(&mut e, ClientInput::Proceed);
    run(&mut e, ClientInput::SelectMapNode(0));
    assert_eq!(e.resources().hp, hp);
    assert!(e.resources().potions.iter().all(Option::is_none));
}

#[test]
fn enemy_escape_cannot_complete_or_continue_a_defeat_segment() {
    let mut p = recipe();
    p.hp = 75;
    p.deck = vec![(CardId::Defend, 0); 5];
    p.relics.clear();
    p.potions.clear();
    p.route[0].encounter = EncounterId::Looter;
    let mut e = NaturalEpisode::from_segment(714, 100, &p).unwrap();
    run(&mut e, ClientInput::SelectMapNode(0));
    while e.stop.is_none() {
        let o = e.options().unwrap();
        let action = o
            .iter()
            .find(|o| matches!(o.combat_choice, Some(ModelChoice::Play { .. })))
            .or_else(|| {
                o.iter()
                    .find(|o| matches!(o.combat_choice, Some(ModelChoice::EndTurn {})))
            })
            .unwrap()
            .action
            .clone();
        e.choose(e.source(), &action).unwrap();
    }
    assert_eq!(e.stop(), Some(&NaturalStop::SegmentNotDefeated));
    assert_eq!(e.results.len(), 1);
    assert!(e.run.current_hp > 0);
    assert!(e.run.room_mugged);
    assert!(!e.results[0].defeated);
    assert!(e.options().unwrap().is_empty());
}
