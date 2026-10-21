use super::*;

impl RunState {
    /// Room-independent entry effects, including synthetic boss/victory rooms.
    /// Type-specific entry effects remain with their room owners.
    pub fn on_enter_room(&mut self) {
        if self
            .relics
            .iter()
            .any(|r| r.id == RelicId::MawBank && !r.used_up)
        {
            self.change_gold_with_source(12, DomainEventSource::Relic(RelicId::MawBank));
        }
    }

    /// Advance Java's global floor number and rebuild every floor-local RNG.
    ///
    /// This applies to ordinary room entry, a boss chest, and the second Act 3
    /// boss at A20. Persistent dungeon RNG streams keep their counters.
    pub fn advance_floor_and_generate_seeds(&mut self) {
        self.floor_num += 1;
        self.rng_pool
            .generate_floor_seeds(self.seed, self.floor_num);
    }

    pub fn should_start_act3_double_boss(&self) -> bool {
        self.act_num == 3 && self.ascension_level >= 20 && self.boss_list.len() == 2
    }

    pub fn enter_final_act(&mut self) {
        self.act_num = 4;
        self.pending_boss_reward = false;
        self.pending_boss_act_transition = false;
        self.apply_dungeon_transition_setup_effects();
        self.map = crate::state::map::state::MapState::new(
            crate::state::map::generator::generate_ending_map(),
        );
        // Java TheEnding initializes mapRng from seed + actNum * 300. Its
        // fixed special map does not consume the stream.
        self.map_rng = crate::runtime::rng::StsRng::new_named(
            self.seed.wrapping_add(u64::from(self.act_num) * 300),
            "run.map",
        );
        self.map.has_emerald_key = self.keys[2];
        self.init_encounter_lists();
        self.init_boss_list();
        self.event_generator.initialize_event_pools(self.act_num);
        self.event_generator.reset_probabilities();
        self.card_upgraded_chance = if self.ascension_level >= 12 {
            0.25
        } else {
            0.5
        };
    }

    /// Advance to the next act after boss defeat.
    /// Regenerates map, encounter lists, event pools, and boss list for the new act.
    /// Java flow: VictoryRoom → Proceed → next Dungeon constructor.
    pub fn advance_act(&mut self) {
        self.act_num += 1;
        self.pending_boss_reward = false;
        self.pending_boss_act_transition = false;

        self.apply_dungeon_transition_setup_effects();

        // AbstractDungeon's constructor generates encounters and the boss
        // before the concrete dungeon constructor generates its map. Preserve
        // this call order as well as each independent stream's final state.
        self.init_encounter_lists();
        self.init_boss_list();

        // Generate new map for the next act
        // Generate map for the new act; returns consumed mapRng for emerald placement.
        let (mut new_map, mut map_rng) = crate::state::map::generator::generate_map_for_act(
            self.seed,
            self.act_num,
            self.ascension_level == 0,
        );

        // Mark emerald elite on new map if Act 4 is enabled and key not yet obtained.
        // Java: setEmeraldElite() reuses the consumed mapRng, not a fresh one.
        if self.is_final_act_available && !self.keys[2] {
            crate::state::map::generator::set_emerald_elite(&mut new_map, &mut map_rng);
        }

        self.map = crate::state::map::state::MapState::new(new_map);
        self.map_rng = map_rng;
        self.map.has_emerald_key = self.keys[2];
        // MainMusic.getSong chooses one of two tracks using gameplay miscRng,
        // even with muted audio. TheEnding has only one track and makes no draw.
        if matches!(self.act_num, 1..=3) {
            self.rng_pool.misc_rng.random(1);
        }

        // Reinitialize event pools for the new act
        self.event_generator.initialize_event_pools(self.act_num);

        // Reset event room-type probabilities (Java: AbstractDungeon.resetProbabilities)
        self.event_generator.reset_probabilities();

        // Update card_upgraded_chance per act (Java: initializeLevelSpecificChances)
        // Asc 12: halves the upgrade chance per act
        self.card_upgraded_chance = match self.act_num {
            1 => 0.0, // Exordium: always 0.0
            2 => {
                if self.ascension_level >= 12 {
                    0.125
                } else {
                    0.25
                }
            } // TheCity
            3 => {
                if self.ascension_level >= 12 {
                    0.25
                } else {
                    0.5
                }
            } // TheBeyond
            _ => {
                if self.ascension_level >= 12 {
                    0.25
                } else {
                    0.5
                }
            } // TheEnding
        };
    }

    pub fn complete_pending_boss_act_transition(&mut self) -> bool {
        if self.pending_boss_act_transition {
            self.advance_act();
            true
        } else {
            false
        }
    }

    pub(super) fn apply_dungeon_transition_setup_effects(&mut self) {
        self.align_card_rng_counter_for_dungeon_transition();
        self.potion_drop_chance_mod = 0;
        self.heal_for_dungeon_transition();
    }

    /// The Act-entry HP calculation, shared by execution and public inspection.
    pub fn dungeon_transition_hp(current_hp: i32, max_hp: i32, ascension_level: u8) -> i32 {
        let missing = max_hp - current_hp;
        let heal_amount = if ascension_level >= 5 {
            (missing as f32 * 0.75).round() as i32
        } else {
            max_hp
        };
        (current_hp + heal_amount).min(max_hp)
    }

    pub(super) fn heal_for_dungeon_transition(&mut self) {
        self.current_hp =
            Self::dungeon_transition_hp(self.current_hp, self.max_hp, self.ascension_level);
    }

    pub(super) fn align_card_rng_counter_for_dungeon_transition(&mut self) {
        let target = match self.rng_pool.card_rng.counter {
            1..=249 => Some(250),
            251..=499 => Some(500),
            501..=749 => Some(750),
            _ => None,
        };
        if let Some(target) = target {
            self.rng_pool.card_rng.advance_counter_to(target);
        }
    }
}
