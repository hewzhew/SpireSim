//! Read-only explanations owned alongside power execution. Missing coverage
//! is explicit; a missing explanation does not imply unsupported execution.
use super::{core, PowerId};

mod profile;
pub use profile::*;

#[derive(Clone, Copy, Debug)]
pub struct PowerMechanics {
    pub text_id: PowerId,
    pub round_duration: bool,
}

impl PowerMechanics {
    fn text(self, locale: sts_presentation::Locale, field: &str) -> String {
        locale
            .definition_text(
                "game",
                &format!("powerRule.{}.{field}", self.text_id.stable_id()),
            )
            .expect("complete power mechanism text")
    }

    pub fn amount_unit(self, locale: sts_presentation::Locale) -> String {
        self.text(locale, "amountUnit")
    }

    pub fn effect(self, locale: sts_presentation::Locale) -> String {
        self.text(locale, "effect")
    }

    pub fn timing(self, locale: sts_presentation::Locale) -> String {
        self.text(locale, "timing")
    }
}

pub fn get(power: PowerId) -> Option<PowerMechanics> {
    Some(match power {
        PowerId::Strength => core::strength::MECHANICS,
        PowerId::Dexterity => core::dexterity::MECHANICS,
        PowerId::Buffer => PowerMechanics {
            text_id: crate::content::powers::PowerId::Buffer,
            round_duration: false,
        },
        PowerId::Barricade => PowerMechanics {
            text_id: crate::content::powers::PowerId::Barricade,
            round_duration: false,
        },
        PowerId::Berserk => PowerMechanics {
            text_id: crate::content::powers::PowerId::Berserk,
            round_duration: false,
        },
        PowerId::Energized | PowerId::EnergizedBlue => PowerMechanics {
            text_id: crate::content::powers::PowerId::Energized,
            round_duration: false,
        },
        PowerId::Brutality => PowerMechanics {
            text_id: crate::content::powers::PowerId::Brutality,
            round_duration: false,
        },
        PowerId::DemonForm => PowerMechanics {
            text_id: crate::content::powers::PowerId::DemonForm,
            round_duration: false,
        },
        PowerId::DoubleTap => PowerMechanics {
            text_id: crate::content::powers::PowerId::DoubleTap,
            round_duration: false,
        },
        PowerId::Evolve => PowerMechanics {
            text_id: crate::content::powers::PowerId::Evolve,
            round_duration: false,
        },
        PowerId::FireBreathing => PowerMechanics {
            text_id: crate::content::powers::PowerId::FireBreathing,
            round_duration: false,
        },
        PowerId::Juggernaut => PowerMechanics {
            text_id: crate::content::powers::PowerId::Juggernaut,
            round_duration: false,
        },
        PowerId::Rupture => PowerMechanics {
            text_id: crate::content::powers::PowerId::Rupture,
            round_duration: false,
        },
        PowerId::MagnetismPower => PowerMechanics {
            text_id: crate::content::powers::PowerId::MagnetismPower,
            round_duration: false,
        },
        PowerId::MayhemPower => PowerMechanics {
            text_id: crate::content::powers::PowerId::MayhemPower,
            round_duration: false,
        },
        PowerId::PanachePower => PowerMechanics {
            text_id: crate::content::powers::PowerId::PanachePower,
            round_duration: false,
        },
        PowerId::SadisticPower => PowerMechanics {
            text_id: crate::content::powers::PowerId::SadisticPower,
            round_duration: false,
        },
        PowerId::Poison => PowerMechanics {
            text_id: crate::content::powers::PowerId::Poison,
            round_duration: false,
        },
        PowerId::Focus => PowerMechanics {
            text_id: crate::content::powers::PowerId::Focus,
            round_duration: false,
        },
        PowerId::Regeneration => PowerMechanics {
            text_id: crate::content::powers::PowerId::Regeneration,
            round_duration: false,
        },
        PowerId::DuplicationPower => PowerMechanics {
            text_id: crate::content::powers::PowerId::DuplicationPower,
            round_duration: false,
        },
        PowerId::Corruption => super::ironclad::corruption::MECHANICS,
        PowerId::DarkEmbrace => super::ironclad::dark_embrace::MECHANICS,
        PowerId::FeelNoPain => super::ironclad::feel_no_pain::MECHANICS,
        PowerId::NoDraw => core::no_draw::MECHANICS,
        PowerId::NoBlock => core::no_block::MECHANICS,
        PowerId::NextTurnBlock => core::next_turn_block::MECHANICS,
        PowerId::Confusion => core::confusion::MECHANICS,
        PowerId::LoseStrength => core::lose_strength::MECHANICS,
        PowerId::DexterityDown => core::dexterity_down::MECHANICS,
        PowerId::Combust => super::ironclad::combust::MECHANICS,
        PowerId::Vulnerable => core::vulnerable::MECHANICS,
        PowerId::Weak => core::weak::MECHANICS,
        PowerId::Frail => core::frail::MECHANICS,
        PowerId::SporeCloud => core::spore_cloud::MECHANICS,
        PowerId::CurlUp => core::curl_up::MECHANICS,
        PowerId::Flight => core::flight::MECHANICS,
        PowerId::Malleable => core::malleable::MECHANICS,
        PowerId::Hex => core::hex::MECHANICS,
        PowerId::Constricted => core::constricted::MECHANICS,
        PowerId::Reactive => core::reactive::MECHANICS,
        PowerId::Ritual => core::ritual::MECHANICS,
        PowerId::GenericStrengthUp => core::generic_strength_up::MECHANICS,
        PowerId::Anger => core::anger::MECHANICS,
        PowerId::Angry => core::angry::MECHANICS,
        PowerId::Rage => core::rage::MECHANICS,
        PowerId::SharpHide => core::sharp_hide::MECHANICS,
        PowerId::Split => core::split::MECHANICS,
        PowerId::ModeShift => {
            crate::content::monsters::exordium::the_guardian::MODE_SHIFT_MECHANICS
        }
        PowerId::Metallicize => super::ironclad::metallicize::MECHANICS,
        PowerId::FlameBarrier => super::ironclad::flame_barrier::MECHANICS,
        PowerId::Thievery => core::thievery::MECHANICS,
        PowerId::Regrow => core::regrow::MECHANICS,
        PowerId::Explosive => core::explosive::MECHANICS,
        PowerId::Fading => core::fading::MECHANICS,
        PowerId::Shifting => core::shifting::MECHANICS,
        PowerId::Slow => core::slow::MECHANICS,
        PowerId::Shackled => core::shackled::MECHANICS,
        PowerId::Artifact => super::ARTIFACT_MECHANICS,
        PowerId::Entangle => core::entangle::MECHANICS,
        PowerId::Curiosity => core::curiosity::MECHANICS,
        PowerId::Unawakened => core::unawakened::MECHANICS,
        PowerId::BeatOfDeath => core::beat_of_death::MECHANICS,
        PowerId::Invincible => core::invincible::MECHANICS,
        PowerId::TimeWarp => core::time_warp::MECHANICS,
        PowerId::Thorns => core::thorns::MECHANICS,
        PowerId::DrawReduction => core::draw_reduction::MECHANICS,
        PowerId::PlatedArmor => core::plated_armor::MECHANICS,
        PowerId::PainfulStabs => core::painful_stabs::MECHANICS,
        PowerId::Minion => core::minion::MECHANICS,
        PowerId::Surrounded => core::surrounded::MECHANICS,
        PowerId::BackAttack => core::surrounded::BACK_ATTACK_MECHANICS,
        PowerId::Intangible => core::intangible::MONSTER_MECHANICS,
        PowerId::IntangiblePlayer => core::intangible::PLAYER_MECHANICS,
        PowerId::Vigor => PowerMechanics {
            text_id: crate::content::powers::PowerId::Vigor,
            round_duration: false,
        },
        // Execution lives in the shared end-of-turn power dispatcher.
        PowerId::TheBombPower => PowerMechanics {
            text_id: crate::content::powers::PowerId::TheBombPower,
            round_duration: false,
        },
        PowerId::Regen => PowerMechanics {
            text_id: crate::content::powers::PowerId::Regen,
            round_duration: false,
        },
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::action::Action;

    #[test]
    fn every_defined_power_has_complete_bilingual_mechanisms() {
        let mut covered = std::collections::HashSet::new();
        for id in PowerId::STABLE_IDS {
            let Some(fact) = get(PowerId::from_stable_id(id).unwrap()) else {
                continue;
            };
            covered.insert(fact.text_id);
            for locale in [sts_presentation::Locale::En, sts_presentation::Locale::ZhCn] {
                for text in [
                    fact.amount_unit(locale),
                    fact.effect(locale),
                    fact.timing(locale),
                ] {
                    assert!(!text.is_empty() && !text.contains("{{"), "{locale}:{id}");
                    if locale == sts_presentation::Locale::ZhCn {
                        assert!(
                            text.chars().any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                            "{id}: {text}"
                        );
                    }
                }
            }
        }
        let catalog: serde_json::Value =
            serde_json::from_str(include_str!("../../../tools/presentation/locales/en.json"))
                .unwrap();
        let keys = catalog["game"]["powerRule"].as_object().unwrap();
        assert_eq!(covered.len(), keys.len());
        for power in covered {
            assert!(keys.contains_key(power.stable_id()));
        }
    }

    #[test]
    fn next_turn_block_explanation_matches_its_single_delayed_trigger() {
        let fact = get(PowerId::NextTurnBlock).unwrap();
        assert!(fact
            .effect(sts_presentation::Locale::En)
            .contains("Multiple applications add"));
        assert!(!fact.round_duration);
        assert!(matches!(
            core::next_turn_block::at_turn_start(0, 9).as_slice(),
            [
                Action::GainBlock {
                    target: 0,
                    amount: 9
                },
                Action::RemovePower {
                    target: 0,
                    power_id: PowerId::NextTurnBlock
                },
            ]
        ));
    }

    #[test]
    fn intangible_explanations_follow_per_hit_caps_and_distinct_tick_hooks() {
        use crate::runtime::action::DamageType;
        for kind in [DamageType::Normal, DamageType::Thorns, DamageType::HpLoss] {
            assert_eq!(core::intangible::at_damage_final_receive(40, 1, kind), 1);
            assert_eq!(core::intangible::at_damage_final_receive(0, 1, kind), 0);
            assert_eq!(core::intangible::at_damage_final_receive(40, 0, kind), 40);
        }
        assert!(get(PowerId::IntangiblePlayer).unwrap().round_duration);
        assert!(!get(PowerId::Intangible).unwrap().round_duration);
        assert!(core::intangible::at_end_of_turn(1, 2, true).is_empty());
        assert!(matches!(
            core::intangible::at_end_of_turn(1, 2, false).as_slice(),
            [Action::ReducePower {
                power_id: PowerId::Intangible,
                amount: 1,
                ..
            }]
        ));
        assert!(matches!(
            core::intangible::at_end_of_round(0, 2).as_slice(),
            [Action::ReducePower {
                power_id: PowerId::IntangiblePlayer,
                amount: 1,
                ..
            }]
        ));
    }

    #[test]
    fn every_typed_power_profile_also_has_a_player_explanation() {
        for id in PowerId::STABLE_IDS {
            let power = PowerId::from_stable_id(id).unwrap();
            if typed_profile(power).is_some() {
                assert!(
                    get(power).is_some(),
                    "typed facts have no explanation: {id}"
                );
            }
        }
    }

    #[test]
    fn artifact_consumes_a_charge_per_application_and_disappears_before_the_next_debuff() {
        use crate::content::powers::store;
        use crate::engine::action_handlers::powers::handle_apply_power;
        let mut state = crate::test_support::blank_test_combat();
        handle_apply_power(0, 0, PowerId::Artifact, 1, &mut state);
        handle_apply_power(1, 0, PowerId::Weak, 3, &mut state);
        assert!(!store::has_power(&state, 0, PowerId::Artifact));
        assert!(!store::has_power(&state, 0, PowerId::Weak));
        handle_apply_power(1, 0, PowerId::Vulnerable, 2, &mut state);
        assert_eq!(store::power_amount(&state, 0, PowerId::Vulnerable), 2);
    }

    #[test]
    fn duration_explanations_follow_real_tick_and_fresh_application_rules() {
        for (power, tick) in [
            (
                PowerId::Vulnerable,
                core::vulnerable::at_end_of_round as fn(_, _, _) -> _,
            ),
            (PowerId::Weak, core::weak::at_end_of_round),
            (PowerId::Frail, core::frail::at_end_of_round),
        ] {
            assert!(get(power).unwrap().round_duration);
            assert!(tick(0, 2, true).is_empty());
            assert!(
                matches!(tick(0, 2, false).as_slice(), [Action::ReducePower { target: 0, power_id, amount: 1 }] if *power_id == power)
            );
        }
        assert!(!get(PowerId::SporeCloud).unwrap().round_duration);
        assert!(!get(PowerId::Corruption).unwrap().round_duration);
    }

    #[test]
    fn exhaust_engine_and_temporary_stat_rules_follow_execution_hooks() {
        use crate::EntityId;
        let mut state = crate::test_support::blank_test_combat();
        let mut monster =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::JawWorm);
        monster.id = 1;
        state.entities.monsters.push(monster);
        assert!(matches!(
            super::super::ironclad::dark_embrace::on_exhaust(&state, 2).as_slice(),
            [Action::DrawCards(2)]
        ));
        assert!(matches!(
            super::super::ironclad::feel_no_pain::on_exhaust(0, 7).as_slice(),
            [Action::GainBlock {
                target: 0,
                amount: 7
            }]
        ));
        for (power, stat, hook) in [
            (
                PowerId::LoseStrength,
                PowerId::Strength,
                core::lose_strength::at_end_of_turn
                    as fn(EntityId, i32) -> smallvec::SmallVec<[Action; 2]>,
            ),
            (
                PowerId::DexterityDown,
                PowerId::Dexterity,
                core::dexterity_down::at_end_of_turn
                    as fn(EntityId, i32) -> smallvec::SmallVec<[Action; 2]>,
            ),
        ] {
            assert!(matches!(hook(0, 3).as_slice(),
                [Action::ApplyPower { power_id, amount: -3, .. }, Action::RemovePower { power_id: removed, .. }]
                if *power_id == stat && *removed == power
            ));
            assert!(get(power)
                .unwrap()
                .effect(sts_presentation::Locale::En)
                .contains("fixed subtraction"));
        }
        for power in [
            PowerId::Corruption,
            PowerId::DarkEmbrace,
            PowerId::FeelNoPain,
            PowerId::NoDraw,
            PowerId::Combust,
        ] {
            assert!(get(power).is_some(), "missing rule: {power:?}");
        }
    }

    #[test]
    fn common_modifier_explanations_match_the_damage_and_block_pipeline() {
        use crate::runtime::combat::{Power, PowerPayload};
        let mut state = crate::test_support::blank_test_combat();
        let mut monster =
            crate::test_support::test_monster(crate::content::monsters::EnemyId::FungiBeast);
        monster.id = 1;
        state.entities.monsters.push(monster);
        let power = |id, amount| Power {
            power_type: id,
            amount,
            extra_data: 0,
            instance_id: None,
            payload: PowerPayload::None,
            just_applied: false,
        };
        state
            .entities
            .power_db
            .insert(1, vec![power(PowerId::Strength, 5)]);
        state
            .entities
            .power_db
            .insert(0, vec![power(PowerId::Vulnerable, 1)]);
        assert_eq!(
            crate::content::powers::calculate_monster_damage(6, 1, 0, &state),
            16
        );
        state
            .entities
            .power_db
            .get_mut(&1)
            .unwrap()
            .push(power(PowerId::Weak, 2));
        assert_eq!(
            crate::content::powers::calculate_monster_damage(6, 1, 0, &state),
            12
        );
        assert_eq!(core::frail::on_calculate_block(5.0, 1), 3.0);
        assert_eq!(core::dexterity::on_calculate_block(5.0, 2), 7.0);
    }
}
