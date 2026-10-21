use crate::content::monsters::factory::EncounterId;
use crate::content::relics::RelicId;
use crate::content::relics::RelicState;
use crate::runtime::combat::{CombatCard, MasterDeckLineageAllocator, PlayerEntity};
use crate::runtime::rng::{RngPool, StsRng};
use crate::state::events::generator::EventGenerator;
use crate::state::map::state::MapState;
use crate::state::relic_pool::{
    random_relic_by_tier_from_pools, random_relic_end_by_tier_from_pools, RelicPoolsMut,
    RelicSpawnContext,
};
use crate::state::selection::{DomainCardSnapshot, DomainEvent, DomainEventSource};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cell::Cell;

thread_local! {
    static SUPPRESS_OBTAIN_LOGS_DEPTH: Cell<usize> = const { Cell::new(0) };
}

pub fn with_suppressed_obtain_logs<T>(f: impl FnOnce() -> T) -> T {
    SUPPRESS_OBTAIN_LOGS_DEPTH.with(|depth| {
        depth.set(depth.get() + 1);
        let result = f();
        depth.set(depth.get().saturating_sub(1));
        result
    })
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunState {
    pub seed: u64,
    pub ascension_level: u8,
    pub is_daily_run: bool,
    pub highest_unlocked_ascension_level: u8,
    pub act_num: u8,
    pub floor_num: i32,
    /// Java `CardCrawlGame.playtime`, in seconds. The simulator does not
    /// advance this from wall-clock time; callers may set it for source-parity
    /// event generation such as Secret Portal.
    pub playtime_seconds: f32,
    pub player_class: &'static str, // "Ironclad", "Silent", "Defect", "Watcher"

    pub map: MapState,
    /// Java `AbstractDungeon.mapRng`, initialized once per act and retained
    /// after map generation. The same stream later rolls the burning-elite
    /// combat buff, so discarding it after emerald-node placement changes
    /// combat semantics.
    pub map_rng: StsRng,
    pub rng_pool: RngPool,
    /// Java `NeowEvent.rng`. Java leaves this `null` for Mini Neow and until
    /// the Rich Neow reward page is generated. It is then reused by reward
    /// activation, so absence is gameplay state rather than a default seed.
    pub neow_rng: Option<StsRng>,

    // Persistent out-of-combat attributes decoupled from CombatState
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub shop_purge_count: i32,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<crate::content::potions::Potion>>,
    pub keys: [bool; 3], // Ruby/Red, Sapphire/Blue, Emerald/Green
    pub is_final_act_available: bool,
    pub master_deck: Vec<CombatCard>,
    pub master_deck_lineage_allocator: MasterDeckLineageAllocator,

    // Drop modifiers
    pub potion_drop_chance_mod: i32,
    pub card_blizz_randomizer: i32,
    /// Java: cardUpgradedChance — probability that a card reward is pre-upgraded.
    /// Set per act: Exordium=0.0, TheCity=0.25 (0.125 Asc12+), TheBeyond=0.5 (0.25 Asc12+)
    pub card_upgraded_chance: f32,

    // Transient states
    pub reward_state: Option<crate::state::rewards::RewardState>,
    pub shop_state: Option<crate::state::shop::ShopState>,
    pub event_state: Option<crate::state::events::EventState>,
    pub note_for_yourself_card: crate::content::cards::CardId,
    pub note_for_yourself_upgrades: u8,

    pub event_generator: crate::state::events::generator::EventGenerator,
    /// Java `AbstractRoom.mugged` equivalent for the current room.
    ///
    /// Set when a thief escapes after stealing gold. Cleared when entering the
    /// next room. Used for post-combat room/reward semantics and future parity.
    pub room_mugged: bool,
    /// Java `AbstractRoom.smoked` equivalent for the current room.
    ///
    /// Set when Smoke Bomb ends the current combat. Cleared when entering the
    /// next room. Used for reward-screen semantics and future parity.
    pub room_smoked: bool,

    // Relic pools — filled at dungeon init. Normal rewards consume from the
    // front; shop/end relic paths consume from the end.
    // Java: commonRelicPool, uncommonRelicPool, rareRelicPool, shopRelicPool, bossRelicPool
    pub common_relic_pool: Vec<crate::content::relics::RelicId>,
    pub uncommon_relic_pool: Vec<crate::content::relics::RelicId>,
    pub rare_relic_pool: Vec<crate::content::relics::RelicId>,
    pub shop_relic_pool: Vec<crate::content::relics::RelicId>,
    pub boss_relic_pool: Vec<crate::content::relics::RelicId>,

    // Encounter scheduling queues (Java: monsterList, eliteMonsterList)
    // Populated at dungeon init via weighted roll; consumed in order as player enters combat rooms.
    pub monster_list: Vec<crate::content::monsters::factory::EncounterId>,
    pub elite_monster_list: Vec<crate::content::monsters::factory::EncounterId>,
    /// Java `AbstractDungeon.bossKey`: the boss currently shown on the map and
    /// used by `MonsterRoomBoss` when the boss room is entered. `boss_list` is
    /// the internal queue and may contain future bosses that are not public
    /// observation.
    pub boss_key: Option<crate::content::monsters::factory::EncounterId>,
    pub boss_list: Vec<crate::content::monsters::factory::EncounterId>,

    /// Set to true after boss combat ends, consumed by post_reward_state() to trigger act advance.
    pub pending_boss_reward: bool,
    /// Set when a boss relic's on-equip effect opens a run-level selection.
    /// Java obtains the boss relic before the dungeon transition; the transition
    /// happens only after that selection resolves and the boss chest is left.
    pub pending_boss_act_transition: bool,
    pub emitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RunStateCheckpointV1 {
    pub seed: u64,
    pub ascension_level: u8,
    pub is_daily_run: bool,
    pub highest_unlocked_ascension_level: u8,
    pub act_num: u8,
    pub floor_num: i32,
    pub playtime_seconds: f32,
    pub player_class: String,
    pub map: MapState,
    pub map_rng: StsRng,
    pub rng_pool: RngPool,
    pub neow_rng: Option<StsRng>,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    pub shop_purge_count: i32,
    pub relics: Vec<RelicState>,
    pub potions: Vec<Option<crate::content::potions::Potion>>,
    pub keys: [bool; 3],
    pub is_final_act_available: bool,
    pub master_deck: Vec<CombatCard>,
    pub master_deck_lineage_allocator: MasterDeckLineageAllocator,
    pub potion_drop_chance_mod: i32,
    pub card_blizz_randomizer: i32,
    pub card_upgraded_chance: f32,
    pub reward_state: Option<crate::state::rewards::RewardState>,
    pub shop_state: Option<crate::state::shop::ShopState>,
    pub event_state: Option<crate::state::events::EventState>,
    pub note_for_yourself_card: crate::content::cards::CardId,
    pub note_for_yourself_upgrades: u8,
    pub event_generator: EventGenerator,
    pub room_mugged: bool,
    pub room_smoked: bool,
    pub common_relic_pool: Vec<RelicId>,
    pub uncommon_relic_pool: Vec<RelicId>,
    pub rare_relic_pool: Vec<RelicId>,
    pub shop_relic_pool: Vec<RelicId>,
    pub boss_relic_pool: Vec<RelicId>,
    pub monster_list: Vec<EncounterId>,
    pub elite_monster_list: Vec<EncounterId>,
    pub boss_key: Option<EncounterId>,
    pub boss_list: Vec<EncounterId>,
    pub pending_boss_reward: bool,
    pub pending_boss_act_transition: bool,
    pub emitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
struct RunStateCheckpointExtrasV1 {
    #[serde(skip_serializing_if = "is_false")]
    is_daily_run: bool,
    #[serde(skip_serializing_if = "is_zero_u8")]
    highest_unlocked_ascension_level: u8,
    #[serde(skip_serializing_if = "is_zero_f32")]
    playtime_seconds: f32,
    #[serde(skip_serializing_if = "player_class_is_default_checkpoint_v1")]
    player_class: String,
    #[serde(skip_serializing_if = "MapState::is_checkpoint_externalized_placeholder")]
    map: MapState,
    #[serde(skip_serializing_if = "sts_rng_is_checkpoint_externalized_placeholder")]
    map_rng: StsRng,
    #[serde(skip_serializing_if = "rng_pool_is_checkpoint_externalized_placeholder")]
    rng_pool: RngPool,
    #[serde(skip_serializing_if = "Option::is_none")]
    neow_rng: Option<StsRng>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    relics: Vec<RelicState>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    potions: Vec<Option<crate::content::potions::Potion>>,
    #[serde(skip_serializing_if = "keys_are_empty_v1")]
    keys: [bool; 3],
    #[serde(skip_serializing_if = "is_false")]
    is_final_act_available: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    master_deck: Vec<CombatCard>,
    #[serde(skip_serializing_if = "master_deck_lineage_allocator_is_empty")]
    master_deck_lineage_allocator: MasterDeckLineageAllocator,
    #[serde(skip_serializing_if = "Option::is_none")]
    reward_state: Option<crate::state::rewards::RewardState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    shop_state: Option<crate::state::shop::ShopState>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_state: Option<crate::state::events::EventState>,
    #[serde(skip_serializing_if = "note_for_yourself_card_is_default_v1")]
    note_for_yourself_card: crate::content::cards::CardId,
    #[serde(skip_serializing_if = "is_zero_u8")]
    note_for_yourself_upgrades: u8,
    #[serde(skip_serializing_if = "EventGenerator::is_checkpoint_externalized_placeholder")]
    event_generator: EventGenerator,
    #[serde(skip_serializing_if = "is_false")]
    room_mugged: bool,
    #[serde(skip_serializing_if = "is_false")]
    room_smoked: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    common_relic_pool: Vec<RelicId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uncommon_relic_pool: Vec<RelicId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    rare_relic_pool: Vec<RelicId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    shop_relic_pool: Vec<RelicId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    boss_relic_pool: Vec<RelicId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    monster_list: Vec<EncounterId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    elite_monster_list: Vec<EncounterId>,
    #[serde(skip_serializing_if = "Option::is_none")]
    boss_key: Option<EncounterId>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    boss_list: Vec<EncounterId>,
    #[serde(skip_serializing_if = "is_false")]
    pending_boss_reward: bool,
    #[serde(skip_serializing_if = "is_false")]
    pending_boss_act_transition: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    emitted_events: Vec<DomainEvent>,
}

impl Default for RunStateCheckpointExtrasV1 {
    fn default() -> Self {
        Self {
            is_daily_run: false,
            highest_unlocked_ascension_level: 0,
            playtime_seconds: 0.0,
            player_class: default_player_class_checkpoint_v1(),
            map: MapState::default(),
            map_rng: StsRng::default(),
            rng_pool: RngPool::default(),
            neow_rng: None,
            relics: Vec::new(),
            potions: Vec::new(),
            keys: [false; 3],
            is_final_act_available: false,
            master_deck: Vec::new(),
            master_deck_lineage_allocator: MasterDeckLineageAllocator::new(),
            reward_state: None,
            shop_state: None,
            event_state: None,
            note_for_yourself_card: default_note_for_yourself_card_v1(),
            note_for_yourself_upgrades: 0,
            event_generator: event_generator_checkpoint_externalized_placeholder(),
            room_mugged: false,
            room_smoked: false,
            common_relic_pool: Vec::new(),
            uncommon_relic_pool: Vec::new(),
            rare_relic_pool: Vec::new(),
            shop_relic_pool: Vec::new(),
            boss_relic_pool: Vec::new(),
            monster_list: Vec::new(),
            elite_monster_list: Vec::new(),
            boss_key: None,
            boss_list: Vec::new(),
            pending_boss_reward: false,
            pending_boss_act_transition: false,
            emitted_events: Vec::new(),
        }
    }
}

impl RunStateCheckpointExtrasV1 {
    fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

impl Serialize for RunStateCheckpointV1 {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let extras = RunStateCheckpointExtrasV1 {
            is_daily_run: self.is_daily_run,
            highest_unlocked_ascension_level: self.highest_unlocked_ascension_level,
            playtime_seconds: self.playtime_seconds,
            player_class: self.player_class.clone(),
            map: self.map.clone(),
            map_rng: self.map_rng.clone(),
            rng_pool: self.rng_pool.clone(),
            neow_rng: self.neow_rng.clone(),
            relics: self.relics.clone(),
            potions: self.potions.clone(),
            keys: self.keys,
            is_final_act_available: self.is_final_act_available,
            master_deck: self.master_deck.clone(),
            master_deck_lineage_allocator: self.master_deck_lineage_allocator.clone(),
            reward_state: self.reward_state.clone(),
            shop_state: self.shop_state.clone(),
            event_state: self.event_state.clone(),
            note_for_yourself_card: self.note_for_yourself_card,
            note_for_yourself_upgrades: self.note_for_yourself_upgrades,
            event_generator: self.event_generator.clone(),
            room_mugged: self.room_mugged,
            room_smoked: self.room_smoked,
            common_relic_pool: self.common_relic_pool.clone(),
            uncommon_relic_pool: self.uncommon_relic_pool.clone(),
            rare_relic_pool: self.rare_relic_pool.clone(),
            shop_relic_pool: self.shop_relic_pool.clone(),
            boss_relic_pool: self.boss_relic_pool.clone(),
            monster_list: self.monster_list.clone(),
            elite_monster_list: self.elite_monster_list.clone(),
            boss_key: self.boss_key,
            boss_list: self.boss_list.clone(),
            pending_boss_reward: self.pending_boss_reward,
            pending_boss_act_transition: self.pending_boss_act_transition,
            emitted_events: self.emitted_events.clone(),
        };
        let extras = (!extras.is_empty()).then_some(extras);
        (
            self.seed,
            self.ascension_level,
            self.act_num,
            self.floor_num,
            self.current_hp,
            self.max_hp,
            self.gold,
            self.shop_purge_count,
            self.potion_drop_chance_mod,
            self.card_blizz_randomizer,
            self.card_upgraded_chance,
            extras,
        )
            .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for RunStateCheckpointV1 {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire {
            Compact(
                u64,
                u8,
                u8,
                i32,
                i32,
                i32,
                i32,
                i32,
                i32,
                i32,
                f32,
                Option<RunStateCheckpointExtrasV1>,
            ),
            Legacy(RunStateCheckpointLegacyV1),
        }

        match Wire::deserialize(deserializer)? {
            Wire::Compact(
                seed,
                ascension_level,
                act_num,
                floor_num,
                current_hp,
                max_hp,
                gold,
                shop_purge_count,
                potion_drop_chance_mod,
                card_blizz_randomizer,
                card_upgraded_chance,
                extras,
            ) => {
                let extras = extras.unwrap_or_default();
                Ok(Self {
                    seed,
                    ascension_level,
                    is_daily_run: extras.is_daily_run,
                    highest_unlocked_ascension_level: extras.highest_unlocked_ascension_level,
                    act_num,
                    floor_num,
                    playtime_seconds: extras.playtime_seconds,
                    player_class: extras.player_class,
                    map: extras.map,
                    map_rng: extras.map_rng,
                    rng_pool: extras.rng_pool,
                    neow_rng: extras.neow_rng,
                    current_hp,
                    max_hp,
                    gold,
                    shop_purge_count,
                    relics: extras.relics,
                    potions: extras.potions,
                    keys: extras.keys,
                    is_final_act_available: extras.is_final_act_available,
                    master_deck: extras.master_deck,
                    master_deck_lineage_allocator: extras.master_deck_lineage_allocator,
                    potion_drop_chance_mod,
                    card_blizz_randomizer,
                    card_upgraded_chance,
                    reward_state: extras.reward_state,
                    shop_state: extras.shop_state,
                    event_state: extras.event_state,
                    note_for_yourself_card: extras.note_for_yourself_card,
                    note_for_yourself_upgrades: extras.note_for_yourself_upgrades,
                    event_generator: extras.event_generator,
                    room_mugged: extras.room_mugged,
                    room_smoked: extras.room_smoked,
                    common_relic_pool: extras.common_relic_pool,
                    uncommon_relic_pool: extras.uncommon_relic_pool,
                    rare_relic_pool: extras.rare_relic_pool,
                    shop_relic_pool: extras.shop_relic_pool,
                    boss_relic_pool: extras.boss_relic_pool,
                    monster_list: extras.monster_list,
                    elite_monster_list: extras.elite_monster_list,
                    boss_key: extras.boss_key,
                    boss_list: extras.boss_list,
                    pending_boss_reward: extras.pending_boss_reward,
                    pending_boss_act_transition: extras.pending_boss_act_transition,
                    emitted_events: extras.emitted_events,
                })
            }
            Wire::Legacy(legacy) => Ok(legacy.into_checkpoint()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(deny_unknown_fields)]
struct RunStateCheckpointLegacyV1 {
    pub seed: u64,
    #[serde(default)]
    pub ascension_level: u8,
    #[serde(default)]
    pub is_daily_run: bool,
    #[serde(default)]
    pub highest_unlocked_ascension_level: u8,
    pub act_num: u8,
    pub floor_num: i32,
    #[serde(default)]
    pub playtime_seconds: f32,
    #[serde(default = "default_player_class_checkpoint_v1")]
    pub player_class: String,
    #[serde(default)]
    pub map: MapState,
    #[serde(default)]
    pub map_rng: StsRng,
    #[serde(default)]
    pub rng_pool: RngPool,
    #[serde(default)]
    pub neow_rng: Option<StsRng>,
    pub current_hp: i32,
    pub max_hp: i32,
    pub gold: i32,
    #[serde(default)]
    pub shop_purge_count: i32,
    #[serde(default)]
    pub relics: Vec<RelicState>,
    #[serde(default)]
    pub potions: Vec<Option<crate::content::potions::Potion>>,
    #[serde(default)]
    pub keys: [bool; 3],
    #[serde(default)]
    pub is_final_act_available: bool,
    #[serde(default)]
    pub master_deck: Vec<CombatCard>,
    #[serde(default)]
    pub master_deck_lineage_allocator: MasterDeckLineageAllocator,
    pub potion_drop_chance_mod: i32,
    pub card_blizz_randomizer: i32,
    pub card_upgraded_chance: f32,
    #[serde(default)]
    pub reward_state: Option<crate::state::rewards::RewardState>,
    #[serde(default)]
    pub shop_state: Option<crate::state::shop::ShopState>,
    #[serde(default)]
    pub event_state: Option<crate::state::events::EventState>,
    #[serde(default = "default_note_for_yourself_card_v1")]
    pub note_for_yourself_card: crate::content::cards::CardId,
    #[serde(default)]
    pub note_for_yourself_upgrades: u8,
    #[serde(default = "event_generator_checkpoint_externalized_placeholder")]
    pub event_generator: EventGenerator,
    #[serde(default)]
    pub room_mugged: bool,
    #[serde(default)]
    pub room_smoked: bool,
    #[serde(default)]
    pub common_relic_pool: Vec<RelicId>,
    #[serde(default)]
    pub uncommon_relic_pool: Vec<RelicId>,
    #[serde(default)]
    pub rare_relic_pool: Vec<RelicId>,
    #[serde(default)]
    pub shop_relic_pool: Vec<RelicId>,
    #[serde(default)]
    pub boss_relic_pool: Vec<RelicId>,
    #[serde(default)]
    pub monster_list: Vec<EncounterId>,
    #[serde(default)]
    pub elite_monster_list: Vec<EncounterId>,
    #[serde(default)]
    pub boss_key: Option<EncounterId>,
    #[serde(default)]
    pub boss_list: Vec<EncounterId>,
    #[serde(default)]
    pub pending_boss_reward: bool,
    #[serde(default)]
    pub pending_boss_act_transition: bool,
    #[serde(default)]
    pub emitted_events: Vec<DomainEvent>,
}

impl RunStateCheckpointLegacyV1 {
    fn into_checkpoint(self) -> RunStateCheckpointV1 {
        RunStateCheckpointV1 {
            seed: self.seed,
            ascension_level: self.ascension_level,
            is_daily_run: self.is_daily_run,
            highest_unlocked_ascension_level: self.highest_unlocked_ascension_level,
            act_num: self.act_num,
            floor_num: self.floor_num,
            playtime_seconds: self.playtime_seconds,
            player_class: self.player_class,
            map: self.map,
            map_rng: self.map_rng,
            rng_pool: self.rng_pool,
            neow_rng: self.neow_rng,
            current_hp: self.current_hp,
            max_hp: self.max_hp,
            gold: self.gold,
            shop_purge_count: self.shop_purge_count,
            relics: self.relics,
            potions: self.potions,
            keys: self.keys,
            is_final_act_available: self.is_final_act_available,
            master_deck: self.master_deck,
            master_deck_lineage_allocator: self.master_deck_lineage_allocator,
            potion_drop_chance_mod: self.potion_drop_chance_mod,
            card_blizz_randomizer: self.card_blizz_randomizer,
            card_upgraded_chance: self.card_upgraded_chance,
            reward_state: self.reward_state,
            shop_state: self.shop_state,
            event_state: self.event_state,
            note_for_yourself_card: self.note_for_yourself_card,
            note_for_yourself_upgrades: self.note_for_yourself_upgrades,
            event_generator: self.event_generator,
            room_mugged: self.room_mugged,
            room_smoked: self.room_smoked,
            common_relic_pool: self.common_relic_pool,
            uncommon_relic_pool: self.uncommon_relic_pool,
            rare_relic_pool: self.rare_relic_pool,
            shop_relic_pool: self.shop_relic_pool,
            boss_relic_pool: self.boss_relic_pool,
            monster_list: self.monster_list,
            elite_monster_list: self.elite_monster_list,
            boss_key: self.boss_key,
            boss_list: self.boss_list,
            pending_boss_reward: self.pending_boss_reward,
            pending_boss_act_transition: self.pending_boss_act_transition,
            emitted_events: self.emitted_events,
        }
    }
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
#[serde(deny_unknown_fields)]
pub struct RunStateScheduleCheckpointV1 {
    #[serde(default)]
    pub map_rng: StsRng,
    pub rng_pool: RngPool,
    pub neow_rng: Option<StsRng>,
    pub event_generator: EventGenerator,
    pub common_relic_pool: Vec<RelicId>,
    pub uncommon_relic_pool: Vec<RelicId>,
    pub rare_relic_pool: Vec<RelicId>,
    pub shop_relic_pool: Vec<RelicId>,
    pub boss_relic_pool: Vec<RelicId>,
    pub monster_list: Vec<EncounterId>,
    pub elite_monster_list: Vec<EncounterId>,
    pub boss_key: Option<EncounterId>,
    pub boss_list: Vec<EncounterId>,
}

mod act_transition;
mod deck_mutation;
mod encounters;
mod random_rewards;
mod relics;
mod run_resources;
mod transform;

#[cfg(test)]
mod tests;

impl RunState {
    pub fn new(
        seed: u64,
        ascension_level: u8,
        final_act: bool,
        player_class: &'static str,
    ) -> Self {
        Self::new_with_neow_eligibility(
            seed,
            ascension_level,
            final_act,
            player_class,
            crate::content::events::neow::NeowEligibilityV1::Rich,
        )
    }

    pub fn new_with_neow_eligibility(
        seed: u64,
        ascension_level: u8,
        final_act: bool,
        player_class: &'static str,
        neow_eligibility: crate::content::events::neow::NeowEligibilityV1,
    ) -> Self {
        let base_max_hp = match player_class {
            "Silent" => 70,
            "Defect" => 75,
            "Watcher" => 72,
            _ => 80,
        };
        // Generate Act 1 map; returns the consumed mapRng for emerald key placement.
        let (mut first_map, mut map_rng) =
            crate::state::map::generator::generate_map_for_act(seed, 1, ascension_level == 0);
        // Mark a random elite node for Emerald Key if Act 4 is enabled.
        // Java: setEmeraldElite() reuses the consumed mapRng, not a fresh one.
        if final_act {
            crate::state::map::generator::set_emerald_elite(&mut first_map, &mut map_rng);
        }
        let mut rs = RunState {
            seed,
            ascension_level,
            is_daily_run: false,
            highest_unlocked_ascension_level: ascension_level,
            act_num: 1,
            floor_num: 0,
            playtime_seconds: 0.0,
            player_class,
            map: MapState::new(first_map),
            map_rng,
            rng_pool: RngPool::new(seed),
            neow_rng: None,

            // Typical Ironclad defaults
            current_hp: base_max_hp,
            max_hp: base_max_hp,
            gold: 99,
            shop_purge_count: 0,
            relics: Vec::new(),
            potions: if ascension_level >= 11 {
                vec![None, None]
            } else {
                vec![None, None, None]
            },
            keys: [false; 3],
            is_final_act_available: final_act,
            master_deck: Vec::new(),
            master_deck_lineage_allocator: MasterDeckLineageAllocator::new(),
            potion_drop_chance_mod: 0,
            card_blizz_randomizer: 5, // Java: cardBlizzStartOffset = 5 (added to roll, makes rare harder)
            card_upgraded_chance: 0.0, // Exordium: always 0.0 (Java: Exordium.cardUpgradedChance = 0.0f)

            // Transient states
            reward_state: None,
            shop_state: None,
            event_state: None,
            note_for_yourself_card: crate::content::cards::CardId::IronWave,
            note_for_yourself_upgrades: 0,

            // Subsystems
            event_generator:
                crate::state::events::generator::EventGenerator::new_with_note_for_yourself(
                    1,
                    ascension_level == 0,
                ),
            room_mugged: false,
            room_smoked: false,

            // Relic pools (filled by init_relic_pools)
            common_relic_pool: Vec::new(),
            uncommon_relic_pool: Vec::new(),
            rare_relic_pool: Vec::new(),
            shop_relic_pool: Vec::new(),
            boss_relic_pool: Vec::new(),

            // Encounter lists (filled by generate_encounter_lists)
            monster_list: Vec::new(),
            elite_monster_list: Vec::new(),
            boss_key: None,
            boss_list: Vec::new(),

            pending_boss_reward: false,
            pending_boss_act_transition: false,
            emitted_events: Vec::new(),
        };
        rs.init_relic_pools();
        rs.init_encounter_lists();
        rs.init_boss_list();
        // Exordium starts MainMusic before Neow. Track selection consumes one
        // miscRng.random(1), including when audio is muted, just as at act exits.
        rs.rng_pool.misc_rng.random(1);
        rs.apply_starting_loadout();

        // --- Ascension metagame effects (Java: AbstractDungeon.java L2562-2580) ---

        // Asc 14: Decrease max HP at run start (Java: player.decreaseMaxHealth(player.getAscensionMaxHPLoss()))
        // Ironclad returns 5, Silent returns 4, Defect returns 4, Watcher returns 4
        if ascension_level >= 14 {
            let hp_loss = match player_class {
                "Ironclad" => 5,
                "Silent" => 4,
                "Defect" => 4,
                "Watcher" => 4,
                _ => 5,
            };
            rs.max_hp -= hp_loss;
            rs.current_hp = rs.current_hp.min(rs.max_hp);
        }

        // Asc 6: Start at 90% of max HP (Java: player.currentHealth = round(maxHealth * 0.9f))
        if ascension_level >= 6 {
            rs.current_hp = (rs.max_hp as f32 * 0.9).round() as i32;
        }

        // Asc 10: Add Ascender's Bane (unremovable curse) to starting deck
        if ascension_level >= 10 {
            let lineage = rs.allocate_master_deck_lineage();
            // Java adds Ascender's Bane in dungeonTransitionSetup(), before
            // initializeStarterDeck() appends the class starter cards. Master
            // deck order is an input to the first combat shuffle, so retain
            // that ordering even though the deck is usually rendered as an
            // unordered collection.
            rs.master_deck.insert(
                0,
                crate::runtime::combat::CombatCard::with_lineage(
                    crate::content::cards::CardId::AscendersBane,
                    lineage,
                ),
            );
        }

        // Initialize only the Neow introduction. Java does not construct the
        // rewards (or Rich Neow's RNG) until the player advances the dialog.
        crate::content::events::neow::setup_neow_event(&mut rs, neow_eligibility);
        rs
    }

    fn apply_starting_loadout(&mut self) {
        use crate::content::cards::CardId;
        use crate::content::relics::{RelicId, RelicState};

        if self.master_deck.is_empty() {
            let starter_cards: &'static [CardId] = match self.player_class {
                "Silent" => &[
                    CardId::StrikeG,
                    CardId::StrikeG,
                    CardId::StrikeG,
                    CardId::StrikeG,
                    CardId::StrikeG,
                    CardId::DefendG,
                    CardId::DefendG,
                    CardId::DefendG,
                    CardId::DefendG,
                    CardId::DefendG,
                    CardId::Survivor,
                    CardId::Neutralize,
                ],
                "Defect" => &[
                    CardId::StrikeB,
                    CardId::StrikeB,
                    CardId::StrikeB,
                    CardId::StrikeB,
                    CardId::DefendB,
                    CardId::DefendB,
                    CardId::DefendB,
                    CardId::DefendB,
                    CardId::Zap,
                    CardId::Dualcast,
                ],
                "Watcher" => &[
                    CardId::StrikeP,
                    CardId::StrikeP,
                    CardId::StrikeP,
                    CardId::StrikeP,
                    CardId::DefendP,
                    CardId::DefendP,
                    CardId::DefendP,
                    CardId::DefendP,
                    CardId::Eruption,
                    CardId::Vigilance,
                ],
                _ => &[
                    CardId::Strike,
                    CardId::Strike,
                    CardId::Strike,
                    CardId::Strike,
                    CardId::Strike,
                    CardId::Defend,
                    CardId::Defend,
                    CardId::Defend,
                    CardId::Defend,
                    CardId::Bash,
                ],
            };

            for &card_id in starter_cards {
                let lineage = self.allocate_master_deck_lineage();
                self.master_deck
                    .push(crate::runtime::combat::CombatCard::with_lineage(
                        card_id, lineage,
                    ));
            }
        }

        if self.relics.is_empty() {
            let starter_relic = match self.player_class {
                "Silent" => RelicId::SnakeRing,
                "Defect" => RelicId::CrackedCore,
                "Watcher" => RelicId::PureWater,
                _ => RelicId::BurningBlood,
            };
            self.relics.push(RelicState::new(starter_relic));
        }
    }

    /// Maps player_class string to PotionClass enum for potion generation.
    pub fn potion_class(&self) -> crate::content::potions::PotionClass {
        match self.player_class {
            "Ironclad" => crate::content::potions::PotionClass::Ironclad,
            "Silent" => crate::content::potions::PotionClass::Silent,
            "Defect" => crate::content::potions::PotionClass::Defect,
            "Watcher" => crate::content::potions::PotionClass::Watcher,
            _ => crate::content::potions::PotionClass::Ironclad,
        }
    }

    /// Extends this RunState's persistent player properties into a temporary CombatState player.
    pub fn build_combat_player(&self, id: crate::EntityId) -> PlayerEntity {
        let master_max_orbs = if self.player_class == "Defect" {
            3
        } else if self
            .relics
            .iter()
            .any(|relic| relic.id == crate::content::relics::RelicId::PrismaticShard)
        {
            1
        } else {
            0
        };
        let mut p = PlayerEntity {
            id,
            current_hp: self.current_hp,
            max_hp: self.max_hp,
            block: 0,
            facing_left: false,
            gold_delta_this_combat: 0,
            gold: self.gold,
            max_orbs: master_max_orbs,
            orbs: (0..master_max_orbs)
                .map(|_| {
                    crate::runtime::combat::OrbEntity::new(crate::runtime::combat::OrbId::Empty)
                })
                .collect(),
            stance: crate::runtime::combat::StanceId::Neutral,
            relics: Vec::new(),
            relic_buses: Default::default(),
            energy_master: 3,
        };
        // Safely re-initialize the bus mappings through the add_relic interface
        for rs in &self.relics {
            p.add_relic(rs.clone());
        }
        p
    }

    /// Recovers persistent properties modified by the combat simulation
    pub fn absorb_combat_player(&mut self, player: PlayerEntity) {
        self.current_hp = player.current_hp;
        self.max_hp = player.max_hp;
        // Don't arbitrarily overwrite gold directly from player.gold in case we have to manage specific event drops,
        // but typically player.gold is synced directly, or using gold_delta_this_combat:
        self.gold = player.gold;

        // Persist counter states inside relics (e.g., Pen Nib)
        self.relics = player.relics;
    }
}

impl RunStateCheckpointV1 {
    pub fn from_run_state(run_state: &RunState) -> Self {
        Self {
            seed: run_state.seed,
            ascension_level: run_state.ascension_level,
            is_daily_run: run_state.is_daily_run,
            highest_unlocked_ascension_level: run_state.highest_unlocked_ascension_level,
            act_num: run_state.act_num,
            floor_num: run_state.floor_num,
            playtime_seconds: run_state.playtime_seconds,
            player_class: run_state.player_class.to_string(),
            map: run_state.map.clone(),
            map_rng: run_state.map_rng.clone(),
            rng_pool: run_state.rng_pool.clone(),
            neow_rng: run_state.neow_rng.clone(),
            current_hp: run_state.current_hp,
            max_hp: run_state.max_hp,
            gold: run_state.gold,
            shop_purge_count: run_state.shop_purge_count,
            relics: run_state.relics.clone(),
            potions: run_state.potions.clone(),
            keys: run_state.keys,
            is_final_act_available: run_state.is_final_act_available,
            master_deck: run_state.master_deck.clone(),
            master_deck_lineage_allocator: run_state.master_deck_lineage_allocator.clone(),
            potion_drop_chance_mod: run_state.potion_drop_chance_mod,
            card_blizz_randomizer: run_state.card_blizz_randomizer,
            card_upgraded_chance: run_state.card_upgraded_chance,
            reward_state: run_state.reward_state.clone(),
            shop_state: run_state.shop_state.clone(),
            event_state: run_state.event_state.clone(),
            note_for_yourself_card: run_state.note_for_yourself_card,
            note_for_yourself_upgrades: run_state.note_for_yourself_upgrades,
            event_generator: run_state.event_generator.clone(),
            room_mugged: run_state.room_mugged,
            room_smoked: run_state.room_smoked,
            common_relic_pool: run_state.common_relic_pool.clone(),
            uncommon_relic_pool: run_state.uncommon_relic_pool.clone(),
            rare_relic_pool: run_state.rare_relic_pool.clone(),
            shop_relic_pool: run_state.shop_relic_pool.clone(),
            boss_relic_pool: run_state.boss_relic_pool.clone(),
            monster_list: run_state.monster_list.clone(),
            elite_monster_list: run_state.elite_monster_list.clone(),
            boss_key: run_state.boss_key,
            boss_list: run_state.boss_list.clone(),
            pending_boss_reward: run_state.pending_boss_reward,
            pending_boss_act_transition: run_state.pending_boss_act_transition,
            emitted_events: run_state.emitted_events.clone(),
        }
    }

    pub fn into_run_state(self) -> Result<RunState, String> {
        validate_checkpoint_master_deck_lineage(
            &self.master_deck,
            &self.master_deck_lineage_allocator,
        )?;
        Ok(RunState {
            seed: self.seed,
            ascension_level: self.ascension_level,
            is_daily_run: self.is_daily_run,
            highest_unlocked_ascension_level: self.highest_unlocked_ascension_level,
            act_num: self.act_num,
            floor_num: self.floor_num,
            playtime_seconds: self.playtime_seconds,
            player_class: checkpoint_player_class(&self.player_class)?,
            map: self.map,
            map_rng: self.map_rng,
            rng_pool: self.rng_pool,
            neow_rng: self.neow_rng,
            current_hp: self.current_hp,
            max_hp: self.max_hp,
            gold: self.gold,
            shop_purge_count: self.shop_purge_count,
            relics: self.relics,
            potions: self.potions,
            keys: self.keys,
            is_final_act_available: self.is_final_act_available,
            master_deck: self.master_deck,
            master_deck_lineage_allocator: self.master_deck_lineage_allocator,
            potion_drop_chance_mod: self.potion_drop_chance_mod,
            card_blizz_randomizer: self.card_blizz_randomizer,
            card_upgraded_chance: self.card_upgraded_chance,
            reward_state: self.reward_state,
            shop_state: self.shop_state,
            event_state: self.event_state,
            note_for_yourself_card: self.note_for_yourself_card,
            note_for_yourself_upgrades: self.note_for_yourself_upgrades,
            event_generator: self.event_generator,
            room_mugged: self.room_mugged,
            room_smoked: self.room_smoked,
            common_relic_pool: self.common_relic_pool,
            uncommon_relic_pool: self.uncommon_relic_pool,
            rare_relic_pool: self.rare_relic_pool,
            shop_relic_pool: self.shop_relic_pool,
            boss_relic_pool: self.boss_relic_pool,
            monster_list: self.monster_list,
            elite_monster_list: self.elite_monster_list,
            boss_key: self.boss_key,
            boss_list: self.boss_list,
            pending_boss_reward: self.pending_boss_reward,
            pending_boss_act_transition: self.pending_boss_act_transition,
            emitted_events: self.emitted_events,
        })
    }

    pub fn take_schedule_for_external_ref(&mut self) -> RunStateScheduleCheckpointV1 {
        RunStateScheduleCheckpointV1 {
            map_rng: std::mem::take(&mut self.map_rng),
            rng_pool: std::mem::take(&mut self.rng_pool),
            neow_rng: std::mem::take(&mut self.neow_rng),
            event_generator: std::mem::replace(
                &mut self.event_generator,
                EventGenerator::checkpoint_externalized_placeholder(),
            ),
            common_relic_pool: std::mem::take(&mut self.common_relic_pool),
            uncommon_relic_pool: std::mem::take(&mut self.uncommon_relic_pool),
            rare_relic_pool: std::mem::take(&mut self.rare_relic_pool),
            shop_relic_pool: std::mem::take(&mut self.shop_relic_pool),
            boss_relic_pool: std::mem::take(&mut self.boss_relic_pool),
            monster_list: std::mem::take(&mut self.monster_list),
            elite_monster_list: std::mem::take(&mut self.elite_monster_list),
            boss_key: self.boss_key.take(),
            boss_list: std::mem::take(&mut self.boss_list),
        }
    }

    pub fn restore_schedule_from_external_ref(&mut self, schedule: RunStateScheduleCheckpointV1) {
        self.map_rng = schedule.map_rng;
        self.rng_pool = schedule.rng_pool;
        self.neow_rng = schedule.neow_rng;
        self.event_generator = schedule.event_generator;
        self.common_relic_pool = schedule.common_relic_pool;
        self.uncommon_relic_pool = schedule.uncommon_relic_pool;
        self.rare_relic_pool = schedule.rare_relic_pool;
        self.shop_relic_pool = schedule.shop_relic_pool;
        self.boss_relic_pool = schedule.boss_relic_pool;
        self.monster_list = schedule.monster_list;
        self.elite_monster_list = schedule.elite_monster_list;
        self.boss_key = schedule.boss_key;
        self.boss_list = schedule.boss_list;
    }

    pub fn take_emitted_events_for_external_ref(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.emitted_events)
    }

    pub fn restore_emitted_events_from_external_ref(&mut self, emitted_events: Vec<DomainEvent>) {
        self.emitted_events = emitted_events;
    }
}

fn rng_pool_is_checkpoint_externalized_placeholder(value: &RngPool) -> bool {
    value == &RngPool::default()
}

fn sts_rng_is_checkpoint_externalized_placeholder(value: &StsRng) -> bool {
    value == &StsRng::default()
}

fn event_generator_checkpoint_externalized_placeholder() -> EventGenerator {
    EventGenerator::checkpoint_externalized_placeholder()
}

fn is_zero_u8(value: &u8) -> bool {
    *value == 0
}

fn is_zero_f32(value: &f32) -> bool {
    *value == 0.0
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn keys_are_empty_v1(value: &[bool; 3]) -> bool {
    !value.iter().any(|key| *key)
}

fn default_player_class_checkpoint_v1() -> String {
    "Ironclad".to_string()
}

fn player_class_is_default_checkpoint_v1(value: &String) -> bool {
    value == "Ironclad"
}

fn default_note_for_yourself_card_v1() -> crate::content::cards::CardId {
    crate::content::cards::CardId::IronWave
}

fn note_for_yourself_card_is_default_v1(value: &crate::content::cards::CardId) -> bool {
    *value == default_note_for_yourself_card_v1()
}

fn master_deck_lineage_allocator_is_empty(value: &MasterDeckLineageAllocator) -> bool {
    value.next_acquisition_ordinal() == 1
}

fn validate_checkpoint_master_deck_lineage(
    cards: &[CombatCard],
    allocator: &MasterDeckLineageAllocator,
) -> Result<(), String> {
    use crate::runtime::combat::CardLineageOrigin;
    use std::collections::HashSet;

    let mut seen = HashSet::with_capacity(cards.len());
    for card in cards {
        if !seen.insert(card.lineage) {
            return Err(format!(
                "checkpoint master deck contains duplicate card lineage {}",
                card.lineage
            ));
        }
        match card.lineage.origin() {
            CardLineageOrigin::MasterDeck if !allocator.contains_allocated(card.lineage) => {
                return Err(format!(
                    "checkpoint allocator has not allocated master-deck lineage {}",
                    card.lineage
                ));
            }
            CardLineageOrigin::CombatGenerated => {
                return Err(format!(
                    "combat-generated lineage {} cannot inhabit the master deck",
                    card.lineage
                ));
            }
            CardLineageOrigin::MasterDeck | CardLineageOrigin::ConstructedScenario => {}
        }
    }
    Ok(())
}

fn checkpoint_player_class(raw: &str) -> Result<&'static str, String> {
    match raw {
        "Ironclad" => Ok("Ironclad"),
        "Silent" => Ok("Silent"),
        "Defect" => Ok("Defect"),
        "Watcher" => Ok("Watcher"),
        _ => Err(format!("unsupported checkpoint player class `{raw}`")),
    }
}

impl RunState {
    pub fn emit_event(&mut self, event: DomainEvent) {
        self.emitted_events.push(event);
    }

    pub fn take_emitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.emitted_events)
    }

    pub(super) fn snapshot_card(card: &CombatCard) -> DomainCardSnapshot {
        DomainCardSnapshot {
            id: card.id,
            upgrades: card.upgrades,
            lineage: card.lineage,
        }
    }
}
