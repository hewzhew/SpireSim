use super::{NaturalAction, NaturalOption};
use sts_core::content::identity::gameplay_display_label_v1 as name;
use sts_core::content::relics::RelicId;
use sts_core::state::core::{ClientInput as I, EngineState as E};
use sts_core::state::rewards::RewardItem;
use sts_core::state::run::RunState;
use sts_core::state::selection::SelectionTargetRef;

// This experiment adapts shared gameplay candidates/predicates only. It has no
// ranking, preferred route, mandatory purchase or default reward selection.
pub(super) fn options(run: &RunState, engine: &E) -> Result<Vec<NaturalOption>, String> {
    let mut out = vec![];
    let mut add = |input, label: String| {
        out.push(NaturalOption {
            action: NaturalAction::Run(input),
            label,
            combat_choice: None,
        })
    };
    let has = |id| run.relics.iter().any(|r| r.id == id);
    match engine {
        E::EventRoom => {
            for (i, o) in sts_core::engine::event_handler::get_event_options(run)
                .iter()
                .enumerate()
            {
                if !o.ui.disabled {
                    add(I::EventChoice(i), o.ui.text.clone());
                }
            }
        }
        E::MapNavigation | E::MapOverlay { .. } => {
            let y = run.map.current_y + 1;
            if let Some(row) = run.map.graph.get(y as usize) {
                let flight = run
                    .relics
                    .iter()
                    .any(|r| r.id == RelicId::WingBoots && !r.used_up && r.counter > 0);
                for n in row {
                    use sts_core::state::map::node::RoomType;
                    let room = match n.class {
                        Some(RoomType::MonsterRoom) => "Monster",
                        Some(RoomType::MonsterRoomElite) => "Elite",
                        Some(RoomType::RestRoom) => "Campfire",
                        Some(RoomType::ShopRoom) => "Shop",
                        Some(RoomType::EventRoom) => "?",
                        Some(RoomType::TreasureRoom) => "Treasure",
                        Some(RoomType::MonsterRoomBoss) => "Boss",
                        _ => "Unknown room",
                    };
                    let label = format!(
                        "F{} x{} {}{}",
                        y + 1,
                        n.x,
                        room,
                        if n.has_emerald_key { " (burning)" } else { "" }
                    );
                    if run.map.can_travel_to(n.x, y, false) {
                        add(I::SelectMapNode(n.x as usize), label);
                    } else if flight && run.map.can_travel_to(n.x, y, true) {
                        add(I::FlyToNode(n.x as usize, y as usize), label);
                    }
                }
            }
            if matches!(engine, E::MapOverlay { .. }) {
                add(I::Cancel, "Back".into());
            }
        }
        E::RewardScreen(r)
        | E::RewardOverlay {
            reward_state: r, ..
        } => {
            if let Some(cards) = &r.pending_card_choice {
                for (i, c) in cards.iter().enumerate() {
                    add(
                        I::SelectCard(i),
                        format!("{}{}", name(c.id), if c.upgrades > 0 { "+" } else { "" }),
                    );
                }
                if has(RelicId::SingingBowl) {
                    add(I::SelectCard(cards.len()), "Singing Bowl: +2 max HP".into());
                }
                add(I::Cancel, "Back to rewards".into());
            } else {
                for (i, item) in r.items.iter().enumerate() {
                    let label = match item {
                        RewardItem::Gold { amount } | RewardItem::StolenGold { amount } => {
                            format!("Gold {amount}")
                        }
                        RewardItem::Card { cards } => format!(
                            "Cards: {}",
                            cards
                                .iter()
                                .map(|c| name(c.id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                        RewardItem::Relic { relic_id } => name(*relic_id).into(),
                        RewardItem::Potion { potion_id } => {
                            if run.find_empty_potion_slot().is_none() && !has(RelicId::Sozu) {
                                continue;
                            }
                            name(*potion_id).into()
                        }
                        RewardItem::EmeraldKey => "Emerald Key".into(),
                        RewardItem::SapphireKey => "Sapphire Key".into(),
                    };
                    add(I::ClaimReward(i), label);
                }
                add(I::Proceed, "Proceed".into());
            }
        }
        E::Campfire => {
            use sts_core::engine::campfire_candidates::{
                legal_campfire_candidates, resolve_campfire_candidate,
            };
            for c in legal_campfire_candidates(run) {
                let choice = resolve_campfire_candidate(run, c).map_err(|e| format!("{e:?}"))?;
                let label = match choice {
                    sts_core::state::core::CampfireChoice::Smith(i) => {
                        format!("Upgrade {}", name(run.master_deck[i].id))
                    }
                    sts_core::state::core::CampfireChoice::Toke(i) => {
                        format!("Remove {}", name(run.master_deck[i].id))
                    }
                    _ => format!("{choice:?}"),
                };
                add(I::CampfireOption(choice), label);
            }
        }
        E::RunPendingChoice(c) => {
            if c.min_choices != c.max_choices {
                return Err(
                    "variable-size strategic selection not admitted by natural experiment".into(),
                );
            }
            let request = c.selection_request(run);
            for (i, card) in run.master_deck.iter().enumerate() {
                if request
                    .targets
                    .contains(&SelectionTargetRef::CardLineage(card.lineage))
                {
                    add(
                        I::SelectCard(i),
                        format!(
                            "{:?} {}{}",
                            c.reason,
                            name(card.id),
                            if card.upgrades > 0 { "+" } else { "" }
                        ),
                    );
                }
            }
            if request.can_cancel {
                add(I::Cancel, "Cancel".into());
            }
        }
        E::Shop(s) => {
            for (i, c) in s.cards.iter().enumerate() {
                if c.can_buy && c.price <= run.gold {
                    add(
                        I::BuyCard(i),
                        format!("Buy {}: {} gold", name(c.card_id), c.price),
                    );
                }
            }
            for (i, r) in s.relics.iter().enumerate() {
                if r.can_buy && r.price <= run.gold {
                    add(
                        I::BuyRelic(i),
                        format!("Buy {}: {} gold", name(r.relic_id), r.price),
                    );
                }
            }
            for (i, p) in s.potions.iter().enumerate() {
                if p.can_buy && p.price <= run.gold && run.find_empty_potion_slot().is_some() {
                    add(
                        I::BuyPotion(i),
                        format!("Buy {}: {} gold", name(p.potion_id), p.price),
                    );
                }
            }
            if s.purge_available && s.purge_cost <= run.gold {
                for (i, c) in run.master_deck.iter().enumerate() {
                    if sts_core::state::core::master_deck_card_is_purgeable(c)
                        && !sts_core::state::core::master_deck_card_is_bottled(c, &run.relics)
                    {
                        add(
                            I::PurgeCard(i),
                            format!("Remove {}: {} gold", name(c.id), s.purge_cost),
                        );
                    }
                }
            }
            if s.pending_reward_overlay.is_some() {
                add(I::OpenRewardOverlay, "Rewards".into());
            }
            add(I::Proceed, "Leave shop".into());
        }
        E::TreasureRoom(_) => add(I::OpenChest, "Open chest".into()),
        E::BossRelicSelect(c) => {
            for (i, r) in c.relics.iter().enumerate() {
                add(I::SubmitRelicChoice(i), name(*r).into());
            }
            add(I::Proceed, "Skip".into());
        }
        E::GameOver(_) => return Ok(out),
        _ => {
            return Err(format!(
                "natural boundary not admitted: {}",
                engine.kind_v1()
            ))
        }
    }
    let event = run
        .event_state
        .as_ref()
        .is_some_and(|e| e.id == sts_core::state::events::EventId::WeMeetAgain);
    for (i, p) in run.potions.iter().enumerate() {
        if let Some(p) = p {
            if p.can_use && sts_core::content::potions::potion_can_use_out_of_combat(p.id, event) {
                add(
                    I::UsePotion {
                        potion_index: i,
                        target: None,
                    },
                    format!("Use {}", name(p.id)),
                );
            }
            if p.can_discard && sts_core::content::potions::potion_can_discard_in_event(event) {
                add(I::DiscardPotion(i), format!("Discard {}", name(p.id)));
            }
        }
    }
    Ok(out)
}
