//! Read-only explanations owned alongside power execution. Missing coverage
//! is explicit; a missing explanation does not imply unsupported execution.
use super::{core, PowerId};

mod profile;
pub use profile::*;

#[derive(Clone, Copy, Debug)]
pub struct PowerMechanics {
    pub amount_unit: &'static str,
    pub effect: &'static str,
    pub timing: &'static str,
    pub round_duration: bool,
}

pub fn get(power: PowerId) -> Option<PowerMechanics> {
    Some(match power {
        PowerId::Strength => core::strength::MECHANICS,
        PowerId::Dexterity => core::dexterity::MECHANICS,
        PowerId::Buffer => PowerMechanics {
            amount_unit: "remaining damage or HP-loss events to prevent",
            effect: "Prevent the next positive damage or HP-loss event, then consume one charge. Fully blocked damage does not consume Buffer. Unblocked Beat of Death can consume it; each hit of a multi-hit attack is a separate event. Direct HP loss also consumes a charge, even though Block cannot absorb that HP loss.",
            timing: "After Block for blockable damage, before positive HP loss; persists until its charges are consumed or it is removed.",
            round_duration: false,
        },
        PowerId::Barricade => PowerMechanics {
            amount_unit: "presence; numeric amount is not a stack count",
            effect: "Keeps the owner's remaining Block at the normal turn-start clearing point. Incoming damage still spends Block; the effect creates no Block itself.",
            timing: "Owner turn start; persists for combat until removed.",
            round_duration: false,
        },
        PowerId::Berserk => PowerMechanics {
            amount_unit: "additional Energy per player turn",
            effect: "Gain amount additional Energy at each player turn start. The card's self-Vulnerable is a separate debuff with its own duration.",
            timing: "Player turn start, from the next turn after deployment; persists for combat.",
            round_duration: false,
        },
        PowerId::Energized | PowerId::EnergizedBlue => PowerMechanics {
            amount_unit: "additional Energy at the next player recharge",
            effect: "Gain amount additional Energy at the next player energy recharge, then remove this power. Repeated applications of the same color add together; green and blue Energized remain separate powers and both trigger.",
            timing: "Next player energy recharge, before ordinary start-of-turn draws; does not grant Energy when applied.",
            round_duration: false,
        },
        PowerId::Brutality => PowerMechanics {
            amount_unit: "additional cards drawn and HP lost per turn",
            effect: "After the normal draw, draw amount additional cards, then lose amount HP. Draw prevention and hand capacity apply to the draw; Block does not prevent the HP loss.",
            timing: "Player turn start after normal draw, from the next turn after deployment; persists for combat.",
            round_duration: false,
        },
        PowerId::DemonForm => PowerMechanics {
            amount_unit: "Strength gained per player turn",
            effect: "Gain amount Strength after each normal start-of-turn draw. Existing Strength remains, so gains accumulate across turns.",
            timing: "Player turn start after normal draw, from the next turn after deployment; persists for combat.",
            round_duration: false,
        },
        PowerId::DoubleTap => PowerMechanics {
            amount_unit: "remaining Attack cards to replay",
            effect: "Each next Attack card is played a second time for free against the same target, after its original effects. Consume one charge per original Attack card; Skills and Powers keep the charges.",
            timing: "On card play; unused charges expire at player turn end.",
            round_duration: false,
        },
        PowerId::Evolve => PowerMechanics {
            amount_unit: "additional cards drawn per Status drawn",
            effect: "Drawing a Status requests amount additional draws. Curses do not trigger it. Draw prevention and hand capacity apply, and newly drawn Statuses can trigger further draws.",
            timing: "On Status draw while active; persists for combat.",
            round_duration: false,
        },
        PowerId::FireBreathing => PowerMechanics {
            amount_unit: "damage to each enemy per Status or Curse drawn",
            effect: "When a Status or Curse is drawn, deal amount THORNS damage to all enemies. Strength, Weak and Vulnerable do not modify this damage; enemy Block and damage caps apply.",
            timing: "On Status or Curse draw while active; persists for combat.",
            round_duration: false,
        },
        PowerId::Juggernaut => PowerMechanics {
            amount_unit: "damage per positive Block gain event",
            effect: "Whenever the player gains positive Block, deal amount THORNS damage to a random living enemy. Separate Block gains can trigger separately. This deals damage rather than granting more Block.",
            timing: "On positive Block gain while active; persists for combat.",
            round_duration: false,
        },
        PowerId::Rupture => PowerMechanics {
            amount_unit: "Strength gained per qualifying self-inflicted HP-loss event",
            effect: "Gain amount Strength when HP is actually lost from a qualifying self-inflicted card effect, such as Offering or Brutality. Fully prevented HP loss supplies no gain.",
            timing: "On qualifying self-inflicted HP loss while active; persists for combat.",
            round_duration: false,
        },
        PowerId::MagnetismPower => PowerMechanics {
            amount_unit: "random Colorless cards generated per player turn",
            effect: "Add amount random temporary Colorless cards to Hand at each turn start. They keep their ordinary costs; hand capacity applies.",
            timing: "Player turn start from the next turn after deployment; persists for combat.",
            round_duration: false,
        },
        PowerId::MayhemPower => PowerMechanics {
            amount_unit: "top draw-pile cards automatically played per player turn",
            effect: "After normal turn draw, automatically play amount cards from the top of the remaining draw pile for free, choosing a random enemy when a target is required. Each played card keeps its own Exhaust rule.",
            timing: "Player turn start after normal draw, from the next turn after deployment; persists for combat.",
            round_duration: false,
        },
        PowerId::PanachePower => PowerMechanics {
            amount_unit: "cards still to play before the next damage trigger this turn",
            effect: "Every fifth card played this turn deals the stored THORNS damage to all enemies, then resets the counter to 5. The card supplies 10 damage (14 if upgraded); repeated deployment adds stored damage.",
            timing: "On card play; the counter resets to 5 at player turn start, and the damage effect persists for combat.",
            round_duration: false,
        },
        PowerId::SadisticPower => PowerMechanics {
            amount_unit: "damage per qualifying debuff application",
            effect: "When the owner applies a debuff to another entity without Artifact, deal amount THORNS damage to that target. Artifact-blocked applications supply no damage.",
            timing: "On debuff application while active; persists for combat.",
            round_duration: false,
        },
        PowerId::Poison => PowerMechanics {
            amount_unit: "HP lost at the next owner turn start",
            effect: "At the owner's turn start, lose HP equal to amount, then reduce Poison by 1. Block does not prevent this loss. Artifact can block a new Poison application.",
            timing: "Owner turn start; decreases per trigger until removed.",
            round_duration: false,
        },
        PowerId::Focus => PowerMechanics {
            amount_unit: "modifier to Lightning, Frost and Dark Orb amounts",
            effect: "Adds amount to affected Orb damage or Block values, with a minimum of 0. Affects Lightning damage, Frost Block and Dark passive charge gain; Plasma Energy and the Dark Orb's already stored charge are unchanged.",
            timing: "When affected Orb values are evaluated; persists until changed or removed.",
            round_duration: false,
        },
        PowerId::Regeneration => PowerMechanics {
            amount_unit: "HP healed at the next owner turn end",
            effect: "Heal amount HP up to Max HP and subject to healing modifiers, then reduce the amount by 1. With 5 stacks and no modifiers, successive healing requests are 5, 4, 3, 2 and 1.",
            timing: "Owner turn end, including the turn applied; decreases per trigger until removed.",
            round_duration: false,
        },
        PowerId::DuplicationPower => PowerMechanics {
            amount_unit: "remaining original cards to replay",
            effect: "Each next original card is played a second time for free against the same target after its first effects, consuming one charge. Works on Attacks, Skills and Powers. Replays themselves consume no additional charge.",
            timing: "On card play; one unused charge is lost at each round end.",
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
        PowerId::ModeShift => crate::content::monsters::exordium::the_guardian::MODE_SHIFT_MECHANICS,
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
            amount_unit: "bonus damage per hit of the next Attack card",
            effect: "Adds amount to every normal damage hit and target of the next Attack card, before damage modifiers. Multiple hits consume one Vigor effect, not one stack per hit.",
            timing: "Consumed when an Attack card is played. Skills and Powers do not consume it.",
            round_duration: false,
        },
        // Execution lives in the shared end-of-turn power dispatcher.
        PowerId::TheBombPower => PowerMechanics {
            amount_unit: "remaining owner turn-end ticks until the explosion",
            effect: "Decrements by 1 at each owner's turn end. At amount 1, removes this countdown and deals its stored damage to all enemies as THORNS damage. The Bomb stores 40 damage, or 50 when upgraded; Strength, Weak and Vulnerable do not modify it. Enemy Block and damage caps still apply.",
            timing: "Owner turn end, including the turn it was applied. Starts at 3, so the explosion occurs on the third turn end.",
            round_duration: false,
        },
        PowerId::Regen => PowerMechanics {
            amount_unit: "HP healed at each owner's turn end",
            effect: "Heal the owner by amount at its turn end, up to Max HP. Does not decrement. Awakened One retains it after Rebirth. This is the monster's Regenerate, distinct from the player's decreasing Regeneration effect.",
            timing: "Owner turn end; persists until removed.",
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
    fn next_turn_block_explanation_matches_its_single_delayed_trigger() {
        let fact = get(PowerId::NextTurnBlock).unwrap();
        assert!(fact.effect.contains("Multiple applications add"));
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
            assert!(get(power).unwrap().effect.contains("fixed subtraction"));
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
