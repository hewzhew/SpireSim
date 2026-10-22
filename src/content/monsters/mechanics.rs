//! Public move definitions, not a prediction of the next rolled move. These
//! constructors take difficulty only: no entity, hidden state or RNG access.
use super::{beyond, city, ending, exordium, EnemyId};
use crate::content::powers::{self, PowerId};
use crate::runtime::monster_move::MonsterTurnPlan;
use sts_presentation::Locale;

/// Display-only rules and owner-provided values. Constructing this tree reads
/// no monster instance or RNG; rendering it does not alter an executable plan.
#[derive(Clone, Debug)]
pub enum MonsterText {
    Rule {
        key: &'static str,
        parameters: Vec<(&'static str, MonsterText)>,
    },
    Number(i32),
    PowerEffect(PowerId),
    PowerName(PowerId),
    EnemyName(EnemyId),
    Sequence(Vec<MonsterText>),
    List(Vec<MonsterText>),
}

impl MonsterText {
    pub fn new(key: &'static str) -> Self {
        Self::Rule {
            key,
            parameters: vec![],
        }
    }

    pub fn with(mut self, name: &'static str, value: impl Into<Self>) -> Self {
        let Self::Rule { parameters, .. } = &mut self else {
            panic!("only rule messages accept parameters")
        };
        assert!(
            !parameters.iter().any(|(key, _)| *key == name),
            "duplicate rule parameter"
        );
        parameters.push((name, value.into()));
        self
    }

    pub fn render(&self, locale: Locale) -> String {
        match self {
            Self::Rule { key, parameters } => {
                let values: Vec<_> = parameters
                    .iter()
                    .map(|(name, value)| (*name, value.render(locale)))
                    .collect();
                let values: Vec<_> = values
                    .iter()
                    .map(|(name, value)| (*name, value.as_str()))
                    .collect();
                locale
                    .definition_text_with("game", &format!("enemyRule.{key}"), &values)
                    .expect("enemy mechanism message")
            }
            Self::Number(value) => value.to_string(),
            Self::PowerEffect(power) => powers::mechanics::get(*power)
                .expect("referenced power mechanism")
                .effect(locale),
            Self::PowerName(power) => locale.entity("power", power.stable_id()).to_owned(),
            Self::EnemyName(enemy) => locale.entity("enemy", enemy.stable_id()).to_owned(),
            Self::Sequence(values) => values
                .iter()
                .map(|value| value.render(locale))
                .collect::<Vec<_>>()
                .join(
                    locale
                        .entry("game", "enemyRule.separator")
                        .expect("enemy rule separator"),
                ),
            Self::List(values) => values
                .iter()
                .map(|value| value.render(locale))
                .collect::<Vec<_>>()
                .join(
                    locale
                        .entry("game", "enemyRule.listSeparator")
                        .expect("enemy list separator"),
                ),
        }
    }
}

impl From<i32> for MonsterText {
    fn from(value: i32) -> Self {
        Self::Number(value)
    }
}

impl From<u8> for MonsterText {
    fn from(value: u8) -> Self {
        Self::Number(i32::from(value))
    }
}

pub struct MonsterMechanics {
    pub moves: Vec<(MonsterText, MonsterTurnPlan)>,
    /// Full execution rules for moves whose effects cannot be expressed by a
    /// difficulty-only plan (for example, healing based on HP at execution).
    pub special_moves: Vec<(MonsterText, MonsterText)>,
    pub notes: MonsterText,
}

/// Reviewed fixed starting effects shared with the encounter's runtime owner.
pub fn starting_powers(
    enemy: EnemyId,
    ascension: u8,
) -> Vec<(crate::content::powers::PowerId, i32)> {
    match enemy {
        EnemyId::CorruptHeart => ending::corrupt_heart::starting_powers(ascension).to_vec(),
        EnemyId::BookOfStabbing => city::book_of_stabbing::STARTING_POWERS.to_vec(),
        EnemyId::GiantHead => beyond::giant_head::STARTING_POWERS.to_vec(),
        EnemyId::BronzeAutomaton => vec![(
            crate::content::powers::PowerId::Artifact,
            city::bronze_automaton::STARTING_ARTIFACT,
        )],
        EnemyId::Spiker => vec![(
            crate::content::powers::PowerId::Thorns,
            beyond::spiker::starting_thorns(ascension),
        )],
        EnemyId::WrithingMass => beyond::writhing_mass::STARTING_POWERS.to_vec(),
        _ => vec![],
    }
}

pub fn get(enemy: EnemyId, ascension: u8) -> Option<MonsterMechanics> {
    Some(match enemy {
        EnemyId::FungiBeast => exordium::fungi_beast::mechanics(ascension),
        EnemyId::JawWorm => exordium::jaw_worm::mechanics(ascension),
        EnemyId::Cultist => exordium::cultist::mechanics(ascension),
        EnemyId::GremlinNob => exordium::gremlin_nob::mechanics(ascension),
        EnemyId::Sentry => exordium::sentry::mechanics(ascension),
        EnemyId::LouseNormal => exordium::louse_normal::mechanics(ascension),
        EnemyId::LouseDefensive => exordium::louse_defensive::mechanics(ascension),
        EnemyId::AcidSlimeS | EnemyId::AcidSlimeM | EnemyId::AcidSlimeL => {
            exordium::acid_slime::mechanics(enemy, ascension)
        }
        EnemyId::SpikeSlimeS | EnemyId::SpikeSlimeM | EnemyId::SpikeSlimeL => {
            exordium::spike_slime::mechanics(enemy, ascension)
        }
        EnemyId::Looter => exordium::looter::mechanics(ascension),
        EnemyId::Mugger => city::mugger::mechanics(ascension),
        EnemyId::SlaverBlue => exordium::slaver_blue::mechanics(ascension),
        EnemyId::SlaverRed => exordium::slaver_red::mechanics(ascension),
        EnemyId::GremlinFat => exordium::gremlin_fat::mechanics(ascension),
        EnemyId::GremlinThief => exordium::gremlin_thief::mechanics(ascension),
        EnemyId::GremlinTsundere => exordium::gremlin_tsundere::mechanics(ascension),
        EnemyId::GremlinWarrior => exordium::gremlin_warrior::mechanics(ascension),
        EnemyId::GremlinWizard => exordium::gremlin_wizard::mechanics(ascension),
        EnemyId::Lagavulin => exordium::lagavulin::mechanics(ascension),
        EnemyId::SlimeBoss => exordium::slime_boss::mechanics(ascension),
        EnemyId::Hexaghost => exordium::hexaghost::mechanics(ascension),
        EnemyId::TheGuardian => exordium::the_guardian::mechanics(ascension),
        EnemyId::Byrd => city::byrd::mechanics(ascension),
        EnemyId::Chosen => city::chosen::mechanics(ascension),
        EnemyId::Taskmaster => city::taskmaster::mechanics(ascension),
        EnemyId::BookOfStabbing => city::book_of_stabbing::mechanics(ascension),
        EnemyId::GremlinLeader => city::gremlin_leader::mechanics(ascension),
        EnemyId::BanditPointy => city::bandit_pointy::mechanics(ascension),
        EnemyId::BanditLeader => city::bandit_leader::mechanics(ascension),
        EnemyId::BanditBear => city::bandit_bear::mechanics(ascension),
        EnemyId::Centurion => city::centurion::mechanics(ascension),
        EnemyId::Healer => city::healer::mechanics(ascension),
        EnemyId::Champ => city::champ::mechanics(ascension),
        EnemyId::SnakePlant => city::snake_plant::mechanics(ascension),
        EnemyId::ShelledParasite => city::shelled_parasite::mechanics(ascension),
        EnemyId::Snecko => city::snecko::mechanics(ascension),
        EnemyId::SphericGuardian => city::spheric_guardian::mechanics(ascension),
        EnemyId::TheCollector => city::the_collector::mechanics(ascension),
        EnemyId::BronzeAutomaton => city::bronze_automaton::mechanics(ascension),
        EnemyId::BronzeOrb => city::bronze_orb::mechanics(),
        EnemyId::TorchHead => city::torch_head::mechanics(),
        EnemyId::AwakenedOne => beyond::awakened_one::mechanics(),
        EnemyId::Darkling => beyond::darkling::mechanics(ascension),
        EnemyId::Exploder => beyond::exploder::mechanics(ascension),
        EnemyId::Repulsor => beyond::repulsor::mechanics(ascension),
        EnemyId::Spiker => beyond::spiker::mechanics(ascension),
        EnemyId::Maw => beyond::maw::mechanics(ascension),
        EnemyId::SpireGrowth => beyond::spire_growth::mechanics(ascension),
        EnemyId::WrithingMass => beyond::writhing_mass::mechanics(ascension),
        EnemyId::OrbWalker => beyond::orb_walker::mechanics(ascension),
        EnemyId::Transient => beyond::transient::mechanics(ascension),
        EnemyId::Nemesis => beyond::nemesis::mechanics(ascension),
        EnemyId::GiantHead => beyond::giant_head::mechanics(ascension),
        EnemyId::Reptomancer => beyond::reptomancer::mechanics(ascension),
        EnemyId::SnakeDagger => beyond::snake_dagger::mechanics(),
        EnemyId::TimeEater => beyond::time_eater::mechanics(ascension),
        EnemyId::Donu => beyond::donu::mechanics(ascension),
        EnemyId::Deca => beyond::deca::mechanics(ascension),
        EnemyId::CorruptHeart => ending::corrupt_heart::mechanics(ascension),
        EnemyId::SpireShield => ending::spire_shield::mechanics(ascension),
        EnemyId::SpireSpear => ending::spire_spear::mechanics(ascension),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::monsters::MonsterBehavior;
    use crate::content::powers::PowerId;
    use crate::runtime::action::Action;

    #[test]
    fn every_implemented_enemy_has_a_public_move_catalog() {
        for asc in 0..=20 {
            for id in EnemyId::STABLE_IDS {
                let enemy = EnemyId::from_stable_id(id).unwrap();
                let info = get(enemy, asc).unwrap_or_else(|| panic!("missing {id} A{asc}"));
                assert!(!info.moves.is_empty(), "{id} A{asc}");
                for locale in [Locale::En, Locale::ZhCn] {
                    let mut texts = vec![info.notes.render(locale)];
                    texts.extend(info.moves.iter().map(|(name, _)| name.render(locale)));
                    for (name, rule) in &info.special_moves {
                        texts.push(name.render(locale));
                        texts.push(rule.render(locale));
                    }
                    for text in texts {
                        assert!(
                            !text.is_empty() && !text.contains("{{"),
                            "{id} A{asc} {locale}: {text}"
                        );
                        if locale == Locale::ZhCn {
                            assert!(
                                text.chars()
                                    .any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch)),
                                "{id} A{asc}: {text}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn chosen_and_remaining_beyond_catalogs_reuse_executable_plans() {
        for asc in [0, 1, 2, 16, 17, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            for enemy in [
                EnemyId::Chosen,
                EnemyId::Spiker,
                EnemyId::Maw,
                EnemyId::SpireGrowth,
                EnemyId::WrithingMass,
            ] {
                let mut monster = crate::test_support::test_monster(enemy);
                for (name, plan) in get(enemy, asc).unwrap().moves {
                    monster.set_planned_move_id(plan.move_id);
                    assert_eq!(
                        plan,
                        super::super::resolve_monster_turn_plan(&state, &monster),
                        "{enemy:?} A{asc} {name:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn act_two_preparation_catalogs_share_execution_plans() {
        for asc in [0, 3, 17, 18, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            let book = crate::test_support::test_monster(EnemyId::BookOfStabbing);
            let prebattle = city::book_of_stabbing::BookOfStabbing::use_pre_battle_actions(
                &mut state,
                &book,
                super::super::PreBattleLegacyRng::Misc,
            );
            let projected: Vec<_> = starting_powers(EnemyId::BookOfStabbing, asc)
                .into_iter()
                .map(|(power_id, amount)| Action::ApplyPower {
                    source: book.id,
                    target: book.id,
                    power_id,
                    amount,
                })
                .collect();
            assert_eq!(prebattle, projected);
            for enemy in [
                EnemyId::BookOfStabbing,
                EnemyId::GremlinLeader,
                EnemyId::BanditPointy,
                EnemyId::BanditLeader,
                EnemyId::BanditBear,
                EnemyId::Snecko,
                EnemyId::ShelledParasite,
                EnemyId::Mugger,
            ] {
                let mut monster = crate::test_support::test_monster(enemy);
                for (_, plan) in get(enemy, asc).unwrap().moves {
                    monster.set_planned_move_id(plan.move_id);
                    assert_eq!(
                        plan,
                        super::super::resolve_monster_turn_plan(&state, &monster)
                    );
                }
            }
        }
    }

    #[test]
    fn centurion_and_mystic_catalogs_share_execution_plans_and_keep_buff_ambiguity() {
        for asc in [0, 1, 2, 16, 17, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            for enemy in [EnemyId::Centurion, EnemyId::Healer] {
                let mut monster = crate::test_support::test_monster(enemy);
                let fact = get(enemy, asc).unwrap();
                for (_, plan) in fact.moves {
                    monster.set_planned_move_id(plan.move_id);
                    assert_eq!(
                        plan,
                        super::super::resolve_monster_turn_plan(&state, &monster)
                    );
                }
            }
            let fact = get(EnemyId::Healer, asc).unwrap();
            for (_, plan) in &fact.moves[1..] {
                assert_eq!(
                    crate::runtime::combat::Intent::from_visible_move_spec(&plan.summary_spec()),
                    crate::runtime::combat::Intent::Buff
                );
            }
            assert!(fact
                .notes
                .render(Locale::En)
                .contains("icon alone does not distinguish them"));
        }
    }

    #[test]
    fn orb_walker_catalog_shares_damage_status_plans_and_scaling_with_execution() {
        for asc in [0, 1, 2, 16, 17, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            let mut monster = crate::test_support::test_monster(EnemyId::OrbWalker);
            let info = get(monster.enemy_id, asc).unwrap();
            for (name, plan) in info.moves {
                monster.set_planned_move_id(plan.move_id);
                assert_eq!(
                    plan,
                    super::super::resolve_monster_turn_plan(&state, &monster),
                    "A{asc} {name:?}"
                );
            }
            let actions = beyond::orb_walker::OrbWalker::use_pre_battle_actions(
                &mut state,
                &monster,
                super::super::PreBattleLegacyRng::Misc,
            );
            let Action::ApplyPower {
                power_id: PowerId::GenericStrengthUp,
                amount,
                ..
            } = actions[0]
            else {
                panic!("missing round scaling power")
            };
            assert!(info.special_moves[0]
                .1
                .render(Locale::En)
                .contains(&format!("Generic Strength Up {amount}")));
        }
    }

    #[test]
    fn observed_late_run_catalogs_share_plans_and_disclose_dynamic_exceptions() {
        for asc in [0, 2, 3, 4, 9, 17, 18, 19, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            for enemy in [
                EnemyId::Champ,
                EnemyId::Darkling,
                EnemyId::Exploder,
                EnemyId::Repulsor,
                EnemyId::Transient,
                EnemyId::Nemesis,
            ] {
                let mut monster = crate::test_support::test_monster(enemy);
                let info = get(enemy, asc).unwrap();
                assert!(
                    !info.special_moves.is_empty(),
                    "{enemy:?} needs execution-only rules"
                );
                for (name, plan) in info.moves {
                    monster.set_planned_move_id(plan.move_id);
                    assert_eq!(
                        plan,
                        super::super::resolve_monster_turn_plan(&state, &monster),
                        "{enemy:?} A{asc} {name:?}"
                    );
                }
            }
            let (min, max) = beyond::darkling::nip_damage_range(asc);
            for seed in 0..32 {
                let rolled = beyond::darkling::roll_nip_damage(
                    &mut crate::runtime::rng::StsRng::new(seed),
                    asc,
                );
                assert!((min..=max).contains(&rolled));
            }
            assert!(get(EnemyId::Darkling, asc).unwrap().special_moves[0]
                .1
                .render(Locale::En)
                .contains(&format!("{min}..{max}")));
        }
    }

    #[test]
    fn observed_city_catalogs_share_executable_plans_and_prebattle_amounts() {
        for asc in [0, 2, 3, 17, 18, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            for enemy in [EnemyId::Byrd, EnemyId::Taskmaster, EnemyId::SnakePlant] {
                let mut monster = crate::test_support::test_monster(enemy);
                let info = get(enemy, asc).unwrap();
                for (name, plan) in info.moves {
                    monster.set_planned_move_id(plan.move_id);
                    assert_eq!(
                        plan,
                        super::super::resolve_monster_turn_plan(&state, &monster),
                        "{enemy:?} A{asc} {name:?}"
                    );
                }
                if enemy == EnemyId::Taskmaster {
                    assert_eq!(!info.special_moves.is_empty(), asc >= 18);
                } else {
                    let actions = match enemy {
                        EnemyId::Byrd => city::byrd::Byrd::use_pre_battle_actions(
                            &mut state,
                            &monster,
                            super::super::PreBattleLegacyRng::Misc,
                        ),
                        EnemyId::SnakePlant => {
                            city::snake_plant::SnakePlant::use_pre_battle_actions(
                                &mut state,
                                &monster,
                                super::super::PreBattleLegacyRng::Misc,
                            )
                        }
                        _ => unreachable!(),
                    };
                    let Action::ApplyPower { amount, .. } = actions[0] else {
                        panic!("missing initial power")
                    };
                    assert!(info.special_moves[0]
                        .1
                        .render(Locale::En)
                        .contains(&format!("{amount}")));
                }
            }
        }
    }

    #[test]
    fn first_act_encounter_catalogs_cover_spawned_enemies_and_share_executable_plans() {
        use super::super::encounter_pool::{public_encounter_pool, EncounterPoolTier};
        use super::super::factory::{build_encounter, EncounterId};
        use crate::runtime::monster_move::MoveStep;
        use crate::runtime::rng::StsRng;

        let mut encounters: Vec<_> = [
            EncounterPoolTier::Weak,
            EncounterPoolTier::Strong,
            EncounterPoolTier::Elite,
        ]
        .into_iter()
        .flat_map(|tier| public_encounter_pool(1, tier).iter().map(|e| e.encounter))
        .collect();
        encounters.extend([
            EncounterId::SlimeBoss,
            EncounterId::Hexaghost,
            EncounterId::TheGuardian,
        ]);
        let mut seen = std::collections::HashSet::new();
        for asc in [0, 1, 2, 3, 4, 7, 9, 17, 18, 19, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            for &encounter in &encounters {
                for seed in 0..32 {
                    for mut monster in build_encounter(
                        encounter,
                        &mut StsRng::new(seed),
                        &mut StsRng::new(seed),
                        asc,
                    ) {
                        let enemy = monster.enemy_id;
                        seen.insert(enemy);
                        let info = get(enemy, asc).unwrap_or_else(|| {
                            panic!("no public definition for {encounter:?} -> {enemy:?}")
                        });
                        assert!(!info.moves.is_empty());
                        for (name, plan) in info.moves {
                            // Sear has two disclosed variants, selected by whether Inferno
                            // has happened, rather than a fabricated fixed Burn upgrade.
                            if enemy == EnemyId::Hexaghost {
                                monster.hexaghost.burn_upgraded = plan
                                    .steps
                                    .iter()
                                    .any(|s| matches!(s, MoveStep::AddCard(c) if c.upgraded));
                            }
                            monster.set_planned_move_id(plan.move_id);
                            assert_eq!(
                                plan,
                                super::super::resolve_monster_turn_plan(&state, &monster),
                                "{enemy:?} A{asc} {name:?}"
                            );
                        }
                    }
                }
            }
        }
        assert_eq!(
            seen.len(),
            25,
            "first-act census must include all variants, elites and bosses"
        );
    }

    #[test]
    fn louse_public_ranges_match_spawned_bites_and_prebattle_curl_up() {
        use super::super::factory::{build_encounter, EncounterId};
        use crate::runtime::rng::StsRng;
        for asc in [0, 1, 2, 6, 7, 16, 17, 20] {
            let (min, max) = exordium::louse_bite_damage_range(asc);
            let (curl_min, curl_max) = exordium::louse_curl_up_range(asc);
            for seed in 0..64 {
                let mut state = crate::test_support::blank_test_combat();
                state.meta.ascension_level = asc;
                for mut monster in build_encounter(
                    EncounterId::ThreeLouse,
                    &mut StsRng::new(seed),
                    &mut StsRng::new(seed),
                    asc,
                ) {
                    let bite = monster.louse.bite_damage.unwrap();
                    assert!((min..=max).contains(&bite));
                    monster.set_planned_move_id(3);
                    let plan = super::super::resolve_monster_turn_plan(&state, &monster);
                    assert!(
                        matches!(&plan.steps[0], crate::runtime::monster_move::MoveStep::Attack(a) if a.attack.base_damage == bite)
                    );
                    let actions = match monster.enemy_id {
                        EnemyId::LouseNormal => {
                            exordium::louse_normal::LouseNormal::use_pre_battle_actions(
                                &mut state,
                                &monster,
                                super::super::PreBattleLegacyRng::Misc,
                            )
                        }
                        EnemyId::LouseDefensive => {
                            exordium::louse_defensive::LouseDefensive::use_pre_battle_actions(
                                &mut state,
                                &monster,
                                super::super::PreBattleLegacyRng::Misc,
                            )
                        }
                        _ => unreachable!(),
                    };
                    assert!(
                        matches!(actions.as_slice(), [Action::ApplyPower { power_id: PowerId::CurlUp, amount, .. }] if (curl_min..=curl_max).contains(amount))
                    );
                    let info = get(monster.enemy_id, asc).unwrap();
                    assert!(info
                        .special_moves
                        .iter()
                        .any(|(name, text)| name.render(Locale::En) == "Bite"
                            && text.render(Locale::En).contains(&format!("{min}..{max}"))));
                    assert!(info
                        .special_moves
                        .iter()
                        .any(|(name, text)| name.render(Locale::En) == "Starting Curl Up"
                            && text
                                .render(Locale::En)
                                .contains(&format!("{curl_min}..{curl_max}"))));
                }
            }
        }
    }

    #[test]
    fn endgame_catalogs_share_the_executable_difficulty_specific_plans() {
        for asc in [0, 3, 4, 18, 19, 20] {
            let mut state = crate::test_support::blank_test_combat();
            state.meta.ascension_level = asc;
            for enemy in [
                EnemyId::BronzeAutomaton,
                EnemyId::BronzeOrb,
                EnemyId::Reptomancer,
                EnemyId::SnakeDagger,
                EnemyId::AwakenedOne,
                EnemyId::TimeEater,
                EnemyId::Donu,
                EnemyId::Deca,
                EnemyId::CorruptHeart,
                EnemyId::SpireShield,
                EnemyId::SpireSpear,
            ] {
                let mut monster = crate::test_support::test_monster(enemy);
                if enemy == EnemyId::CorruptHeart {
                    ending::corrupt_heart::initialize_runtime_state(&mut monster, asc);
                } else if enemy == EnemyId::SpireSpear {
                    ending::spire_spear::initialize_runtime_state(&mut monster, asc);
                }
                for (name, plan) in get(enemy, asc).unwrap().moves {
                    monster.set_planned_move_id(plan.move_id);
                    assert_eq!(
                        plan,
                        super::super::resolve_monster_turn_plan(&state, &monster),
                        "{enemy:?} A{asc} {name:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn sentry_catalog_matches_executable_bolt_at_ascension_boundaries() {
        let mut state = crate::test_support::blank_test_combat();
        let monster = crate::test_support::test_monster(EnemyId::Sentry);
        for (asc, damage, dazed) in [
            (0, 9, 2),
            (2, 9, 2),
            (3, 10, 2),
            (17, 10, 2),
            (18, 10, 3),
            (20, 10, 3),
        ] {
            let info = get(EnemyId::Sentry, asc).unwrap();
            let beam = &info.moves[0].1;
            assert!(
                matches!(&beam.steps[0], crate::runtime::monster_move::MoveStep::Attack(a) if a.attack.base_damage == damage)
            );
            let actions =
                exordium::sentry::Sentry::take_turn_plan(&mut state, &monster, &info.moves[1].1);
            assert!(
                matches!(actions[0], Action::MakeTempCardInDiscard { card_id: crate::content::cards::CardId::Dazed, amount, upgraded: false } if amount == dazed)
            );
        }
    }

    #[test]
    fn fungi_catalog_uses_executable_plans_at_each_difficulty_boundary() {
        let mut state = crate::test_support::blank_test_combat();
        let mut monster = crate::test_support::test_monster(EnemyId::FungiBeast);
        monster.id = 1;
        for (asc, expected) in [(0, 3), (1, 3), (2, 4), (16, 4), (17, 5), (20, 5)] {
            let info = get(EnemyId::FungiBeast, asc).unwrap();
            let grow = info
                .moves
                .iter()
                .find(|(name, _)| name.render(Locale::En) == "Grow")
                .unwrap();
            let actions =
                exordium::fungi_beast::FungiBeast::take_turn_plan(&mut state, &monster, &grow.1);
            assert!(
                matches!(actions[0], Action::ApplyPower { source: 1, target: 1, power_id: PowerId::Strength, amount } if amount == expected)
            );
        }
    }
}
