use super::*;

impl RunState {
    pub(super) fn build_event_context(
        &self,
        tiny_chest_counter: i32,
        previous_room_was_shop: bool,
    ) -> crate::state::events::context::EventContext {
        use crate::content::relics::RelicId;

        let mut has_juzu = false;
        let mut has_golden_idol = false;

        for relic in &self.relics {
            match relic.id {
                RelicId::JuzuBracelet => has_juzu = true,
                RelicId::GoldenIdol => has_golden_idol = true,
                _ => {}
            }
        }

        // Java `AbstractPlayer.isCursed()` is the availability gate for the
        // Fountain of Cleansing. It excludes the three unpurgeable curses,
        // so Ascender's Bane alone must not change the event candidate count.
        let has_curses = self.master_deck.iter().any(|card| {
            crate::content::cards::get_card_definition(card.id).card_type
                == crate::content::cards::CardType::Curse
                && crate::state::core::master_deck_card_is_purgeable(card)
        });

        crate::state::events::context::EventContext {
            act_num: self.act_num,
            ascension_level: self.ascension_level,
            is_daily_run: self.is_daily_run,
            highest_unlocked_ascension_level: self.highest_unlocked_ascension_level,
            floor_num: self.floor_num,
            map_current_y: (self.map.current_y >= 0).then_some(self.map.current_y),
            map_height: self.map.graph.len(),
            gold: self.gold,
            current_hp: self.current_hp,
            max_hp: self.max_hp,
            playtime_seconds: self.playtime_seconds,
            has_curses,
            has_golden_idol,
            tiny_chest_counter,
            has_juzu_bracelet: has_juzu,
            relic_count: self.relics.len(),
            previous_room_was_shop,
        }
    }

    /// Mirrors Java EventHelper.roll() for `?` map nodes. This mutates Tiny
    /// Chest before room-type chances are interpreted, but EventHelper itself
    /// still consumes eventRng before checking the forced-chest flag.
    pub fn roll_question_mark_room_type(
        &mut self,
        previous_room_type: Option<crate::state::map::node::RoomType>,
    ) -> crate::state::events::generator::RoomRoll {
        use crate::content::relics::RelicId;

        let mut tiny_chest_counter = 0;
        for relic in &mut self.relics {
            if relic.id == RelicId::TinyChest {
                relic.counter += 1;
                if relic.counter == 4 {
                    relic.counter = 0;
                    // Preserve Java's post-increment trigger without colliding
                    // with the ordinary third-room counter value.
                    tiny_chest_counter = 4;
                } else {
                    tiny_chest_counter = relic.counter;
                }
            }
        }

        let ctx = self.build_event_context(
            tiny_chest_counter,
            previous_room_type == Some(crate::state::map::node::RoomType::ShopRoom),
        );

        // AbstractDungeon.nextRoomTransition creates a seed+counter duplicate,
        // rolls the `?` room type through it, then promotes that duplicate to
        // the persistent eventRng. Replaying the constructor matters because
        // Java advances it with random(999), rather than cloning XS128 bits.
        let mut rng_duplicate = self.rng_pool.clone();
        rng_duplicate.event_rng = self
            .rng_pool
            .event_rng
            .duplicate_from_seed_and_counter(self.seed);
        let room_roll = self
            .event_generator
            .roll_room_type(&mut rng_duplicate, &ctx);
        self.rng_pool.event_rng = rng_duplicate.event_rng;
        room_roll
    }

    /// Publicly reachable `?` outcomes for the next room entry. This mirrors
    /// the prospective Tiny Chest increment but does not mutate relic state,
    /// adaptive chances, or event RNG.
    pub fn possible_question_mark_room_types(
        &self,
        previous_room_type: Option<crate::state::map::node::RoomType>,
    ) -> Vec<crate::state::events::generator::RoomRoll> {
        use crate::content::relics::RelicId;

        let tiny_chest_counter = self
            .relics
            .iter()
            .find(|relic| relic.id == RelicId::TinyChest)
            .map(|relic| {
                let next = relic.counter + 1;
                if next == 4 {
                    4
                } else {
                    next
                }
            })
            .unwrap_or_default();
        let ctx = self.build_event_context(
            tiny_chest_counter,
            previous_room_type == Some(crate::state::map::node::RoomType::ShopRoom),
        );
        self.event_generator.possible_room_rolls(&ctx)
    }

    /// Mirrors `EventRoom.onPlayerEntry()`.
    ///
    /// Java creates `new Random(Settings.seed, AbstractDungeon.eventRng.counter)`
    /// and passes that duplicate into `AbstractDungeon.generateEvent(...)`.
    /// The event/shrine pools mutate, but the global `eventRng` counter is not
    /// advanced by the specific event selection after the `?` room-type roll.
    pub fn generate_event_from_event_room_duplicate(&mut self) -> crate::state::events::EventId {
        let tiny_chest_counter = self
            .relics
            .iter()
            .find(|relic| relic.id == crate::content::relics::RelicId::TinyChest)
            .map(|relic| relic.counter)
            .unwrap_or(0);
        let ctx = self.build_event_context(tiny_chest_counter, false);
        let mut rng_duplicate = self.rng_pool.clone();
        rng_duplicate.event_rng = self
            .rng_pool
            .event_rng
            .duplicate_from_seed_and_counter(self.seed);

        self.event_generator
            .generate_event(&mut rng_duplicate, &ctx)
    }

    /// Initialize encounter scheduling lists for the current act.
    /// Java: generateMonsters() in Exordium/TheCity/TheBeyond
    pub fn init_encounter_lists(&mut self) {
        let (ml, el) = crate::content::monsters::encounter_pool::generate_encounter_lists(
            self.act_num,
            &mut self.rng_pool.monster_rng,
        );
        self.monster_list = ml;
        self.elite_monster_list = el;
    }

    /// Read the next normal encounter from the pre-scheduled list without
    /// consuming it.
    ///
    /// Java `MonsterRoom.onPlayerEntry()` calls
    /// `getMonsterForRoomCreation()`, which reads `monsterList.get(0)` and
    /// constructs the encounter. The list entry is removed later by
    /// `AbstractDungeon.nextRoomTransition()` when leaving the current
    /// `MonsterRoom`.
    pub fn peek_next_encounter(&self) -> Option<crate::content::monsters::factory::EncounterId> {
        self.monster_list.first().copied()
    }

    /// Read the next elite encounter without consuming it.
    ///
    /// Java mirrors normal encounter timing for elites:
    /// `MonsterRoomElite.onPlayerEntry()` reads the front entry, and
    /// `nextRoomTransition()` removes it when leaving the elite room.
    pub fn peek_next_elite(&self) -> Option<crate::content::monsters::factory::EncounterId> {
        self.elite_monster_list.first().copied()
    }

    /// Pop the next normal encounter from the pre-scheduled list.
    /// Java: `monsterList.remove(0)` in `nextRoomTransition()` when leaving a
    /// `MonsterRoom`.
    pub fn next_encounter(&mut self) -> Option<crate::content::monsters::factory::EncounterId> {
        if self.monster_list.is_empty() {
            None
        } else {
            Some(self.monster_list.remove(0))
        }
    }

    /// Pop the next elite encounter from the pre-scheduled list.
    /// Java: `eliteMonsterList.remove(0)` in `nextRoomTransition()` when
    /// leaving a `MonsterRoomElite`.
    pub fn next_elite(&mut self) -> Option<crate::content::monsters::factory::EncounterId> {
        if self.elite_monster_list.is_empty() {
            None
        } else {
            Some(self.elite_monster_list.remove(0))
        }
    }

    /// Consume the encounter queue for the room being left.
    ///
    /// This is intentionally not part of combat creation. Java keeps the queue
    /// front visible to the room for the whole combat/reward lifecycle and
    /// removes it only during the next room transition.
    pub fn complete_current_room_encounter(
        &mut self,
        room_type: Option<crate::state::map::node::RoomType>,
    ) -> Option<crate::content::monsters::factory::EncounterId> {
        match room_type {
            Some(
                crate::state::map::node::RoomType::MonsterRoom
                | crate::state::map::node::RoomType::MonsterRoomBoss,
            ) => self.next_encounter(),
            Some(crate::state::map::node::RoomType::MonsterRoomElite) => self.next_elite(),
            _ => None,
        }
    }

    /// Initialize the boss list for the current act and publish the visible
    /// boss key.
    ///
    /// Java: dungeon constructors call `initializeBoss()`, then
    /// `setBoss(bossList.get(0))`. The list itself is not truncated; on A20
    /// Act 3, the first boss removes index 0, then `ProceedButton` sees two
    /// remaining entries and sets `bossKey = bossList.get(0)` for the second
    /// boss.
    pub fn init_boss_list(&mut self) {
        let settings = crate::content::monsters::encounter_pool::BossGenerationSettings {
            is_daily_run: self.is_daily_run,
            is_demo: false,
            seen: crate::content::monsters::encounter_pool::BossSeenState::all_seen(),
        };
        self.boss_list = crate::content::monsters::encounter_pool::generate_boss_list_with_settings(
            self.act_num,
            &mut self.rng_pool.monster_rng,
            settings,
        );
        self.boss_key = self.boss_list.first().copied();
    }

    /// Pop the next boss from the pre-scheduled list.
    /// Java: `getBoss()` uses `bossKey`, then
    /// `MonsterRoomBoss.onPlayerEntry()` removes `bossList[0]`.
    pub fn next_boss(&mut self) -> Option<crate::content::monsters::factory::EncounterId> {
        let boss = self.boss_key.or_else(|| self.boss_list.first().copied());
        if !self.boss_list.is_empty() {
            self.boss_list.remove(0);
        }
        boss
    }

    pub fn reveal_next_boss_from_list(
        &mut self,
    ) -> Option<crate::content::monsters::factory::EncounterId> {
        self.boss_key = self.boss_list.first().copied();
        self.boss_key
    }
}
