use super::*;
use crate::natural::SegmentRoom;
use crate::observation::ModelChoice;
use sts_core::content::{cards::CardId, monsters::factory::EncounterId, relics::RelicId};

fn spec() -> SegmentTaskSpec {
    SegmentTaskSpec {
        seed: 921001,
        recipe: SegmentRecipe {
            hp: 60,
            deck: vec![(CardId::SearingBlow, 10); 5], // Plumbing sentinel, never training material.
            relics: vec![RelicId::HappyFlower],
            potions: vec![PotionId::FirePotion],
            route: vec![
                SegmentRoom {
                    encounter: EncounterId::Cultist,
                    room: RoomType::MonsterRoom,
                },
                SegmentRoom {
                    encounter: EncounterId::GremlinNob,
                    room: RoomType::MonsterRoomElite,
                },
            ],
        },
        requirement: SegmentRequirement {
            min_hp: 70,
            reserve: vec![PotionRequirement {
                potion: PotionId::FirePotion,
                count: 1,
            }],
        },
        between_rooms: BetweenRooms::GoldOnly,
    }
}
fn play(t: &mut SegmentTask) {
    let index = t
        .episode
        .options()
        .unwrap()
        .iter()
        .position(|o| matches!(o.combat_choice, Some(ModelChoice::Play { .. })))
        .unwrap();
    t.advance(t.source(), index).unwrap();
}

#[test]
fn same_combat_has_distinct_goals_and_future_context_without_changing_legal_actions() {
    let a = SegmentTask::new(spec(), 40).unwrap();
    let mut other = spec();
    other.requirement.min_hp = 40;
    other.requirement.reserve.clear();
    other.recipe.route[1].encounter = EncounterId::Lagavulin;
    let b = SegmentTask::new(other, 40).unwrap();
    let before = a.episode.snapshot();
    let pa = a.decision().unwrap().unwrap();
    let pb = b.decision().unwrap().unwrap();
    assert_eq!(pa.combat, pb.combat);
    assert_ne!(pa.context_ids, pb.context_ids);
    assert_ne!(pa.context_values[0], pb.context_values[0]);
    assert_eq!(a.episode.snapshot(), before); // Reading cannot consume RNG.
    assert_eq!(a.target().unwrap(), None);
    assert_eq!(pa.context_ids.iter().filter(|x| x[0] == 2).count(), 2);
}

#[test]
fn task_credits_the_final_settled_segment_and_records_fixed_intermission() {
    let mut t = SegmentTask::new(spec(), 40).unwrap();
    let first = t.source();
    play(&mut t);
    assert_eq!(t.episode.run().floor_num, 2);
    assert_eq!(t.episode.resources().hp, 66);
    assert!(t.episode.run().gold > 99);
    assert_eq!(t.episode.run().master_deck.len(), 5);
    assert_eq!(t.episode.resources().potions.iter().flatten().count(), 1);
    let p = t.decision().unwrap().unwrap();
    assert_eq!(p.context_ids.iter().filter(|x| x[0] == 2).count(), 1);
    assert_eq!(p.context_values[0][1], 1. / 8.);
    assert_eq!(t.target().unwrap(), None);
    let snapshot = t.episode.snapshot();
    assert!(t.advance(first, 0).is_err());
    assert!(t.advance(t.source(), usize::MAX).is_err());
    assert_eq!(t.episode.snapshot(), snapshot);
    play(&mut t);
    assert!(t.decision().unwrap().is_none());
    assert_eq!(t.episode.resources().hp, 72);
    assert_eq!(t.target().unwrap(), Some(true));
    t.verify().unwrap();
    let mut higher = t.spec.requirement.clone();
    higher.min_hp = 73;
    assert_eq!(higher.assess(&t.episode).unwrap(), Some(false));
    assert!(t.transitions.iter().any(|r| matches!(
        r.action,
        NaturalAction::Run(ClientInput::ClaimReward(_))
    ) && r.decision.is_none()));
    assert_eq!(
        t.transitions
            .iter()
            .filter(|r| r.decision.is_some())
            .count(),
        2
    );
    t.transitions[0].after.push('x');
    assert!(t.verify().is_err());
}

#[test]
fn potion_requirements_are_type_multiplicities_and_not_action_bans() {
    let mut t = SegmentTask::new(spec(), 40).unwrap();
    let potion = t
        .episode
        .options()
        .unwrap()
        .iter()
        .position(|o| {
            matches!(
                o.combat_choice,
                Some(ModelChoice::UsePotion { slot: 0, .. })
            )
        })
        .unwrap();
    t.advance(t.source(), potion).unwrap();
    assert_eq!(t.target().unwrap(), None);
    play(&mut t);
    play(&mut t);
    assert_eq!(t.target().unwrap(), Some(false));
    t.verify().unwrap();
    // Regenerated/replaced inventory meets the requirement; identity is not part of it.
    t.episode.run.potions[0] = Some(sts_core::content::potions::Potion::new(
        PotionId::FirePotion,
        999,
    ));
    assert_eq!(t.target().unwrap(), Some(true));
    t.spec.requirement.reserve[0].count = 2;
    assert_eq!(t.target().unwrap(), Some(false));
}

#[test]
fn budget_at_a_room_boundary_is_unknown_even_if_resources_meet_requirements() {
    let mut s = spec();
    s.requirement.min_hp = 1;
    let mut t = SegmentTask::new(s, 2).unwrap(); // First map choice + first combat choice.
    play(&mut t);
    assert_eq!(t.episode.stop(), Some(&NaturalStop::ActionBudget));
    assert_eq!(t.episode.results().len(), 1);
    assert_eq!(t.target().unwrap(), None);
    assert!(t.decision().unwrap().is_none());
    t.verify().unwrap(); // Reproducible Unknown is still not a training outcome.
}

#[test]
fn pending_permanent_changes_bind_to_typed_master_deck_sources() {
    let mut t = SegmentTask::new(spec(), 40).unwrap();
    let lineage = t.episode.run.master_deck[2].lineage;
    t.episode
        .active
        .as_mut()
        .unwrap()
        .combat_state
        .meta
        .meta_changes = vec![
        MetaChange::ModifyCardMisc {
            card_lineage: lineage,
            amount: 5,
        },
        MetaChange::UpgradeMasterDeckCard {
            card_lineage: lineage,
        },
    ];
    let p = t.decision().unwrap().unwrap();
    let master_row = p
        .context_ids
        .iter()
        .enumerate()
        .filter(|(_, x)| x[0] == 4)
        .nth(2)
        .unwrap()
        .0;
    let change_rows = p
        .context_ids
        .iter()
        .enumerate()
        .filter(|(_, x)| x[0] >= 5)
        .map(|(i, _)| i)
        .collect::<Vec<_>>();
    assert_eq!(change_rows.len(), 2);
    for row in change_rows {
        assert_eq!(p.context_refs[row], [master_row as i64, -1, -1]);
    }
    t.episode.run.master_deck.remove(2);
    assert!(t.decision().is_err()); // Cannot rebind to another same-named copy.
}

#[test]
fn reserve_set_order_is_not_a_feature_and_invalid_requirements_are_rejected() {
    let mut a = spec();
    a.requirement.reserve.push(PotionRequirement {
        potion: PotionId::ExplosivePotion,
        count: 1,
    });
    let mut b = a.clone();
    b.requirement.reserve.reverse();
    assert_eq!(
        SegmentTask::new(a.clone(), 30).unwrap().decision().unwrap(),
        SegmentTask::new(b, 30).unwrap().decision().unwrap()
    );
    a.requirement.reserve.push(a.requirement.reserve[0].clone());
    assert!(SegmentTask::new(a, 30).is_err());
    for hp in [0, -1, 1000] {
        let mut s = spec();
        s.requirement.min_hp = hp;
        assert!(SegmentTask::new(s, 30).is_err());
    }
}
