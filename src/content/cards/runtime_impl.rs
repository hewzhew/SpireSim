// Mechanical split from cards/mod.rs for runtime card play/evaluation helpers.

use crate::content::powers::PowerId;
use crate::EntityId;
use crate::runtime::action::{Action, ActionInfo, AddTo};
use crate::runtime::combat::{CardBlueprint, CombatCard, CombatState};
use smallvec::SmallVec;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CardUseContext {
    /// Java `AbstractPlayer.useCard` calls `card.use(...)` before removing a
    /// normally played card from hand. Autoplay/direct paths can call the same
    /// card code while the card is already outside the hand.
    pub played_from_hand: bool,
}

/// Central dispatch table for resolving card play mechanics.
pub fn resolve_card_play(
    card_id: CardId,
    state: &CombatState,
    card: &CombatCard,
    target: Option<EntityId>,
) -> SmallVec<[ActionInfo; 4]> {
    resolve_card_play_with_context(card_id, state, card, target, CardUseContext::default())
}

pub fn resolve_card_play_with_context(
    card_id: CardId,
    _state: &CombatState,
    _card: &CombatCard,
    target: Option<EntityId>,
    context: CardUseContext,
) -> SmallVec<[ActionInfo; 4]> {
    let t = target;
    match card_id {
        CardId::Strike => ironclad::strike::strike_play(_state, _card, t),
        CardId::StrikeG => silent::strike_green::strike_green_play(_state, _card, t),
        CardId::StrikeB => defect::strike_blue::strike_blue_play(_state, _card, t),
        CardId::StrikeP => watcher::strike_purple::strike_purple_play(_state, _card, t),
        CardId::Bash => ironclad::bash::bash_play(_state, _card, t),
        CardId::Cleave => ironclad::cleave::cleave_play(_state, _card),
        CardId::IronWave => ironclad::iron_wave::iron_wave_play(_state, _card, t),
        CardId::PerfectedStrike => {
            ironclad::perfected_strike::perfected_strike_play(_state, _card, t)
        }
        CardId::TwinStrike => ironclad::twin_strike::twin_strike_play(_state, _card, t),
        CardId::ThunderClap => ironclad::thunderclap::thunderclap_play(_state, _card),
        CardId::Defend => ironclad::defend::defend_play(_state, _card),
        CardId::DefendG => silent::defend_green::defend_green_play(_state, _card),
        CardId::DefendB => defect::defend_blue::defend_blue_play(_state, _card),
        CardId::DefendP => watcher::defend_watcher::defend_watcher_play(_state, _card),
        CardId::Eruption => watcher::eruption::eruption_play(_state, _card, t),
        CardId::Vigilance => watcher::vigilance::vigilance_play(_state, _card),
        CardId::Consecrate => watcher::consecrate::consecrate_play(_state, _card),
        CardId::BowlingBash => watcher::bowling_bash::bowling_bash_play(_state, _card, t),
        CardId::EmptyBody => watcher::empty_body::empty_body_play(_state, _card),
        CardId::EmptyFist => watcher::empty_fist::empty_fist_play(_state, _card, t),
        CardId::EmptyMind => watcher::empty_mind::empty_mind_play(_state, _card),
        CardId::JustLucky => watcher::just_lucky::just_lucky_play(_state, _card, t),
        CardId::CutThroughFate => {
            watcher::cut_through_fate::cut_through_fate_play(_state, _card, t)
        }
        CardId::ThirdEye => watcher::third_eye::third_eye_play(_state, _card),
        CardId::Prostrate => watcher::prostrate::prostrate_play(_state, _card),
        CardId::WheelKick => watcher::wheel_kick::wheel_kick_play(_state, _card, t),
        CardId::InnerPeace => watcher::inner_peace::inner_peace_play(_state, _card),
        CardId::FearNoEvil => watcher::fear_no_evil::fear_no_evil_play(_state, _card, t),
        CardId::Indignation => watcher::indignation::indignation_play(_state, _card),
        CardId::FollowUp => watcher::follow_up::follow_up_play(_state, _card, t),
        CardId::ForeignInfluence => {
            watcher::foreign_influence::foreign_influence_play(_state, _card)
        }
        CardId::LessonLearned => watcher::lesson_learned::lesson_learned_play(_state, _card, t),
        CardId::Sanctity => watcher::sanctity::sanctity_play(_state, _card),
        CardId::CrushJoints => watcher::crush_joints::crush_joints_play(_state, _card, t),
        CardId::SashWhip => watcher::sash_whip::sash_whip_play(_state, _card, t),
        CardId::Conclude => watcher::conclude::conclude_play(_state, _card),
        CardId::Brilliance => watcher::brilliance::brilliance_play(_state, _card, t),
        CardId::Ragnarok => watcher::ragnarok::ragnarok_play(_state, _card),
        CardId::WreathOfFlame => watcher::wreath_of_flame::wreath_of_flame_play(_state, _card),
        CardId::SignatureMove => watcher::signature_move::signature_move_play(_state, _card, t),
        CardId::SimmeringFury => watcher::simmering_fury::simmering_fury_play(_state, _card),
        CardId::SpiritShield => watcher::spirit_shield::spirit_shield_play(_state, _card),
        CardId::Study => watcher::study::study_play(_state, _card),
        CardId::Swivel => watcher::swivel::swivel_play(_state, _card),
        CardId::Smite => colorless::smite::smite_play(_state, _card, t),
        CardId::Safety => colorless::safety::safety_play(_state, _card),
        CardId::BecomeAlmighty | CardId::FameAndFortune | CardId::LiveForever => {
            resolve_choose_one_option(card_id, _card.upgrades, _state)
        }
        CardId::Crescendo => watcher::crescendo::crescendo_play(_state, _card),
        CardId::Tranquility => watcher::tranquility::tranquility_play(_state, _card),
        CardId::Protect => watcher::protect::protect_play(_state, _card),
        CardId::CarveReality => watcher::carve_reality::carve_reality_play(_state, _card, t),
        CardId::DeceiveReality => watcher::deceive_reality::deceive_reality_play(_state, _card),
        CardId::Evaluate => watcher::evaluate::evaluate_play(_state, _card),
        CardId::FlyingSleeves => watcher::flying_sleeves::flying_sleeves_play(_state, _card, t),
        CardId::Halt => watcher::halt::halt_play(_state, _card),
        CardId::Pray => watcher::pray::pray_play(_state, _card),
        CardId::Worship => watcher::worship::worship_play(_state, _card),
        CardId::Scrawl => watcher::scrawl::scrawl_play(_state, _card),
        CardId::MasterReality => watcher::master_reality::master_reality_play(_state, _card),
        CardId::Tantrum => watcher::tantrum::tantrum_play(_state, _card, t),
        CardId::SandsOfTime => watcher::sands_of_time::sands_of_time_play(_state, _card, t),
        CardId::Perseverance => watcher::perseverance::perseverance_play(_state, _card),
        CardId::WindmillStrike => watcher::windmill_strike::windmill_strike_play(_state, _card, t),
        CardId::Wallop => watcher::wallop::wallop_play(_state, _card, t),
        CardId::TalkToTheHand => {
            watcher::talk_to_the_hand::talk_to_the_hand_play(_state, _card, t)
        }
        CardId::BattleHymn => watcher::battle_hymn::battle_hymn_play(_state, _card),
        CardId::Devotion => watcher::devotion::devotion_play(_state, _card),
        CardId::LikeWater => watcher::like_water::like_water_play(_state, _card),
        CardId::MentalFortress => {
            watcher::mental_fortress::mental_fortress_play(_state, _card)
        }
        CardId::Rushdown => watcher::rushdown::rushdown_play(_state, _card),
        CardId::Foresight => watcher::foresight::foresight_play(_state, _card),
        CardId::Nirvana => watcher::nirvana::nirvana_play(_state, _card),
        CardId::Weave => watcher::weave::weave_play(_state, _card, t),
        CardId::ReachHeaven => watcher::reach_heaven::reach_heaven_play(_state, _card, t),
        CardId::ThroughViolence => {
            colorless::through_violence::through_violence_play(_state, _card, t)
        }
        CardId::Judgement => watcher::judgement::judgement_play(_state, _card, t),
        CardId::PressurePoints => watcher::pressure_points::pressure_points_play(_state, _card, t),
        CardId::FlurryOfBlows => watcher::flurry_of_blows::flurry_of_blows_play(_state, _card, t),
        CardId::Establishment => watcher::establishment::establishment_play(_state, _card),
        CardId::WaveOfTheHand => watcher::wave_of_the_hand::wave_of_the_hand_play(_state, _card),
        CardId::Fasting => watcher::fasting::fasting_play(_state, _card),
        CardId::DevaForm => watcher::deva_form::deva_form_play(_state, _card),
        CardId::Blasphemy => watcher::blasphemy::blasphemy_play(_state, _card),
        CardId::Collect => watcher::collect::collect_play(_state, _card),
        CardId::DeusExMachina => watcher::deus_ex_machina::deus_ex_machina_play(_state, _card),
        CardId::Alpha => watcher::alpha::alpha_play(_state, _card),
        CardId::Beta => colorless::beta::beta_play(_state, _card),
        CardId::Omega => colorless::omega::omega_play(_state, _card),
        CardId::ConjureBlade => watcher::conjure_blade::conjure_blade_play(_state, _card),
        CardId::Expunger => colorless::expunger::expunger_play(_state, _card, t),
        CardId::Meditate => watcher::meditate::meditate_play(_state, _card),
        CardId::Vault => watcher::vault::vault_play(_state, _card),
        CardId::Omniscience => watcher::omniscience::omniscience_play(_state, _card),
        CardId::Wish => watcher::wish::wish_play(_state, _card),
        CardId::Zap => defect::zap::zap_play(_state, _card),
        CardId::Dualcast => defect::dualcast::dualcast_play(_state, _card),
        CardId::BallLightning => defect::ball_lightning::ball_lightning_play(_state, _card, t),
        CardId::BeamCell => defect::beam_cell::beam_cell_play(_state, _card, t),
        CardId::ColdSnap => defect::cold_snap::cold_snap_play(_state, _card, t),
        CardId::ConserveBattery => {
            defect::conserve_battery::conserve_battery_play(_state, _card)
        }
        CardId::Coolheaded => defect::coolheaded::coolheaded_play(_state, _card),
        CardId::Leap => defect::leap::leap_play(_state, _card),
        CardId::Turbo => defect::turbo::turbo_play(_state, _card),
        CardId::SweepingBeam => defect::sweeping_beam::sweeping_beam_play(_state, _card),
        CardId::Hologram => defect::hologram::hologram_play(_state, _card),
        CardId::Stack => defect::stack::stack_play(_state, _card),
        CardId::CompileDriver => defect::compile_driver::compile_driver_play(_state, _card, t),
        CardId::Barrage => defect::barrage::barrage_play(_state, _card, t),
        CardId::GoForTheEyes => defect::go_for_the_eyes::go_for_the_eyes_play(_state, _card, t),
        CardId::Recursion => defect::recursion::recursion_play(_state, _card),
        CardId::Streamline => defect::streamline::streamline_play(_state, _card, t),
        CardId::Rebound => defect::rebound::rebound_play(_state, _card, t),
        CardId::Claw => defect::claw::claw_play(_state, _card, t),
        CardId::SteamBarrier => defect::steam_barrier::steam_barrier_play(_state, _card),
        CardId::Defragment => defect::defragment::defragment_play(_state, _card),
        CardId::Fusion => defect::fusion::fusion_play(_state, _card),
        CardId::Glacier => defect::glacier::glacier_play(_state, _card),
        CardId::Skim => defect::skim::skim_play(_state, _card),
        CardId::Overclock => defect::overclock::overclock_play(_state, _card),
        CardId::DoubleEnergy => defect::double_energy::double_energy_play(_state, _card),
        CardId::Reprogram => defect::reprogram::reprogram_play(_state, _card),
        CardId::Melter => defect::melter::melter_play(_state, _card, t),
        CardId::Ftl => defect::ftl::ftl_play(_state, _card, t),
        CardId::RipAndTear => defect::rip_and_tear::rip_and_tear_play(_state, _card),
        CardId::Tempest => defect::tempest::tempest_play(_state, _card),
        CardId::Darkness => defect::darkness::darkness_play(_state, _card),
        CardId::Aggregate => defect::aggregate::aggregate_play(_state, _card),
        CardId::AutoShields => defect::auto_shields::auto_shields_play(_state, _card),
        CardId::BootSequence => defect::boot_sequence::boot_sequence_play(_state, _card),
        CardId::Capacitor => defect::capacitor::capacitor_play(_state, _card),
        CardId::Chill => defect::chill::chill_play(_state, _card),
        CardId::ReinforcedBody => defect::reinforced_body::reinforced_body_play(_state, _card),
        CardId::DoomAndGloom => defect::doom_and_gloom::doom_and_gloom_play(_state, _card),
        CardId::Consume => defect::consume::consume_play(_state, _card),
        CardId::Sunder => defect::sunder::sunder_play(_state, _card, t),
        CardId::Chaos => defect::chaos::chaos_play(_state, _card),
        CardId::Buffer => defect::buffer::buffer_play(_state, _card),
        CardId::Equilibrium => defect::equilibrium::equilibrium_play(_state, _card),
        CardId::SelfRepair => defect::self_repair::self_repair_play(_state, _card),
        CardId::LockOn => defect::lock_on::lock_on_play(_state, _card, t),
        CardId::CoreSurge => defect::core_surge::core_surge_play(_state, _card, t),
        CardId::BiasedCognition => {
            defect::biased_cognition::biased_cognition_play(_state, _card)
        }
        CardId::Loop => defect::loop_card::loop_play(_state, _card),
        CardId::MeteorStrike => defect::meteor_strike::meteor_strike_play(_state, _card, t),
        CardId::Hyperbeam => defect::hyperbeam::hyperbeam_play(_state, _card),
        CardId::Electrodynamics => {
            defect::electrodynamics::electrodynamics_play(_state, _card)
        }
        CardId::Rainbow => defect::rainbow::rainbow_play(_state, _card),
        CardId::Reboot => defect::reboot::reboot_play(_state, _card),
        CardId::Impulse => defect::impulse::impulse_play(_state, _card),
        CardId::MachineLearning => defect::machine_learning::machine_learning_play(_state, _card),
        CardId::ForceField => defect::force_field::force_field_play(_state, _card),
        CardId::StaticDischarge => defect::static_discharge::static_discharge_play(_state, _card),
        CardId::Heatsinks => defect::heatsinks::heatsinks_play(_state, _card),
        CardId::Storm => defect::storm::storm_play(_state, _card),
        CardId::Amplify => defect::amplify::amplify_play(_state, _card),
        CardId::Fission => defect::fission::fission_play(_state, _card),
        CardId::MultiCast => defect::multi_cast::multi_cast_play(_state, _card),
        CardId::AllForOne => defect::all_for_one::all_for_one_play(_state, _card, t),
        CardId::GeneticAlgorithm => {
            defect::genetic_algorithm::genetic_algorithm_play(_state, _card)
        }
        CardId::Blizzard => defect::blizzard::blizzard_play(_state, _card),
        CardId::ThunderStrike => defect::thunder_strike::thunder_strike_play(_state, _card),
        CardId::Scrape => defect::scrape::scrape_play(_state, _card, t),
        CardId::Seek => defect::seek::seek_play(_state, _card),
        CardId::Recycle => defect::recycle::recycle_play(_state, _card),
        CardId::CreativeAI => defect::creative_ai::creative_ai_play(_state, _card),
        CardId::HelloWorld => defect::hello_world::hello_world_play(_state, _card),
        CardId::WhiteNoise => defect::white_noise::white_noise_play(_state, _card),
        CardId::EchoForm => defect::echo_form::echo_form_play(_state, _card),
        CardId::Neutralize => silent::neutralize::neutralize_play(_state, _card, t),
        CardId::Survivor => silent::survivor::survivor_play(_state, _card),
        CardId::ShrugItOff => ironclad::shrug_it_off::shrug_it_off_play(_state, _card),
        CardId::Flex => ironclad::flex::flex_play(_state, _card),
        CardId::TrueGrit => ironclad::true_grit::true_grit_play(_state, _card),
        CardId::Inflame => ironclad::inflame::inflame_play(_state, _card),
        CardId::DemonForm => ironclad::demon_form::demon_form_play(_state, _card),
        CardId::Corruption => ironclad::corruption::corruption_play(_state, _card),
        CardId::HeavyBlade => ironclad::heavy_blade::heavy_blade_play(_state, _card, t),
        CardId::Whirlwind => ironclad::whirlwind::whirlwind_play(_state, _card),
        CardId::Bloodletting => ironclad::bloodletting::bloodletting_play(_state, _card),
        CardId::Offering => ironclad::offering::offering_play(_state, _card),
        CardId::SwordBoomerang => ironclad::sword_boomerang::sword_boomerang_play(_state, _card),
        CardId::Dropkick => ironclad::dropkick::dropkick_play(_state, _card, t),
        CardId::PommelStrike => ironclad::pommel_strike::pommel_strike_play(_state, _card, t),
        CardId::Headbutt => ironclad::headbutt::headbutt_play(_state, _card, t),
        CardId::Bludgeon => ironclad::bludgeon::bludgeon_play(_state, _card, t),
        CardId::DoubleTap => ironclad::double_tap::double_tap_play(_state, _card, t),
        CardId::FeelNoPain => ironclad::feel_no_pain::feel_no_pain_play(_state, _card),
        CardId::DarkEmbrace => ironclad::dark_embrace::dark_embrace_play(_state, _card),
        CardId::Sentinel => ironclad::sentinel::sentinel_play(_state, _card),
        CardId::FiendFire => ironclad::fiend_fire::fiend_fire_play(_state, _card, t),
        CardId::SeverSoul => ironclad::sever_soul::sever_soul_play(_state, _card, t),
        CardId::SecondWind => ironclad::second_wind::second_wind_play(_state, _card),
        CardId::Exhume => ironclad::exhume::exhume_play(_state, _card),
        CardId::BurningPact => ironclad::burning_pact::burning_pact_play(_state, _card),
        CardId::Reaper => ironclad::reaper::reaper_play(_state, _card),
        CardId::Feed => ironclad::feed::feed_play(_state, _card, t),
        CardId::BloodForBlood => ironclad::blood_for_blood::blood_for_blood_play(_state, _card, t),
        CardId::Rupture => ironclad::rupture::rupture_play(_state, _card),
        CardId::Hemokinesis => ironclad::hemokinesis::hemokinesis_play(_state, _card, t),
        CardId::Combust => ironclad::combust::combust_play(_state, _card),
        CardId::Brutality => ironclad::brutality::brutality_play(_state, _card),
        CardId::LimitBreak => ironclad::limit_break::limit_break_play(_state, _card),
        CardId::SpotWeakness => ironclad::spot_weakness::spot_weakness_play(_state, _card, t),
        CardId::Barricade => ironclad::barricade::barricade_play(_state, _card),
        CardId::Entrench => ironclad::entrench::entrench_play(_state, _card),
        CardId::Juggernaut => ironclad::juggernaut::juggernaut_play(_state, _card),
        CardId::FlameBarrier => ironclad::flame_barrier::flame_barrier_play(_state, _card),
        CardId::Metallicize => ironclad::metallicize::metallicize_play(_state, _card),
        CardId::GhostlyArmor => ironclad::ghostly_armor::ghostly_armor_play(_state, _card),
        CardId::Impervious => ironclad::impervious::impervious_play(_state, _card),
        CardId::PowerThrough => ironclad::power_through::power_through_play(_state, _card),
        CardId::Evolve => ironclad::evolve::evolve_play(_state, _card),
        CardId::FireBreathing => ironclad::fire_breathing::fire_breathing_play(_state, _card),
        CardId::Immolate => ironclad::immolate::immolate_play(_state, _card),
        CardId::WildStrike => ironclad::wild_strike::wild_strike_play(_state, _card, t),
        CardId::RecklessCharge => ironclad::reckless_charge::reckless_charge_play(_state, _card, t),
        CardId::Havoc => ironclad::havoc::havoc_play(_state, _card, t),
        CardId::Warcry => ironclad::warcry::warcry_play(_state, _card),
        CardId::BattleTrance => ironclad::battle_trance::battle_trance_play(_state, _card),
        CardId::Rampage => ironclad::rampage::rampage_play(_state, _card, t),
        CardId::SearingBlow => ironclad::searing_blow::searing_blow_play(_state, _card, t),
        CardId::Anger => ironclad::anger::anger_play(_state, _card, t),
        CardId::Armaments => ironclad::armaments::armaments_play(_state, _card),
        CardId::DualWield => ironclad::dual_wield::dual_wield_play(_state, _card),
        CardId::InfernalBlade => ironclad::infernal_blade::infernal_blade_play(_state, _card),
        CardId::SeeingRed => ironclad::seeing_red::seeing_red_play(_state, _card),
        CardId::Rage => ironclad::rage::rage_play(_state, _card),
        CardId::Berserk => ironclad::berserk::berserk_play(_state, _card),
        CardId::Shockwave => ironclad::shockwave::shockwave_play(_state, _card),
        CardId::Uppercut => ironclad::uppercut::uppercut_play(_state, _card, t),
        CardId::Clothesline => ironclad::clothesline::clothesline_play(_state, _card, t),
        CardId::Disarm => ironclad::disarm::disarm_play(_state, _card, t),
        CardId::Intimidate => ironclad::intimidate::intimidate_play(_state, _card),
        CardId::Carnage => ironclad::carnage::carnage_play(_state, _card, t),
        CardId::Clash => ironclad::clash::clash_play(_state, _card, t),
        CardId::BodySlam => ironclad::body_slam::body_slam_play(_state, _card, t),
        CardId::Pummel => ironclad::pummel::pummel_play(_state, _card, t),
        CardId::Miracle => {
            let amount = if _card.upgrades > 0 { 2 } else { 1 };
            smallvec::smallvec![ActionInfo {
                action: crate::runtime::action::Action::GainEnergy { amount },
                insertion_mode: crate::runtime::action::AddTo::Bottom,
            }]
        }
        CardId::Shiv => {
            let mut actions = smallvec::SmallVec::new();
            if let Some(t) = t {
                let def = get_card_definition(CardId::Shiv);
                actions.push(ActionInfo {
                    action: crate::runtime::action::Action::Damage(
                        crate::runtime::action::DamageInfo {
                            source: 0,
                            target: t,
                            base: def.base_damage,
                            output: _card.base_damage_mut,
                            damage_type: crate::runtime::action::DamageType::Normal,
                            is_modified: _card.base_damage_mut != def.base_damage,
                        },
                    ),
                    insertion_mode: crate::runtime::action::AddTo::Bottom,
                });
            }
            actions
        }
        CardId::Bite => colorless::bite::bite_play(_state, _card, t),
        CardId::Apparition => smallvec::smallvec![ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::IntangiblePlayer,
                amount: _card.base_magic_num_mut.max(1),
            },
            insertion_mode: crate::runtime::action::AddTo::Bottom,
        }],
        CardId::DeadlyPoison => silent::deadly_poison::deadly_poison_play(_state, _card, t),
        CardId::BouncingFlask => silent::bouncing_flask::bouncing_flask_play(_state, _card),
        CardId::Catalyst => silent::catalyst::catalyst_play(_state, _card, t),
        CardId::NoxiousFumes => silent::noxious_fumes::noxious_fumes_play(_state, _card),
        CardId::Footwork => silent::footwork::footwork_play(_state, _card),
        CardId::BladeDance => silent::blade_dance::blade_dance_play(_state, _card),
        CardId::CloakAndDagger => silent::cloak_and_dagger::cloak_and_dagger_play(_state, _card),
        CardId::Backflip => silent::backflip::backflip_play(_state, _card),
        CardId::Acrobatics => silent::acrobatics::acrobatics_play(_state, _card),
        CardId::Prepared => silent::prepared::prepared_play(_state, _card),
        CardId::DaggerThrow => silent::dagger_throw::dagger_throw_play(_state, _card, t),
        CardId::PoisonedStab => silent::poisoned_stab::poisoned_stab_play(_state, _card, t),
        CardId::DaggerSpray => silent::dagger_spray::dagger_spray_play(_state, _card),
        CardId::Deflect => silent::deflect::deflect_play(_state, _card),
        CardId::QuickSlash => silent::quick_slash::quick_slash_play(_state, _card, t),
        CardId::Slice => silent::slice::slice_play(_state, _card, t),
        CardId::FlyingKnee => silent::flying_knee::flying_knee_play(_state, _card, t),
        CardId::DodgeAndRoll => silent::dodge_and_roll::dodge_and_roll_play(_state, _card),
        CardId::Doppelganger => silent::doppelganger::doppelganger_play(_state, _card),
        CardId::Distraction => silent::distraction::distraction_play(_state, _card),
        CardId::SuckerPunch => silent::sucker_punch::sucker_punch_play(_state, _card, t),
        CardId::Outmaneuver => silent::outmaneuver::outmaneuver_play(_state, _card),
        CardId::SneakyStrike => silent::sneaky_strike::sneaky_strike_play(_state, _card, t),
        CardId::Dash => silent::dash::dash_play(_state, _card, t),
        CardId::Bane => silent::bane::bane_play(_state, _card, t),
        CardId::AllOutAttack => silent::all_out_attack::all_out_attack_play(_state, _card),
        CardId::Concentrate => silent::concentrate::concentrate_play(_state, _card),
        CardId::CalculatedGamble => {
            silent::calculated_gamble::calculated_gamble_play(_state, _card)
        }
        CardId::StormOfSteel => silent::storm_of_steel::storm_of_steel_play(_state, _card),
        CardId::Unload => silent::unload::unload_play(_state, _card, t),
        CardId::PiercingWail => silent::piercing_wail::piercing_wail_play(_state, _card),
        CardId::Backstab => silent::backstab::backstab_play(_state, _card, t),
        CardId::RiddleWithHoles => {
            silent::riddle_with_holes::riddle_with_holes_play(_state, _card, t)
        }
        CardId::Skewer => silent::skewer::skewer_play(_state, _card, t),
        CardId::DieDieDie => silent::die_die_die::die_die_die_play(_state, _card),
        CardId::Finisher => silent::finisher::finisher_play(_state, _card, t),
        CardId::Flechettes => silent::flechettes::flechettes_play(_state, _card, t),
        CardId::HeelHook => silent::heel_hook::heel_hook_play(_state, _card, t),
        CardId::Expertise => silent::expertise::expertise_play(_state, _card),
        CardId::EscapePlan => silent::escape_plan::escape_plan_play(_state, _card),
        CardId::Predator => silent::predator::predator_play(_state, _card, t),
        CardId::Accuracy => silent::accuracy::accuracy_play(_state, _card),
        CardId::Caltrops => silent::caltrops::caltrops_play(_state, _card),
        CardId::InfiniteBlades => silent::infinite_blades::infinite_blades_play(_state, _card),
        CardId::Envenom => silent::envenom::envenom_play(_state, _card),
        CardId::Eviscerate => silent::eviscerate::eviscerate_play(_state, _card, t),
        CardId::MasterfulStab => silent::masterful_stab::masterful_stab_play(_state, _card, t),
        CardId::Blur => silent::blur::blur_play(_state, _card),
        CardId::Choke => silent::choke::choke_play(_state, _card, t),
        CardId::CripplingPoison => silent::crippling_poison::crippling_poison_play(_state, _card),
        CardId::CorpseExplosion => {
            silent::corpse_explosion::corpse_explosion_play(_state, _card, t)
        }
        CardId::LegSweep => silent::leg_sweep::leg_sweep_play(_state, _card, t),
        CardId::Malaise => silent::malaise::malaise_play(_state, _card, t),
        CardId::Terror => silent::terror::terror_play(_state, _card, t),
        CardId::EndlessAgony => silent::endless_agony::endless_agony_play(_state, _card, t),
        CardId::GlassKnife => silent::glass_knife::glass_knife_play(_state, _card, t),
        CardId::GrandFinale => silent::grand_finale::grand_finale_play(_state, _card),
        CardId::Setup => silent::setup::setup_play(_state, _card),
        CardId::WraithForm => silent::wraith_form::wraith_form_play(_state, _card),
        CardId::PhantasmalKiller => {
            silent::phantasmal_killer::phantasmal_killer_play(_state, _card)
        }
        CardId::ToolsOfTheTrade => {
            silent::tools_of_the_trade::tools_of_the_trade_play(_state, _card)
        }
        CardId::WellLaidPlans => silent::well_laid_plans::well_laid_plans_play(_state, _card),
        CardId::Nightmare => silent::nightmare::nightmare_play(_state, _card),
        CardId::Adrenaline => silent::adrenaline::adrenaline_play(_state, _card),
        CardId::AfterImage => silent::after_image::after_image_play(_state, _card),
        CardId::Alchemize => silent::alchemize::alchemize_play(_state, _card),
        CardId::AThousandCuts => silent::a_thousand_cuts::a_thousand_cuts_play(_state, _card),
        CardId::BulletTime => silent::bullet_time::bullet_time_play(_state, _card),
        CardId::Burst => silent::burst::burst_play(_state, _card),
        CardId::Pride => smallvec::smallvec![], // Coast 1 but does nothing on play
        CardId::BandageUp => colorless::bandage_up::bandage_up_play(_state, _card),
        CardId::Finesse => colorless::finesse::finesse_play(_state, _card),
        CardId::Blind => colorless::blind::blind_play(_state, _card, t),
        CardId::FlashOfSteel => colorless::flash_of_steel::flash_of_steel_play(_state, _card, t),
        CardId::GoodInstincts => colorless::good_instincts::good_instincts_play(_state, _card),
        CardId::Panacea => colorless::panacea::panacea_play(_state, _card),
        CardId::PanicButton => colorless::panic_button::panic_button_play(_state, _card),
        CardId::SwiftStrike => colorless::swift_strike::swift_strike_play(_state, _card, t),
        CardId::Trip => colorless::trip::trip_play(_state, _card, t),
        CardId::DeepBreath => colorless::deep_breath::deep_breath_play(_state, _card),
        CardId::Discovery => colorless::discovery::discovery_play(_state, _card),
        CardId::DramaticEntrance => {
            colorless::dramatic_entrance::dramatic_entrance_play(_state, _card)
        }
        CardId::Forethought => colorless::forethought::forethought_play(_state, _card),
        CardId::Impatience => colorless::impatience::impatience_play(_state, _card),
        CardId::JackOfAllTrades => {
            colorless::jack_of_all_trades::jack_of_all_trades_play(_state, _card)
        }
        CardId::MindBlast => colorless::mind_blast::mind_blast_play(_state, _card, t),
        CardId::Purity => colorless::purity::purity_play(_state, _card),
        CardId::MasterOfStrategy => {
            colorless::master_of_strategy::master_of_strategy_play(_state, _card)
        }
        CardId::Insight => colorless::insight::insight_play(_state, _card),
        CardId::ThinkingAhead => {
            colorless::thinking_ahead::thinking_ahead_play(_state, _card, context)
        }
        CardId::Violence => colorless::violence::violence_play(_state, _card),
        CardId::DarkShackles => colorless::dark_shackles::dark_shackles_play(_state, _card, t),
        CardId::Enlightenment => colorless::enlightenment::enlightenment_play(_state, _card),
        CardId::JAX => colorless::jax::jax_play(_state, _card),
        CardId::Apotheosis => colorless::apotheosis::apotheosis_play(_state, _card),
        CardId::Chrysalis => colorless::chrysalis::chrysalis_play(_state, _card),
        CardId::Magnetism => colorless::magnetism::magnetism_play(_state, _card),
        CardId::Mayhem => colorless::mayhem::mayhem_play(_state, _card),
        CardId::Metamorphosis => colorless::metamorphosis::metamorphosis_play(_state, _card),
        CardId::Panache => colorless::panache::panache_play(_state, _card),
        CardId::SadisticNature => colorless::sadistic_nature::sadistic_nature_play(_state, _card),
        CardId::TheBomb => colorless::the_bomb::the_bomb_play(_state, _card),
        CardId::HandOfGreed => colorless::hand_of_greed::hand_of_greed_play(_state, _card, t),
        CardId::RitualDagger => colorless::ritual_dagger::ritual_dagger_play(_state, _card, t),
        CardId::SecretTechnique => {
            colorless::secret_technique::secret_technique_play(_state, _card)
        }
        CardId::SecretWeapon => colorless::secret_weapon::secret_weapon_play(_state, _card),
        CardId::Transmutation => colorless::transmutation::transmutation_play(_state, _card),
        // Unplayable stubs — curses, status, and special cards
        CardId::Wound
        | CardId::Burn
        | CardId::Dazed
        | CardId::Slimed
        | CardId::Parasite
        | CardId::Void
        | CardId::Regret
        | CardId::AscendersBane
        | CardId::Clumsy
        | CardId::CurseOfTheBell
        | CardId::Decay
        | CardId::Doubt
        | CardId::Injury
        | CardId::Necronomicurse
        | CardId::Normality
        | CardId::Pain
        | CardId::Shame
        | CardId::Writhe => smallvec::smallvec![], // Unplayable / Stub
        CardId::Reflex => silent::reflex::reflex_play(_state, _card),
        CardId::Tactician => silent::tactician::tactician_play(_state, _card),
        CardId::Madness => smallvec::smallvec![ActionInfo {
            action: Action::Madness,
            insertion_mode: crate::runtime::action::AddTo::Bottom,
        }],
    }
}

/// Evaluates a card's damage, block, and magic number based on player powers, target powers, and specific card rules.
/// Maps directly to Java Spire's `applyPowers()` (when target is None) and `calculateCardDamage()` (when target is Some).
pub fn evaluate_card(card: &mut CombatCard, state: &CombatState, target: Option<EntityId>) {
    let def = get_card_definition(card.id);
    let u = if card.upgrades > 0 { 1 } else { 0 };
    let base_numbers = mechanics::card_persistent_base_numbers_v1(card);
    let mut damage = base_numbers.damage as f32;
    let mut block = base_numbers.block as f32;
    card.base_magic_num_mut = def.base_magic + u * def.upgrade_magic;
    let mut halt_wrath_block = if card.id == CardId::Halt {
        Some(card.base_magic_num_mut as f32)
    } else {
        None
    };

    if card.id == CardId::Wish {
        card.base_damage_mut = damage.max(0.0) as i32;
        card.base_block_mut = block.max(0.0) as i32;
        card.multi_damage.clear();
        return;
    }

    // 1. Card specific base overrides (Perfected Strike)
    if card.id == CardId::PerfectedStrike {
        let mut strike_count = 0;
        let is_strike = |id| crate::content::cards::card_has_tag(id, CardTag::Strike);

        for c in &state.zones.hand {
            if is_strike(c.id) && c.lineage != card.lineage {
                strike_count += 1;
            }
        }
        for c in &state.zones.draw_pile {
            if is_strike(c.id) && c.lineage != card.lineage {
                strike_count += 1;
            }
        }
        for c in &state.zones.discard_pile {
            if is_strike(c.id) && c.lineage != card.lineage {
                strike_count += 1;
            }
        }
        // Count the card itself definitively once
        if is_strike(card.id) {
            strike_count += 1;
        }

        damage += (card.base_magic_num_mut as f32) * (strike_count as f32);
    } else if card.id == CardId::BodySlam {
        damage = state.entities.player.block as f32;
    } else if card.id == CardId::MindBlast {
        // Java Mind Blast refreshes its base damage from the current draw pile
        // in applyPowers/calculateCardDamage. Keep the shared evaluation path
        // truthful so public action projections and actual play use the same
        // dynamic value.
        damage = state.zones.draw_pile.len() as f32;
    } else if card.id == CardId::SpiritShield {
        let other_hand_cards = state
            .zones
            .hand
            .iter()
            .filter(|hand_card| hand_card.lineage != card.lineage)
            .count() as i32;
        block = (other_hand_cards * card.base_magic_num_mut).max(0) as f32;
    } else if card.id == CardId::Brilliance {
        let mantra_gained = state.turn.counters.mantra_gained_this_combat.max(0);
        card.base_magic_num_mut = mantra_gained;
        damage += mantra_gained as f32;
    } else if card.id == CardId::Expunger {
        card.base_magic_num_mut = card.misc_value.max(0);
    } else if card.id == CardId::Stack {
        block = state.zones.discard_pile.len() as f32;
        if card.upgrades > 0 {
            block += 3.0;
        }
    }

    // 2. Relic atDamageModify hooks (Java: AbstractCard.applyPowers/calculateCardDamage)
    // run before player power atDamageGive hooks. This ordering matters for cases like
    // Strike Dummy under Weak: (base + 3) * 0.75, not base * 0.75 + 3.
    damage =
        crate::content::relics::hooks::modify_player_attack_damage_for_card(state, card, damage);

    // 3. Player Powers
    if let Some(powers) = crate::content::powers::store::powers_for(state, 0) {
        for power in powers {
            damage = crate::content::powers::resolve_power_on_calculate_damage_to_enemy(
                power.power_type,
                state,
                card,
                damage,
                power.amount,
            );
            block = crate::content::powers::resolve_power_on_calculate_block(
                power.power_type,
                state,
                card,
                block,
                power.amount,
            );
            if let Some(wrath_block) = &mut halt_wrath_block {
                *wrath_block = crate::content::powers::resolve_power_on_calculate_block(
                    power.power_type,
                    state,
                    card,
                    *wrath_block,
                    power.amount,
                );
            }
        }
    }

    // 4. Stance
    if def.card_type == crate::content::cards::CardType::Attack {
        match state.entities.player.stance {
            crate::runtime::combat::StanceId::Wrath => damage *= 2.0,
            crate::runtime::combat::StanceId::Divinity => damage *= 3.0,
            _ => {}
        }
    }
    // 5. Target Powers
    if def.is_multi_damage {
        card.multi_damage.clear();
        for m in &state.entities.monsters {
            let mut mdmg = damage;
            // Target specific powers (Vulnerable)
            if let Some(target_powers) = crate::content::powers::store::powers_for(state, m.id) {
                for power in target_powers {
                    mdmg = crate::content::powers::resolve_power_on_calculate_damage_from_player(
                        power.power_type,
                        state,
                        card,
                        m.id,
                        mdmg,
                        power.amount,
                    );
                }
                let mut final_receive = mdmg.max(0.0).floor() as i32;
                for power in target_powers {
                    final_receive = crate::content::powers::resolve_power_at_damage_final_receive(
                        power.power_type,
                        final_receive,
                        power.amount,
                        crate::runtime::action::DamageType::Normal,
                    );
                }
                mdmg = final_receive as f32;
            }
            if mdmg < 0.0 {
                mdmg = 0.0;
            }
            card.multi_damage.push(mdmg as i32);
        }
        if let Some(first) = card.multi_damage.first() {
            damage = *first as f32;
        }
    } else if let Some(target_id) = target {
        if let Some(target_powers) = crate::content::powers::store::powers_for(state, target_id) {
            for power in target_powers {
                damage = crate::content::powers::resolve_power_on_calculate_damage_from_player(
                    power.power_type,
                    state,
                    card,
                    target_id,
                    damage,
                    power.amount,
                );
            }
            let mut final_receive = damage.max(0.0).floor() as i32;
            for power in target_powers {
                final_receive = crate::content::powers::resolve_power_at_damage_final_receive(
                    power.power_type,
                    final_receive,
                    power.amount,
                    crate::runtime::action::DamageType::Normal,
                );
            }
            damage = final_receive as f32;
        }
    }

    if damage < 0.0 {
        damage = 0.0;
    }
    if block < 0.0 {
        block = 0.0;
    }

    card.base_damage_mut = damage as i32;
    card.base_block_mut = block as i32;
    if let Some(wrath_block) = halt_wrath_block {
        card.base_magic_num_mut = wrath_block.max(0.0) as i32;
    }
}

/// Produces a freshly evaluated combat card for actual play execution.
///
/// This avoids relying on potentially stale cached mutation fields on the card
/// object when generating execution-time actions.
pub fn evaluate_card_for_play(
    card: &CombatCard,
    state: &CombatState,
    target: Option<EntityId>,
) -> CombatCard {
    let mut evaluated = card.clone();
    evaluate_card(&mut evaluated, state, target);
    evaluated
}

/// Creates a fresh combat card using Java `makeCopy()` state hooks that depend
/// on the current combat.
///
/// Most cards return a plain new instance. `Blood for Blood` is an important
/// exception: its Java `makeCopy()` immediately applies the player's
/// `damagedThisCombat` cost reduction, which matters for random-card sources
/// such as Dead Branch, Nilry's Codex, Discovery, Infernal Blade, and
/// Metamorphosis.
pub fn make_fresh_card_copy_for_combat(
    card_id: CardId,
    lineage: crate::runtime::combat::CardLineageId,
    state: &CombatState,
) -> CombatCard {
    let mut card = CombatCard::with_lineage(card_id, lineage);
    if card_id == CardId::BloodForBlood {
        let damaged = state.turn.counters.times_damaged_this_combat as i32;
        card.update_cost_java(-damaged);
    }
    configure_costs_on_new_card(&mut card, state);
    card
}

pub fn configure_costs_on_new_card(card: &mut CombatCard, state: &CombatState) {
    if card.id == CardId::ForceField {
        let powers_played = state
            .turn
            .counters
            .card_ids_played_this_combat
            .iter()
            .filter(|&&id| get_card_definition(id).card_type == CardType::Power)
            .count() as i32;
        if powers_played > 0 {
            card.update_cost_java(-powers_played);
        }
    }
}

pub fn trigger_cards_on_card_played(played_card: &CombatCard, state: &mut CombatState) {
    if get_card_definition(played_card.id).card_type != CardType::Power {
        return;
    }

    for card in state
        .zones
        .hand
        .iter_mut()
        .chain(state.zones.discard_pile.iter_mut())
        .chain(state.zones.draw_pile.iter_mut())
    {
        if card.id == CardId::ForceField {
            card.update_cost_java(-1);
        }
    }
}

pub fn class_combat_card_pool_for_type(
    player_class: &str,
    card_type: Option<CardType>,
) -> Vec<CardId> {
    let mut pool = Vec::new();
    for &rarity in &[CardRarity::Common, CardRarity::Uncommon, CardRarity::Rare] {
        let rarity_pool = match player_class {
            "Ironclad" => ironclad_pool_for_rarity(rarity),
            "Silent" => silent_pool_for_rarity(rarity),
            "Defect" => defect_pool_for_rarity(rarity),
            "Watcher" => watcher_pool_for_rarity(rarity),
            _ => ironclad_pool_for_rarity(rarity),
        };
        // AbstractDungeon.initializeCardPools partitions CardLibrary's
        // HashMap-ordered tmpPool with CardGroup.addToTop(), reversing each
        // rarity subsequence before copying it into src*CardPool.
        for &id in rarity_pool.iter().rev() {
            let def = get_card_definition(id);
            if def.tags.contains(&CardTag::Healing) {
                continue;
            }
            if let Some(card_type) = card_type {
                if def.card_type != card_type {
                    continue;
                }
            }
            pool.push(id);
        }
    }
    pool
}

pub fn make_random_class_card_copy_for_combat(
    state: &mut CombatState,
    card_type: Option<CardType>,
) -> Option<CardBlueprint> {
    let pool = class_combat_card_pool_for_type(state.meta.player_class.as_str(), card_type);
    if pool.is_empty() {
        return None;
    }

    let idx = state.rng.card_random_rng.random(pool.len() as i32 - 1) as usize;
    Some(make_fresh_card_blueprint_for_combat(pool[idx], state))
}

pub fn make_fresh_card_blueprint_for_combat(
    card_id: CardId,
    state: &CombatState,
) -> CardBlueprint {
    let mut card = CardBlueprint::fresh(card_id);
    if card_id == CardId::BloodForBlood {
        card.update_cost_java(-(state.turn.counters.times_damaged_this_combat as i32));
    }
    if card_id == CardId::ForceField {
        let powers_played = state
            .turn
            .counters
            .card_ids_played_this_combat
            .iter()
            .filter(|&&id| get_card_definition(id).card_type == CardType::Power)
            .count() as i32;
        if powers_played > 0 {
            card.update_cost_java(-powers_played);
        }
    }
    card
}

pub fn can_upgrade_card_once(card: &CombatCard) -> bool {
    can_upgrade_card_once_for(card.id, card.upgrades)
}

pub fn can_upgrade_card_once_for(card_id: CardId, upgrades: u8) -> bool {
    if card_id == CardId::SearingBlow {
        return true;
    }
    let def = get_card_definition(card_id);
    upgrades == 0 && def.card_type != CardType::Status && def.card_type != CardType::Curse
}

pub fn upgrade_card_once_java(card: &mut CombatCard) -> bool {
    if !can_upgrade_card_once(card) {
        return false;
    }

    apply_card_cost_upgrade_java(
        card.id,
        card.combat_cost_without_turn_override_java(),
        &mut card.cost_modifier,
        &mut card.cost_for_turn,
    );
    let def = get_card_definition(card.id);
    if let Some(base_damage) = &mut card.base_damage_override {
        *base_damage += def.upgrade_damage;
    }
    if let Some(base_block) = &mut card.base_block_override {
        *base_block += def.upgrade_block;
    }
    card.upgrades += 1;
    true
}

/// Applies Java Master Reality upgrade semantics to generated cards.
///
/// Java routes generated cards through action constructors and card-manipulation
/// effects. Those are UI classes, but they also call `upgrade()`. Normal cards
/// ignore the second call after the first upgrade; `Searing Blow` does not, so
/// callers must pass the number of Java upgrade call sites for that path.
pub fn apply_master_reality_to_generated_card(
    card: &mut CombatCard,
    state: &CombatState,
    upgrade_call_sites: u8,
) {
    let def = get_card_definition(card.id);
    if def.card_type == CardType::Curse || def.card_type == CardType::Status {
        return;
    }
    if !crate::content::powers::store::has_power(
        state,
        0,
        crate::content::powers::PowerId::MasterRealityPower,
    ) {
        return;
    }
    for _ in 0..upgrade_call_sites {
        upgrade_card_once_java(card);
    }
}

/// Applies the mechanical part of Java `MakeTempCardInHandAction` construction
/// to a concrete generated card.
///
/// Java calls `upgrade()` for Master Reality in the action constructor, before
/// the action later resolves through `ShowCardAndAddToHandEffect` or
/// `ShowCardAndAddToDiscardEffect`. Rust paths that have already materialized a
/// concrete card should pass it through this helper before queuing
/// `Action::MakeConstructedCopyInHand`.
pub fn prepare_make_temp_card_in_hand_constructor(
    mut card: CombatCard,
    state: &CombatState,
) -> CardBlueprint {
    apply_master_reality_to_generated_card(&mut card, state, 1);
    CardBlueprint::from_stat_equivalent(&card)
}

pub fn prepare_make_temp_card_blueprint_in_hand_constructor(
    mut card: CardBlueprint,
    state: &CombatState,
) -> CardBlueprint {
    apply_master_reality_to_generated_blueprint(&mut card, state, 1);
    card
}

pub fn make_constructed_temp_card_in_hand_action(
    card_id: CardId,
    amount: u8,
    upgraded: bool,
    state: &CombatState,
) -> crate::runtime::action::Action {
    let mut card = make_fresh_card_blueprint_for_combat(card_id, state);
    if upgraded {
        upgrade_card_blueprint_once_java(&mut card);
    }
    let card = prepare_make_temp_card_blueprint_in_hand_constructor(card, state);
    crate::runtime::action::Action::MakeConstructedCopyInHand {
        original: Box::new(card),
        amount,
    }
}

pub fn upgrade_card_blueprint_once_java(card: &mut CardBlueprint) -> bool {
    let definition = get_card_definition(card.id);
    if card.id != CardId::SearingBlow
        && (card.upgrades > 0
            || definition.card_type == CardType::Status
            || definition.card_type == CardType::Curse)
    {
        return false;
    }
    apply_card_cost_upgrade_java(
        card.id,
        card.combat_cost_without_turn_override_java(),
        &mut card.cost_modifier,
        &mut card.cost_for_turn,
    );
    if let Some(base_damage) = &mut card.base_damage_override {
        *base_damage += definition.upgrade_damage;
    }
    if let Some(base_block) = &mut card.base_block_override {
        *base_block += definition.upgrade_block;
    }
    card.upgrades += 1;
    true
}

/// Java AbstractCard.upgradeBaseCost replaces the current cost rather than
/// retaining prior cost modifiers. BloodForBlood requests a relative decrease
/// below its printed cost and clamps cost only after the turn-cost update.
/// Both materialized cards and identity-free blueprints use this rule.
fn apply_card_cost_upgrade_java(
    id: CardId,
    previous_cost: i32,
    cost_modifier: &mut i8,
    cost_for_turn: &mut Option<u8>,
) {
    let Some(upgraded_base_cost) = upgraded_base_cost_override_for(id, 1) else {
        return;
    };
    let requested_cost = if id == CardId::BloodForBlood && previous_cost < 4 {
        previous_cost - 1
    } else {
        i32::from(upgraded_base_cost)
    };
    *cost_modifier = (requested_cost.max(0) - i32::from(upgraded_base_cost)) as i8;
    if let Some(turn_cost) = cost_for_turn.as_mut().filter(|cost| **cost > 0) {
        let difference = i32::from(*turn_cost) - previous_cost;
        *turn_cost = (requested_cost + difference).clamp(0, i32::from(u8::MAX)) as u8;
    }
}

pub fn apply_master_reality_to_generated_blueprint(
    card: &mut CardBlueprint,
    state: &CombatState,
    upgrade_call_sites: u8,
) {
    let definition = get_card_definition(card.id);
    if definition.card_type == CardType::Curse || definition.card_type == CardType::Status {
        return;
    }
    if !crate::content::powers::store::has_power(
        state,
        0,
        crate::content::powers::PowerId::MasterRealityPower,
    ) {
        return;
    }
    for _ in 0..upgrade_call_sites {
        upgrade_card_blueprint_once_java(card);
    }
}

/// Returns the card's intrinsic exhaust-on-play behavior after applying
/// upgrade-sensitive card rules.
pub fn exhausts_when_played(card: &CombatCard) -> bool {
    mechanics::card_exhausts_when_played(card.id, card.upgrades)
}

/// Returns the card's effective ethereal status after upgrade-sensitive overrides.
pub fn is_ethereal(card: &CombatCard) -> bool {
    mechanics::card_is_ethereal(card.id, card.upgrades)
}

pub fn is_self_retain(card: &CombatCard) -> bool {
    match card.id {
        CardId::Miracle
        | CardId::Insight
        | CardId::Smite
        | CardId::Safety
        | CardId::Crescendo
        | CardId::Tranquility
        | CardId::Protect
        | CardId::FlyingSleeves
        | CardId::ThroughViolence
        | CardId::SandsOfTime
        | CardId::Perseverance
        | CardId::WindmillStrike => true,
        CardId::Worship => card.upgrades > 0,
        CardId::Blasphemy => card.upgrades > 0,
        _ => false,
    }
}

pub fn shuffle_back_into_draw_pile_when_played(card: &CombatCard) -> bool {
    matches!(card.id, CardId::Tantrum)
}

pub fn trigger_on_retained(card: &mut CombatCard) {
    let def = get_card_definition(card.id);
    let upgraded = i32::from(card.upgrades > 0);
    match card.id {
        CardId::SandsOfTime => {
            card.modify_cost_for_combat_java(-1);
        }
        CardId::Perseverance => {
            let base = def.base_block + upgraded * def.upgrade_block;
            let current = card.base_block_override.unwrap_or(base);
            let gain = def.base_magic + upgraded * def.upgrade_magic;
            card.base_block_override = Some(current + gain);
        }
        CardId::WindmillStrike => {
            let base = def.base_damage + upgraded * def.upgrade_damage;
            let current = card.base_damage_override.unwrap_or(base);
            let gain = def.base_magic + upgraded * def.upgrade_magic;
            card.base_damage_override = Some(current + gain);
        }
        _ => {}
    }
}

pub fn upgraded_base_cost_override(card: &CombatCard) -> Option<i8> {
    upgraded_base_cost_override_for(card.id, card.upgrades)
}

/// Returns the upgrade-sensitive printed cost without requiring a runtime
/// card instance or inventing a card lineage for a hypothetical candidate.
pub fn upgraded_base_cost_override_for(card: CardId, upgrades: u8) -> Option<i8> {
    if upgrades == 0 {
        return None;
    }
    match card {
        CardId::Alchemize => Some(0),
        CardId::Apotheosis => Some(1),
        CardId::Barricade => Some(2),
        CardId::BloodForBlood => Some(3),
        CardId::BodySlam => Some(0),
        CardId::BulletTime => Some(2),
        CardId::Corruption => Some(2),
        CardId::CreativeAI => Some(2),
        CardId::DarkEmbrace => Some(1),
        CardId::Distraction => Some(0),
        CardId::Envenom => Some(1),
        CardId::Entrench => Some(1),
        CardId::Exhume => Some(0),
        CardId::Havoc => Some(0),
        CardId::InfernalBlade => Some(0),
        CardId::Madness => Some(0),
        CardId::Magnetism | CardId::Mayhem | CardId::MindBlast => Some(1),
        CardId::Nightmare => Some(2),
        CardId::PhantasmalKiller => Some(0),
        CardId::Recycle => Some(0),
        CardId::SeeingRed => Some(0),
        CardId::Setup => Some(0),
        CardId::Study => Some(1),
        CardId::Terror => Some(0),
        CardId::ToolsOfTheTrade => Some(0),
        CardId::WhiteNoise => Some(0),
        CardId::Dualcast => Some(0),
        CardId::Zap => Some(0),
        CardId::Recursion => Some(0),
        CardId::Fusion => Some(1),
        CardId::DoubleEnergy => Some(0),
        CardId::Eruption => Some(1),
        CardId::Crescendo => Some(0),
        CardId::Tranquility => Some(0),
        CardId::Scrawl => Some(0),
        CardId::MasterReality => Some(0),
        CardId::Rushdown => Some(0),
        CardId::Beta => Some(1),
        CardId::Vault => Some(2),
        CardId::Omniscience => Some(3),
        _ => None,
    }
}

pub fn effective_target(card: &CombatCard) -> CardTarget {
    mechanics::effective_card_target(card.id, card.upgrades)
}

/// Validates whether a card can be played based on energy, status locks, and curses like Normality.
pub fn can_play_card(card: &CombatCard, state: &CombatState) -> Result<(), &'static str> {
    can_play_card_internal(card, state, false)
}

/// Java queued/autoplay cards still call `canUse`, but `isInAutoplay` only
/// bypasses the final energy-count check inside `hasEnoughEnergy`.
pub fn can_play_card_ignoring_energy(
    card: &CombatCard,
    state: &CombatState,
) -> Result<(), &'static str> {
    can_play_card_internal(card, state, true)
}

fn can_play_card_internal(
    card: &CombatCard,
    state: &CombatState,
    ignore_energy: bool,
) -> Result<(), &'static str> {
    // Curse: Normality Lock
    if state.zones.hand.iter().any(|c| c.id == CardId::Normality)
        && state.turn.counters.cards_played_this_turn >= 3 {
            return Err("Normality: Cannot play more than 3 cards this turn.");
        }

    let def = crate::content::cards::get_card_definition(card.id);
    let cost = card.get_cost();

    // In Slay the Spire, internally cost -2 means the card is Unplayable.
    if cost < -1 {
        if def.card_type == crate::content::cards::CardType::Curse
            && state
                .entities
                .player
                .has_relic(crate::content::relics::RelicId::BlueCandle)
        {
            // Blue Candle override
        } else if def.card_type == crate::content::cards::CardType::Status
            && state
                .entities
                .player
                .has_relic(crate::content::relics::RelicId::MedicalKit)
        {
            // Medical Kit override
        } else {
            return Err("Card is unplayable.");
        }
    }

    // Java: hasEnoughEnergy() — Power.canPlayCard() hook
    // Iterates all player powers; if any returns false, card cannot be played.
    if let Some(player_powers) = crate::content::powers::store::powers_for(state, 0) {
        for ps in player_powers {
            if !crate::content::powers::resolve_power_can_play_card(ps.power_type, card) {
                return Err("A power prevents playing this card.");
            }
        }
    }

    // Java: hasEnoughEnergy() — Entangled hardcode (L857-860)
    // This is separate from the canPlayCard hook; Java checks it explicitly.
    if let Some(player_powers) = crate::content::powers::store::powers_for(state, 0) {
        if player_powers
            .iter()
            .any(|p| p.power_type == crate::content::powers::PowerId::Entangle)
            && def.card_type == crate::content::cards::CardType::Attack
        {
            return Err("Entangled: Cannot play Attacks this turn.");
        }
    }

    // AbstractCard.hasEnoughEnergy calls Relic.canPlay even for autoplay;
    // isInAutoplay bypasses only the final energy check. Use the relic's own
    // counter, not GameActionManager's independent cards-played history.
    for relic in &state.entities.player.relics {
        if relic.id == crate::content::relics::RelicId::VelvetChoker
            && !crate::content::relics::velvet_choker::can_play_card(relic.counter)
        {
            return Err("VelvetChoker: card play limit reached (6)");
        }
    }

    // Card-specific overrides (Java: card.canUse overrides)
    match card.id {
        CardId::Clash => {
            let has_non_attack = state.zones.hand.iter().any(|c| {
                let d = crate::content::cards::get_card_definition(c.id);
                d.card_type != crate::content::cards::CardType::Attack
            });
            if has_non_attack {
                return Err("Can only play Clash if every card in your hand is an Attack.");
            }
        }
        CardId::SecretTechnique => {
            let has_skill = state
                .zones
                .draw_pile
                .iter()
                .any(|c| get_card_definition(c.id).card_type == CardType::Skill);
            if !has_skill {
                return Err("No Skill cards in draw pile.");
            }
        }
        CardId::SecretWeapon => {
            let has_attack = state
                .zones
                .draw_pile
                .iter()
                .any(|c| get_card_definition(c.id).card_type == CardType::Attack);
            if !has_attack {
                return Err("No Attack cards in draw pile.");
            }
        }
        CardId::GrandFinale => {
            if !state.zones.draw_pile.is_empty() {
                return Err("Can only play Grand Finale if the draw pile is empty.");
            }
        }
        CardId::SignatureMove => {
            let has_other_attack = state.zones.hand.iter().any(|c| {
                c.lineage != card.lineage
                    && get_card_definition(c.id).card_type == CardType::Attack
            });
            if has_other_attack {
                return Err("Can only play Signature Move if there are no other Attacks in hand.");
            }
        }
        _ => {}
    }

    // Default cost validation
    let free_attack_power_applies = def.card_type == CardType::Attack
        && crate::content::powers::store::power_amount(
            state,
            0,
            PowerId::FreeAttackPower,
        ) > 0;
    if !ignore_energy
        && !card.free_to_play_once
        && !free_attack_power_applies
        && cost >= 0
        && state.turn.energy < (cost as u8)
    {
        return Err("Not enough energy.");
    }

    Ok(())
}

pub fn resolve_choose_one_option(
    card_id: CardId,
    upgrades: u8,
    _state: &CombatState,
) -> SmallVec<[ActionInfo; 4]> {
    let definition = get_card_definition(card_id);
    let magic = definition.base_magic
        + i32::from(upgrades > 0) * definition.upgrade_magic;
    match card_id {
        CardId::BecomeAlmighty => smallvec::smallvec![ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Strength,
                amount: magic,
            },
            insertion_mode: AddTo::Bottom,
        }],
        CardId::FameAndFortune => smallvec::smallvec![ActionInfo {
            action: Action::GainGold {
                amount: magic,
            },
            insertion_mode: AddTo::Bottom,
        }],
        CardId::LiveForever => smallvec::smallvec![ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::PlatedArmor,
                amount: magic,
            },
            insertion_mode: AddTo::Bottom,
        }],
        _ => smallvec::smallvec![],
    }
}

/// A global hook called immediately after a card is played to aggregate passive triggers from the state (e.g. Curses).
pub fn on_play_card(played_card: &CombatCard, state: &CombatState) -> SmallVec<[ActionInfo; 4]> {
    let mut passive_actions = smallvec::SmallVec::new();

    // Curse: Pain (Lose 1 HP for every card played)
    for card in &state.zones.hand {
        if card.id == CardId::Pain && card.lineage != played_card.lineage {
            passive_actions.push(crate::content::cards::curses::pain::on_other_card_played());
        }
    }

    // Other hooks (Time Eater, Velvet Choker, etc.) can be placed here later.

    passive_actions
}

pub fn resolve_card_on_manual_discard(
    card: &CombatCard,
    _state: &CombatState,
) -> SmallVec<[ActionInfo; 4]> {
    match card.id {
        CardId::Reflex => silent::reflex::reflex_manual_discard(card),
        CardId::Tactician => silent::tactician::tactician_manual_discard(card),
        _ => smallvec::smallvec![],
    }
}
