//! Small, declared setup policy for the early-run diagnostic. Scores are
//! provisional preferences, not learned values, optimality labels or mechanics.
use super::super::{NaturalAction, NaturalEpisode, NaturalOption};
use std::collections::HashMap;
use sts_core::content::{
    cards::{get_card_definition, CardId, CardType},
    events::neow::{choice_identity, NeowDrawback, NeowRewardType},
    potions::PotionId,
    relics::RelicId,
};
use sts_core::state::{
    core::{CampfireChoice, ClientInput as I, EngineState as E, RunPendingChoiceReason as R},
    events::{EventActionKind, EventId},
    map::node::RoomType,
    rewards::RewardItem,
    run::RunState,
};

#[derive(Default)]
pub(super) struct SetupPolicy {
    declined_offer: bool,
    visited_campfire: bool,
}

fn card_score(run: &RunState, card: CardId) -> i32 {
    card_assessment(run, card).0
}

fn card_assessment(run: &RunState, card: CardId) -> (i32, bool) {
    use CardId::*;
    let attacks = run
        .master_deck
        .iter()
        .filter(|c| {
            c.id != Strike
                && c.id != Bash
                && get_card_definition(c.id).card_type == CardType::Attack
        })
        .count();
    let copies = run.master_deck.iter().filter(|c| c.id == card).count() as i32;
    let mut specific = true;
    let base = match card {
        Immolate => 95,
        Carnage | Bludgeon | Uppercut | Hemokinesis => 75,
        PommelStrike | ShrugItOff | Shockwave | Disarm => 70,
        BattleTrance | Offering | Feed | FlameBarrier => 65,
        Cleave | Whirlwind | PowerThrough | Inflame => 60,
        Headbutt | IronWave | Anger | TwinStrike | PerfectedStrike => 55,
        TrueGrit | Armaments | BurningPact | FlashOfSteel => 50,
        Apotheosis => 100,
        Corruption | FeelNoPain | DarkEmbrace => 35,
        Strike | Defend | Bash => 0,
        _ => {
            specific = false;
            match get_card_definition(card).card_type {
                CardType::Attack => 40,
                CardType::Skill => 30,
                _ => 15,
            }
        }
    };
    (
        base - copies * 25
            + if get_card_definition(card).card_type == CardType::Attack {
                if attacks < 2 {
                    20
                } else if attacks >= 4 {
                    -25
                } else {
                    0
                }
            } else {
                0
            },
        specific,
    )
}

fn remove_score(card: CardId) -> i32 {
    if get_card_definition(card).card_type == CardType::Curse {
        100
    } else if card == CardId::Strike {
        70
    } else if card == CardId::Defend {
        60
    } else {
        0
    }
}

fn upgrade_score(run: &RunState, card: CardId) -> i32 {
    match card {
        CardId::Bash => 85,
        CardId::Armaments | CardId::PommelStrike | CardId::Uppercut => 95,
        CardId::Defend | CardId::Strike => 10,
        _ => card_score(run, card) + 25,
    }
}

impl SetupPolicy {
    pub(super) fn choose(
        &mut self,
        episode: &NaturalEpisode,
        options: &[NaturalOption],
    ) -> Result<(usize, String, Vec<super::SetupCandidateScore>), String> {
        let run = episode.run();
        let engine = episode.engine();
        if !matches!(engine, E::RewardScreen(_) | E::RewardOverlay { .. }) {
            self.declined_offer = false;
        }
        let mut scores: Vec<(usize, i32, &str)> = Vec::new();
        let mut memo = HashMap::new();
        for (i, option) in options.iter().enumerate() {
            let NaturalAction::Run(input) = &option.action else {
                continue;
            };
            let scored = match (engine,input) {
                (E::EventRoom, I::EventChoice(index)) => {
                    let event = run.event_state.as_ref().ok_or("missing event")?;
                    if event.id == EventId::Neow {
                        if event.current_screen == 1 {
                            choice_identity(event,*index).and_then(|(reward,drawback)| (drawback == NeowDrawback::None).then_some((match reward {
                                NeowRewardType::RandomCommonRelic => 70, NeowRewardType::ThreeCards => 65,
                                NeowRewardType::OneRandomRareCard => 60, NeowRewardType::HundredGold => 55,
                                _ => 45,
                            }, "Neow: prefer a declared no-drawback benefit")))
                        } else { Some((50,"Advance Neow dialogue")) }
                    } else {
                        let event_options = sts_core::engine::event_handler::get_event_options(run);
                        event_options.get(*index).and_then(|o| match o.semantics.action {
                            EventActionKind::Leave | EventActionKind::Decline => Some((60,"Event fallback: leave without taking an unassessed offer")),
                            EventActionKind::Continue if event_options.iter().filter(|o| !o.ui.disabled).count() == 1 => Some((50,"Continue the only event transition")),
                            _ => None,
                        })
                    }
                }
                (E::MapNavigation | E::MapOverlay { .. }, I::SelectMapNode(x)) => {
                    let fights = episode.results().iter().filter(|r| r.defeated).count().min(4);
                    route_score(run,*x, (run.map.current_y+1) as usize, fights, self.visited_campfire, &mut memo)
                        .map(|s|(s,"Visible route: develop through fights and a campfire before a F6-F8 elite"))
                }
                (E::RewardScreen(rewards) | E::RewardOverlay { reward_state: rewards, .. }, I::ClaimReward(index)) => {
                    match &rewards.items[*index] {
                        RewardItem::Gold { .. } | RewardItem::StolenGold { .. } => Some((100,"Collect gold")),
                        RewardItem::Relic { .. } => Some((90,"Collect the offered relic")),
                        RewardItem::Potion { .. } => Some((80,"Fill an available potion slot")),
                        RewardItem::Card { .. } if !self.declined_offer => Some((70,"Inspect the card reward")),
                        _ => None,
                    }
                }
                (E::RewardScreen(rewards) | E::RewardOverlay { reward_state: rewards, .. }, I::SelectCard(index)) => {
                    rewards.pending_card_choice.as_ref().and_then(|cards| cards.get(*index))
                        .map(|card| { let (score,specific)=card_assessment(run,card.id); (score,if specific {
                            "Card reward: named early utility rule, adjusted for duplicates and attack count"
                        } else { "Card reward: generic card-type fallback; individual mechanics unassessed" }) })
                }
                (E::RewardScreen(_) | E::RewardOverlay { .. }, I::Cancel) => Some((40,"Skip this card offer")),
                (E::RewardScreen(_) | E::RewardOverlay { .. }, I::Proceed) => Some((0,"Leave after collecting useful rewards")),
                (E::Campfire, I::CampfireOption(CampfireChoice::Rest)) => Some((if run.current_hp*100 < run.max_hp*55 { 150 } else { 0 },"Rest below 55% HP")),
                (E::Campfire, I::CampfireOption(CampfireChoice::Smith(index))) => Some((upgrade_score(run,run.master_deck[*index].id),"Upgrade early combat utility")),
                (E::Campfire, I::CampfireOption(CampfireChoice::Recall)) => Some((-1,"Recall if rest and upgrades are unavailable")),
                (E::RunPendingChoice(choice), I::SelectCard(index)) => {
                    let id = run.master_deck[*index].id;
                    let score = match choice.reason {
                        R::Purge | R::PurgeNonBottled | R::Transform | R::TransformNonBottled | R::TransformUpgraded => remove_score(id),
                        R::Upgrade => upgrade_score(run,id),
                        R::Duplicate | R::BottleFlame | R::BottleLightning | R::BottleTornado => card_score(run,id),
                    };
                    Some((score,"Select an eligible deck card using the declared early utility rule"))
                }
                (E::Shop(shop), I::BuyCard(index)) => {
                    let score = card_score(run,shop.cards[*index].card_id);
                    (score >= 65).then_some((score,"Buy a high-priority early card"))
                }
                (E::Shop(shop), I::BuyRelic(index)) => matches!(shop.relics[*index].relic_id,
                    RelicId::MembershipCard | RelicId::BagOfPreparation | RelicId::Vajra | RelicId::Anchor)
                    .then_some((90,"Buy a listed early utility relic")),
                (E::Shop(shop), I::BuyPotion(index)) => matches!(shop.potions[*index].potion_id,
                    PotionId::FirePotion | PotionId::ExplosivePotion | PotionId::FearPotion | PotionId::AttackPotion | PotionId::StrengthPotion)
                    .then_some((60,"Fill a slot with a listed early combat potion")),
                (E::Shop(_), I::PurgeCard(index)) => (remove_score(run.master_deck[*index].id) >= 60)
                    .then_some((45,"Remove a curse or starter after priority purchases")),
                (E::Shop(_), I::Proceed) => Some((0,"Leave shop after listed purchases")),
                (E::TreasureRoom(_), I::OpenChest) => Some((50,"Open chest")),
                (_, I::UsePotion { potion_index, .. }) if run.potions[*potion_index].as_ref().is_some_and(|p| p.id == PotionId::FruitJuice) => Some((110,"Use Fruit Juice to gain max HP and free its slot")),
                _ => None,
            };
            if let Some((score, reason)) = scored {
                scores.push((i, score, reason));
            }
        }
        let (index, _, reason) = scores
            .iter()
            .copied()
            .max_by_key(|(i, s, _)| (*s, std::cmp::Reverse(*i)))
            .ok_or_else(|| {
                format!(
                    "no declared rule for {} ({} legal options)",
                    engine.kind_v1(),
                    options.len()
                )
            })?;
        if matches!(options[index].action, NaturalAction::Run(I::Cancel))
            && matches!(engine, E::RewardScreen(_) | E::RewardOverlay { .. })
        {
            self.declined_offer = true;
        }
        if matches!(
            options[index].action,
            NaturalAction::Run(I::CampfireOption(_))
        ) {
            self.visited_campfire = true;
        }
        Ok((
            index,
            reason.into(),
            scores
                .into_iter()
                .map(|(option, score, reason)| super::SetupCandidateScore {
                    option,
                    score,
                    reason: reason.into(),
                })
                .collect(),
        ))
    }
}

type RouteMemo = HashMap<(usize, usize, usize, bool), Option<i32>>;
fn route_score(
    run: &RunState,
    x: usize,
    y: usize,
    fights: usize,
    rested: bool,
    memo: &mut RouteMemo,
) -> Option<i32> {
    if y >= 8 {
        return None;
    }
    let key = (x, y, fights, rested);
    if let Some(score) = memo.get(&key) {
        return *score;
    }
    let n = run.map.graph.get(y)?.get(x)?;
    if n.class == Some(RoomType::MonsterRoomElite) {
        return (y >= 5).then_some(
            100 + fights.min(3) as i32 * 12 + i32::from(rested) * 20
                - i32::from(n.has_emerald_key) * 25,
        );
    }
    let (bonus, next_fights, next_rest) = match n.class? {
        RoomType::MonsterRoom => (
            if fights < 3 { 10 } else { -6 },
            (fights + 1).min(4),
            rested,
        ),
        RoomType::RestRoom => (if rested { 0 } else { 18 }, fights, true),
        RoomType::ShopRoom => (if run.gold >= 150 { 8 } else { -8 }, fights, rested),
        RoomType::EventRoom => (-12, fights, rested),
        _ => (0, fights, rested),
    };
    let best = n
        .edges
        .iter()
        .filter_map(|edge| {
            route_score(
                run,
                edge.dst_x as usize,
                edge.dst_y as usize,
                next_fights,
                next_rest,
                memo,
            )
        })
        .max()
        .map(|s| s + bonus);
    memo.insert(key, best);
    best
}
