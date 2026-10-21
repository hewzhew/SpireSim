use super::*;
use crate::state::DomainCardSnapshot;

mod sorted_combat_entity_map {
    use serde::ser::SerializeMap;
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    use super::CombatEntityMap;

    pub(super) fn serialize<S, V>(
        map: &CombatEntityMap<V>,
        serializer: S,
    ) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        V: Serialize,
    {
        let mut entries = map.iter().collect::<Vec<_>>();
        entries.sort_unstable_by_key(|(entity_id, _)| **entity_id);
        let mut encoded = serializer.serialize_map(Some(entries.len()))?;
        for (entity_id, value) in entries {
            encoded.serialize_entry(entity_id, value)?;
        }
        encoded.end()
    }

    pub(super) fn deserialize<'de, D, V>(deserializer: D) -> Result<CombatEntityMap<V>, D::Error>
    where
        D: Deserializer<'de>,
        V: Deserialize<'de>,
    {
        CombatEntityMap::<V>::deserialize(deserializer)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub enum MetaChange {
    /// Already obtained in combat; the run owner persists this exact product.
    AddCardToMasterDeck(MasterDeckAcquisition),
    ModifyCardMisc {
        card_lineage: CardLineageId,
        amount: i32,
    },
    UpgradeMasterDeckCard {
        card_lineage: CardLineageId,
    },
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Hash, Serialize)]
pub struct MasterDeckAcquisition {
    pub card: CombatCard,
    pub monster_id: crate::content::monsters::EnemyId,
    pub monster_slot: u32,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CombatState {
    pub meta: CombatMeta,
    pub turn: TurnRuntime,
    pub zones: CardZones,
    pub entities: EntityState,
    pub engine: EngineRuntime,
    pub rng: CombatRng,
    pub runtime: CombatRuntimeHints,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CombatMeta {
    pub ascension_level: u8,
    pub player_class: String,
    pub is_boss_fight: bool,
    pub is_elite_fight: bool,
    pub master_deck_snapshot: MasterDeckSnapshot,
    pub master_deck_lineage_allocator: MasterDeckLineageAllocator,
    pub meta_changes: Vec<MetaChange>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct TurnRuntime {
    pub turn_count: u32,
    pub current_phase: CombatPhase,
    pub energy: u8,
    /// Narrow Rust equivalent of Java's start-of-turn draw target adjustments.
    /// This stores only modifier semantics for next turn draw count, not a
    /// full copy of Java's mutable `player.gameHandSize`.
    pub turn_start_draw_modifier: i32,
    pub counters: EphemeralCounters,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EngineRuntime {
    pub action_queue: VecDeque<Action>,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CombatRng {
    pub pool: crate::runtime::rng::RngPool,
}

impl CombatRng {
    pub fn new(pool: crate::runtime::rng::RngPool) -> Self {
        Self { pool }
    }
}

impl Deref for CombatRng {
    type Target = crate::runtime::rng::RngPool;

    fn deref(&self) -> &Self::Target {
        &self.pool
    }
}

impl DerefMut for CombatRng {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.pool
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct CardZones {
    pub draw_pile: DrawPile,
    pub hand: Vec<CombatCard>,
    pub discard_pile: DiscardPile,
    pub exhaust_pile: SharedCardPile,
    pub limbo: Vec<CombatCard>,
    pub queued_cards: VecDeque<QueuedCardPlay>,
    pub combat_lineage_allocator: CombatGeneratedLineageAllocator,
}

impl CardZones {
    /// Internal Rust draw-pile invariant: index 0 is the next card drawn.
    /// Java CardGroup stores the draw-pile top at the end of the list, so all
    /// Java addToTop/addToBottom/addToRandomSpot semantics must pass through
    /// these helpers instead of writing `draw_pile` indices directly.
    pub fn add_to_draw_pile_top(&mut self, card: CombatCard) {
        self.draw_pile.push_top(card);
    }

    pub fn add_to_draw_pile_bottom(&mut self, card: CombatCard) {
        self.draw_pile.push(card);
    }

    pub fn draw_top_card(&mut self) -> Option<CombatCard> {
        self.draw_pile.draw_top()
    }

    pub fn add_to_draw_pile_random_spot_from_java_index(
        &mut self,
        card: CombatCard,
        java_insert_index: usize,
    ) {
        let len = self.draw_pile.len();
        if len == 0 {
            self.draw_pile.push(card);
            return;
        }
        let rust_insert_index = len.saturating_sub(java_insert_index.min(len - 1));
        self.draw_pile.insert(rust_insert_index, card);
    }

    /// Internal Rust discard-pile invariant intentionally preserves Java
    /// CardGroup order: index 0 is the bottom and the last element is the top.
    /// This differs from Rust draw_pile because Java shuffles discardPile.group
    /// in this order before moving cards back to the draw pile.
    pub fn add_to_discard_pile_top(&mut self, card: CombatCard) {
        self.discard_pile.push(card);
    }

    /// Java exhaustPile is also a CardGroup: addToTop appends to the end.
    /// Preserve that internal order here for parity with any later selection
    /// or replay that observes exhaust-pile group order.
    pub fn add_to_exhaust_pile_top(&mut self, card: CombatCard) {
        self.exhaust_pile.push(card);
    }

    /// Applies a mutation to every visible battle instance whose Rust lineage
    /// represents the same Java UUID. This is the common path for effects such
    /// as Rampage/Searing Blow
    /// growth that call AbstractDungeon.player.masterDeck.getSpecificCard()
    /// plus GetAllInBattleInstances-like combat copies.
    pub fn for_each_java_battle_instance_mut_by_lineage(
        &mut self,
        card_lineage: CardLineageId,
        mut apply: impl FnMut(&mut CombatCard),
    ) {
        for card in self
            .hand
            .iter_mut()
            .chain(self.draw_pile.iter_mut())
            .chain(self.discard_pile.iter_mut())
            .chain(self.exhaust_pile.iter_mut())
            .chain(self.limbo.iter_mut())
        {
            if card.lineage == card_lineage {
                apply(card);
            }
        }
    }

    /// Queued card plays are a Rust runtime artifact rather than a Java
    /// CardGroup, but some delayed play paths carry stat-equivalent copies
    /// there before execution. Use this only for mutations that must preserve
    /// existing queued-card behavior.
    pub fn for_each_queued_instance_mut_by_lineage(
        &mut self,
        card_lineage: CardLineageId,
        mut apply: impl FnMut(&mut CombatCard),
    ) {
        for queued in &mut self.queued_cards {
            if queued.card.lineage == card_lineage {
                apply(&mut queued.card);
            }
        }
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct EntityState {
    pub player: PlayerEntity,
    pub monsters: Vec<MonsterEntity>,
    pub potions: Vec<Option<crate::content::potions::Potion>>,
    #[serde(with = "sorted_combat_entity_map")]
    pub power_db: CombatEntityMap<Vec<Power>>,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Hash, Serialize)]
pub enum QueuedCardSource {
    Normal,
    /// Java cards such as Burn, Regret, Decay, Doubt, and Shame enqueue their
    /// own live hand instance from `triggerOnEndOfTurnForPlayingCard()`.  The
    /// queued play bypasses ordinary play triggers, then moves that instance
    /// through UseCardAction before the remaining hand is discarded.
    EndTurnTriggered,
    Necronomicon,
    DoubleTap,
    Duplication,
    Burst,
    Amplify,
    EchoForm,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct QueuedCardPlay {
    pub card: CombatCard,
    pub target: Option<EntityId>,
    pub energy_on_use: i32,
    pub ignore_energy_total: bool,
    pub autoplay: bool,
    pub random_target: bool,
    pub is_end_turn_autoplay: bool,
    pub purge_on_use: bool,
    pub source: QueuedCardSource,
}

impl QueuedCardPlay {
    /// Mirrors the `CardQueueItem` created by Java same-instance replay
    /// effects such as Double Tap, Burst, Echo Form, and Duplication Potion.
    ///
    /// `makeSameInstanceOf()` deliberately does not copy `energyOnUse`; Java
    /// carries that per-play value in the queue item instead. Keeping this in
    /// one constructor prevents X-cost copies from silently replaying at X=0.
    pub fn same_instance_replay_java(
        original: &CombatCard,
        target: Option<EntityId>,
        source: QueuedCardSource,
    ) -> Self {
        Self {
            card: original.make_same_instance_of_java(),
            target,
            energy_on_use: original.energy_on_use,
            ignore_energy_total: true,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: true,
            source,
        }
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub struct DrawnCardRecord {
    pub card_lineage: CardLineageId,
    pub card_id: CardId,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatRuntimeHints {
    pub using_card: bool,
    pub colorless_combat_pool: Vec<CardId>,
    /// Room-owned actions queued after every monster's `usePreBattleAction`
    /// and before relic `atPreBattle` hooks. Burning-elite buffs are the
    /// current Java mechanic requiring this exact boundary.
    #[serde(default)]
    pub room_pre_battle_actions: Vec<crate::runtime::action::Action>,
    /// Output-only mailbox consumed by committed run execution.
    /// Speculative steps clear it at stable boundaries; it is not state identity.
    pub emitted_events: Vec<DomainEvent>,
    /// Output-only observer for one explicitly requested transition.
    ///
    /// It is absent from checkpoints and exact state identity. Ordinary
    /// simulation leaves it disabled, while parity/evidence callers may
    /// request a typed birth receipt for one transition.
    #[serde(skip)]
    pub(crate) card_birth_observation: CombatCardBirthObservation,
    #[serde(skip)]
    pub(crate) oracle_shuffle_observation: OracleShuffleObservation,
    /// Output-only engine warnings for the caller of a committed transition.
    /// These diagnostics never influence subsequent combat rules.
    pub engine_diagnostics: Vec<EngineDiagnostic>,
    pub pending_rewards: Vec<crate::state::rewards::RewardItem>,
    pub power_instance_counter: u32,
    /// Narrow Rust equivalent of Java `DrawCardAction.drawnCards`.
    ///
    /// It is updated only by draw actions that explicitly opt into draw
    /// history because only Java follow-up actions should observe it.
    pub last_drawn_cards: Vec<DrawnCardRecord>,
    #[serde(with = "sorted_combat_entity_map")]
    pub monster_protocol: CombatEntityMap<MonsterProtocolState>,
    /// Java `AbstractRoom.mugged` equivalent at combat scope.
    ///
    /// Set when a thief escapes after actually stealing gold. This is not used
    /// for reward contents, only for post-combat presentation/state.
    pub combat_mugged: bool,
    /// Java `AbstractRoom.smoked` equivalent at combat scope.
    ///
    /// Set when Smoke Bomb ends combat. This changes reward-screen behavior:
    /// no normal combat rewards should be generated.
    pub combat_smoked: bool,
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
pub enum CombatPhase {
    PlayerTurn,
    MonsterTurn,
    TurnTransition,
}

impl CombatPhase {
    pub const fn vocabulary_token_v1(self) -> &'static str {
        match self {
            Self::PlayerTurn => "player_turn",
            Self::MonsterTurn => "monster_turn",
            Self::TurnTransition => "turn_transition",
        }
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct EphemeralCounters {
    pub cards_played_this_turn: u8,
    pub attacks_played_this_turn: u8,
    pub cards_discarded_this_turn: u16,
    pub card_ids_played_this_turn: Vec<CardId>,
    pub card_ids_played_this_combat: Vec<CardId>,
    pub orbs_channeled_this_turn: Vec<OrbId>,
    pub orbs_channeled_this_combat: Vec<OrbId>,
    pub mantra_gained_this_combat: i32,
    pub times_damaged_this_combat: u8,
    pub victory_triggered: bool,
    pub discovery_cost_for_turn: Option<u8>,
    pub early_end_turn_pending: bool,
    pub skip_monster_turn_pending: bool,
    /// Ordinary player power hooks have run after the end-turn card queue.
    /// Persist this across pending choices created by those hooks.
    #[serde(default)]
    pub end_turn_powers_triggered: bool,
    /// Java's DiscardAtEndOfTurnAction has already discarded the hand, while
    /// bottom-queued Exhaust hooks are still resolving.
    #[serde(skip)]
    pub end_turn_hand_discarded: bool,
    pub player_escaping: bool,
    pub escape_pending_reward: bool,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
pub struct RelicBuses {
    pub at_pre_battle: smallvec::SmallVec<[usize; 4]>,
    pub at_battle_start_pre_draw: smallvec::SmallVec<[usize; 4]>,
    pub at_battle_start: smallvec::SmallVec<[usize; 4]>,
    pub at_turn_start: smallvec::SmallVec<[usize; 4]>,
    pub at_turn_start_post_draw: smallvec::SmallVec<[usize; 4]>,
    pub on_use_card: smallvec::SmallVec<[usize; 4]>,
    pub on_shuffle: smallvec::SmallVec<[usize; 4]>,
    pub on_exhaust: smallvec::SmallVec<[usize; 4]>,
    pub on_lose_hp: smallvec::SmallVec<[usize; 4]>,
    pub on_victory: smallvec::SmallVec<[usize; 4]>,
    pub on_apply_power: smallvec::SmallVec<[usize; 4]>,
    pub on_monster_death: smallvec::SmallVec<[usize; 4]>,
    pub on_spawn_monster: smallvec::SmallVec<[usize; 4]>,
    pub at_end_of_turn: smallvec::SmallVec<[usize; 4]>,
    pub on_use_potion: smallvec::SmallVec<[usize; 4]>,
    pub on_discard: smallvec::SmallVec<[usize; 4]>,
    pub on_change_stance: smallvec::SmallVec<[usize; 4]>,
    pub on_attacked_to_change_damage: smallvec::SmallVec<[usize; 4]>,
    pub on_lose_hp_last: smallvec::SmallVec<[usize; 4]>,

    // Core Engine Value Modifiers
    pub on_calculate_heal: smallvec::SmallVec<[usize; 4]>,
    pub on_calculate_x_cost: smallvec::SmallVec<[usize; 4]>,
    pub on_calculate_block_retained: smallvec::SmallVec<[usize; 4]>,
    pub on_calculate_energy_retained: smallvec::SmallVec<[usize; 4]>,
    pub on_scry: smallvec::SmallVec<[usize; 4]>,
    pub on_receive_power_modify: smallvec::SmallVec<[usize; 4]>,
    pub on_calculate_vulnerable_multiplier: smallvec::SmallVec<[usize; 4]>,
}

impl CombatState {
    pub fn monster_protocol(&self, monster_id: EntityId) -> Option<&MonsterProtocolState> {
        self.runtime.monster_protocol.get(&monster_id)
    }

    pub fn monster_protocol_mut(&mut self, monster_id: EntityId) -> &mut MonsterProtocolState {
        self.runtime.monster_protocol.entry(monster_id).or_default()
    }

    pub fn clear_monster_protocol_observation(&mut self, monster_id: EntityId) {
        let observation = &mut self.monster_protocol_mut(monster_id).observation;
        observation.visible_intent = Intent::Unknown;
        observation.preview_damage_per_hit = 0;
    }

    pub fn set_monster_protocol_visible_intent(&mut self, monster_id: EntityId, intent: Intent) {
        self.monster_protocol_mut(monster_id)
            .observation
            .visible_intent = intent;
    }

    pub fn set_monster_protocol_preview_damage_per_hit(
        &mut self,
        monster_id: EntityId,
        damage: i32,
    ) {
        self.monster_protocol_mut(monster_id)
            .observation
            .preview_damage_per_hit = damage;
    }

    pub fn monster_protocol_visible_intent(&self, monster_id: EntityId) -> &Intent {
        self.monster_protocol(monster_id)
            .map(|state| &state.observation.visible_intent)
            .unwrap_or(&Intent::Unknown)
    }

    pub fn monster_protocol_preview_damage_per_hit(&self, monster_id: EntityId) -> i32 {
        self.monster_protocol(monster_id)
            .map(|state| state.observation.preview_damage_per_hit)
            .unwrap_or(0)
    }

    pub fn record_monster_protocol_executed_move(&mut self, monster_id: EntityId, move_id: u8) {
        self.monster_protocol_mut(monster_id)
            .observation
            .executed_move_history
            .push(move_id);
    }

    pub fn monster_protocol_executed_move_history(&self, monster_id: EntityId) -> &[u8] {
        self.monster_protocol(monster_id)
            .map(|state| state.observation.executed_move_history.as_slice())
            .unwrap_or_default()
    }

    pub fn monster_has_protocol_visible_intent(&self, monster_id: EntityId) -> bool {
        !matches!(
            self.monster_protocol_visible_intent(monster_id),
            Intent::Unknown
        )
    }

    pub fn monster_protocol_identity(
        &self,
        monster_id: EntityId,
    ) -> Option<&MonsterProtocolIdentity> {
        self.monster_protocol(monster_id)
            .map(|state| &state.identity)
    }

    pub fn monster_protocol_identity_mut(
        &mut self,
        monster_id: EntityId,
    ) -> &mut MonsterProtocolIdentity {
        &mut self.monster_protocol_mut(monster_id).identity
    }

    pub fn emit_event(&mut self, event: DomainEvent) {
        self.runtime.emitted_events.push(event);
    }

    pub fn take_emitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.runtime.emitted_events)
    }

    pub fn clear_card_draw_observation_events(&mut self) {
        self.runtime
            .emitted_events
            .retain(|event| !matches!(event, DomainEvent::CardDrawn { .. }));
    }

    pub fn take_card_draw_observation_events_v1(&mut self) -> Vec<DomainCardSnapshot> {
        let mut drawn_cards = Vec::new();
        self.runtime.emitted_events.retain(|event| match event {
            DomainEvent::CardDrawn { card, .. } => {
                drawn_cards.push(*card);
                false
            }
            _ => true,
        });
        drawn_cards
    }

    pub fn emit_diagnostic(&mut self, diagnostic: EngineDiagnostic) {
        self.runtime.engine_diagnostics.push(diagnostic);
    }

    pub fn take_engine_diagnostics(&mut self) -> Vec<EngineDiagnostic> {
        std::mem::take(&mut self.runtime.engine_diagnostics)
    }

    /// Java `AbstractRoom.addStolenGoldToRewards` mutates the reward list
    /// synchronously from Looter/Mugger `die()`, merging repeated thieves into
    /// one reward. This must happen before post-combat action cleanup: a queued
    /// reward action for the final monster would otherwise be discarded.
    pub fn add_stolen_gold_to_rewards(&mut self, amount: i32) {
        if amount <= 0 {
            return;
        }
        if let Some(existing) =
            self.runtime
                .pending_rewards
                .iter_mut()
                .find_map(|reward| match reward {
                    crate::state::rewards::RewardItem::StolenGold { amount } => Some(amount),
                    _ => None,
                })
        {
            *existing = existing.saturating_add(amount);
        } else {
            self.runtime
                .pending_rewards
                .push(crate::state::rewards::RewardItem::StolenGold { amount });
        }
    }

    /// Clears output-only mailboxes at a stable simulation boundary.
    ///
    /// Domain events and engine diagnostics describe how a transition was
    /// presented; they do not affect any later combat rule. Committed run
    /// execution consumes them through `RunControlSession`, while speculative
    /// combat search must not carry them into the next exact state.
    pub fn clear_transition_observations(&mut self) {
        self.take_oracle_shuffle_observation();
        self.zones.draw_pile.take_observation();
        self.runtime.emitted_events.clear();
        self.runtime.engine_diagnostics.clear();
        self.clear_card_birth_observation();
    }

    pub fn queue_action_front(&mut self, action: Action) {
        self.prepare_action_constructor(&action);
        self.engine.push_front(action);
    }

    pub fn queue_action_back(&mut self, action: Action) {
        self.prepare_action_constructor(&action);
        self.engine.push_back(action);
    }

    pub fn queue_actions(&mut self, actions: smallvec::SmallVec<[ActionInfo; 4]>) {
        // Java constructs and inserts each action in call order. Constructor
        // hooks may append actions of their own between neighboring actions.
        for action in actions {
            match action.insertion_mode {
                AddTo::Top => self.queue_action_front(action.action),
                AddTo::Bottom => self.queue_action_back(action.action),
            }
        }
    }

    fn prepare_action_constructor(&mut self, action: &Action) {
        if matches!(
            action,
            Action::EmptyDeckShuffle | Action::ShuffleAllIntoDraw
        ) {
            let relic_actions = crate::content::relics::hooks::on_shuffle(self);
            self.queue_actions(relic_actions);
        }
        // ApplyPowerAction mutates these card groups in its constructor, before
        // later on-use relic hooks, even while the power itself is still queued.
        if matches!(
            action,
            Action::ApplyPower {
                power_id: PowerId::Corruption,
                ..
            } | Action::ApplyPowerDetailed {
                power_id: PowerId::Corruption,
                ..
            } | Action::ApplyPowerWithPayload {
                power_id: PowerId::Corruption,
                ..
            }
        ) {
            crate::content::cards::ironclad::corruption::corruption_on_apply(self);
        }
    }

    pub fn pop_next_action(&mut self) -> Option<Action> {
        self.engine.pop_front()
    }

    pub fn has_pending_actions(&self) -> bool {
        self.engine.has_actions()
    }

    pub fn action_queue_len(&self) -> usize {
        self.engine.len()
    }

    pub fn clear_pending_actions(&mut self) {
        self.engine.clear();
    }

    pub fn clear_post_combat_actions(&mut self) {
        self.engine
            .retain(|action| action.retained_by_java_clear_post_combat_actions());
    }

    pub fn ensure_flush_next_queued_card(&mut self) {
        if !self.engine.has_actions() && !self.zones.queued_cards.is_empty() {
            self.engine.push_back(Action::FlushNextQueuedCard);
        }
    }

    pub fn begin_turn_transition(&mut self) {
        self.turn.begin_turn_transition();
    }

    pub fn begin_monster_turn(&mut self) {
        self.turn.begin_monster_turn();
    }

    pub fn begin_next_player_turn(&mut self) {
        let energy = if crate::content::relics::hooks::on_calculate_energy_retained(self) {
            self.turn
                .energy
                .saturating_add(self.entities.player.energy_master)
        } else {
            self.entities.player.energy_master
        };
        self.turn.begin_next_player_turn(energy);
    }

    pub fn reset_turn_energy_from_player(&mut self) {
        self.turn.set_energy(self.entities.player.energy_master);
        self.recompute_turn_start_draw_modifier();
    }
}

impl TurnRuntime {
    pub fn fresh_player_turn(energy: u8) -> Self {
        Self {
            turn_count: 0,
            current_phase: CombatPhase::PlayerTurn,
            energy,
            turn_start_draw_modifier: 0,
            counters: Default::default(),
        }
    }

    pub fn set_energy(&mut self, energy: u8) {
        self.energy = energy;
    }

    pub fn adjust_energy(&mut self, amount: i32) {
        self.energy = (self.energy as i32 + amount).max(0) as u8;
    }

    pub fn spend_energy(&mut self, amount: i32) {
        self.adjust_energy(-amount);
    }

    pub fn increment_cards_played(&mut self) {
        self.counters.cards_played_this_turn += 1;
    }

    pub fn record_card_played(&mut self, card_id: CardId) {
        self.increment_cards_played();
        self.counters.card_ids_played_this_turn.push(card_id);
        self.counters.card_ids_played_this_combat.push(card_id);
    }

    pub fn record_orb_channeled(&mut self, orb_id: OrbId) {
        self.counters.orbs_channeled_this_turn.push(orb_id);
        self.counters.orbs_channeled_this_combat.push(orb_id);
    }

    pub fn increment_attacks_played(&mut self) {
        self.counters.attacks_played_this_turn += 1;
    }

    pub fn increment_cards_discarded(&mut self) {
        self.counters.cards_discarded_this_turn =
            self.counters.cards_discarded_this_turn.saturating_add(1);
    }

    pub fn increment_times_damaged_this_combat(&mut self) {
        self.counters.times_damaged_this_combat += 1;
    }

    pub fn mark_early_end_turn_pending(&mut self) {
        self.counters.early_end_turn_pending = true;
    }

    pub fn clear_early_end_turn_pending(&mut self) {
        self.counters.early_end_turn_pending = false;
    }

    pub fn mark_skip_monster_turn_pending(&mut self) {
        self.counters.skip_monster_turn_pending = true;
    }

    pub fn clear_skip_monster_turn_pending(&mut self) {
        self.counters.skip_monster_turn_pending = false;
    }

    pub fn mark_end_turn_hand_discarded(&mut self) {
        self.counters.end_turn_hand_discarded = true;
    }

    pub fn clear_end_turn_hand_discarded(&mut self) {
        self.counters.end_turn_hand_discarded = false;
    }

    pub fn set_discovery_cost_for_turn(&mut self, cost: Option<u8>) {
        self.counters.discovery_cost_for_turn = cost;
    }

    pub fn take_discovery_cost_for_turn(&mut self) -> Option<u8> {
        self.counters.discovery_cost_for_turn.take()
    }

    pub fn mark_player_escaping(&mut self) {
        self.counters.player_escaping = true;
    }

    pub fn clear_escape_pending_reward(&mut self) {
        self.counters.escape_pending_reward = false;
    }

    pub fn mark_escape_pending_reward(&mut self) {
        self.counters.escape_pending_reward = true;
    }

    pub fn mark_victory_triggered(&mut self) {
        self.counters.victory_triggered = true;
    }

    pub fn begin_player_phase(&mut self) {
        self.current_phase = CombatPhase::PlayerTurn;
    }

    pub fn begin_turn_transition(&mut self) {
        self.current_phase = CombatPhase::TurnTransition;
        self.counters.end_turn_powers_triggered = false;
    }

    pub fn begin_monster_turn(&mut self) {
        self.current_phase = CombatPhase::MonsterTurn;
    }

    pub fn begin_next_player_turn(&mut self, energy: u8) {
        self.turn_count += 1;
        self.begin_player_phase();
        self.energy = energy;
        self.counters.cards_played_this_turn = 0;
        self.counters.attacks_played_this_turn = 0;
        self.counters.cards_discarded_this_turn = 0;
        self.counters.card_ids_played_this_turn.clear();
        self.counters.orbs_channeled_this_turn.clear();
        self.counters.skip_monster_turn_pending = false;
        self.counters.end_turn_hand_discarded = false;
        self.counters.end_turn_powers_triggered = false;
    }
}

impl EngineRuntime {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push_front(&mut self, action: Action) {
        self.action_queue.push_front(action);
    }

    pub fn push_back(&mut self, action: Action) {
        self.action_queue.push_back(action);
    }

    pub fn pop_front(&mut self) -> Option<Action> {
        self.action_queue.pop_front()
    }

    pub fn has_actions(&self) -> bool {
        !self.action_queue.is_empty()
    }

    pub fn len(&self) -> usize {
        self.action_queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.action_queue.is_empty()
    }

    pub fn clear(&mut self) {
        self.action_queue.clear();
    }

    pub fn retain(&mut self, f: impl FnMut(&Action) -> bool) {
        self.action_queue.retain(f);
    }

    pub fn queue_actions(&mut self, actions: smallvec::SmallVec<[ActionInfo; 4]>) {
        // `ActionInfo` order is the Java call order.  Java `addToTop`
        // inserts at index 0 immediately, so later top insertions run before
        // earlier top insertions.
        for a in actions {
            match a.insertion_mode {
                AddTo::Top => self.push_front(a.action),
                AddTo::Bottom => self.push_back(a.action),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DomainCardSnapshot, DomainEventSource};

    #[test]
    fn sorted_entity_maps_preserve_json_checkpoint_round_trips() {
        let mut combat = crate::test_support::blank_test_combat();
        combat.entities.power_db.insert(11, Vec::new());
        combat.entities.power_db.insert(7, Vec::new());
        combat
            .runtime
            .monster_protocol
            .insert(11, Default::default());
        combat
            .runtime
            .monster_protocol
            .insert(7, Default::default());

        let encoded = serde_json::to_vec(&combat).expect("encode combat checkpoint as JSON");
        let restored: CombatState =
            serde_json::from_slice(&encoded).expect("restore combat checkpoint from JSON");

        assert_eq!(restored, combat);
    }

    #[test]
    fn runtime_hints_reject_the_removed_legacy_card_queue() {
        let expected = CombatRuntimeHints::default();
        let mut old_json = serde_json::to_value(&expected).expect("serialize runtime hints");
        old_json
            .as_object_mut()
            .expect("runtime hints serialize as an object")
            .insert(
                "card_queue".to_owned(),
                serde_json::json!([{
                    "card_lineage": 17,
                    "target": 2,
                    "energy_on_use": 3
                }]),
            );

        let error = serde_json::from_value::<CombatRuntimeHints>(old_json)
            .expect_err("removed ghost queue must fail the current wire contract");
        assert!(error.to_string().contains("unknown field `card_queue`"));
    }

    #[test]
    fn take_card_draw_observation_events_v1_preserves_non_draw_events() {
        let mut combat = crate::test_support::blank_test_combat();
        let first = DomainCardSnapshot {
            id: CardId::Defend,
            upgrades: 0,
            lineage: crate::runtime::combat::CardLineageId::constructed_scenario(10),
        };
        let second = DomainCardSnapshot {
            id: CardId::Strike,
            upgrades: 1,
            lineage: crate::runtime::combat::CardLineageId::constructed_scenario(11),
        };
        let preserved = DomainEvent::CardObtained {
            card: first,
            source: DomainEventSource::DeckMutation,
        };
        combat.runtime.emitted_events = vec![
            DomainEvent::CardDrawn {
                card: first,
                source: DomainEventSource::CombatDraw,
            },
            preserved.clone(),
            DomainEvent::CardDrawn {
                card: second,
                source: DomainEventSource::CombatDraw,
            },
        ];

        let observed = combat.take_card_draw_observation_events_v1();

        assert_eq!(observed, vec![first, second]);
        assert_eq!(combat.runtime.emitted_events, vec![preserved]);
    }
}
