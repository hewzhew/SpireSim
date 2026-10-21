use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo};
use crate::runtime::combat::{CombatState, MonsterEntity, Power};
use crate::semantics::combat::MonsterTurnPlan;
use serde::{Deserialize, Serialize};

pub mod beyond;
pub mod city;
pub mod ending;
pub mod exordium;

pub mod encounter_pool;
/// The trait that all Monster implementations must satisfy.
pub mod factory;
pub mod mechanics;

/// Slay the Spire handles state dynamically through CombatState/MonsterEntity,
/// so implementations are stateless trait objects or static dispatch endpoints.
pub trait MonsterBehavior {
    fn roll_move_plan(
        _rng: &mut crate::runtime::rng::StsRng,
        _entity: &MonsterEntity,
        _ascension_level: u8,
        _num: i32,
    ) -> MonsterTurnPlan {
        panic!("semantic roll_move_plan required")
    }

    fn roll_move_plan_with_context(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        _ctx: MonsterRollContext<'_>,
    ) -> MonsterTurnPlan {
        Self::roll_move_plan(rng, entity, ascension_level, num)
    }

    /// Optional actions emitted while resolving a new move.
    ///
    /// Use this for monsters whose hidden runtime truth mutates inside Java
    /// `getMove()` rather than `takeTurn()`.
    fn on_roll_move(
        _ascension_level: u8,
        _entity: &MonsterEntity,
        _num: i32,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        Vec::new()
    }

    fn roll_move_outcome(
        rng: &mut crate::runtime::rng::StsRng,
        entity: &MonsterEntity,
        ascension_level: u8,
        num: i32,
        ctx: MonsterRollContext<'_>,
    ) -> MonsterRollOutcome {
        let plan = Self::roll_move_plan_with_context(rng, entity, ascension_level, num, ctx);
        let setup_actions = Self::on_roll_move(ascension_level, entity, num, &plan);
        MonsterRollOutcome {
            plan,
            setup_actions,
        }
    }

    fn take_turn_plan(
        _state: &mut CombatState,
        _entity: &MonsterEntity,
        _plan: &MonsterTurnPlan,
    ) -> Vec<Action> {
        panic!("semantic take_turn_plan required")
    }

    /// Semantic turn-plan entry point. The default remains observation-backed
    /// during migration, but monsters can override this to define execution
    /// truth from runtime state and combat rules instead of visible intent.
    fn turn_plan(_state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
        entity.turn_plan()
    }

    /// Optional sequence of actions to be executed immediately upon spawning (equivalent to usePreBattleAction in Java).
    ///
    /// New semantic entry point: monsters can inspect full combat state but must
    /// still return actions rather than mutating state directly.
    fn use_pre_battle_actions(
        _state: &mut CombatState,
        _entity: &MonsterEntity,
        _legacy_rng: PreBattleLegacyRng,
    ) -> Vec<Action> {
        Vec::new()
    }

    /// Invoked whenever the monster loses HP (from attacks, poison, thorns, etc).
    fn on_damaged(
        _state: &mut CombatState,
        _entity: &MonsterEntity,
        _amount: i32,
    ) -> smallvec::SmallVec<[ActionInfo; 4]> {
        smallvec::SmallVec::new()
    }

    /// Sequence of actions to run when the entity dies.
    fn on_death(_state: &mut CombatState, _entity: &MonsterEntity) -> Vec<Action> {
        Vec::new()
    }
}

crate::stable_gameplay_id! {
/// Identifiers for mapping specific AI routines.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EnemyId {
    JawWorm => ("JawWorm", 0),
    Cultist => ("Cultist", 1),
    LouseNormal => ("LouseNormal", 2),
    LouseDefensive => ("LouseDefensive", 3),
    SpikeSlimeS => ("SpikeSlimeS", 4),
    SpikeSlimeM => ("SpikeSlimeM", 5),
    AcidSlimeS => ("AcidSlimeS", 6),
    AcidSlimeM => ("AcidSlimeM", 7),
    FungiBeast => ("FungiBeast", 8),
    Looter => ("Looter", 9),
    SlaverBlue => ("SlaverBlue", 10),
    SlaverRed => ("SlaverRed", 11),
    AcidSlimeL => ("AcidSlimeL", 12),
    SpikeSlimeL => ("SpikeSlimeL", 13),
    GremlinFat => ("GremlinFat", 14),
    GremlinThief => ("GremlinThief", 15),
    GremlinTsundere => ("GremlinTsundere", 16),
    GremlinWarrior => ("GremlinWarrior", 17),
    GremlinWizard => ("GremlinWizard", 18),
    GremlinNob => ("GremlinNob", 19),
    Lagavulin => ("Lagavulin", 20),
    Sentry => ("Sentry", 21),
    SlimeBoss => ("SlimeBoss", 22),
    Hexaghost => ("Hexaghost", 23),
    TheGuardian => ("TheGuardian", 24),
    SphericGuardian => ("SphericGuardian", 25),
    Byrd => ("Byrd", 26),
    Chosen => ("Chosen", 27),
    Snecko => ("Snecko", 28),
    Centurion => ("Centurion", 29),
    Healer => ("Healer", 30),
    SnakePlant => ("SnakePlant", 31),
    ShelledParasite => ("ShelledParasite", 32),
    Mugger => ("Mugger", 33),
    BookOfStabbing => ("BookOfStabbing", 34),
    Taskmaster => ("Taskmaster", 35),
    GremlinLeader => ("GremlinLeader", 36),
    BronzeAutomaton => ("BronzeAutomaton", 37),
    BronzeOrb => ("BronzeOrb", 38),
    TheCollector => ("TheCollector", 39),
    TorchHead => ("TorchHead", 40),
    Champ => ("Champ", 41),
    BanditBear => ("BanditBear", 42),
    BanditLeader => ("BanditLeader", 43),
    BanditPointy => ("BanditPointy", 44),
    Exploder => ("Exploder", 45),
    Repulsor => ("Repulsor", 46),
    Spiker => ("Spiker", 47),
    OrbWalker => ("OrbWalker", 48),
    Darkling => ("Darkling", 49),
    Maw => ("Maw", 50),
    SpireGrowth => ("SpireGrowth", 51),
    Transient => ("Transient", 52),
    WrithingMass => ("WrithingMass", 53),
    GiantHead => ("GiantHead", 54),
    Nemesis => ("Nemesis", 55),
    Reptomancer => ("Reptomancer", 56),
    SnakeDagger => ("SnakeDagger", 57),
    AwakenedOne => ("AwakenedOne", 58),
    TimeEater => ("TimeEater", 59),
    Donu => ("Donu", 60),
    Deca => ("Deca", 61),
    SpireShield => ("SpireShield", 62),
    SpireSpear => ("SpireSpear", 63),
    CorruptHeart => ("CorruptHeart", 64),
}
}

impl EnemyId {
    pub fn is_boss(self) -> bool {
        matches!(
            self,
            EnemyId::SlimeBoss
                | EnemyId::Hexaghost
                | EnemyId::TheGuardian
                | EnemyId::BronzeAutomaton
                | EnemyId::TheCollector
                | EnemyId::Champ
                | EnemyId::AwakenedOne
                | EnemyId::TimeEater
                | EnemyId::Donu
                | EnemyId::Deca
                | EnemyId::CorruptHeart
        )
    }

    pub fn get_name(&self) -> &'static str {
        match self {
            EnemyId::JawWorm => "Jaw Worm",
            EnemyId::Cultist => "Cultist",
            EnemyId::LouseNormal => "Red Louse",
            EnemyId::LouseDefensive => "Green Louse",
            EnemyId::SpikeSlimeS => "Spike Slime (S)",
            EnemyId::SpikeSlimeM => "Spike Slime (M)",
            EnemyId::AcidSlimeS => "Acid Slime (S)",
            EnemyId::AcidSlimeM => "Acid Slime (M)",
            EnemyId::FungiBeast => "Fungi Beast",
            EnemyId::Looter => "Looter",
            EnemyId::SlaverBlue => "Blue Slaver",
            EnemyId::SlaverRed => "Red Slaver",
            EnemyId::AcidSlimeL => "Acid Slime (L)",
            EnemyId::SpikeSlimeL => "Spike Slime (L)",
            EnemyId::GremlinFat => "Fat Gremlin",
            EnemyId::GremlinThief => "Sneaky Gremlin",
            EnemyId::GremlinTsundere => "Shield Gremlin",
            EnemyId::GremlinWarrior => "Mad Gremlin",
            EnemyId::GremlinWizard => "Gremlin Wizard",
            EnemyId::GremlinNob => "Gremlin Nob",
            EnemyId::Lagavulin => "Lagavulin",
            EnemyId::Sentry => "Sentry",
            EnemyId::SlimeBoss => "Slime Boss",
            EnemyId::Hexaghost => "Hexaghost",
            EnemyId::TheGuardian => "The Guardian",
            EnemyId::SphericGuardian => "Spheric Guardian",
            EnemyId::Byrd => "Byrd",
            EnemyId::Chosen => "Chosen",
            EnemyId::Snecko => "Snecko",
            EnemyId::Centurion => "Centurion",
            EnemyId::Healer => "Mystic",
            EnemyId::SnakePlant => "Snake Plant",
            EnemyId::ShelledParasite => "Shelled Parasite",
            EnemyId::Mugger => "Mugger",
            EnemyId::BookOfStabbing => "Book of Stabbing",
            EnemyId::Taskmaster => "Taskmaster",
            EnemyId::GremlinLeader => "Gremlin Leader",
            EnemyId::BronzeAutomaton => "Bronze Automaton",
            EnemyId::BronzeOrb => "Bronze Orb",
            EnemyId::TheCollector => "The Collector",
            EnemyId::TorchHead => "Torch Head",
            EnemyId::Champ => "The Champ",
            EnemyId::BanditBear => "Bear",
            EnemyId::BanditLeader => "Romeo",
            EnemyId::BanditPointy => "Pointy",
            EnemyId::Exploder => "Exploder",
            EnemyId::Repulsor => "Repulsor",
            EnemyId::Spiker => "Spiker",
            EnemyId::OrbWalker => "Orb Walker",
            EnemyId::Darkling => "Darkling",
            EnemyId::Maw => "The Maw",
            EnemyId::SpireGrowth => "Spire Growth",
            EnemyId::Transient => "Transient",
            EnemyId::WrithingMass => "Writhing Mass",
            EnemyId::GiantHead => "Giant Head",
            EnemyId::Nemesis => "Nemesis",
            EnemyId::Reptomancer => "Reptomancer",
            EnemyId::SnakeDagger => "Dagger",
            EnemyId::AwakenedOne => "Awakened One",
            EnemyId::TimeEater => "Time Eater",
            EnemyId::Donu => "Donu",
            EnemyId::Deca => "Deca",
            EnemyId::SpireShield => "Spire Shield",
            EnemyId::SpireSpear => "Spire Spear",
            EnemyId::CorruptHeart => "Corrupt Heart",
        }
    }
}

impl crate::content::identity::GameplayIdentitySurface for EnemyId {
    fn display_name_v1(self) -> Option<&'static str> {
        Some(self.get_name())
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Eq, Serialize)]
pub enum PreBattleLegacyRng {
    Misc,
    MonsterHp,
}

pub fn legacy_pre_battle_rng(
    state: &mut CombatState,
    legacy_rng: PreBattleLegacyRng,
) -> (&mut crate::runtime::rng::StsRng, u8) {
    let ascension_level = state.meta.ascension_level;
    let rng = match legacy_rng {
        PreBattleLegacyRng::Misc => &mut state.rng.misc_rng,
        PreBattleLegacyRng::MonsterHp => &mut state.rng.monster_hp_rng,
    };
    (rng, ascension_level)
}

#[derive(Clone, Copy)]
pub struct MonsterRollContext<'a> {
    pub monsters: &'a [MonsterEntity],
    pub player_powers: &'a [Power],
}

impl MonsterRollContext<'_> {
    pub fn player_has_power(self, power_id: PowerId) -> bool {
        self.player_powers
            .iter()
            .any(|power| power.power_type == power_id)
    }
}

pub struct MonsterRollOutcome {
    pub plan: MonsterTurnPlan,
    pub setup_actions: Vec<Action>,
}

fn enemy_type_for_monster(entity: &MonsterEntity, _context: &str) -> EnemyId {
    entity.enemy_id
}

macro_rules! dispatch_monster_behavior_method {
    ($enemy_type:expr, $method:ident ( $($args:expr),* $(,)? )) => {
        match $enemy_type {
            EnemyId::Cultist => exordium::cultist::Cultist::$method($($args),*),
            EnemyId::Lagavulin => exordium::lagavulin::Lagavulin::$method($($args),*),
            EnemyId::Hexaghost => exordium::hexaghost::Hexaghost::$method($($args),*),
            EnemyId::SpikeSlimeM => exordium::spike_slime::SpikeSlimeM::$method($($args),*),
            EnemyId::AcidSlimeM => exordium::acid_slime::AcidSlimeM::$method($($args),*),
            EnemyId::SpikeSlimeL => exordium::spike_slime::SpikeSlimeL::$method($($args),*),
            EnemyId::AcidSlimeL => exordium::acid_slime::AcidSlimeL::$method($($args),*),
            EnemyId::SlimeBoss => exordium::slime_boss::SlimeBoss::$method($($args),*),
            EnemyId::TheGuardian => exordium::the_guardian::TheGuardian::$method($($args),*),
            EnemyId::Sentry => exordium::sentry::Sentry::$method($($args),*),
            EnemyId::JawWorm => exordium::jaw_worm::JawWorm::$method($($args),*),
            EnemyId::LouseNormal => exordium::louse_normal::LouseNormal::$method($($args),*),
            EnemyId::LouseDefensive => {
                exordium::louse_defensive::LouseDefensive::$method($($args),*)
            }
            EnemyId::SpikeSlimeS => exordium::spike_slime::SpikeSlimeS::$method($($args),*),
            EnemyId::AcidSlimeS => exordium::acid_slime::AcidSlimeS::$method($($args),*),
            EnemyId::FungiBeast => exordium::fungi_beast::FungiBeast::$method($($args),*),
            EnemyId::Looter => exordium::looter::Looter::$method($($args),*),
            EnemyId::SlaverBlue => exordium::slaver_blue::SlaverBlue::$method($($args),*),
            EnemyId::SlaverRed => exordium::slaver_red::SlaverRed::$method($($args),*),
            EnemyId::GremlinFat => exordium::gremlin_fat::GremlinFat::$method($($args),*),
            EnemyId::GremlinThief => exordium::gremlin_thief::GremlinThief::$method($($args),*),
            EnemyId::GremlinTsundere => {
                exordium::gremlin_tsundere::GremlinTsundere::$method($($args),*)
            }
            EnemyId::GremlinWarrior => {
                exordium::gremlin_warrior::GremlinWarrior::$method($($args),*)
            }
            EnemyId::GremlinWizard => {
                exordium::gremlin_wizard::GremlinWizard::$method($($args),*)
            }
            EnemyId::GremlinNob => exordium::gremlin_nob::GremlinNob::$method($($args),*),
            EnemyId::SphericGuardian => {
                city::spheric_guardian::SphericGuardian::$method($($args),*)
            }
            EnemyId::Byrd => city::byrd::Byrd::$method($($args),*),
            EnemyId::Chosen => city::chosen::Chosen::$method($($args),*),
            EnemyId::Snecko => city::snecko::Snecko::$method($($args),*),
            EnemyId::Mugger => city::mugger::Mugger::$method($($args),*),
            EnemyId::ShelledParasite => {
                city::shelled_parasite::ShelledParasite::$method($($args),*)
            }
            EnemyId::Centurion => city::centurion::Centurion::$method($($args),*),
            EnemyId::Healer => city::healer::Healer::$method($($args),*),
            EnemyId::SnakePlant => city::snake_plant::SnakePlant::$method($($args),*),
            EnemyId::GremlinLeader => city::gremlin_leader::GremlinLeader::$method($($args),*),
            EnemyId::TheCollector => city::the_collector::TheCollector::$method($($args),*),
            EnemyId::TorchHead => city::torch_head::TorchHead::$method($($args),*),
            EnemyId::Taskmaster => city::taskmaster::Taskmaster::$method($($args),*),
            EnemyId::BookOfStabbing => city::book_of_stabbing::BookOfStabbing::$method($($args),*),
            EnemyId::BronzeAutomaton => {
                city::bronze_automaton::BronzeAutomaton::$method($($args),*)
            }
            EnemyId::BronzeOrb => city::bronze_orb::BronzeOrb::$method($($args),*),
            EnemyId::Champ => city::champ::Champ::$method($($args),*),
            EnemyId::BanditBear => city::bandit_bear::BanditBear::$method($($args),*),
            EnemyId::BanditLeader => city::bandit_leader::BanditLeader::$method($($args),*),
            EnemyId::BanditPointy => city::bandit_pointy::BanditPointy::$method($($args),*),
            EnemyId::Maw => beyond::maw::Maw::$method($($args),*),
            EnemyId::SpireGrowth => beyond::spire_growth::SpireGrowth::$method($($args),*),
            EnemyId::Transient => beyond::transient::Transient::$method($($args),*),
            EnemyId::TimeEater => beyond::time_eater::TimeEater::$method($($args),*),
            EnemyId::AwakenedOne => beyond::awakened_one::AwakenedOne::$method($($args),*),
            EnemyId::Donu => beyond::donu::Donu::$method($($args),*),
            EnemyId::Deca => beyond::deca::Deca::$method($($args),*),
            EnemyId::SpireShield => ending::spire_shield::SpireShield::$method($($args),*),
            EnemyId::SpireSpear => ending::spire_spear::SpireSpear::$method($($args),*),
            EnemyId::CorruptHeart => ending::corrupt_heart::CorruptHeart::$method($($args),*),
            EnemyId::Exploder => beyond::exploder::Exploder::$method($($args),*),
            EnemyId::Repulsor => beyond::repulsor::Repulsor::$method($($args),*),
            EnemyId::Spiker => beyond::spiker::Spiker::$method($($args),*),
            EnemyId::OrbWalker => beyond::orb_walker::OrbWalker::$method($($args),*),
            EnemyId::SnakeDagger => beyond::snake_dagger::SnakeDagger::$method($($args),*),
            EnemyId::Darkling => beyond::darkling::Darkling::$method($($args),*),
            EnemyId::WrithingMass => beyond::writhing_mass::WrithingMass::$method($($args),*),
            EnemyId::GiantHead => beyond::giant_head::GiantHead::$method($($args),*),
            EnemyId::Nemesis => beyond::nemesis::Nemesis::$method($($args),*),
            EnemyId::Reptomancer => beyond::reptomancer::Reptomancer::$method($($args),*),
        }
    };
}

pub fn roll_monster_turn_plan(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    ascension_level: u8,
    num: i32,
    monsters: &[MonsterEntity],
    player_powers: &[Power],
) -> MonsterTurnPlan {
    let ctx = MonsterRollContext {
        monsters,
        player_powers,
    };
    let enemy_type = enemy_type_for_monster(entity, "roll_monster_turn_plan");
    dispatch_monster_behavior_method!(
        enemy_type,
        roll_move_plan_with_context(rng, entity, ascension_level, num, ctx)
    )
}

pub fn roll_monster_turn_outcome(
    rng: &mut crate::runtime::rng::StsRng,
    entity: &MonsterEntity,
    ascension_level: u8,
    num: i32,
    monsters: &[MonsterEntity],
    player_powers: &[Power],
) -> MonsterRollOutcome {
    let ctx = MonsterRollContext {
        monsters,
        player_powers,
    };
    let enemy_type = enemy_type_for_monster(entity, "roll_monster_turn_outcome");
    dispatch_monster_behavior_method!(
        enemy_type,
        roll_move_outcome(rng, entity, ascension_level, num, ctx)
    )
}

pub fn resolve_roll_move_actions(
    state: &CombatState,
    entity: &MonsterEntity,
    num: i32,
    plan: &MonsterTurnPlan,
) -> Vec<Action> {
    let enemy_type = enemy_type_for_monster(entity, "resolve_roll_move_actions");
    dispatch_monster_behavior_method!(
        enemy_type,
        on_roll_move(state.meta.ascension_level, entity, num, plan)
    )
}

pub fn resolve_monster_turn(state: &mut CombatState, entity: &MonsterEntity) -> Vec<Action> {
    let plan = resolve_monster_turn_plan(state, entity);
    let enemy_type = enemy_type_for_monster(entity, "resolve_monster_turn");
    dispatch_monster_behavior_method!(enemy_type, take_turn_plan(state, entity, &plan))
}

pub fn resolve_monster_turn_plan(state: &CombatState, entity: &MonsterEntity) -> MonsterTurnPlan {
    let enemy_type = enemy_type_for_monster(entity, "resolve_monster_turn_plan");
    dispatch_monster_behavior_method!(enemy_type, turn_plan(state, entity))
}

pub fn resolve_pre_battle_actions(
    state: &mut CombatState,
    id: EnemyId,
    entity: &MonsterEntity,
    legacy_rng: PreBattleLegacyRng,
) -> Vec<Action> {
    dispatch_monster_behavior_method!(id, use_pre_battle_actions(state, entity, legacy_rng))
}

pub fn dispatch_on_damaged(
    id: EnemyId,
    state: &mut CombatState,
    entity: &MonsterEntity,
    amount: i32,
) -> smallvec::SmallVec<[ActionInfo; 4]> {
    dispatch_monster_behavior_method!(id, on_damaged(state, entity, amount))
}

pub fn resolve_on_death(
    id: EnemyId,
    state: &mut CombatState,
    entity: &MonsterEntity,
) -> Vec<Action> {
    dispatch_monster_behavior_method!(id, on_death(state, entity))
}

pub fn get_hp_range(id: EnemyId, ascension_level: u8) -> (i32, i32) {
    let asc_hp = ascension_level >= 7; // Normal monsters
    let asc_elite_hp = ascension_level >= 8; // Elites
    let asc_boss_hp = ascension_level >= 9; // Bosses

    match id {
        EnemyId::JawWorm => {
            if asc_hp {
                (42, 46)
            } else {
                (40, 44)
            }
        }
        EnemyId::Cultist => {
            if asc_hp {
                (50, 56)
            } else {
                (48, 54)
            }
        }
        EnemyId::LouseNormal => {
            if asc_hp {
                (11, 16)
            } else {
                (10, 15)
            }
        }
        EnemyId::LouseDefensive => {
            if asc_hp {
                (12, 18)
            } else {
                (11, 17)
            }
        }
        EnemyId::SpikeSlimeS => {
            if asc_hp {
                (11, 15)
            } else {
                (10, 14)
            }
        }
        EnemyId::SpikeSlimeM => {
            if asc_hp {
                (29, 34)
            } else {
                (28, 32)
            }
        }
        EnemyId::AcidSlimeS => {
            if asc_hp {
                (9, 13)
            } else {
                (8, 12)
            }
        }
        EnemyId::AcidSlimeM => {
            if asc_hp {
                (29, 34)
            } else {
                (28, 32)
            }
        }
        EnemyId::FungiBeast => {
            if asc_hp {
                (24, 28)
            } else {
                (22, 28)
            }
        }
        EnemyId::Looter => {
            if asc_hp {
                (46, 50)
            } else {
                (44, 48)
            }
        }
        EnemyId::SlaverBlue => {
            if asc_hp {
                (48, 52)
            } else {
                (46, 50)
            }
        }
        EnemyId::SlaverRed => {
            if asc_hp {
                (48, 52)
            } else {
                (46, 50)
            }
        }
        EnemyId::AcidSlimeL => {
            if asc_hp {
                (68, 72)
            } else {
                (65, 69)
            }
        }
        EnemyId::SpikeSlimeL => {
            if asc_hp {
                (67, 73)
            } else {
                (64, 70)
            }
        }
        EnemyId::GremlinFat => {
            if asc_hp {
                (14, 18)
            } else {
                (13, 17)
            }
        }
        EnemyId::GremlinThief => {
            if asc_hp {
                (11, 15)
            } else {
                (10, 14)
            }
        }
        EnemyId::GremlinTsundere => {
            if asc_hp {
                (13, 17)
            } else {
                (12, 15)
            }
        }
        EnemyId::GremlinWarrior => {
            if asc_hp {
                (21, 25)
            } else {
                (20, 24)
            }
        }
        EnemyId::GremlinWizard => {
            if asc_hp {
                (22, 26)
            } else {
                (21, 25)
            }
        }
        EnemyId::GremlinNob => {
            if asc_elite_hp {
                (85, 90)
            } else {
                (82, 86)
            }
        }
        EnemyId::Lagavulin => {
            if asc_elite_hp {
                (112, 115)
            } else {
                (109, 111)
            }
        }
        EnemyId::Sentry => {
            if asc_elite_hp {
                (39, 45)
            } else {
                (38, 42)
            }
        }
        EnemyId::SlimeBoss => {
            if asc_boss_hp {
                (150, 150)
            } else {
                (140, 140)
            }
        }
        EnemyId::Hexaghost => {
            if asc_boss_hp {
                (264, 264)
            } else {
                (250, 250)
            }
        }
        EnemyId::TheGuardian => {
            if asc_boss_hp {
                (250, 250)
            } else {
                (240, 240)
            }
        }
        EnemyId::SphericGuardian => (20, 20),
        EnemyId::Byrd => {
            if asc_hp {
                (26, 33)
            } else {
                (25, 31)
            }
        }
        EnemyId::Chosen => {
            if asc_hp {
                (98, 103)
            } else {
                (95, 99)
            }
        }
        EnemyId::Snecko => {
            if asc_hp {
                (120, 125)
            } else {
                (114, 120)
            }
        }
        EnemyId::Centurion => {
            if asc_hp {
                (78, 83)
            } else {
                (76, 80)
            }
        }
        EnemyId::Healer => {
            if asc_hp {
                (50, 58)
            } else {
                (48, 56)
            }
        }
        EnemyId::SnakePlant => {
            if asc_hp {
                (78, 82)
            } else {
                (75, 79)
            }
        }
        EnemyId::ShelledParasite => {
            if asc_hp {
                (70, 75)
            } else {
                (68, 72)
            }
        }
        EnemyId::Mugger => {
            if asc_hp {
                (50, 54)
            } else {
                (48, 52)
            }
        }
        EnemyId::BookOfStabbing => {
            if asc_elite_hp {
                (168, 172)
            } else {
                (160, 164)
            }
        }
        EnemyId::Taskmaster => {
            if asc_elite_hp {
                (57, 64)
            } else {
                (54, 60)
            }
        }
        EnemyId::GremlinLeader => {
            if asc_elite_hp {
                (145, 155)
            } else {
                (140, 148)
            }
        }
        EnemyId::BronzeAutomaton => {
            if asc_boss_hp {
                (320, 320)
            } else {
                (300, 300)
            }
        }
        EnemyId::BronzeOrb => {
            if asc_boss_hp {
                (54, 60)
            } else {
                (52, 58)
            }
        }
        EnemyId::TheCollector => {
            if asc_boss_hp {
                (300, 300)
            } else {
                (282, 282)
            }
        }
        EnemyId::TorchHead => {
            if asc_hp {
                (40, 45)
            } else {
                (38, 40)
            }
        }
        EnemyId::Champ => {
            if asc_boss_hp {
                (440, 440)
            } else {
                (420, 420)
            }
        }
        EnemyId::BanditBear => {
            if asc_hp {
                (40, 44)
            } else {
                (38, 42)
            }
        }
        EnemyId::BanditLeader => {
            if asc_hp {
                (37, 41)
            } else {
                (35, 39)
            }
        }
        EnemyId::BanditPointy => {
            if asc_hp {
                (34, 34)
            } else {
                (30, 30)
            }
        }
        EnemyId::Exploder => {
            if asc_hp {
                (30, 35)
            } else {
                (30, 30)
            }
        }
        EnemyId::Repulsor => {
            if asc_hp {
                (31, 38)
            } else {
                (29, 35)
            }
        }
        EnemyId::Spiker => {
            if asc_hp {
                (44, 60)
            } else {
                (42, 56)
            }
        }
        EnemyId::OrbWalker => {
            if asc_hp {
                (92, 102)
            } else {
                (90, 96)
            }
        }
        EnemyId::Darkling => {
            if asc_hp {
                (50, 59)
            } else {
                (48, 56)
            }
        }
        EnemyId::Maw => (300, 300),
        EnemyId::SpireGrowth => {
            if asc_hp {
                (190, 190)
            } else {
                (170, 170)
            }
        }
        EnemyId::Transient => (999, 999),
        EnemyId::WrithingMass => {
            if asc_hp {
                (175, 175)
            } else {
                (160, 160)
            }
        }
        EnemyId::GiantHead => {
            if asc_elite_hp {
                (520, 520)
            } else {
                (500, 500)
            }
        }
        EnemyId::Nemesis => {
            if asc_elite_hp {
                (200, 200)
            } else {
                (185, 185)
            }
        }
        EnemyId::Reptomancer => {
            if asc_elite_hp {
                (190, 200)
            } else {
                (180, 190)
            }
        }
        EnemyId::SnakeDagger => (20, 25),
        EnemyId::AwakenedOne => {
            if asc_boss_hp {
                (320, 320)
            } else {
                (300, 300)
            }
        }
        EnemyId::TimeEater => {
            if asc_boss_hp {
                (480, 480)
            } else {
                (456, 456)
            }
        }
        EnemyId::Donu => {
            if asc_boss_hp {
                (265, 265)
            } else {
                (250, 250)
            }
        }
        EnemyId::Deca => {
            if asc_boss_hp {
                (265, 265)
            } else {
                (250, 250)
            }
        }
        EnemyId::SpireShield => {
            if asc_hp {
                (125, 125)
            } else {
                (110, 110)
            }
        }
        EnemyId::SpireSpear => {
            if asc_hp {
                (180, 180)
            } else {
                (160, 160)
            }
        }
        EnemyId::CorruptHeart => {
            if asc_boss_hp {
                (800, 800)
            } else {
                (750, 750)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{get_hp_range, EnemyId};

    #[test]
    fn enemy_display_names_match_original_game_names() {
        let names = [
            (EnemyId::LouseNormal, "Red Louse"),
            (EnemyId::LouseDefensive, "Green Louse"),
            (EnemyId::SlaverBlue, "Blue Slaver"),
            (EnemyId::SlaverRed, "Red Slaver"),
            (EnemyId::GremlinFat, "Fat Gremlin"),
            (EnemyId::GremlinThief, "Sneaky Gremlin"),
            (EnemyId::GremlinTsundere, "Shield Gremlin"),
            (EnemyId::GremlinWarrior, "Mad Gremlin"),
            (EnemyId::Healer, "Mystic"),
            (EnemyId::Champ, "The Champ"),
            (EnemyId::BanditBear, "Bear"),
            (EnemyId::BanditLeader, "Romeo"),
            (EnemyId::BanditPointy, "Pointy"),
            (EnemyId::Maw, "The Maw"),
        ];
        for (enemy, expected) in names {
            assert_eq!(enemy.get_name(), expected);
        }
    }

    #[test]
    fn louse_hp_ranges_match_java_sources() {
        // Java sources:
        // LouseNormal.java: setHp(10, 15), ascension >= 7 setHp(11, 16).
        // LouseDefensive.java: setHp(11, 17), ascension >= 7 setHp(12, 18).
        assert_eq!(get_hp_range(EnemyId::LouseNormal, 0), (10, 15));
        assert_eq!(get_hp_range(EnemyId::LouseNormal, 7), (11, 16));
        assert_eq!(get_hp_range(EnemyId::LouseDefensive, 0), (11, 17));
        assert_eq!(get_hp_range(EnemyId::LouseDefensive, 7), (12, 18));
    }
}
