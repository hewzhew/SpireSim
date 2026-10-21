//! Replaceable decision facts and rendering. Never a replay input or parity claim.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sts_oracle_run_control::{
    agent::information::monster_move::{
        public_monster_move_knowledge_v1, MonsterMoveEffectV1, MonsterMoveKnowledgeV1,
    },
    agent::information::state::{
        public_combat_state_v1, CombatLearningCardV1, CombatLearningEnemyIdentityV1,
        CombatLearningPowerV1, PublicCombatStateV1,
    },
    ai::planner_core::{PlannerPublicMap, PlannerShopInventoryV1},
    content::{cards::CardId, identity::gameplay_display_label_v1 as label},
    eval::run_control::{planner_public_map_v1, planner_shop_inventory_v1, RunControlSession},
    runtime::combat::Intent,
    state::core::{EngineState, GridSelectReason, HandSelectReason, PendingChoice, PileType},
};
use sts_parity_lockstep::{
    NaturalRunKeysV2, NaturalRunRewardItemV1, NaturalRunRewardsV2, NaturalRunStateV2,
    ParityDomainV1, ParitySnapshotV1, SemanticActionV1, SemanticChoiceV1, SemanticScalarV1,
};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DecisionStateV1 {
    pub run: DecisionRunV1,
    pub map: PlannerPublicMap,
    pub combat: Option<PublicCombatStateV1>,
    /// Public pile order with the bindings already established by the adapters.
    /// The draw pile is deliberately absent from this decision-facing view.
    pub card_sources: std::collections::BTreeMap<String, Vec<SemanticChoiceV1>>,
    pub monster_moves: Vec<MonsterMoveKnowledgeV1>,
    pub selection: Option<PendingChoice>,
    pub hand_candidates: Option<Vec<CombatLearningCardV1>>,
    pub boundary: String,
    pub deck: Value,
    pub relics: Value,
    pub potions: Value,
    pub offers: Vec<SemanticActionV1>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rewards: Option<NaturalRunRewardsV2>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shop: Option<PlannerShopInventoryV1>,
    pub room: Value,
    pub back: Option<String>,
}

/// Decision-facing run facts. Deterministic pools and schedules stay in the
/// parity snapshot and journal, not in ordinary inspection.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct DecisionRunV1 {
    pub ascension: u64,
    pub character: String,
    pub act: u64,
    pub floor: i64,
    pub current_hp: i64,
    pub max_hp: i64,
    pub gold: i64,
    pub keys: NaturalRunKeysV2,
    pub shop_purge_base_cost: i64,
}

impl From<NaturalRunStateV2> for DecisionRunV1 {
    fn from(run: NaturalRunStateV2) -> Self {
        Self {
            ascension: run.ascension,
            character: run.character,
            act: run.act,
            floor: run.floor,
            current_hp: run.current_hp,
            max_hp: run.max_hp,
            gold: run.gold,
            keys: run.keys,
            shop_purge_base_cost: run.shop_purge_base_cost,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionSectionV1 {
    #[default]
    Position,
    Map,
    Potions,
    Shop,
    Removal,
    Board,
}

impl DecisionStateV1 {
    pub fn capture(
        session: &RunControlSession,
        snapshot: &ParitySnapshotV1,
    ) -> Result<Self, String> {
        let decode = |domain| crate::manual::decode_domain(snapshot, domain);
        let actions = sts_parity_lockstep::SemanticActionSpaceV1::from_snapshot(snapshot)?;
        let mut room: Value = decode(ParityDomainV1::Room)?;
        crate::decision_match::redact(&mut room);
        let combat = session
            .active_combat
            .as_ref()
            .map(|active| public_combat_state_v1(&active.combat_state));
        let mut card_sources = std::collections::BTreeMap::new();
        if combat.is_some() {
            if let sts_parity_lockstep::NaturalRunCombatV4::PlayerDecision { state } =
                crate::manual::decode_domain(snapshot, ParityDomainV1::Combat)?
            {
                for (zone, cards, choice) in [
                    (
                        "hand",
                        state.zones.hand,
                        sts_parity_lockstep::natural_hand_card_choice_v1 as fn(&str, &str, u8) -> _,
                    ),
                    (
                        "discard",
                        state.zones.discard_bottom_first,
                        sts_parity_lockstep::natural_discard_card_choice_v1,
                    ),
                    (
                        "exhaust",
                        state.zones.exhaust_by_binding,
                        sts_parity_lockstep::natural_exhaust_card_choice_v1,
                    ),
                ] {
                    card_sources.insert(
                        zone.into(),
                        cards
                            .iter()
                            .map(|card| choice(&card.binding, &card.card_id, card.upgrades))
                            .collect::<Result<Vec<_>, _>>()?,
                    );
                }
            }
        }
        let hand_candidates = match (&session.engine_state, &session.active_combat, &combat) {
            (
                EngineState::PendingChoice(PendingChoice::HandSelect {
                    candidate_lineages, ..
                }),
                Some(active),
                Some(public),
            ) => {
                // Both vectors are the same Rust hand observation. Lineages
                // filter the real candidates; they never bind a Java identity.
                Some(
                    active
                        .combat_state
                        .zones
                        .hand
                        .iter()
                        .zip(&public.cards.hand.cards)
                        .filter(|(card, _)| candidate_lineages.contains(&card.lineage))
                        .map(|(_, card)| card.clone())
                        .collect(),
                )
            }
            _ => None,
        };
        Ok(Self {
            run: crate::manual::decode_domain::<NaturalRunStateV2>(snapshot, ParityDomainV1::Run)?.into(),
            map: planner_public_map_v1(session),
            // The paused Rust combat remains real even where the parity
            // contract has not projected it. Reuse the gameplay observation.
            monster_moves: combat.as_ref().map(|c| c.monsters.iter()
                .filter_map(public_monster_move_knowledge_v1).collect()).unwrap_or_default(),
            combat,
            card_sources,
            hand_candidates,
            selection: match &session.engine_state {
                EngineState::PendingChoice(choice) => Some(choice.clone()),
                _ => None,
            },
            boundary: snapshot.boundary_kind.clone(),
            shop: match &session.engine_state {
                EngineState::Shop(shop) => Some(planner_shop_inventory_v1(session, shop)),
                _ => None,
            },
            deck: decode(ParityDomainV1::MasterDeck)?,
            relics: decode(ParityDomainV1::Relics)?,
            potions: decode(ParityDomainV1::Potions)?,
            // Visible inventory is independent of executable pickup actions.
            // In particular a full potion belt must not hide a dropped potion.
            rewards: if matches!(snapshot.boundary_kind.as_str(), "natural_combat_reward" | "natural_treasure_reward") {
                Some(crate::manual::decode_domain(snapshot, ParityDomainV1::Rewards)?)
            } else { None },
            offers: actions.actions.into_iter().map(|a| a.action).filter(|a| matches!(a,
                SemanticActionV1::Select { choice, .. } if matches!(choice.kind.as_str(), "run_reward" | "reward_card" | "discovery_card")
            )).collect(),
            room,
            back: match &session.engine_state {
                EngineState::MapOverlay { return_state } => Some(match return_state.as_ref() {
                    EngineState::RewardScreen(_) => "Back to rewards",
                    EngineState::EventRoom => "Back to event",
                    _ => "Back to room",
                }.into()),
                _ => None,
            },
        })
    }

    /// Compare field values before rendering. Display wording, line order and
    /// identifiers in the action menu do not determine what changed.
    pub fn render(&self, before: Option<&Self>, detail: bool) -> Result<String, String> {
        self.render_inner(before, detail, true)
    }

    pub fn render_map(&self) -> String {
        crate::decision_map::render(&self.map, self.run.act)
    }

    pub fn render_potions(&self) -> Result<String, String> {
        let slots = self
            .potions
            .as_array()
            .ok_or("decision potions are not an array")?;
        let held = slots
            .iter()
            .filter_map(|p| {
                p["id"].as_str().filter(|id| *id != "empty").map(|id| {
                    format!(
                        "P{} {}",
                        p["slot"].as_u64().unwrap_or_default(),
                        crate::presentation_names::name::<
                            sts_oracle_run_control::content::potions::PotionId,
                        >(id)
                    )
                })
            })
            .collect::<Vec<_>>();
        Ok(format!(
            "Potions: {}",
            if held.is_empty() {
                format!("empty ({} slots)", slots.len())
            } else {
                held.join(", ")
            }
        ))
    }

    /// A caller showing a separate action menu already displays these offers.
    pub fn render_with_action_menu(
        &self,
        before: Option<&Self>,
        detail: bool,
    ) -> Result<String, String> {
        self.render_inner(before, detail, false)
    }

    fn render_inner(
        &self,
        before: Option<&Self>,
        detail: bool,
        include_offers: bool,
    ) -> Result<String, String> {
        let mut lines = Vec::new();
        let resources = self.resources();
        if changed(&resources, before.map(Self::resources).as_ref()) {
            let (act, floor, hp, max_hp, gold) = resources;
            lines.push(format!(
                "Act {act} Floor {floor}, HP {hp}/{max_hp}, Gold {gold}"
            ));
        }
        if changed(&self.run.keys, before.map(|b| &b.run.keys)) {
            let owned = [
                ("Ruby", self.run.keys.ruby),
                ("Emerald", self.run.keys.emerald),
                ("Sapphire", self.run.keys.sapphire),
            ]
            .into_iter()
            .filter_map(|(name, owned)| owned.then_some(name))
            .collect::<Vec<_>>();
            if !owned.is_empty() || before.is_some() {
                lines.push(format!(
                    "Keys: {}",
                    if owned.is_empty() {
                        "none".into()
                    } else {
                        owned.join(", ")
                    }
                ));
            }
        }
        let combat_before = before.and_then(|b| b.combat.as_ref());
        if let Some(c) = &self.combat {
            let turn = (c.turn.turn_count, c.turn.energy, c.player.block);
            if changed(
                &turn,
                combat_before
                    .map(|b| (b.turn.turn_count, b.turn.energy, b.player.block))
                    .as_ref(),
            ) {
                lines.push(format!(
                    "Turn {}: Energy {}, Block {}",
                    c.turn.turn_count + 1,
                    c.turn.energy,
                    c.player.block
                ));
            }
            if changed(&c.player.powers, combat_before.map(|b| &b.player.powers))
                && (!c.player.powers.is_empty()
                    || combat_before.is_some_and(|b| !b.player.powers.is_empty()))
            {
                lines.push(format!("Powers: {}", powers(&c.player.powers)));
            }
            let roster_changed = combat_before.is_none_or(|b| {
                c.monsters
                    .iter()
                    .map(|m| (m.slot, m.enemy))
                    .collect::<Vec<_>>()
                    != b.monsters
                        .iter()
                        .map(|m| (m.slot, m.enemy))
                        .collect::<Vec<_>>()
            });
            let mut defeated = std::collections::BTreeMap::<String, usize>::new();
            let mut escaped = std::collections::BTreeMap::<String, usize>::new();
            for (index, m) in c.monsters.iter().enumerate() {
                if !roster_changed
                    && !changed(
                        m,
                        combat_before
                            .and_then(|b| b.monsters.iter().find(|other| other.slot == m.slot)),
                    )
                {
                    continue;
                }
                let CombatLearningEnemyIdentityV1::Known { enemy_id } = m.enemy;
                let title = label(enemy_id);
                if m.escaped {
                    *escaped.entry(title.into()).or_default() += 1;
                    continue;
                }
                if !m.alive && !m.half_dead && !m.escaped {
                    *defeated.entry(title.into()).or_default() += 1;
                    continue;
                }
                let title = if c
                    .monsters
                    .iter()
                    .filter(|other| other.enemy == m.enemy)
                    .count()
                    > 1
                {
                    let occurrence = c.monsters[..index]
                        .iter()
                        .filter(|other| other.enemy == m.enemy)
                        .count()
                        + 1;
                    format!("{title} #{occurrence}")
                } else {
                    title.into()
                };
                let intent = if m.half_dead {
                    "Reviving".into()
                } else if !m.alive {
                    "Defeated".into()
                } else {
                    m.intent
                        .intent
                        .as_ref()
                        .map(|i| intent(i, m.intent.preview_damage_per_hit))
                        .unwrap_or_else(|| "Unknown intent".into())
                };
                lines.push(format!(
                    "{}: HP {}/{}{}; {}{}",
                    title,
                    m.hp,
                    m.max_hp,
                    if m.block > 0 {
                        format!(", Block {}", m.block)
                    } else {
                        String::new()
                    },
                    intent,
                    if m.powers.is_empty() {
                        String::new()
                    } else {
                        format!("; {}", powers(&m.powers))
                    }
                ));
                if let Some(knowledge) =
                    self.monster_moves.iter().find(|k| k.monster_slot == m.slot)
                {
                    let previous = before
                        .and_then(|b| b.monster_moves.iter().find(|k| k.monster_slot == m.slot));
                    if detail || roster_changed || changed(knowledge, previous) {
                        lines.push(format!("  {}", move_knowledge(knowledge)));
                    }
                }
            }
            for (status, census) in [("Defeated", defeated), ("Escaped", escaped)] {
                if census.is_empty() {
                    continue;
                }
                lines.push(format!(
                    "{status}: {}",
                    census
                        .into_iter()
                        .map(|(name, count)| {
                            if count == 1 {
                                name
                            } else {
                                format!("{count}x {name}")
                            }
                        })
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
            if let Some(old) = combat_before {
                for m in old.monsters.iter().filter(|m| {
                    (m.alive || m.half_dead)
                        && !c.monsters.iter().any(|current| current.slot == m.slot)
                }) {
                    let CombatLearningEnemyIdentityV1::Known { enemy_id } = m.enemy;
                    lines.push(format!("{}: left combat", label(enemy_id)));
                }
            }
            // Hand order is preserved. Grouping a pile's display never merges
            // the separately source-bound operation objects.
            if changed(&c.cards.hand, combat_before.map(|b| &b.cards.hand)) {
                lines.push(format!("Hand: {}", cards(&c.cards.hand.cards, true)));
            }
            let counts = [
                (
                    "Draw",
                    c.cards.draw.cards.len(),
                    combat_before.map(|b| b.cards.draw.cards.len()),
                ),
                (
                    "Discard",
                    c.cards.discard.cards.len(),
                    combat_before.map(|b| b.cards.discard.cards.len()),
                ),
                (
                    "Exhaust",
                    c.cards.exhaust.cards.len(),
                    combat_before.map(|b| b.cards.exhaust.cards.len()),
                ),
            ]
            .into_iter()
            .filter(|(_, count, old)| Some(*count) != *old)
            .map(|(name, count, _)| format!("{name} {count}"))
            .collect::<Vec<_>>();
            if !counts.is_empty() {
                lines.push(counts.join(", "));
            }
            for (name, pile, old) in [
                (
                    "Exhaust",
                    &c.cards.exhaust,
                    combat_before.map(|b| &b.cards.exhaust),
                ),
                (
                    "Discard",
                    &c.cards.discard,
                    combat_before.map(|b| &b.cards.discard),
                ),
                (
                    "Resolving",
                    &c.cards.limbo,
                    combat_before.map(|b| &b.cards.limbo),
                ),
            ] {
                if detail || old.is_none() || name == "Resolving" {
                    if (detail || name == "Resolving")
                        && changed(pile, old)
                        && (!pile.cards.is_empty() || old.is_some_and(|p| !p.cards.is_empty()))
                    {
                        lines.push(format!(
                            "{name}: {}",
                            render_inventory(&pile_inventory(&pile.cards))
                        ));
                    }
                } else if let Some(old) = old {
                    let (added, removed) = pile_difference(&old.cards, &pile.cards);
                    if !added.is_empty() {
                        lines.push(format!("{name} added: {}", render_inventory(&added)));
                    }
                    if !removed.is_empty() {
                        lines.push(format!("{name} removed: {}", render_inventory(&removed)));
                    }
                }
            }
        } else if combat_before.is_some() {
            lines.push("Combat ended".into());
        }
        if changed(&self.potions, before.map(|b| &b.potions)) {
            lines.push(self.render_potions()?);
        }
        if !detail {
            if let Some(before) = before.filter(|b| b.relics != self.relics) {
                let (gained, lost) = relic_difference(&before.relics, &self.relics)?;
                for (verb, ids) in [("lost", lost), ("gained", gained)] {
                    if !ids.is_empty() {
                        lines.push(format!(
                            "Relics {verb}: {}",
                            ids.iter()
                                .map(|id| crate::presentation_names::name::<
                                    sts_oracle_run_control::content::relics::RelicId,
                                >(id))
                                .collect::<Vec<_>>()
                                .join(", ")
                        ));
                    }
                }
            }
        }
        if let Some(before) = before {
            if self.deck != before.deck {
                lines.extend(deck_difference(&before.deck, &self.deck)?);
            }
        } else if detail {
            lines.push(format!(
                "Deck: {}",
                crate::manual::card_collection_summary(&self.deck)?
            ));
        }
        if detail && changed(&self.relics, before.map(|b| &b.relics)) {
            lines.push(format!(
                "Relics: {}",
                crate::manual::relic_summary(&self.relics)?
            ));
        }
        if self.boundary == "natural_event"
            && before.is_none_or(|b| {
                b.boundary != self.boundary || b.room["event_id"] != self.room["event_id"]
            })
        {
            if let Some(id) = self.room["event_id"].as_str() {
                lines.push(format!(
                    "Event: {}",
                    crate::presentation_names::name::<
                        sts_oracle_run_control::state::events::EventId,
                    >(id)
                ));
            }
        }
        if changed(&self.room, before.map(|b| &b.room)) {
            if let Some(board) = crate::decision_match::render(&self.room, before.map(|b| &b.room))
            {
                lines.push(board);
            }
            if let Some(prompt) = self.relic_selection_prompt()? {
                lines.push(prompt);
            }
        }
        if before.is_none_or(|b| b.selection != self.selection || b.boundary != self.boundary) {
            if let Some(selection) = &self.selection {
                lines.push(selection_prompt(selection));
            } else {
                match self.boundary.as_str() {
                    "natural_combat" if before.is_some_and(|b| b.selection.is_some()) => {
                        lines.push("Choose a card to play".into())
                    }
                    "natural_combat_reward" => lines.push("Combat rewards".into()),
                    "natural_card_reward" => lines.push("Choose a card reward".into()),
                    "natural_boss_relic" => lines.push("Choose a Boss relic".into()),
                    "natural_treasure" => lines.push("Treasure chest".into()),
                    "natural_treasure_reward" => lines.push("Treasure rewards".into()),
                    "natural_map" => lines.push("Map: choose the next room".into()),
                    _ => {}
                }
            }
        }
        if changed(&self.hand_candidates, before.map(|b| &b.hand_candidates)) {
            if let Some(candidates) = &self.hand_candidates {
                if self
                    .combat
                    .as_ref()
                    .is_some_and(|c| candidates != &c.cards.hand.cards)
                {
                    lines.push(format!("Eligible cards: {}", cards(candidates, true)));
                }
            }
        }
        if changed(&self.shop, before.map(|b| &b.shop)) {
            if let Some(shop) = &self.shop {
                lines.extend(crate::decision_shop::render(shop));
            }
        }
        let slots_full = |state: &Self| {
            state.potions.as_array().is_some_and(|slots| {
                !slots.is_empty()
                    && slots
                        .iter()
                        .all(|slot| slot["id"].as_str().is_some_and(|id| id != "empty"))
            })
        };
        if changed(&self.rewards, before.map(|b| &b.rewards))
            || before.is_some_and(|b| slots_full(self) != slots_full(b))
        {
            if let Some(
                NaturalRunRewardsV2::Combat { items, .. }
                | NaturalRunRewardsV2::Treasure { items, .. },
            ) = &self.rewards
            {
                let rewards = items
                    .iter()
                    .map(|item| {
                        // Reuse the shared typed reward description only for text;
                        // it never enters offers or the executable action view.
                        let mut text =
                            offer(&sts_parity_lockstep::natural_reward_choice_v1(item, 0));
                        if matches!(item, NaturalRunRewardItemV1::Potion { .. }) && slots_full(self)
                        {
                            text.push_str(" (slots full)");
                        }
                        text
                    })
                    .collect::<Vec<_>>();
                lines.push(format!(
                    "Rewards: {}",
                    if rewards.is_empty() {
                        "none".into()
                    } else {
                        rewards.join(", ")
                    }
                ));
            }
        }
        if self.rewards.is_none()
            && include_offers
            && changed(&self.offers, before.map(|b| &b.offers))
            && !self.offers.is_empty()
        {
            let offers = self
                .offers
                .iter()
                .filter_map(|a| match a {
                    SemanticActionV1::Select { choice, .. } => Some(offer(choice)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            lines.push(format!("Offers: {}", offers.join(", ")));
        }
        Ok(lines.join("\n"))
    }

    fn relic_selection_prompt(&self) -> Result<Option<String>, String> {
        if self.room["kind"] != "relic_selection"
            || self.room["selection_kind"] != "deck_transform_upgraded"
        {
            return Ok(None);
        }
        let relic = self.room["relic_id"]
            .as_str()
            .ok_or("relic selection omitted its relic")?;
        let relic = crate::presentation_names::name::<
            sts_oracle_run_control::content::relics::RelicId,
        >(relic);
        let total = self.room["requested_cards"]
            .as_u64()
            .ok_or("relic selection omitted its requested count")?;
        let selected = self.room["selected"]
            .as_array()
            .ok_or("relic selection omitted its selected cards")?;
        let remaining = total
            .checked_sub(selected.len() as u64)
            .ok_or("relic selection exceeds its requested count")?;
        if selected.is_empty() {
            return Ok(Some(format!(
                "{relic}: choose {remaining} cards to transform and upgrade"
            )));
        }
        let labels = crate::manual::deck_card_labels(&self.deck)?;
        let selected_labels = selected
            .iter()
            .map(|binding| {
                binding
                    .as_str()
                    .and_then(|id| labels.get(id))
                    .cloned()
                    .ok_or("selected card is absent from the decision deck")
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Some(format!(
            "{relic}: selected {}/{total} ({}); choose {remaining} more to transform and upgrade",
            selected.len(),
            selected_labels.join(", ")
        )))
    }

    fn resources(&self) -> (u64, i64, i64, i64, i64) {
        let (hp, max_hp, gold) = self
            .combat
            .as_ref()
            .map(|c| {
                (
                    i64::from(c.player.hp),
                    i64::from(c.player.max_hp),
                    i64::from(c.player.gold),
                )
            })
            .unwrap_or((self.run.current_hp, self.run.max_hp, self.run.gold));
        (self.run.act, self.run.floor, hp, max_hp, gold)
    }
}

fn changed<T: PartialEq>(value: &T, previous: Option<&T>) -> bool {
    previous != Some(value)
}

// Compare card properties with multiplicity. Bindings and ordinals still live
// in the structured state; this census is not a cross-end identity mapping.
fn deck_difference(before: &Value, after: &Value) -> Result<Vec<String>, String> {
    let properties = |deck: &Value| -> Result<Vec<Value>, String> {
        deck.as_array()
            .ok_or("decision deck is not an array")?
            .iter()
            .map(|card| {
                let mut card = card
                    .as_object()
                    .ok_or("decision deck card is not an object")?
                    .clone();
                card.remove("binding");
                card.remove("ordinal");
                Ok(Value::Object(card))
            })
            .collect()
    };
    let before = properties(before)?;
    let after = properties(after)?;
    let mut remaining = before.clone();
    let mut added = Vec::new();
    for card in &after {
        if let Some(index) = remaining.iter().position(|candidate| candidate == card) {
            remaining.remove(index);
        } else {
            added.push(card.clone());
        }
    }
    let mut lines = Vec::new();
    if added.is_empty() && remaining.is_empty() && before != after {
        lines.push("Deck order changed".into());
    }
    for (verb, cards) in [("removed", remaining), ("added", added)] {
        if !cards.is_empty() {
            lines.push(format!(
                "Deck {verb}: {}",
                crate::manual::card_collection_summary(&Value::Array(cards))?
            ));
        }
    }
    Ok(lines)
}

fn relic_difference(before: &Value, after: &Value) -> Result<(Vec<String>, Vec<String>), String> {
    fn inventory(value: &Value) -> Result<std::collections::BTreeMap<String, usize>, String> {
        let mut ids = std::collections::BTreeMap::new();
        for relic in value.as_array().ok_or("decision relics are not an array")? {
            let id = relic["id"]
                .as_str()
                .ok_or("decision relic omitted its ID")?;
            *ids.entry(id.to_owned()).or_insert(0) += 1;
        }
        Ok(ids)
    }
    let before = inventory(before)?;
    let after = inventory(after)?;
    let difference = |current: &std::collections::BTreeMap<String, usize>,
                      old: &std::collections::BTreeMap<String, usize>| {
        current
            .iter()
            .flat_map(|(id, count)| {
                std::iter::repeat_n(id.clone(), count.saturating_sub(*old.get(id).unwrap_or(&0)))
            })
            .collect()
    };
    Ok((difference(&after, &before), difference(&before, &after)))
}

/// Render the selected presentation group without interpreting its labels.
pub fn render_decision_choices_v1(
    actions: &sts_parity_lockstep::SemanticActionSpaceV1,
    labels: &std::collections::BTreeMap<String, String>,
) -> String {
    actions
        .actions
        .iter()
        .filter(|action| !matches!(&action.action, SemanticActionV1::Select { choice, .. } if choice.kind == "match_card"))
        .filter_map(|action| labels.get(&action.handle).map(|label| format!("  {label}")))
        .collect::<Vec<_>>()
        .join("\n")
}

pub(crate) fn offer(choice: &SemanticChoiceV1) -> String {
    let attr = |name: &str| match choice.attributes.get(name) {
        Some(SemanticScalarV1::Text(s)) => s.clone(),
        Some(SemanticScalarV1::Integer(n)) => n.to_string(),
        _ => "?".into(),
    };
    match choice.id.as_str() {
        "gold" | "stolen_gold" => format!("{} gold", attr("amount")),
        "potion" => crate::presentation_names::name::<
            sts_oracle_run_control::content::potions::PotionId,
        >(&attr("potion")),
        "relic" => crate::presentation_names::name::<
            sts_oracle_run_control::content::relics::RelicId,
        >(&attr("relic")),
        "card" if choice.kind == "run_reward" => {
            let mut cards = Vec::new();
            for index in 0.. {
                let Some(SemanticScalarV1::Text(id)) =
                    choice.attributes.get(&format!("card_{index}"))
                else {
                    break;
                };
                let upgrades = match choice.attributes.get(&format!("upgrade_{index}")) {
                    Some(SemanticScalarV1::Integer(n)) => (*n).max(0) as u64,
                    _ => 0,
                };
                cards.push(crate::manual::format_card(id, upgrades, None));
            }
            if cards.is_empty() {
                "Card reward".into()
            } else {
                format!("Card reward: {}", cards.join(" / "))
            }
        }
        "emerald_key" => "Emerald Key".into(),
        "sapphire_key" => "Sapphire Key".into(),
        _ => crate::manual::format_card(
            &choice.id,
            match choice.attributes.get("upgrades") {
                Some(SemanticScalarV1::Integer(n)) if *n > 0 => *n as u64,
                _ => 0,
            },
            None,
        ),
    }
}

fn move_knowledge(knowledge: &MonsterMoveKnowledgeV1) -> String {
    format!(
        "{}: {}",
        knowledge.move_id.name(),
        knowledge
            .effects
            .iter()
            .map(|effect| match effect {
                MonsterMoveEffectV1::HealSelfForDamageDealt => "heals for damage dealt".to_owned(),
                MonsterMoveEffectV1::Summon { enemy_id, count } => {
                    format!("summons {} x{count}", enemy_id.get_name())
                }
                MonsterMoveEffectV1::AddCardToMasterDeck { card_id, count } => {
                    format!("adds {} x{count} to your deck", label(*card_id))
                }
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

fn intent(i: &Intent, damage: Option<i32>) -> String {
    let effect = match i {
        Intent::Attack { .. } => "",
        Intent::AttackBuff { .. } | Intent::Buff => "Buff",
        Intent::AttackDebuff { .. } | Intent::Debuff => "Debuff",
        Intent::AttackDefend { .. } | Intent::Defend => "Defend",
        Intent::StrongDebuff => "Strong Debuff",
        Intent::DefendDebuff => "Defend + Debuff",
        Intent::DefendBuff => "Defend + Buff",
        Intent::Heal { .. } => "Heal",
        Intent::Escape => "Escape",
        Intent::Magic => "Magic",
        Intent::Sleep => "Sleep",
        Intent::Stun => "Stunned",
        Intent::None => "No action",
        Intent::Unknown | Intent::Debug => "Unknown intent",
    };
    if !i.is_java_attack_intent() {
        return effect.into();
    }
    format!(
        "Attack {}{}{}",
        damage.map(|d| d.to_string()).unwrap_or_else(|| "?".into()),
        if i.hits() > 1 {
            format!("x{}", i.hits())
        } else {
            String::new()
        },
        if effect.is_empty() {
            String::new()
        } else {
            format!(" + {effect}")
        }
    )
}

fn cards(cards: &[CombatLearningCardV1], cost: bool) -> String {
    if cards.is_empty() {
        return "empty".into();
    }
    cards
        .iter()
        .map(|c| {
            let mut text = crate::manual::format_card(
                label(c.card_id),
                u64::from(c.upgrades),
                cost.then_some(c.effective_cost),
            );
            if let Some(damage) = c.base_damage_override {
                text.push_str(&format!("(base damage {damage})"));
            }
            text
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn powers(powers: &[CombatLearningPowerV1]) -> String {
    if powers.is_empty() {
        return "none".into();
    }
    powers
        .iter()
        .map(|p| {
            let mut text =
                crate::presentation_names::power(p.power.stable_id(), i64::from(p.amount));
            if let Some(card) = &p.payload_card {
                text.push_str(": ");
                text.push_str(&cards(std::slice::from_ref(card), true));
            }
            text
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn selection_prompt(choice: &PendingChoice) -> String {
    match choice {
        PendingChoice::HandSelect {
            reason,
            min_cards,
            max_cards,
            can_cancel,
            ..
        } => {
            let verb = match reason {
                HandSelectReason::Exhaust | HandSelectReason::PotionExhaust { .. } => "Exhaust",
                HandSelectReason::Discard | HandSelectReason::GamblingChip => "Discard",
                HandSelectReason::Retain => "Retain",
                HandSelectReason::PutOnDrawPile => "Place on top of draw pile",
                HandSelectReason::PutToBottomOfDraw => "Place on bottom of draw pile",
                HandSelectReason::Setup => "Set up",
                HandSelectReason::Copy { .. } => "Copy",
                HandSelectReason::Nightmare { .. } => "Copy next turn",
                HandSelectReason::Upgrade => "Upgrade",
                HandSelectReason::Recycle => "Recycle",
            };
            let count = if min_cards == max_cards {
                min_cards.to_string()
            } else {
                format!("{min_cards}-{max_cards}")
            };
            format!(
                "{verb} {count} {} from hand{}",
                if *max_cards == 1 { "card" } else { "cards" },
                if *can_cancel && *min_cards > 0 {
                    " (or choose none)"
                } else {
                    ""
                }
            )
        }
        PendingChoice::GridSelect {
            source_pile,
            reason,
            min_cards,
            max_cards,
            can_cancel,
            ..
        } => {
            let source = match source_pile {
                PileType::Draw => "draw pile",
                PileType::Discard => "discard pile",
                PileType::Exhaust => "exhaust pile",
                PileType::Hand => "hand",
                PileType::Limbo => "limbo",
                PileType::MasterDeck => "deck",
            };
            let count = if min_cards == max_cards {
                min_cards.to_string()
            } else {
                format!("{min_cards}-{max_cards}")
            };
            let card = match (reason, *max_cards == 1) {
                (GridSelectReason::SkillFromDeckToHand, true) => "Skill",
                (GridSelectReason::SkillFromDeckToHand, false) => "Skills",
                (GridSelectReason::AttackFromDeckToHand, true) => "Attack",
                (GridSelectReason::AttackFromDeckToHand, false) => "Attacks",
                (_, true) => "card",
                (_, false) => "cards",
            };
            let destination = match reason {
                GridSelectReason::MoveToDrawPile => " to put on top of the draw pile".into(),
                GridSelectReason::Omniscience { play_amount } => {
                    format!(" to play {play_amount} times")
                }
                GridSelectReason::Exhume { .. }
                | GridSelectReason::DrawPileToHand
                | GridSelectReason::SkillFromDeckToHand
                | GridSelectReason::AttackFromDeckToHand
                | GridSelectReason::DiscardToHand
                | GridSelectReason::DiscardToHandNoCostChange
                | GridSelectReason::DiscardToHandRetain => " to return to hand".into(),
            };
            format!(
                "Choose {count} {card} from {source}{destination}{}",
                if *can_cancel { " (or cancel)" } else { "" }
            )
        }
        PendingChoice::DiscoverySelect(_)
        | PendingChoice::ForeignInfluenceSelect { .. }
        | PendingChoice::ChooseOneSelect { .. } => "Choose a card".into(),
        PendingChoice::CardRewardSelect { .. } => "Choose a card reward".into(),
        PendingChoice::ScrySelect { .. } => "Choose scry cards to discard".into(),
        PendingChoice::StanceChoice => "Choose a stance".into(),
    }
}

// A census of displayed card properties, not instance identity or movement
// evidence. Turn-local costs/previews must not invent a pile addition/removal.
#[derive(Clone, Debug, PartialEq, Eq)]
struct PileCard {
    id: CardId,
    upgrades: u8,
    base_damage: Option<i32>,
}

type PileInventory = Vec<(PileCard, usize)>;

fn pile_inventory(cards: &[CombatLearningCardV1]) -> PileInventory {
    let mut inventory = PileInventory::new();
    for card in cards {
        let key = PileCard {
            id: card.card_id,
            upgrades: card.upgrades,
            base_damage: card.base_damage_override,
        };
        if let Some((_, count)) = inventory
            .iter_mut()
            .find(|(candidate, _)| *candidate == key)
        {
            *count += 1;
        } else {
            inventory.push((key, 1));
        }
    }
    inventory
}

fn pile_difference(
    before: &[CombatLearningCardV1],
    after: &[CombatLearningCardV1],
) -> (PileInventory, PileInventory) {
    let before = pile_inventory(before);
    let after = pile_inventory(after);
    let difference = |left: &PileInventory, right: &PileInventory| {
        left.iter()
            .filter_map(|(card, count)| {
                let other = right
                    .iter()
                    .find(|(candidate, _)| candidate == card)
                    .map_or(0, |(_, count)| *count);
                let extra = count.saturating_sub(other);
                (extra > 0).then(|| (card.clone(), extra))
            })
            .collect()
    };
    (difference(&after, &before), difference(&before, &after))
}

fn render_inventory(inventory: &PileInventory) -> String {
    if inventory.is_empty() {
        return "empty".into();
    }
    inventory
        .iter()
        .map(|(card, count)| {
            let mut name =
                crate::manual::format_card(label(card.id), u64::from(card.upgrades), None);
            if let Some(damage) = card.base_damage {
                name.push_str(&format!("(base damage {damage})"));
            }
            if *count > 1 {
                name.push_str(&format!(" x{count}"));
            }
            name
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deck_delta_keeps_counts_upgrades_and_order_without_binding_noise() {
        let before = serde_json::json!([
            {"binding":"a", "ordinal":0, "card_id":"strike", "upgrades":0},
            {"binding":"b", "ordinal":1, "card_id":"strike", "upgrades":0},
            {"binding":"c", "ordinal":2, "card_id":"defend", "upgrades":1}
        ]);
        let after = serde_json::json!([
            {"binding":"x", "ordinal":0, "card_id":"strike", "upgrades":0},
            {"binding":"y", "ordinal":1, "card_id":"defend", "upgrades":1},
            {"binding":"z", "ordinal":2, "card_id":"sentinel", "upgrades":1}
        ]);
        let lines = deck_difference(&before, &after).unwrap();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Strike"));
        assert!(lines[1].contains("Sentinel+"));
        assert!(!lines.join(" ").contains("Defend"));
        let mut rebound = before.clone();
        rebound[0]["binding"] = "other-source".into();
        assert!(deck_difference(&before, &rebound).unwrap().is_empty());
        rebound.as_array_mut().unwrap().swap(0, 2);
        assert!(deck_difference(&before, &rebound).unwrap()[0].contains("order"));
        let empty = serde_json::json!([]);
        assert!(deck_difference(&before, &empty).unwrap()[0].contains("2x Strike"));
    }

    #[test]
    fn relic_trade_delta_ignores_ordinals_and_counters_but_keeps_multiplicity() {
        let before = serde_json::json!([
            {"id":"warpaint", "ordinal":0, "counter":-1},
            {"id":"happyflower", "ordinal":1, "counter":1},
            {"id":"circlet", "ordinal":2, "counter":-1}
        ]);
        let after = serde_json::json!([
            {"id":"happyflower", "ordinal":0, "counter":2},
            {"id":"circlet", "ordinal":1, "counter":-1},
            {"id":"nlothsgift", "ordinal":2, "counter":-1},
            {"id":"circlet", "ordinal":3, "counter":-1}
        ]);
        assert_eq!(
            relic_difference(&before, &after).unwrap(),
            (
                vec!["circlet".to_owned(), "nlothsgift".to_owned()],
                vec!["warpaint".to_owned()]
            )
        );
    }

    #[test]
    fn shop_removal_is_folded_until_its_source_bound_group_is_requested() {
        use sts_parity_lockstep::{DecisionSurfaceV1, SemanticActionSpaceV1};
        let actions = SemanticActionSpaceV1::new(["card-one", "card-two"].into_iter().map(|id| {
            let mut choice = SemanticChoiceV1::new("deck_card", id, 0);
            choice
                .attributes
                .insert("price".into(), SemanticScalarV1::Integer(100));
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Shop,
                choice,
            }
        }))
        .unwrap();
        let labels = actions
            .actions
            .iter()
            .enumerate()
            .map(|(i, a)| {
                (
                    a.handle.clone(),
                    format!("purge Strike #{} (100 gold)", i + 1),
                )
            })
            .collect();
        let summary =
            render_decision_choices_v1(&DecisionSectionV1::Position.choices(&actions), &labels);
        let detail =
            render_decision_choices_v1(&DecisionSectionV1::Removal.choices(&actions), &labels);
        assert!(summary.is_empty());
        assert!(detail.contains("Strike #1"));
        assert!(detail.contains("Strike #2"));
        assert_eq!(actions.actions.len(), 2);
        assert_ne!(actions.actions[0].action, actions.actions[1].action);
    }

    fn card(id: CardId) -> CombatLearningCardV1 {
        CombatLearningCardV1 {
            card_id: id,
            upgrades: 0,
            misc_value: 0,
            base_damage_override: None,
            base_block_override: None,
            cost_modifier: 0,
            cost_for_turn: None,
            effective_cost: 1,
            current_damage: 6,
            current_block: 0,
            current_magic_number: 0,
            damage_by_monster_order: vec![],
            exhaust_override: None,
            retain_override: None,
            free_to_play_once: false,
            energy_on_use: 0,
        }
    }

    #[test]
    fn pile_census_preserves_multiplicity_and_card_changes_without_inventing_moves() {
        let strike = card(CardId::Strike);
        let defend = card(CardId::Defend);
        let before = [strike.clone(), strike.clone(), defend.clone()];
        let mut changed_strike = strike.clone();
        changed_strike.effective_cost = 0;
        changed_strike.current_damage = 9;
        changed_strike.damage_by_monster_order = vec![12];
        let equivalent = [defend.clone(), changed_strike, strike.clone()];
        assert_eq!(pile_difference(&before, &equivalent), (vec![], vec![]));
        let after = [strike.clone(), defend.clone(), defend.clone()];
        let (added, removed) = pile_difference(&before, &after);
        assert_eq!(added, pile_inventory(&[defend]));
        assert_eq!(removed, pile_inventory(std::slice::from_ref(&strike)));
        let mut upgraded = strike.clone();
        upgraded.upgrades = 1;
        let (added, removed) = pile_difference(&[strike], &[upgraded.clone()]);
        assert_eq!(added, pile_inventory(&[upgraded]));
        assert_eq!(removed[0].0.upgrades, 0);
        assert_eq!(removed[0].1, 1);
    }
}
