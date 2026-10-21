use crate::runtime::rng::RngPool;
use crate::state::events::context::EventContext;
use crate::state::events::EventId;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum RoomRoll {
    Monster,
    Shop,
    Treasure,
    Event,
    Elite, // Requires DeadlyEvents mod
}

fn fill_java_room_results(
    results: &mut [RoomRoll; 100],
    fill_index: i32,
    size: i32,
    value: RoomRoll,
) {
    let from = fill_index.min(99) as usize;
    let to = (fill_index + size).min(100) as usize;
    if from < to {
        for slot in &mut results[from..to] {
            *slot = value;
        }
    }
}

fn java_room_choice_from_sizes(
    roll_idx: usize,
    monster_size: i32,
    shop_size: i32,
    treasure_size: i32,
) -> RoomRoll {
    let mut possible_results = [RoomRoll::Event; 100];
    let mut fill_index = 0;

    // Java EventHelper.roll fills a 100-entry array. The start index is clamped
    // to 99 instead of 100, so late categories can overwrite the final slot
    // when earlier chances have already filled the whole array.
    fill_java_room_results(
        &mut possible_results,
        fill_index,
        monster_size,
        RoomRoll::Monster,
    );
    fill_index += monster_size;
    fill_java_room_results(&mut possible_results, fill_index, shop_size, RoomRoll::Shop);
    fill_index += shop_size;
    fill_java_room_results(
        &mut possible_results,
        fill_index,
        treasure_size,
        RoomRoll::Treasure,
    );

    possible_results[roll_idx]
}

#[derive(Debug, Clone, PartialEq, serde::Deserialize, serde::Serialize)]
pub struct EventGenerator {
    pub event_pool: Vec<EventId>,
    pub shrine_pool: Vec<EventId>,
    pub one_time_event_pool: Vec<EventId>,
    one_time_event_pool_initialized: bool,

    pub elite_chance: f32,
    pub monster_chance: f32,
    pub shop_chance: f32,
    pub treasure_chance: f32,
    pub shrine_chance: f32,
}

impl EventGenerator {
    pub fn checkpoint_externalized_placeholder() -> Self {
        Self {
            event_pool: Vec::new(),
            shrine_pool: Vec::new(),
            one_time_event_pool: Vec::new(),
            one_time_event_pool_initialized: false,
            elite_chance: 0.0,
            monster_chance: 0.0,
            shop_chance: 0.0,
            treasure_chance: 0.0,
            shrine_chance: 0.0,
        }
    }

    pub fn is_checkpoint_externalized_placeholder(&self) -> bool {
        self.event_pool.is_empty()
            && self.shrine_pool.is_empty()
            && self.one_time_event_pool.is_empty()
            && !self.one_time_event_pool_initialized
            && self.elite_chance == 0.0
            && self.monster_chance == 0.0
            && self.shop_chance == 0.0
            && self.treasure_chance == 0.0
            && self.shrine_chance == 0.0
    }

    pub fn new(act_num: u8) -> Self {
        Self::new_with_note_for_yourself(act_num, true)
    }

    pub fn new_with_note_for_yourself(act_num: u8, include_note_for_yourself: bool) -> Self {
        let mut gen = Self {
            event_pool: Vec::new(),
            shrine_pool: Vec::new(),
            one_time_event_pool: Vec::new(),
            one_time_event_pool_initialized: false,
            elite_chance: 0.0,
            monster_chance: 0.10,
            shop_chance: 0.03,
            treasure_chance: 0.02,
            shrine_chance: 0.25,
        };
        gen.initialize_event_pools(act_num);
        if !include_note_for_yourself {
            gen.one_time_event_pool
                .retain(|&event| event != EventId::NoteForYourself);
        }
        gen
    }

    pub fn initialize_event_pools(&mut self, act_num: u8) {
        // Act-specific event pool
        self.event_pool = match act_num {
            1 => vec![
                EventId::BigFish,
                EventId::Cleric,
                EventId::DeadAdventurer,
                EventId::GoldenIdol,
                EventId::GoldenWing,
                EventId::WorldOfGoop,
                EventId::Ssssserpent,
                EventId::LivingWall,
                EventId::Mushrooms,
                EventId::ScrapOoze,
                EventId::ShiningLight,
            ],
            2 => vec![
                EventId::Addict,
                EventId::BackTotheBasics,
                EventId::Beggar,
                EventId::Colosseum,
                EventId::CursedTome,
                EventId::DrugDealer,
                EventId::ForgottenAltar,
                EventId::Ghosts,
                EventId::MaskedBandits,
                EventId::Nest,
                EventId::TheLibrary,
                EventId::Mausoleum,
                EventId::Vampires,
            ],
            3 => vec![
                EventId::Falling,
                EventId::MindBloom,
                EventId::MoaiHead,
                EventId::MysteriousSphere,
                EventId::SensoryStone,
                EventId::TombRedMask,
                EventId::WindingHalls,
            ],
            _ => Vec::new(),
        };

        // Per-act shrine pool
        self.shrine_pool = match act_num {
            1 => vec![
                EventId::MatchAndKeep,
                EventId::GoldenShrine,
                EventId::Transmorgrifier,
                EventId::Purifier,
                EventId::UpgradeShrine,
                EventId::GremlinWheelGame,
            ],
            2 => vec![
                EventId::MatchAndKeep,
                EventId::GremlinWheelGame,
                EventId::GoldenShrine,
                EventId::Transmorgrifier,
                EventId::Purifier,
                EventId::UpgradeShrine,
            ],
            3 => vec![
                EventId::MatchAndKeep,
                EventId::GremlinWheelGame,
                EventId::GoldenShrine,
                EventId::Transmorgrifier,
                EventId::Purifier,
                EventId::UpgradeShrine,
            ],
            _ => Vec::new(),
        };

        if !self.one_time_event_pool_initialized {
            self.one_time_event_pool = vec![
                EventId::AccursedBlacksmith,
                EventId::BonfireElementals,
                EventId::Designer,
                EventId::Duplicator,
                EventId::FaceTrader,
                EventId::FountainOfCurseCleansing,
                EventId::KnowingSkull,
                EventId::Lab,
                EventId::Nloth,
                EventId::NoteForYourself,
                EventId::SecretPortal,
                EventId::TheJoust,
                EventId::WeMeetAgain,
                EventId::WomanInBlue,
            ];
            self.one_time_event_pool_initialized = true;
        }
    }

    /// Mirrors Java's EventHelper.roll(Random eventRng)
    pub fn roll_room_type(&mut self, rng: &mut RngPool, ctx: &EventContext) -> RoomRoll {
        let roll = rng.event_rng.random_f32();

        let mut force_chest = false;
        if ctx.tiny_chest_counter == 4 {
            // Java computes eventRng.random() before checking the Tiny Chest counter.
            force_chest = true;
        }

        let monster_size = (self.monster_chance * 100.0) as i32;
        let shop_size = if ctx.previous_room_was_shop {
            0
        } else {
            (self.shop_chance * 100.0) as i32
        };
        let treasure_size = (self.treasure_chance * 100.0) as i32;

        let roll_idx = (roll * 100.0) as usize;

        let mut choice =
            java_room_choice_from_sizes(roll_idx, monster_size, shop_size, treasure_size);

        if force_chest {
            choice = RoomRoll::Treasure;
        }

        // Vanilla never allocates a question-mark roll slot to Elite without
        // Deadly Events, but EventHelper still advances this persisted chance.
        self.elite_chance = if choice == RoomRoll::Elite {
            0.0
        } else {
            self.elite_chance + 0.10
        };

        if choice == RoomRoll::Monster {
            if ctx.has_juzu_bracelet {
                // Juzu Bracelet converts Monster to Event, but Monster chance still resets to 0.10.
                choice = RoomRoll::Event;
            }
            self.monster_chance = 0.10;
        } else {
            self.monster_chance += 0.10;
        }

        if choice == RoomRoll::Shop {
            self.shop_chance = 0.03;
        } else {
            self.shop_chance += 0.03;
        }

        if choice == RoomRoll::Treasure {
            self.treasure_chance = 0.02;
        } else {
            self.treasure_chance += 0.02;
        }

        choice
    }

    /// Returns every room kind reachable by Java's current 100-slot roll
    /// without consuming RNG or updating the adaptive chances.
    pub fn possible_room_rolls(&self, ctx: &EventContext) -> Vec<RoomRoll> {
        if ctx.tiny_chest_counter == 4 {
            return vec![RoomRoll::Treasure];
        }

        let monster_size = (self.monster_chance * 100.0) as i32;
        let shop_size = if ctx.previous_room_was_shop {
            0
        } else {
            (self.shop_chance * 100.0) as i32
        };
        let treasure_size = (self.treasure_chance * 100.0) as i32;
        let mut reachable = Vec::new();
        for roll_idx in 0..100 {
            let mut choice =
                java_room_choice_from_sizes(roll_idx, monster_size, shop_size, treasure_size);
            if choice == RoomRoll::Monster && ctx.has_juzu_bracelet {
                choice = RoomRoll::Event;
            }
            if !reachable.contains(&choice) {
                reachable.push(choice);
            }
        }
        [
            RoomRoll::Event,
            RoomRoll::Monster,
            RoomRoll::Shop,
            RoomRoll::Treasure,
            RoomRoll::Elite,
        ]
        .into_iter()
        .filter(|outcome| reachable.contains(outcome))
        .collect()
    }

    pub fn reset_probabilities(&mut self) {
        self.elite_chance = 0.0;
        self.monster_chance = 0.10;
        self.shop_chance = 0.03;
        self.treasure_chance = 0.02;
    }

    /// Mirrors Java's AbstractDungeon.generateEvent(Random rng).
    ///
    /// Java does not repopulate event pools or invent a fallback event when
    /// both event and shrine pools are exhausted. The public wrapper panics
    /// with a clear message in that invalid run state; tests and callers that
    /// want to inspect depletion can use `try_generate_event`.
    pub fn generate_event(&mut self, rng: &mut RngPool, ctx: &EventContext) -> EventId {
        self.try_generate_event(rng, ctx)
            .expect("Java generateEvent has no valid event or shrine candidate")
    }

    pub fn try_generate_event(&mut self, rng: &mut RngPool, ctx: &EventContext) -> Option<EventId> {
        let shrine_roll = rng.event_rng.random_f32_range(1.0);

        if shrine_roll < self.shrine_chance {
            if !self.shrine_pool.is_empty() || !self.one_time_event_pool.is_empty() {
                return self.get_shrine_event(rng, ctx);
            }
            if !self.event_pool.is_empty() {
                return self.get_pool_event(rng, ctx);
            }
            return None;
        }

        if let Some(event) = self.try_get_pool_event(rng, ctx) {
            return Some(event);
        }
        if !self.shrine_pool.is_empty() || !self.one_time_event_pool.is_empty() {
            return self.get_shrine_event(rng, ctx);
        }
        None
    }

    fn get_shrine_event(&mut self, rng: &mut RngPool, ctx: &EventContext) -> Option<EventId> {
        let mut candidates: Vec<EventId> = Vec::new();
        candidates.extend_from_slice(&self.shrine_pool);

        for &event in &self.one_time_event_pool {
            let ok = is_one_time_event_candidate(event, ctx);
            if ok {
                candidates.push(event);
            }
        }

        if candidates.is_empty() {
            return None;
        }

        let idx = rng.event_rng.random((candidates.len() - 1) as i32) as usize;
        let chosen = candidates[idx];

        if let Some(pos) = self.shrine_pool.iter().position(|&e| e == chosen) {
            self.shrine_pool.remove(pos);
        }
        if let Some(pos) = self.one_time_event_pool.iter().position(|&e| e == chosen) {
            self.one_time_event_pool.remove(pos);
        }

        Some(chosen)
    }

    fn get_pool_event(&mut self, rng: &mut RngPool, ctx: &EventContext) -> Option<EventId> {
        self.try_get_pool_event(rng, ctx)
    }

    fn try_get_pool_event(&mut self, rng: &mut RngPool, ctx: &EventContext) -> Option<EventId> {
        let hp_pct = if ctx.max_hp > 0 {
            ctx.current_hp as f32 / ctx.max_hp as f32
        } else {
            1.0
        };
        let map_midpoint = (ctx.map_height / 2) as i32;

        let mut candidates: Vec<EventId> = Vec::new();
        for &event in &self.event_pool {
            let ok = is_pool_event_candidate(event, ctx, hp_pct, map_midpoint);
            if ok {
                candidates.push(event);
            }
        }

        if candidates.is_empty() {
            return None;
        }

        let idx = rng.event_rng.random((candidates.len() - 1) as i32) as usize;
        let chosen = candidates[idx];

        if let Some(pos) = self.event_pool.iter().position(|&e| e == chosen) {
            self.event_pool.remove(pos);
        }

        Some(chosen)
    }
}

fn is_note_for_yourself_available(ctx: &EventContext) -> bool {
    if ctx.is_daily_run || ctx.ascension_level >= 15 {
        return false;
    }
    ctx.ascension_level == 0 || ctx.ascension_level < ctx.highest_unlocked_ascension_level
}

fn is_one_time_event_candidate(event: EventId, ctx: &EventContext) -> bool {
    match event {
        EventId::FountainOfCurseCleansing => ctx.has_curses,
        EventId::Designer => (ctx.act_num == 2 || ctx.act_num == 3) && ctx.gold >= 75,
        EventId::Duplicator => ctx.act_num == 2 || ctx.act_num == 3,
        EventId::FaceTrader => ctx.act_num == 1 || ctx.act_num == 2,
        EventId::KnowingSkull => ctx.act_num == 2 && ctx.current_hp > 12,
        EventId::Nloth => ctx.act_num == 2 && ctx.relic_count >= 2,
        EventId::TheJoust => ctx.act_num == 2 && ctx.gold >= 50,
        EventId::WomanInBlue => ctx.gold >= 50,
        EventId::SecretPortal => {
            ctx.act_num == 3
                && crate::runtime::event_time::secret_portal_playtime(ctx.playtime_seconds) >= 800.0
        }
        EventId::NoteForYourself => is_note_for_yourself_available(ctx),
        _ => true,
    }
}

#[cfg(test)]
mod time_input_tests {
    use super::*;
    use crate::runtime::event_time::EventTimeScope;

    #[test]
    fn portal_gate_consumes_exact_external_time_only_in_act_three() {
        let mut ctx = EventContext {
            act_num: 3,
            playtime_seconds: 0.0,
            ..super::tests::ctx()
        };
        for (time, expected) in [
            (f32::from_bits(800.0f32.to_bits() - 1), false),
            (800.0, true),
            (800.01, true),
        ] {
            let scope = EventTimeScope::begin(vec![time]).unwrap();
            assert_eq!(
                is_one_time_event_candidate(EventId::SecretPortal, &ctx),
                expected
            );
            assert_eq!(scope.finish(), (1, 1));
        }
        let scope = EventTimeScope::begin(Vec::new()).unwrap();
        assert!(!is_one_time_event_candidate(EventId::SecretPortal, &ctx));
        assert_eq!(scope.finish(), (0, 1));
        ctx.act_num = 2;
        let scope = EventTimeScope::begin(Vec::new()).unwrap();
        assert!(!is_one_time_event_candidate(EventId::SecretPortal, &ctx));
        assert_eq!(scope.finish(), (0, 0));
        ctx.act_num = 3;
        ctx.playtime_seconds = 900.0;
        assert!(is_one_time_event_candidate(EventId::SecretPortal, &ctx));
    }
}

fn is_pool_event_candidate(
    event: EventId,
    ctx: &EventContext,
    hp_pct: f32,
    map_midpoint: i32,
) -> bool {
    match event {
        EventId::DeadAdventurer => ctx.floor_num > 6,
        EventId::Mushrooms => ctx.floor_num > 6,
        EventId::MoaiHead => ctx.has_golden_idol || hp_pct <= 0.5,
        EventId::Cleric => ctx.gold >= 35,
        EventId::Beggar => ctx.gold >= 75,
        EventId::Colosseum => ctx.map_current_y.is_some_and(|map_y| map_y > map_midpoint),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    pub(super) fn ctx() -> EventContext {
        EventContext {
            act_num: 1,
            ascension_level: 0,
            is_daily_run: false,
            highest_unlocked_ascension_level: 0,
            floor_num: 1,
            map_current_y: Some(0),
            map_height: 15,
            gold: 99,
            current_hp: 80,
            max_hp: 80,
            playtime_seconds: 0.0,
            has_curses: false,
            tiny_chest_counter: 0,
            has_golden_idol: false,
            has_juzu_bracelet: false,
            previous_room_was_shop: false,
            relic_count: 0,
        }
    }

    #[test]
    fn note_for_yourself_uses_java_daily_ascension_and_profile_gate() {
        let mut c = ctx();
        assert!(is_one_time_event_candidate(EventId::NoteForYourself, &c));

        c.is_daily_run = true;
        assert!(!is_one_time_event_candidate(EventId::NoteForYourself, &c));

        c.is_daily_run = false;
        c.ascension_level = 15;
        c.highest_unlocked_ascension_level = 20;
        assert!(!is_one_time_event_candidate(EventId::NoteForYourself, &c));

        c.ascension_level = 10;
        c.highest_unlocked_ascension_level = 10;
        assert!(!is_one_time_event_candidate(EventId::NoteForYourself, &c));

        c.highest_unlocked_ascension_level = 20;
        assert!(is_one_time_event_candidate(EventId::NoteForYourself, &c));
    }

    #[test]
    fn event_room_roll_blocks_shop_result_after_shop_room_like_java() {
        let mut generator = EventGenerator::new(1);
        generator.monster_chance = 0.0;
        generator.shop_chance = 1.0;
        generator.treasure_chance = 0.0;
        let mut rng = crate::runtime::rng::RngPool::new(1);
        let mut context = ctx();
        context.previous_room_was_shop = true;

        assert_eq!(
            generator.roll_room_type(&mut rng, &context),
            RoomRoll::Event,
            "Java EventHelper.roll sets shopSize to 0 when the previous current room is ShopRoom"
        );
    }

    #[test]
    fn vanilla_event_roll_tracks_hidden_elite_chance_without_enabling_elite_rooms() {
        let mut generator = EventGenerator::new(1);
        let mut rng = crate::runtime::rng::RngPool::new(1);

        assert_eq!(generator.elite_chance, 0.0);
        assert_ne!(generator.roll_room_type(&mut rng, &ctx()), RoomRoll::Elite);
        assert_eq!(generator.elite_chance, 0.10);
        assert!(!generator
            .possible_room_rolls(&ctx())
            .contains(&RoomRoll::Elite));

        generator.reset_probabilities();
        assert_eq!(generator.elite_chance, 0.0);
    }

    #[test]
    fn possible_room_rolls_reports_current_reachable_set_without_consuming_state() {
        let generator = EventGenerator::new(1);
        let ordinary = ctx();
        assert_eq!(
            generator.possible_room_rolls(&ordinary),
            vec![
                RoomRoll::Event,
                RoomRoll::Monster,
                RoomRoll::Shop,
                RoomRoll::Treasure,
            ]
        );

        let mut after_shop = ordinary.clone();
        after_shop.previous_room_was_shop = true;
        assert!(!generator
            .possible_room_rolls(&after_shop)
            .contains(&RoomRoll::Shop));

        let mut third_unknown_room = ordinary.clone();
        third_unknown_room.tiny_chest_counter = 3;
        assert!(generator
            .possible_room_rolls(&third_unknown_room)
            .contains(&RoomRoll::Event));

        let mut forced_chest = ordinary;
        forced_chest.tiny_chest_counter = 4;
        assert_eq!(
            generator.possible_room_rolls(&forced_chest),
            vec![RoomRoll::Treasure]
        );
    }

    #[test]
    fn event_room_roll_uses_java_final_slot_overwrite_when_chances_overflow() {
        assert_eq!(java_room_choice_from_sizes(98, 90, 30, 2), RoomRoll::Shop);
        assert_eq!(
            java_room_choice_from_sizes(99, 90, 30, 2),
            RoomRoll::Treasure,
            "Java clamps the fill start to 99, so Treasure overwrites the final Shop slot"
        );
    }

    #[test]
    fn one_time_event_candidate_gates_match_java_abstract_dungeon() {
        let mut c = ctx();

        c.has_curses = false;
        assert!(!is_one_time_event_candidate(
            EventId::FountainOfCurseCleansing,
            &c
        ));
        c.has_curses = true;
        assert!(is_one_time_event_candidate(
            EventId::FountainOfCurseCleansing,
            &c
        ));

        c.act_num = 1;
        c.gold = 99;
        assert!(!is_one_time_event_candidate(EventId::Designer, &c));
        c.act_num = 2;
        c.gold = 74;
        assert!(!is_one_time_event_candidate(EventId::Designer, &c));
        c.gold = 75;
        assert!(is_one_time_event_candidate(EventId::Designer, &c));

        c.act_num = 1;
        assert!(!is_one_time_event_candidate(EventId::Duplicator, &c));
        c.act_num = 3;
        assert!(is_one_time_event_candidate(EventId::Duplicator, &c));

        c.act_num = 3;
        assert!(!is_one_time_event_candidate(EventId::FaceTrader, &c));
        c.act_num = 1;
        assert!(is_one_time_event_candidate(EventId::FaceTrader, &c));

        c.act_num = 2;
        c.current_hp = 12;
        assert!(!is_one_time_event_candidate(EventId::KnowingSkull, &c));
        c.current_hp = 13;
        assert!(is_one_time_event_candidate(EventId::KnowingSkull, &c));

        c.relic_count = 1;
        assert!(!is_one_time_event_candidate(EventId::Nloth, &c));
        c.relic_count = 2;
        assert!(is_one_time_event_candidate(EventId::Nloth, &c));

        c.gold = 49;
        assert!(!is_one_time_event_candidate(EventId::TheJoust, &c));
        c.gold = 50;
        assert!(is_one_time_event_candidate(EventId::TheJoust, &c));

        c.gold = 49;
        assert!(!is_one_time_event_candidate(EventId::WomanInBlue, &c));
        c.gold = 50;
        assert!(is_one_time_event_candidate(EventId::WomanInBlue, &c));

        c.act_num = 2;
        c.playtime_seconds = 900.0;
        assert!(!is_one_time_event_candidate(EventId::SecretPortal, &c));
        c.act_num = 3;
        c.playtime_seconds = 799.9;
        assert!(!is_one_time_event_candidate(EventId::SecretPortal, &c));
        c.playtime_seconds = 800.0;
        assert!(is_one_time_event_candidate(EventId::SecretPortal, &c));
    }

    #[test]
    fn one_time_event_pool_is_initialized_once_across_act_transitions() {
        let mut generator = EventGenerator::new(1);

        assert!(generator
            .one_time_event_pool
            .contains(&EventId::AccursedBlacksmith));

        generator.one_time_event_pool.clear();
        generator.initialize_event_pools(2);

        assert!(generator.one_time_event_pool.is_empty());
        assert!(generator.event_pool.contains(&EventId::Addict));
        assert!(generator.shrine_pool.contains(&EventId::MatchAndKeep));
    }

    #[test]
    fn note_for_yourself_presence_is_decided_when_one_time_pool_initializes() {
        let with_note = EventGenerator::new_with_note_for_yourself(1, true);
        assert!(with_note
            .one_time_event_pool
            .contains(&EventId::NoteForYourself));

        let without_note = EventGenerator::new_with_note_for_yourself(1, false);
        assert!(!without_note
            .one_time_event_pool
            .contains(&EventId::NoteForYourself));
    }

    #[test]
    fn ordinary_event_pool_does_not_repopulate_when_exhausted() {
        let mut generator = EventGenerator::new(1);
        let c = ctx();
        let mut rng = RngPool::new(7);

        generator.event_pool = vec![EventId::Cleric];
        generator.shrine_pool.clear();
        generator.one_time_event_pool.clear();

        assert_eq!(
            generator.try_get_pool_event(&mut rng, &c),
            Some(EventId::Cleric)
        );
        assert!(generator.event_pool.is_empty());
        assert_eq!(generator.try_get_pool_event(&mut rng, &c), None);
    }

    #[test]
    fn exhausted_event_and_shrine_pools_do_not_fabricate_fallback_event() {
        let mut generator = EventGenerator::new(1);
        let c = ctx();
        let mut rng = RngPool::new(7);

        generator.event_pool.clear();
        generator.shrine_pool.clear();
        generator.one_time_event_pool.clear();

        assert_eq!(
            generator.try_generate_event(&mut rng, &c),
            None,
            "Java generateEvent returns no event when both event and shrine pools are empty; it does not fabricate Cleric/Golden Idol/Golden Shrine"
        );
    }

    #[test]
    fn act_four_event_and_shrine_pools_match_empty_java_the_ending_lists() {
        let generator = EventGenerator::new(4);

        assert!(generator.event_pool.is_empty());
        assert!(generator.shrine_pool.is_empty());
    }

    #[test]
    fn pool_event_candidate_gates_match_java_abstract_dungeon() {
        let mut c = ctx();

        c.floor_num = 6;
        assert!(!is_pool_event_candidate(
            EventId::DeadAdventurer,
            &c,
            1.0,
            7
        ));
        assert!(!is_pool_event_candidate(EventId::Mushrooms, &c, 1.0, 7));
        c.floor_num = 7;
        assert!(is_pool_event_candidate(EventId::DeadAdventurer, &c, 1.0, 7));
        assert!(is_pool_event_candidate(EventId::Mushrooms, &c, 1.0, 7));

        c.has_golden_idol = false;
        assert!(!is_pool_event_candidate(EventId::MoaiHead, &c, 0.51, 7));
        assert!(is_pool_event_candidate(EventId::MoaiHead, &c, 0.5, 7));
        c.has_golden_idol = true;
        assert!(is_pool_event_candidate(EventId::MoaiHead, &c, 1.0, 7));

        c.gold = 34;
        assert!(!is_pool_event_candidate(EventId::Cleric, &c, 1.0, 7));
        c.gold = 35;
        assert!(is_pool_event_candidate(EventId::Cleric, &c, 1.0, 7));

        c.gold = 74;
        assert!(!is_pool_event_candidate(EventId::Beggar, &c, 1.0, 7));
        c.gold = 75;
        assert!(is_pool_event_candidate(EventId::Beggar, &c, 1.0, 7));

        c.floor_num = 99;
        c.map_current_y = Some(7);
        assert!(!is_pool_event_candidate(EventId::Colosseum, &c, 1.0, 7));
        c.map_current_y = None;
        assert!(!is_pool_event_candidate(EventId::Colosseum, &c, 1.0, 7));
        c.map_current_y = Some(8);
        assert!(is_pool_event_candidate(EventId::Colosseum, &c, 1.0, 7));
    }
}
