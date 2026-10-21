use super::*;
use crate::content::monsters::EnemyId;
use crate::content::powers::PowerId;
use crate::runtime::action::{Action, ActionInfo, AddTo, DamageInfo, DamageType, NO_SOURCE};
use crate::runtime::combat::{
    CombatCard, DrawnCardRecord, Intent, OrbEntity, OrbId, Power, PowerPayload, StanceId,
};

#[test]
fn ironclad_starter_basic_definitions_match_java_sources() {
    let strike = get_card_definition(CardId::Strike);
    assert_eq!(strike.name, "Strike");
    assert_eq!(strike.card_type, CardType::Attack);
    assert_eq!(strike.rarity, CardRarity::Basic);
    assert_eq!(strike.cost, 1);
    assert_eq!(strike.base_damage, 6);
    assert_eq!(strike.base_block, 0);
    assert_eq!(strike.base_magic, 0);
    assert_eq!(strike.target, CardTarget::Enemy);
    assert_eq!(strike.upgrade_damage, 3);
    assert_eq!(strike.upgrade_block, 0);
    assert_eq!(strike.upgrade_magic, 0);
    assert!(strike.tags.contains(&CardTag::Strike));
    assert!(strike.tags.contains(&CardTag::StarterStrike));

    let defend = get_card_definition(CardId::Defend);
    assert_eq!(defend.name, "Defend");
    assert_eq!(defend.card_type, CardType::Skill);
    assert_eq!(defend.rarity, CardRarity::Basic);
    assert_eq!(defend.cost, 1);
    assert_eq!(defend.base_damage, 0);
    assert_eq!(defend.base_block, 5);
    assert_eq!(defend.base_magic, 0);
    assert_eq!(defend.target, CardTarget::SelfTarget);
    assert_eq!(defend.upgrade_damage, 0);
    assert_eq!(defend.upgrade_block, 3);
    assert_eq!(defend.upgrade_magic, 0);
    assert!(defend.tags.contains(&CardTag::StarterDefend));

    let bash = get_card_definition(CardId::Bash);
    assert_eq!(bash.name, "Bash");
    assert_eq!(bash.card_type, CardType::Attack);
    assert_eq!(bash.rarity, CardRarity::Basic);
    assert_eq!(bash.cost, 2);
    assert_eq!(bash.base_damage, 8);
    assert_eq!(bash.base_block, 0);
    assert_eq!(bash.base_magic, 2);
    assert_eq!(bash.target, CardTarget::Enemy);
    assert_eq!(bash.upgrade_damage, 2);
    assert_eq!(bash.upgrade_block, 0);
    assert_eq!(bash.upgrade_magic, 1);
    assert!(bash.tags.is_empty());
}

#[test]
fn ironclad_starter_basic_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let strike_actions = resolve_card_play(
        CardId::Strike,
        &state,
        &CombatCard::for_scenario(CardId::Strike, 1),
        Some(7),
    );
    assert_eq!(strike_actions.len(), 1);
    match &strike_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Strike should emit DamageAction, got {other:?}"),
    }

    let defend_actions = resolve_card_play(
        CardId::Defend,
        &state,
        &CombatCard::for_scenario(CardId::Defend, 2),
        None,
    );
    assert_eq!(defend_actions.len(), 1);
    match &defend_actions[0].action {
        Action::GainBlock { target, amount } => {
            assert_eq!(*target, 0);
            assert_eq!(*amount, 5);
        }
        other => panic!("Defend should emit GainBlockAction, got {other:?}"),
    }

    let bash_actions = resolve_card_play(
        CardId::Bash,
        &state,
        &CombatCard::for_scenario(CardId::Bash, 3),
        Some(7),
    );
    assert_eq!(bash_actions.len(), 2);
    match &bash_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 8);
            assert_eq!(info.output, 8);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Bash first action should be DamageAction, got {other:?}"),
    }
    match &bash_actions[1].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 7);
            assert_eq!(*power_id, PowerId::Vulnerable);
            assert_eq!(*amount, 2);
        }
        other => panic!("Bash second action should be ApplyPowerAction, got {other:?}"),
    }
}

#[test]
fn silent_starter_and_discard_only_cards_are_registered_like_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::StrikeG), "Strike_G");
    assert_eq!(java_id(CardId::DefendG), "Defend_G");
    assert_eq!(java_id(CardId::Reflex), "Reflex");
    assert_eq!(java_id(CardId::Tactician), "Tactician");
    assert_eq!(java_map.get("Strike_G"), Some(&CardId::StrikeG));
    assert_eq!(java_map.get("Defend_G"), Some(&CardId::DefendG));
    assert_eq!(java_map.get("Reflex"), Some(&CardId::Reflex));
    assert_eq!(java_map.get("Tactician"), Some(&CardId::Tactician));

    let strike_g = get_card_definition(CardId::StrikeG);
    assert_eq!(strike_g.name, "Strike");
    assert_eq!(strike_g.card_type, CardType::Attack);
    assert_eq!(strike_g.rarity, CardRarity::Basic);
    assert_eq!(strike_g.cost, 1);
    assert_eq!(strike_g.base_damage, 6);
    assert_eq!(strike_g.target, CardTarget::Enemy);
    assert_eq!(strike_g.upgrade_damage, 3);
    assert!(strike_g.tags.contains(&CardTag::Strike));
    assert!(strike_g.tags.contains(&CardTag::StarterStrike));

    let defend_g = get_card_definition(CardId::DefendG);
    assert_eq!(defend_g.name, "Defend");
    assert_eq!(defend_g.card_type, CardType::Skill);
    assert_eq!(defend_g.rarity, CardRarity::Basic);
    assert_eq!(defend_g.cost, 1);
    assert_eq!(defend_g.base_block, 5);
    assert_eq!(defend_g.target, CardTarget::SelfTarget);
    assert_eq!(defend_g.upgrade_block, 3);
    assert!(defend_g.tags.contains(&CardTag::StarterDefend));

    let reflex = get_card_definition(CardId::Reflex);
    assert_eq!(reflex.card_type, CardType::Skill);
    assert_eq!(reflex.rarity, CardRarity::Uncommon);
    assert_eq!(reflex.cost, -2);
    assert_eq!(reflex.base_magic, 2);
    assert_eq!(reflex.upgrade_magic, 1);
    assert!(SILENT_UNCOMMON_POOL.contains(&CardId::Reflex));

    let tactician = get_card_definition(CardId::Tactician);
    assert_eq!(tactician.card_type, CardType::Skill);
    assert_eq!(tactician.rarity, CardRarity::Uncommon);
    assert_eq!(tactician.cost, -2);
    assert_eq!(tactician.base_magic, 1);
    assert_eq!(tactician.upgrade_magic, 1);
    assert!(SILENT_UNCOMMON_POOL.contains(&CardId::Tactician));

    let state = crate::test_support::blank_test_combat();
    let strike_actions = resolve_card_play(
        CardId::StrikeG,
        &state,
        &CombatCard::for_scenario(CardId::StrikeG, 4),
        Some(7),
    );
    assert_eq!(strike_actions.len(), 1);
    match &strike_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Strike_G should emit DamageAction, got {other:?}"),
    }

    let defend_actions = resolve_card_play(
        CardId::DefendG,
        &state,
        &CombatCard::for_scenario(CardId::DefendG, 5),
        None,
    );
    assert_eq!(
        defend_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 5,
        }
    );

    assert!(resolve_card_play(
        CardId::Reflex,
        &state,
        &CombatCard::for_scenario(CardId::Reflex, 6),
        None,
    )
    .is_empty());
    assert!(resolve_card_play(
        CardId::Tactician,
        &state,
        &CombatCard::for_scenario(CardId::Tactician, 7),
        None,
    )
    .is_empty());
}

#[test]
fn defect_starter_basic_cards_are_registered_like_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::StrikeB), "Strike_B");
    assert_eq!(java_id(CardId::DefendB), "Defend_B");
    assert_eq!(java_id(CardId::Zap), "Zap");
    assert_eq!(java_id(CardId::Dualcast), "Dualcast");
    assert_eq!(java_map.get("Strike_B"), Some(&CardId::StrikeB));
    assert_eq!(java_map.get("Defend_B"), Some(&CardId::DefendB));
    assert_eq!(java_map.get("Zap"), Some(&CardId::Zap));
    assert_eq!(java_map.get("Dualcast"), Some(&CardId::Dualcast));

    let strike_b = get_card_definition(CardId::StrikeB);
    assert_eq!(strike_b.name, "Strike");
    assert_eq!(strike_b.card_type, CardType::Attack);
    assert_eq!(strike_b.rarity, CardRarity::Basic);
    assert_eq!(strike_b.cost, 1);
    assert_eq!(strike_b.base_damage, 6);
    assert_eq!(strike_b.target, CardTarget::Enemy);
    assert_eq!(strike_b.upgrade_damage, 3);
    assert!(strike_b.tags.contains(&CardTag::Strike));
    assert!(strike_b.tags.contains(&CardTag::StarterStrike));

    let defend_b = get_card_definition(CardId::DefendB);
    assert_eq!(defend_b.name, "Defend");
    assert_eq!(defend_b.card_type, CardType::Skill);
    assert_eq!(defend_b.rarity, CardRarity::Basic);
    assert_eq!(defend_b.cost, 1);
    assert_eq!(defend_b.base_block, 5);
    assert_eq!(defend_b.target, CardTarget::SelfTarget);
    assert_eq!(defend_b.upgrade_block, 3);
    assert!(defend_b.tags.contains(&CardTag::StarterDefend));

    let zap = get_card_definition(CardId::Zap);
    assert_eq!(zap.name, "Zap");
    assert_eq!(zap.card_type, CardType::Skill);
    assert_eq!(zap.rarity, CardRarity::Basic);
    assert_eq!(zap.cost, 1);
    assert_eq!(zap.base_magic, 1);
    assert_eq!(zap.target, CardTarget::SelfTarget);
    let mut zap_plus = CombatCard::for_scenario(CardId::Zap, 100);
    zap_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&zap_plus), Some(0));
    assert_eq!(zap_plus.get_cost(), 0);

    let dualcast = get_card_definition(CardId::Dualcast);
    assert_eq!(dualcast.name, "Dualcast");
    assert_eq!(dualcast.card_type, CardType::Skill);
    assert_eq!(dualcast.rarity, CardRarity::Basic);
    assert_eq!(dualcast.cost, 1);
    assert_eq!(dualcast.target, CardTarget::None);
    let mut dualcast_plus = CombatCard::for_scenario(CardId::Dualcast, 101);
    dualcast_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&dualcast_plus), Some(0));
    assert_eq!(dualcast_plus.get_cost(), 0);

    assert!(is_starter_strike(CardId::StrikeB));
    assert!(is_starter_defend(CardId::DefendB));
    assert!(is_starter_basic(CardId::StrikeB));
    assert!(is_starter_basic(CardId::DefendB));
}

#[test]
fn defect_starter_basic_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let strike_actions = resolve_card_play(
        CardId::StrikeB,
        &state,
        &CombatCard::for_scenario(CardId::StrikeB, 102),
        Some(7),
    );
    assert_eq!(strike_actions.len(), 1);
    match &strike_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Strike_B should emit DamageAction, got {other:?}"),
    }

    let defend_actions = resolve_card_play(
        CardId::DefendB,
        &state,
        &CombatCard::for_scenario(CardId::DefendB, 103),
        None,
    );
    assert_eq!(
        defend_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 5,
        }
    );

    let zap_actions = resolve_card_play(
        CardId::Zap,
        &state,
        &CombatCard::for_scenario(CardId::Zap, 104),
        None,
    );
    assert_eq!(zap_actions.len(), 1);
    assert_eq!(zap_actions[0].action, Action::ChannelOrb(OrbId::Lightning));

    let dualcast_actions = resolve_card_play(
        CardId::Dualcast,
        &state,
        &CombatCard::for_scenario(CardId::Dualcast, 105),
        None,
    );
    assert_eq!(dualcast_actions.len(), 2);
    assert_eq!(dualcast_actions[0].action, Action::EvokeOrbWithoutRemoving);
    assert_eq!(dualcast_actions[1].action, Action::EvokeOrb);
}

#[test]
fn watcher_starter_basic_cards_are_registered_like_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::StrikeP), "Strike_P");
    assert_eq!(java_id(CardId::DefendP), "Defend_P");
    assert_eq!(java_id(CardId::Eruption), "Eruption");
    assert_eq!(java_id(CardId::Vigilance), "Vigilance");
    assert_eq!(java_map.get("Strike_P"), Some(&CardId::StrikeP));
    assert_eq!(java_map.get("Defend_P"), Some(&CardId::DefendP));
    assert_eq!(java_map.get("Eruption"), Some(&CardId::Eruption));
    assert_eq!(java_map.get("Vigilance"), Some(&CardId::Vigilance));

    let strike_p = get_card_definition(CardId::StrikeP);
    assert_eq!(strike_p.name, "Strike");
    assert_eq!(strike_p.card_type, CardType::Attack);
    assert_eq!(strike_p.rarity, CardRarity::Basic);
    assert_eq!(strike_p.cost, 1);
    assert_eq!(strike_p.base_damage, 6);
    assert_eq!(strike_p.target, CardTarget::Enemy);
    assert_eq!(strike_p.upgrade_damage, 3);
    assert!(strike_p.tags.contains(&CardTag::Strike));
    assert!(strike_p.tags.contains(&CardTag::StarterStrike));

    let defend_p = get_card_definition(CardId::DefendP);
    assert_eq!(defend_p.name, "Defend");
    assert_eq!(defend_p.card_type, CardType::Skill);
    assert_eq!(defend_p.rarity, CardRarity::Basic);
    assert_eq!(defend_p.cost, 1);
    assert_eq!(defend_p.base_block, 5);
    assert_eq!(defend_p.target, CardTarget::SelfTarget);
    assert_eq!(defend_p.upgrade_block, 3);
    assert!(defend_p.tags.contains(&CardTag::StarterDefend));

    let eruption = get_card_definition(CardId::Eruption);
    assert_eq!(eruption.name, "Eruption");
    assert_eq!(eruption.card_type, CardType::Attack);
    assert_eq!(eruption.rarity, CardRarity::Basic);
    assert_eq!(eruption.cost, 2);
    assert_eq!(eruption.base_damage, 9);
    assert_eq!(eruption.target, CardTarget::Enemy);
    let mut eruption_plus = CombatCard::for_scenario(CardId::Eruption, 200);
    eruption_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&eruption_plus), Some(1));
    assert_eq!(eruption_plus.get_cost(), 1);

    let vigilance = get_card_definition(CardId::Vigilance);
    assert_eq!(vigilance.name, "Vigilance");
    assert_eq!(vigilance.card_type, CardType::Skill);
    assert_eq!(vigilance.rarity, CardRarity::Basic);
    assert_eq!(vigilance.cost, 2);
    assert_eq!(vigilance.base_block, 8);
    assert_eq!(vigilance.target, CardTarget::SelfTarget);
    assert_eq!(vigilance.upgrade_block, 4);

    assert!(is_starter_strike(CardId::StrikeP));
    assert!(is_starter_defend(CardId::DefendP));
    assert!(is_starter_basic(CardId::StrikeP));
    assert!(is_starter_basic(CardId::DefendP));
}

#[test]
fn watcher_starter_basic_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let strike_actions = resolve_card_play(
        CardId::StrikeP,
        &state,
        &CombatCard::for_scenario(CardId::StrikeP, 201),
        Some(7),
    );
    assert_eq!(strike_actions.len(), 1);
    match &strike_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Strike_P should emit DamageAction, got {other:?}"),
    }

    let defend_actions = resolve_card_play(
        CardId::DefendP,
        &state,
        &CombatCard::for_scenario(CardId::DefendP, 202),
        None,
    );
    assert_eq!(
        defend_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 5,
        }
    );

    let eruption_actions = resolve_card_play(
        CardId::Eruption,
        &state,
        &CombatCard::for_scenario(CardId::Eruption, 203),
        Some(7),
    );
    assert_eq!(eruption_actions.len(), 2);
    match &eruption_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 9);
            assert_eq!(info.output, 9);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Eruption first action should be DamageAction, got {other:?}"),
    }
    assert_eq!(
        eruption_actions[1].action,
        Action::EnterStance("Wrath".to_string())
    );

    let vigilance_actions = resolve_card_play(
        CardId::Vigilance,
        &state,
        &CombatCard::for_scenario(CardId::Vigilance, 204),
        None,
    );
    assert_eq!(vigilance_actions.len(), 2);
    assert_eq!(
        vigilance_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 8,
        }
    );
    assert_eq!(
        vigilance_actions[1].action,
        Action::EnterStance("Calm".to_string())
    );
}

#[test]
fn watcher_first_common_batch_definitions_match_java_sources() {
    let java_map = build_java_id_map();
    for (id, java) in [
        (CardId::Consecrate, "Consecrate"),
        (CardId::BowlingBash, "BowlingBash"),
        (CardId::EmptyBody, "EmptyBody"),
        (CardId::EmptyFist, "EmptyFist"),
        (CardId::EmptyMind, "EmptyMind"),
        (CardId::JustLucky, "JustLucky"),
        (CardId::CutThroughFate, "CutThroughFate"),
        (CardId::ThirdEye, "ThirdEye"),
        (CardId::Prostrate, "Prostrate"),
        (CardId::WheelKick, "WheelKick"),
        (CardId::InnerPeace, "InnerPeace"),
        (CardId::FearNoEvil, "FearNoEvil"),
        (CardId::Indignation, "Indignation"),
        (CardId::FollowUp, "FollowUp"),
        (CardId::Sanctity, "Sanctity"),
        (CardId::CrushJoints, "CrushJoints"),
        (CardId::SashWhip, "SashWhip"),
        (CardId::Conclude, "Conclude"),
        (CardId::Brilliance, "Brilliance"),
        (CardId::Ragnarok, "Ragnarok"),
        (CardId::WreathOfFlame, "WreathOfFlame"),
        (CardId::SignatureMove, "SignatureMove"),
        (CardId::SimmeringFury, "Vengeance"),
        (CardId::SpiritShield, "SpiritShield"),
        (CardId::Study, "Study"),
        (CardId::Swivel, "Swivel"),
        (CardId::Insight, "Insight"),
    ] {
        assert_eq!(java_id(id), java);
        assert_eq!(java_map.get(java), Some(&id));
    }

    let cases = [
        (
            CardId::Consecrate,
            "Consecrate",
            CardType::Attack,
            CardRarity::Common,
            0,
            5,
            0,
            0,
            CardTarget::AllEnemy,
            3,
            0,
            0,
        ),
        (
            CardId::BowlingBash,
            "Bowling Bash",
            CardType::Attack,
            CardRarity::Common,
            1,
            7,
            0,
            0,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::EmptyBody,
            "Empty Body",
            CardType::Skill,
            CardRarity::Common,
            1,
            0,
            7,
            0,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::EmptyFist,
            "Empty Fist",
            CardType::Attack,
            CardRarity::Common,
            1,
            9,
            0,
            0,
            CardTarget::Enemy,
            5,
            0,
            0,
        ),
        (
            CardId::EmptyMind,
            "Empty Mind",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            0,
            2,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::JustLucky,
            "Just Lucky",
            CardType::Attack,
            CardRarity::Common,
            0,
            3,
            2,
            1,
            CardTarget::Enemy,
            1,
            1,
            1,
        ),
        (
            CardId::CutThroughFate,
            "Cut Through Fate",
            CardType::Attack,
            CardRarity::Common,
            1,
            7,
            0,
            2,
            CardTarget::Enemy,
            2,
            0,
            1,
        ),
        (
            CardId::ThirdEye,
            "Third Eye",
            CardType::Skill,
            CardRarity::Common,
            1,
            0,
            7,
            3,
            CardTarget::SelfTarget,
            0,
            2,
            2,
        ),
        (
            CardId::Prostrate,
            "Prostrate",
            CardType::Skill,
            CardRarity::Common,
            0,
            0,
            4,
            2,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::WheelKick,
            "Wheel Kick",
            CardType::Attack,
            CardRarity::Uncommon,
            2,
            15,
            0,
            2,
            CardTarget::Enemy,
            5,
            0,
            0,
        ),
        (
            CardId::InnerPeace,
            "Inner Peace",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            0,
            3,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::FearNoEvil,
            "Fear No Evil",
            CardType::Attack,
            CardRarity::Uncommon,
            1,
            8,
            0,
            0,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::Indignation,
            "Indignation",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            0,
            3,
            CardTarget::None,
            0,
            0,
            2,
        ),
        (
            CardId::FollowUp,
            "Follow-Up",
            CardType::Attack,
            CardRarity::Common,
            1,
            7,
            0,
            0,
            CardTarget::Enemy,
            4,
            0,
            0,
        ),
        (
            CardId::Sanctity,
            "Sanctity",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            6,
            2,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::CrushJoints,
            "Crush Joints",
            CardType::Attack,
            CardRarity::Common,
            1,
            8,
            0,
            1,
            CardTarget::Enemy,
            2,
            0,
            1,
        ),
        (
            CardId::SashWhip,
            "Sash Whip",
            CardType::Attack,
            CardRarity::Common,
            1,
            8,
            0,
            1,
            CardTarget::Enemy,
            2,
            0,
            1,
        ),
        (
            CardId::Conclude,
            "Conclude",
            CardType::Attack,
            CardRarity::Uncommon,
            1,
            12,
            0,
            0,
            CardTarget::AllEnemy,
            4,
            0,
            0,
        ),
        (
            CardId::Brilliance,
            "Brilliance",
            CardType::Attack,
            CardRarity::Rare,
            1,
            12,
            0,
            0,
            CardTarget::Enemy,
            4,
            0,
            0,
        ),
        (
            CardId::Ragnarok,
            "Ragnarok",
            CardType::Attack,
            CardRarity::Rare,
            3,
            5,
            0,
            5,
            CardTarget::AllEnemy,
            1,
            0,
            1,
        ),
        (
            CardId::WreathOfFlame,
            "Wreath of Flame",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            0,
            5,
            CardTarget::SelfTarget,
            0,
            0,
            3,
        ),
        (
            CardId::SignatureMove,
            "Signature Move",
            CardType::Attack,
            CardRarity::Uncommon,
            2,
            30,
            0,
            0,
            CardTarget::Enemy,
            10,
            0,
            0,
        ),
        (
            CardId::SimmeringFury,
            "Simmering Fury",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            0,
            2,
            CardTarget::None,
            0,
            0,
            1,
        ),
        (
            CardId::SpiritShield,
            "Spirit Shield",
            CardType::Skill,
            CardRarity::Rare,
            2,
            0,
            0,
            3,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::Study,
            "Study",
            CardType::Power,
            CardRarity::Uncommon,
            2,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Swivel,
            "Swivel",
            CardType::Skill,
            CardRarity::Uncommon,
            2,
            0,
            8,
            0,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::Crescendo,
            "Crescendo",
            CardType::Skill,
            CardRarity::Common,
            1,
            0,
            0,
            0,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Tranquility,
            "Tranquility",
            CardType::Skill,
            CardRarity::Common,
            1,
            0,
            0,
            0,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Protect,
            "Protect",
            CardType::Skill,
            CardRarity::Common,
            2,
            0,
            12,
            0,
            CardTarget::SelfTarget,
            0,
            4,
            0,
        ),
        (
            CardId::CarveReality,
            "Carve Reality",
            CardType::Attack,
            CardRarity::Uncommon,
            1,
            6,
            0,
            0,
            CardTarget::Enemy,
            4,
            0,
            0,
        ),
        (
            CardId::DeceiveReality,
            "Deceive Reality",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            4,
            0,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::Evaluate,
            "Evaluate",
            CardType::Skill,
            CardRarity::Common,
            1,
            0,
            6,
            0,
            CardTarget::SelfTarget,
            0,
            4,
            0,
        ),
        (
            CardId::FlyingSleeves,
            "Flying Sleeves",
            CardType::Attack,
            CardRarity::Common,
            1,
            4,
            0,
            0,
            CardTarget::Enemy,
            2,
            0,
            0,
        ),
        (
            CardId::Halt,
            "Halt",
            CardType::Skill,
            CardRarity::Common,
            0,
            0,
            3,
            9,
            CardTarget::SelfTarget,
            0,
            1,
            5,
        ),
        (
            CardId::Pray,
            "Pray",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            0,
            3,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::Worship,
            "Worship",
            CardType::Skill,
            CardRarity::Uncommon,
            2,
            0,
            0,
            5,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Scrawl,
            "Scrawl",
            CardType::Skill,
            CardRarity::Rare,
            1,
            0,
            0,
            0,
            CardTarget::None,
            0,
            0,
            0,
        ),
        (
            CardId::MasterReality,
            "Master Reality",
            CardType::Power,
            CardRarity::Rare,
            1,
            0,
            0,
            0,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Tantrum,
            "Tantrum",
            CardType::Attack,
            CardRarity::Uncommon,
            1,
            3,
            0,
            3,
            CardTarget::Enemy,
            0,
            0,
            1,
        ),
        (
            CardId::SandsOfTime,
            "Sands of Time",
            CardType::Attack,
            CardRarity::Uncommon,
            4,
            20,
            0,
            0,
            CardTarget::Enemy,
            6,
            0,
            0,
        ),
        (
            CardId::Perseverance,
            "Perseverance",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            5,
            2,
            CardTarget::SelfTarget,
            0,
            2,
            1,
        ),
        (
            CardId::WindmillStrike,
            "Windmill Strike",
            CardType::Attack,
            CardRarity::Uncommon,
            2,
            7,
            0,
            4,
            CardTarget::Enemy,
            3,
            0,
            1,
        ),
        (
            CardId::Wallop,
            "Wallop",
            CardType::Attack,
            CardRarity::Uncommon,
            2,
            9,
            0,
            0,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::TalkToTheHand,
            "Talk to the Hand",
            CardType::Attack,
            CardRarity::Uncommon,
            1,
            5,
            0,
            2,
            CardTarget::Enemy,
            2,
            0,
            1,
        ),
        (
            CardId::BattleHymn,
            "Battle Hymn",
            CardType::Power,
            CardRarity::Uncommon,
            1,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Devotion,
            "Devotion",
            CardType::Power,
            CardRarity::Rare,
            1,
            0,
            0,
            2,
            CardTarget::None,
            0,
            0,
            1,
        ),
        (
            CardId::LikeWater,
            "Like Water",
            CardType::Power,
            CardRarity::Uncommon,
            1,
            0,
            0,
            5,
            CardTarget::None,
            0,
            0,
            2,
        ),
        (
            CardId::MentalFortress,
            "Mental Fortress",
            CardType::Power,
            CardRarity::Uncommon,
            1,
            0,
            0,
            4,
            CardTarget::SelfTarget,
            0,
            0,
            2,
        ),
        (
            CardId::Rushdown,
            "Rushdown",
            CardType::Power,
            CardRarity::Uncommon,
            1,
            0,
            0,
            2,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Foresight,
            "Foresight",
            CardType::Power,
            CardRarity::Uncommon,
            1,
            0,
            0,
            3,
            CardTarget::None,
            0,
            0,
            1,
        ),
        (
            CardId::Nirvana,
            "Nirvana",
            CardType::Power,
            CardRarity::Uncommon,
            1,
            0,
            0,
            3,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::Weave,
            "Weave",
            CardType::Attack,
            CardRarity::Uncommon,
            0,
            4,
            0,
            0,
            CardTarget::Enemy,
            2,
            0,
            0,
        ),
        (
            CardId::PressurePoints,
            "Pressure Points",
            CardType::Skill,
            CardRarity::Common,
            1,
            0,
            0,
            8,
            CardTarget::Enemy,
            0,
            0,
            3,
        ),
        (
            CardId::ReachHeaven,
            "Reach Heaven",
            CardType::Attack,
            CardRarity::Uncommon,
            2,
            10,
            0,
            0,
            CardTarget::Enemy,
            5,
            0,
            0,
        ),
        (
            CardId::Judgement,
            "Judgment",
            CardType::Skill,
            CardRarity::Rare,
            1,
            0,
            0,
            30,
            CardTarget::Enemy,
            0,
            0,
            10,
        ),
        (
            CardId::ThroughViolence,
            "Through Violence",
            CardType::Attack,
            CardRarity::Special,
            0,
            20,
            0,
            0,
            CardTarget::Enemy,
            10,
            0,
            0,
        ),
        (
            CardId::FlurryOfBlows,
            "Flurry of Blows",
            CardType::Attack,
            CardRarity::Common,
            0,
            4,
            0,
            0,
            CardTarget::Enemy,
            2,
            0,
            0,
        ),
        (
            CardId::Establishment,
            "Establishment",
            CardType::Power,
            CardRarity::Rare,
            1,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::WaveOfTheHand,
            "Wave of the Hand",
            CardType::Skill,
            CardRarity::Uncommon,
            1,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::Fasting,
            "Fasting",
            CardType::Power,
            CardRarity::Uncommon,
            2,
            0,
            0,
            3,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::DevaForm,
            "Deva Form",
            CardType::Power,
            CardRarity::Rare,
            3,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Smite,
            "Smite",
            CardType::Attack,
            CardRarity::Special,
            1,
            12,
            0,
            0,
            CardTarget::Enemy,
            4,
            0,
            0,
        ),
        (
            CardId::Safety,
            "Safety",
            CardType::Skill,
            CardRarity::Special,
            1,
            0,
            12,
            0,
            CardTarget::SelfTarget,
            0,
            4,
            0,
        ),
        (
            CardId::Insight,
            "Insight",
            CardType::Skill,
            CardRarity::Special,
            0,
            0,
            0,
            2,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
    ];

    for (
        id,
        name,
        card_type,
        rarity,
        cost,
        base_damage,
        base_block,
        base_magic,
        target,
        upgrade_damage,
        upgrade_block,
        upgrade_magic,
    ) in cases
    {
        let def = get_card_definition(id);
        assert_eq!(def.name, name);
        assert_eq!(def.card_type, card_type);
        assert_eq!(def.rarity, rarity);
        assert_eq!(def.cost, cost);
        assert_eq!(def.base_damage, base_damage);
        assert_eq!(def.base_block, base_block);
        assert_eq!(def.base_magic, base_magic);
        assert_eq!(def.target, target);
        assert_eq!(def.upgrade_damage, upgrade_damage);
        assert_eq!(def.upgrade_block, upgrade_block);
        assert_eq!(def.upgrade_magic, upgrade_magic);
    }

    for empty in [CardId::EmptyBody, CardId::EmptyFist, CardId::EmptyMind] {
        assert!(get_card_definition(empty).tags.contains(&CardTag::Empty));
    }
    assert!(WATCHER_COMMON_POOL.contains(&CardId::Consecrate));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::BowlingBash));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::EmptyBody));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::EmptyFist));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::JustLucky));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::CutThroughFate));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::ThirdEye));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::Prostrate));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::FollowUp));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::CrushJoints));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::SashWhip));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::Crescendo));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::Protect));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::Tranquility));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::Evaluate));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::FlyingSleeves));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::Halt));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::PressurePoints));
    assert!(WATCHER_COMMON_POOL.contains(&CardId::FlurryOfBlows));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::EmptyMind));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::WheelKick));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::InnerPeace));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::FearNoEvil));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Indignation));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Sanctity));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Conclude));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::WreathOfFlame));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::SignatureMove));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::CarveReality));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::DeceiveReality));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::SimmeringFury));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Pray));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Worship));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Study));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Swivel));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Tantrum));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::SandsOfTime));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Perseverance));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::WindmillStrike));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Wallop));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::TalkToTheHand));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::BattleHymn));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::LikeWater));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::MentalFortress));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Rushdown));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Foresight));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Nirvana));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Weave));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::ReachHeaven));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::WaveOfTheHand));
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Fasting));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Brilliance));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Ragnarok));
    assert!(WATCHER_RARE_POOL.contains(&CardId::SpiritShield));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Scrawl));
    assert!(WATCHER_RARE_POOL.contains(&CardId::MasterReality));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Devotion));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Judgement));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Establishment));
    assert!(WATCHER_RARE_POOL.contains(&CardId::DevaForm));
}

#[test]
fn watcher_first_common_batch_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    let mut escaped = crate::test_support::test_monster(EnemyId::LouseNormal);
    escaped.id = 9;
    escaped.is_escaped = true;
    state.entities.monsters = vec![first, second, escaped];

    let consecrate_actions = resolve_card_play(
        CardId::Consecrate,
        &state,
        &CombatCard::for_scenario(CardId::Consecrate, 300),
        None,
    );
    assert_eq!(consecrate_actions.len(), 1);
    match &consecrate_actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            ..
        } => {
            assert_eq!(*source, 0);
            assert_eq!(damages.as_slice(), &[5, 5, 5]);
            assert_eq!(*damage_type, DamageType::Normal);
        }
        other => panic!("Consecrate should emit DamageAllEnemiesAction, got {other:?}"),
    }

    let bowling_actions = resolve_card_play(
        CardId::BowlingBash,
        &state,
        &CombatCard::for_scenario(CardId::BowlingBash, 301),
        Some(7),
    );
    assert_eq!(
        bowling_actions.len(),
        2,
        "Java BowlingBash repeats once per monster that is not dead or escaped"
    );
    for action in &bowling_actions {
        match &action.action {
            Action::Damage(info) => {
                assert_eq!(info.target, 7);
                assert_eq!(info.base, 7);
                assert_eq!(info.output, 7);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            other => panic!("Bowling Bash should emit repeated DamageAction, got {other:?}"),
        }
    }

    let empty_body = resolve_card_play(
        CardId::EmptyBody,
        &state,
        &CombatCard::for_scenario(CardId::EmptyBody, 302),
        None,
    );
    assert_eq!(
        empty_body
            .iter()
            .map(|info| &info.action)
            .collect::<Vec<_>>(),
        vec![
            &Action::GainBlock {
                target: 0,
                amount: 7,
            },
            &Action::EnterStance("Neutral".to_string()),
        ]
    );

    let empty_fist = resolve_card_play(
        CardId::EmptyFist,
        &state,
        &CombatCard::for_scenario(CardId::EmptyFist, 303),
        Some(7),
    );
    assert_eq!(empty_fist.len(), 2);
    match &empty_fist[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 9);
        }
        other => panic!("Empty Fist first action should be DamageAction, got {other:?}"),
    }
    assert_eq!(
        empty_fist[1].action,
        Action::EnterStance("Neutral".to_string())
    );

    let mut empty_mind_plus = CombatCard::for_scenario(CardId::EmptyMind, 304);
    empty_mind_plus.upgrades = 1;
    let empty_mind = resolve_card_play(CardId::EmptyMind, &state, &empty_mind_plus, None);
    assert_eq!(
        empty_mind
            .iter()
            .map(|info| &info.action)
            .collect::<Vec<_>>(),
        vec![
            &Action::DrawCards(3),
            &Action::EnterStance("Neutral".to_string()),
        ]
    );
}

#[test]
fn watcher_retain_stance_and_generated_temp_cards_match_java_sources() {
    let state = crate::test_support::blank_test_combat();

    assert_eq!(java_id(CardId::Tranquility), "ClearTheMind");
    for id in [
        CardId::Crescendo,
        CardId::Tranquility,
        CardId::Protect,
        CardId::Smite,
        CardId::Safety,
    ] {
        assert!(
            crate::content::cards::is_self_retain(&CombatCard::for_scenario(id, 900)),
            "{id:?} should preserve Java selfRetain"
        );
    }
    assert!(crate::content::cards::exhausts_when_played(
        &CombatCard::for_scenario(CardId::Crescendo, 901)
    ));
    assert!(crate::content::cards::exhausts_when_played(
        &CombatCard::for_scenario(CardId::Tranquility, 902)
    ));
    assert!(crate::content::cards::exhausts_when_played(
        &CombatCard::for_scenario(CardId::Smite, 903)
    ));
    assert!(crate::content::cards::exhausts_when_played(
        &CombatCard::for_scenario(CardId::Safety, 904)
    ));
    assert!(!crate::content::cards::exhausts_when_played(
        &CombatCard::for_scenario(CardId::Protect, 905)
    ));

    let mut crescendo_plus = CombatCard::for_scenario(CardId::Crescendo, 906);
    crescendo_plus.upgrades = 1;
    assert_eq!(
        crate::content::cards::upgraded_base_cost_override(&crescendo_plus),
        Some(0)
    );
    let mut tranquility_plus = CombatCard::for_scenario(CardId::Tranquility, 907);
    tranquility_plus.upgrades = 1;
    assert_eq!(
        crate::content::cards::upgraded_base_cost_override(&tranquility_plus),
        Some(0)
    );

    assert_eq!(
        resolve_card_play(
            CardId::Crescendo,
            &state,
            &CombatCard::for_scenario(CardId::Crescendo, 908),
            None,
        )[0]
        .action,
        Action::EnterStance("Wrath".to_string())
    );
    assert_eq!(
        resolve_card_play(
            CardId::Tranquility,
            &state,
            &CombatCard::for_scenario(CardId::Tranquility, 909),
            None,
        )[0]
        .action,
        Action::EnterStance("Calm".to_string())
    );

    let mut protect_plus = CombatCard::for_scenario(CardId::Protect, 910);
    protect_plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::Protect, &state, &protect_plus, None)[0].action,
        Action::GainBlock {
            target: 0,
            amount: 16,
        }
    );

    let mut smite_plus = CombatCard::for_scenario(CardId::Smite, 911);
    smite_plus.upgrades = 1;
    let smite = resolve_card_play(CardId::Smite, &state, &smite_plus, Some(7));
    match &smite[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 16);
            assert_eq!(info.output, 16);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Smite+ should emit upgraded DamageAction, got {other:?}"),
    }

    let mut safety_plus = CombatCard::for_scenario(CardId::Safety, 912);
    safety_plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::Safety, &state, &safety_plus, None)[0].action,
        Action::GainBlock {
            target: 0,
            amount: 16,
        }
    );

    let mut carve_plus = CombatCard::for_scenario(CardId::CarveReality, 913);
    carve_plus.upgrades = 1;
    let carve = resolve_card_play(CardId::CarveReality, &state, &carve_plus, Some(7));
    assert_eq!(carve.len(), 2);
    match &carve[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 10);
            assert_eq!(info.output, 10);
        }
        other => panic!("Carve Reality+ should emit upgraded DamageAction, got {other:?}"),
    }
    match &carve[1].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Smite);
            assert_eq!(*amount, 1);
        }
        other => panic!("Carve Reality should generate constructed Smite, got {other:?}"),
    }

    let mut deceive_plus = CombatCard::for_scenario(CardId::DeceiveReality, 914);
    deceive_plus.upgrades = 1;
    let deceive = resolve_card_play(CardId::DeceiveReality, &state, &deceive_plus, None);
    assert_eq!(deceive.len(), 2);
    assert_eq!(
        deceive[0].action,
        Action::GainBlock {
            target: 0,
            amount: 7,
        }
    );
    match &deceive[1].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Safety);
            assert_eq!(*amount, 1);
        }
        other => panic!("Deceive Reality should generate constructed Safety, got {other:?}"),
    }
}

#[test]
fn watcher_evaluate_flying_sleeves_and_halt_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.stance = StanceId::Neutral;

    assert!(crate::content::cards::is_self_retain(
        &CombatCard::for_scenario(CardId::FlyingSleeves, 920,)
    ));

    let mut evaluate_plus = CombatCard::for_scenario(CardId::Evaluate, 921);
    evaluate_plus.upgrades = 1;
    let evaluate = resolve_card_play(CardId::Evaluate, &state, &evaluate_plus, None);
    assert_eq!(
        evaluate.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::GainBlock {
                target: 0,
                amount: 10,
            },
            &Action::MakeTempCardInDrawPile {
                card_id: CardId::Insight,
                amount: 1,
                random_spot: true,
                to_bottom: false,
                upgraded: false,
            },
        ]
    );

    let mut flying_plus = CombatCard::for_scenario(CardId::FlyingSleeves, 922);
    flying_plus.upgrades = 1;
    let flying = resolve_card_play(CardId::FlyingSleeves, &state, &flying_plus, Some(7));
    assert_eq!(flying.len(), 2);
    for action in &flying {
        match &action.action {
            Action::Damage(info) => {
                assert_eq!(info.target, 7);
                assert_eq!(info.base, 6);
                assert_eq!(info.output, 6);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            other => panic!("Flying Sleeves+ should emit two DamageActions, got {other:?}"),
        }
    }

    let mut halt_plus = CombatCard::for_scenario(CardId::Halt, 923);
    halt_plus.upgrades = 1;
    let halt = resolve_card_play(CardId::Halt, &state, &halt_plus, None);
    assert_eq!(
        halt[0].action,
        Action::Halt {
            block: 4,
            additional: 14,
        },
        "Java Halt.use queues HaltAction(block, additional); stance is checked when the action executes"
    );

    crate::engine::action_handlers::execute_action(halt[0].action.clone(), &mut state);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 4,
        })
    );

    state.entities.player.stance = StanceId::Wrath;
    crate::engine::action_handlers::execute_action(
        Action::Halt {
            block: 4,
            additional: 14,
        },
        &mut state,
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 18,
        })
    );

    let mut dex_state = crate::test_support::blank_test_combat();
    dex_state.entities.power_db.insert(
        0,
        vec![Power {
            power_type: PowerId::Dexterity,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let dex_halt = resolve_card_play(CardId::Halt, &dex_state, &halt_plus, None);
    assert_eq!(
        dex_halt[0].action,
        Action::Halt {
            block: 6,
            additional: 16,
        },
        "Java Halt.applyPowers applies block modifiers to both normal block and the Wrath block value"
    );
    let mut halt_hand_state = dex_state.clone();
    halt_hand_state.zones.hand = vec![halt_plus.clone()];
    halt_hand_state.update_hand_cards();
    assert_eq!(halt_hand_state.zones.hand[0].base_block_mut, 6);
    assert_eq!(
        halt_hand_state.zones.hand[0].base_magic_num_mut, 16,
        "Java Halt.applyPowers leaves magicNumber as the modified Wrath block value for rendering/observation"
    );
}

#[test]
fn watcher_pray_worship_and_scrawl_match_java_sources() {
    let state = crate::test_support::blank_test_combat();

    let mut pray_plus = CombatCard::for_scenario(CardId::Pray, 930);
    pray_plus.upgrades = 1;
    let pray = resolve_card_play(CardId::Pray, &state, &pray_plus, None);
    assert_eq!(
        pray.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Mantra,
                amount: 4,
            },
            &Action::MakeTempCardInDrawPile {
                card_id: CardId::Insight,
                amount: 1,
                random_spot: true,
                to_bottom: false,
                upgraded: false,
            },
        ]
    );

    let worship = CombatCard::for_scenario(CardId::Worship, 931);
    assert!(
        !crate::content::cards::is_self_retain(&worship),
        "Java Worship only gains selfRetain from upgrade()"
    );
    let mut worship_plus = worship.clone();
    worship_plus.upgrades = 1;
    assert!(crate::content::cards::is_self_retain(&worship_plus));
    let worship_actions = resolve_card_play(CardId::Worship, &state, &worship_plus, None);
    assert_eq!(
        worship_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Mantra,
            amount: 5,
        }
    );

    let mut scrawl_plus = CombatCard::for_scenario(CardId::Scrawl, 932);
    scrawl_plus.upgrades = 1;
    assert_eq!(
        crate::content::cards::upgraded_base_cost_override(&scrawl_plus),
        Some(0)
    );
    assert!(crate::content::cards::exhausts_when_played(
        &CombatCard::for_scenario(CardId::Scrawl, 933)
    ));
    assert_eq!(
        resolve_card_play(CardId::Scrawl, &state, &scrawl_plus, None)[0].action,
        Action::ExpertiseDraw {
            target_hand_size: 10,
        }
    );

    let mut master_reality_plus = CombatCard::for_scenario(CardId::MasterReality, 934);
    master_reality_plus.upgrades = 1;
    assert_eq!(
        crate::content::cards::upgraded_base_cost_override(&master_reality_plus),
        Some(0)
    );
    assert_eq!(
        resolve_card_play(CardId::MasterReality, &state, &master_reality_plus, None)[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::MasterRealityPower,
            amount: -1,
        }
    );
}

#[test]
fn watcher_tantrum_and_retained_attack_cards_match_java_sources() {
    let state = crate::test_support::blank_test_combat();

    let mut tantrum_plus = CombatCard::for_scenario(CardId::Tantrum, 940);
    tantrum_plus.upgrades = 1;
    assert!(crate::content::cards::shuffle_back_into_draw_pile_when_played(&tantrum_plus));
    let tantrum = resolve_card_play(CardId::Tantrum, &state, &tantrum_plus, Some(7));
    assert_eq!(
        tantrum.len(),
        5,
        "Java Tantrum+ hits magicNumber times, then enters Wrath"
    );
    for action in &tantrum[..4] {
        match &action.action {
            Action::Damage(info) => {
                assert_eq!(info.target, 7);
                assert_eq!(info.base, 3);
                assert_eq!(info.output, 3);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            other => panic!("Tantrum+ should emit repeated DamageAction, got {other:?}"),
        }
    }
    assert_eq!(tantrum[4].action, Action::EnterStance("Wrath".to_string()));

    let mut cleanup_state = crate::test_support::blank_test_combat();
    cleanup_state
        .zones
        .limbo
        .push(CombatCard::for_scenario(CardId::Tantrum, 941));
    crate::engine::action_handlers::execute_action(
        Action::UseCardDone {
            should_exhaust: false,
            trigger_after_use_hooks: true,
        },
        &mut cleanup_state,
    );
    assert!(cleanup_state.zones.discard_pile.is_empty());
    assert_eq!(cleanup_state.zones.draw_pile.len(), 1);
    assert_eq!(cleanup_state.zones.draw_pile[0].id, CardId::Tantrum);

    let mut sands_plus = CombatCard::for_scenario(CardId::SandsOfTime, 942);
    sands_plus.upgrades = 1;
    assert!(crate::content::cards::is_self_retain(&sands_plus));
    crate::content::cards::trigger_on_retained(&mut sands_plus);
    assert_eq!(sands_plus.get_cost(), 3);
    let sands = resolve_card_play(CardId::SandsOfTime, &state, &sands_plus, Some(7));
    match &sands[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 26);
        }
        other => panic!("Sands of Time+ should emit DamageAction, got {other:?}"),
    }
}

#[test]
fn watcher_retained_block_and_strike_growth_match_java_on_retained() {
    let state = crate::test_support::blank_test_combat();

    let mut perseverance_plus = CombatCard::for_scenario(CardId::Perseverance, 950);
    perseverance_plus.upgrades = 1;
    assert!(crate::content::cards::is_self_retain(&perseverance_plus));
    crate::content::cards::trigger_on_retained(&mut perseverance_plus);
    crate::content::cards::trigger_on_retained(&mut perseverance_plus);
    let perseverance = resolve_card_play(CardId::Perseverance, &state, &perseverance_plus, None);
    assert_eq!(
        perseverance[0].action,
        Action::GainBlock {
            target: 0,
            amount: 13,
        },
        "Perseverance+ starts at 7 block and gains 3 block per retain"
    );

    let mut retained_then_upgraded_perseverance =
        CombatCard::for_scenario(CardId::Perseverance, 952);
    crate::content::cards::trigger_on_retained(&mut retained_then_upgraded_perseverance);
    let mut upgrade_state = crate::test_support::blank_test_combat();
    upgrade_state.zones.hand = vec![retained_then_upgraded_perseverance];
    crate::engine::action_handlers::execute_action(
        Action::UpgradeAllCardsInCombat,
        &mut upgrade_state,
    );
    crate::content::cards::trigger_on_retained(&mut upgrade_state.zones.hand[0]);
    let perseverance_after_upgrade = resolve_card_play(
        CardId::Perseverance,
        &state,
        &upgrade_state.zones.hand[0],
        None,
    );
    assert_eq!(
        perseverance_after_upgrade[0].action,
        Action::GainBlock {
            target: 0,
            amount: 12,
        },
        "Java onRetained mutates baseBlock, so a later upgrade adds +2 to the retained base and future retains use upgraded magic"
    );

    let mut windmill_plus = CombatCard::for_scenario(CardId::WindmillStrike, 951);
    windmill_plus.upgrades = 1;
    assert!(get_card_definition(CardId::WindmillStrike)
        .tags
        .contains(&CardTag::Strike));
    assert!(crate::content::cards::is_self_retain(&windmill_plus));
    crate::content::cards::trigger_on_retained(&mut windmill_plus);
    crate::content::cards::trigger_on_retained(&mut windmill_plus);
    let windmill = resolve_card_play(CardId::WindmillStrike, &state, &windmill_plus, Some(7));
    match &windmill[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 20);
            assert_eq!(info.output, 20);
        }
        other => panic!("Windmill Strike+ should emit retained DamageAction, got {other:?}"),
    }

    let mut retained_then_upgraded_windmill = CombatCard::for_scenario(CardId::WindmillStrike, 953);
    crate::content::cards::trigger_on_retained(&mut retained_then_upgraded_windmill);
    let mut upgrade_state = crate::test_support::blank_test_combat();
    upgrade_state.zones.hand = vec![retained_then_upgraded_windmill];
    crate::engine::action_handlers::execute_action(
        Action::UpgradeAllCardsInCombat,
        &mut upgrade_state,
    );
    let windmill_after_upgrade = resolve_card_play(
        CardId::WindmillStrike,
        &state,
        &upgrade_state.zones.hand[0],
        Some(7),
    );
    match &windmill_after_upgrade[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 14);
            assert_eq!(info.output, 14);
        }
        other => {
            panic!("Windmill Strike retained-then-upgraded should emit DamageAction, got {other:?}")
        }
    }
}

#[test]
fn watcher_wallop_and_talk_to_the_hand_match_java_execution_hooks() {
    let state = crate::test_support::blank_test_combat();

    let mut wallop_plus = CombatCard::for_scenario(CardId::Wallop, 960);
    wallop_plus.upgrades = 1;
    let wallop = resolve_card_play(CardId::Wallop, &state, &wallop_plus, Some(7));
    assert_eq!(wallop.len(), 1);
    match &wallop[0].action {
        Action::WallopDamage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 12);
            assert_eq!(info.output, 12);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Wallop+ should emit WallopAction-equivalent damage, got {other:?}"),
    }

    let mut wallop_state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    target.current_hp = 40;
    target.max_hp = 40;
    target.block = 5;
    wallop_state.entities.monsters = vec![target];
    crate::engine::action_handlers::execute_action(wallop[0].action.clone(), &mut wallop_state);
    assert_eq!(
        wallop_state.entities.monsters[0].current_hp, 33,
        "Wallop actual HP loss should be after target block"
    );
    assert_eq!(
        wallop_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 7,
        }),
        "Java WallopAction gains block equal to target.lastDamageTaken"
    );

    let mut talk_plus = CombatCard::for_scenario(CardId::TalkToTheHand, 961);
    talk_plus.upgrades = 1;
    let talk = resolve_card_play(CardId::TalkToTheHand, &state, &talk_plus, Some(7));
    assert_eq!(talk.len(), 2);
    assert!(get_card_definition(CardId::TalkToTheHand).exhaust);
    match &talk[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 7);
            assert_eq!(info.output, 7);
        }
        other => panic!("Talk to the Hand+ first action should damage, got {other:?}"),
    }
    assert_eq!(
        talk[1].action,
        Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::BlockReturnPower,
            amount: 3,
        }
    );

    let mut return_state = crate::test_support::blank_test_combat();
    let mut marked = crate::test_support::test_monster(EnemyId::JawWorm);
    marked.id = 7;
    marked.current_hp = 40;
    marked.max_hp = 40;
    return_state.entities.monsters = vec![marked];
    crate::content::powers::store::set_powers_for(
        &mut return_state,
        7,
        vec![Power {
            power_type: PowerId::BlockReturnPower,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 0,
            target: 7,
            base: 1,
            output: 1,
            damage_type: DamageType::Normal,
            is_modified: true,
        }),
        &mut return_state,
    );
    assert_eq!(
        return_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 3,
        }),
        "Java BlockReturnPower grants player block when a different non-null source attacks"
    );

    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 0,
            target: 7,
            base: 1,
            output: 1,
            damage_type: DamageType::Thorns,
            is_modified: true,
        }),
        &mut return_state,
    );
    assert_eq!(
        return_state.pop_next_action(),
        None,
        "Java BlockReturnPower ignores THORNS and HP_LOSS damage"
    );
}

#[test]
fn watcher_power_hook_batch_matches_java_sources() {
    let state = crate::test_support::blank_test_combat();

    let mut battle_hymn_plus = CombatCard::for_scenario(CardId::BattleHymn, 970);
    battle_hymn_plus.upgrades = 1;
    assert!(
        is_innate_card(&battle_hymn_plus),
        "Java Battle Hymn+ changes only innate"
    );
    let battle_hymn = resolve_card_play(CardId::BattleHymn, &state, &battle_hymn_plus, None);
    assert_eq!(
        battle_hymn[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::BattleHymnPower,
            amount: 1,
        }
    );
    let mut hymn_state = crate::test_support::blank_test_combat();
    hymn_state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    let hymn_actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::BattleHymnPower,
        &mut hymn_state,
        0,
        2,
    );
    assert_eq!(hymn_actions.len(), 1);
    match &hymn_actions[0] {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Smite);
            assert_eq!(*amount, 2);
        }
        other => panic!("Battle Hymn should generate constructed Smites, got {other:?}"),
    }

    let mut devotion_plus = CombatCard::for_scenario(CardId::Devotion, 971);
    devotion_plus.upgrades = 1;
    let devotion = resolve_card_play(CardId::Devotion, &state, &devotion_plus, None);
    assert_eq!(
        devotion[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::DevotionPower,
            amount: 3,
        }
    );
    assert_eq!(
        crate::content::powers::resolve_power_on_post_draw(PowerId::DevotionPower, &state, 0, 3)
            .as_slice(),
        &[Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Mantra,
            amount: 3,
        }]
    );
    assert_eq!(
        crate::content::powers::resolve_power_on_post_draw(PowerId::DevotionPower, &state, 0, 10)
            .as_slice(),
        &[Action::EnterStance("Divinity".to_string())],
        "Java DevotionPower directly enters Divinity if no Mantra exists and amount >= 10"
    );

    let mut calm_state = crate::test_support::blank_test_combat();
    calm_state.entities.player.stance = StanceId::Calm;
    let like_water_power = Power {
        power_type: PowerId::LikeWaterPower,
        instance_id: None,
        amount: 7,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    };
    assert_eq!(
        crate::content::powers::resolve_power_at_end_of_turn(&like_water_power, &mut calm_state, 0)
            .as_slice(),
        &[Action::GainBlock {
            target: 0,
            amount: 7,
        }]
    );

    let mut like_water_plus = CombatCard::for_scenario(CardId::LikeWater, 972);
    like_water_plus.upgrades = 1;
    let like_water = resolve_card_play(CardId::LikeWater, &state, &like_water_plus, None);
    assert_eq!(
        like_water[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::LikeWaterPower,
            amount: 7,
        }
    );
    let mut like_water_stack_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut like_water_stack_state,
        0,
        vec![Power {
            power_type: PowerId::LikeWaterPower,
            instance_id: None,
            amount: 995,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::LikeWaterPower,
            amount: 10,
        },
        &mut like_water_stack_state,
    );
    assert_eq!(
        crate::content::powers::store::power_amount(
            &like_water_stack_state,
            0,
            PowerId::LikeWaterPower,
        ),
        999,
        "Java LikeWaterPower.stackPower caps stacked amount at 999"
    );

    let mut mental_plus = CombatCard::for_scenario(CardId::MentalFortress, 973);
    mental_plus.upgrades = 1;
    let mental = resolve_card_play(CardId::MentalFortress, &state, &mental_plus, None);
    assert_eq!(
        mental[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::MentalFortressPower,
            amount: 6,
        }
    );

    let mut rushdown_plus = CombatCard::for_scenario(CardId::Rushdown, 974);
    rushdown_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&rushdown_plus), Some(0));
    let rushdown = resolve_card_play(CardId::Rushdown, &state, &rushdown_plus, None);
    assert_eq!(
        rushdown[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::RushdownPower,
            amount: 2,
        }
    );

    let mut stance_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut stance_state,
        0,
        vec![
            Power {
                power_type: PowerId::MentalFortressPower,
                instance_id: None,
                amount: 6,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::RushdownPower,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
        ],
    );
    crate::engine::action_handlers::execute_action(
        Action::EnterStance("Wrath".to_string()),
        &mut stance_state,
    );
    assert_eq!(stance_state.entities.player.stance, StanceId::Wrath);
    assert_eq!(
        stance_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 6,
        })
    );
    assert_eq!(stance_state.pop_next_action(), Some(Action::DrawCards(2)));
}

#[test]
fn watcher_scry_hook_batch_matches_java_sources() {
    let state = crate::test_support::blank_test_combat();

    let mut foresight_plus = CombatCard::for_scenario(CardId::Foresight, 980);
    foresight_plus.upgrades = 1;
    let foresight = resolve_card_play(CardId::Foresight, &state, &foresight_plus, None);
    assert_eq!(
        foresight[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::ForesightPower,
            amount: 4,
        }
    );
    let foresight_power = Power {
        power_type: PowerId::ForesightPower,
        instance_id: None,
        amount: 4,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    };
    let mut empty_draw_state = crate::test_support::blank_test_combat();
    empty_draw_state.queue_action_back(Action::MakeTempCardInHand {
        card_id: CardId::Smite,
        amount: 1,
        upgraded: false,
    });
    assert_eq!(
        crate::content::powers::resolve_power_instance_at_turn_start(
            &foresight_power,
            &mut empty_draw_state,
            0,
        )
        .as_slice(),
        &[Action::Scry(4)],
        "Java ForesightPower addToBot queues ScryAction normally"
    );
    assert_eq!(
        empty_draw_state.pop_next_action(),
        Some(Action::EmptyDeckShuffle),
        "Java ForesightPower uses addToTop for EmptyDeckShuffleAction, so it must run before already queued start-of-turn actions"
    );
    assert_eq!(
        empty_draw_state.pop_next_action(),
        Some(Action::MakeTempCardInHand {
            card_id: CardId::Smite,
            amount: 1,
            upgraded: false,
        })
    );

    let mut nirvana_plus = CombatCard::for_scenario(CardId::Nirvana, 981);
    nirvana_plus.upgrades = 1;
    let nirvana = resolve_card_play(CardId::Nirvana, &state, &nirvana_plus, None);
    assert_eq!(
        nirvana[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::NirvanaPower,
            amount: 4,
        }
    );
    assert_eq!(
        crate::content::powers::resolve_power_on_scry(PowerId::NirvanaPower, 0, 4).as_slice(),
        &[Action::GainBlock {
            target: 0,
            amount: 4,
        }]
    );

    let mut weave_plus = CombatCard::for_scenario(CardId::Weave, 982);
    weave_plus.upgrades = 1;
    let weave = resolve_card_play(CardId::Weave, &state, &weave_plus, Some(7));
    match &weave[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
        }
        other => panic!("Weave+ should emit DamageAction, got {other:?}"),
    }

    let mut hook_state = crate::test_support::blank_test_combat();
    hook_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Weave, 983)].into();
    assert_eq!(
        crate::content::cards::hooks::on_scry(&hook_state).as_slice(),
        &[ActionInfo {
            action: Action::DiscardToHand {
                card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(983),
                cost_for_turn: None,
            },
            insertion_mode: AddTo::Bottom,
        }],
        "Java Weave.triggerOnScry queues DiscardToHandAction from discard pile"
    );
}

#[test]
fn watcher_mark_judgement_and_reach_heaven_match_java_sources() {
    let state = crate::test_support::blank_test_combat();

    assert_eq!(java_id(CardId::PressurePoints), "PathToVictory");
    assert!(crate::content::powers::is_debuff(PowerId::MarkPower, 8));
    assert!(crate::content::powers::is_debuff_application(
        PowerId::MarkPower,
        8
    ));

    let mut pressure_plus = CombatCard::for_scenario(CardId::PressurePoints, 990);
    pressure_plus.upgrades = 1;
    let pressure = resolve_card_play(CardId::PressurePoints, &state, &pressure_plus, Some(7));
    assert_eq!(
        pressure.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::ApplyPower {
                source: 0,
                target: 7,
                power_id: PowerId::MarkPower,
                amount: 11,
            },
            &Action::TriggerMarks {
                card_id: CardId::PressurePoints,
            },
        ],
        "Java PressurePoints applies MarkPower, then TriggerMarksAction"
    );

    let mut mark_state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    first.current_hp = 50;
    first.max_hp = 50;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    second.current_hp = 50;
    second.max_hp = 50;
    mark_state.entities.monsters = vec![first, second];
    crate::content::powers::store::set_powers_for(
        &mut mark_state,
        8,
        vec![Power {
            power_type: PowerId::MarkPower,
            instance_id: None,
            amount: 5,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(pressure[0].action.clone(), &mut mark_state);
    crate::engine::action_handlers::execute_action(pressure[1].action.clone(), &mut mark_state);
    assert_eq!(
        mark_state.pop_next_action(),
        Some(Action::LoseHp {
            target: 7,
            amount: 11,
            triggers_rupture: false,
        })
    );
    assert_eq!(
        mark_state.pop_next_action(),
        Some(Action::LoseHp {
            target: 8,
            amount: 5,
            triggers_rupture: false,
        })
    );

    let mut judgement_plus = CombatCard::for_scenario(CardId::Judgement, 991);
    judgement_plus.upgrades = 1;
    let judgement = resolve_card_play(CardId::Judgement, &state, &judgement_plus, Some(7));
    assert_eq!(
        judgement[0].action,
        Action::Judgement {
            target: 7,
            cutoff: 40,
        }
    );

    let mut kill_state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    target.current_hp = 40;
    target.max_hp = 50;
    kill_state.entities.monsters = vec![target];
    crate::engine::action_handlers::execute_action(judgement[0].action.clone(), &mut kill_state);
    assert_eq!(
        kill_state.pop_next_action(),
        Some(Action::InstantKill { target: 7 }),
        "Java JudgementAction queues InstantKillAction only when currentHealth <= cutoff"
    );
    crate::engine::action_handlers::execute_action(
        Action::InstantKill { target: 7 },
        &mut kill_state,
    );
    assert_eq!(kill_state.entities.monsters[0].current_hp, 0);
    assert!(kill_state.entities.monsters[0].is_dying);

    let mut survive_state = crate::test_support::blank_test_combat();
    let mut survivor = crate::test_support::test_monster(EnemyId::JawWorm);
    survivor.id = 7;
    survivor.current_hp = 41;
    survive_state.entities.monsters = vec![survivor];
    crate::engine::action_handlers::execute_action(judgement[0].action.clone(), &mut survive_state);
    assert_eq!(survive_state.pop_next_action(), None);

    let mut reach_plus = CombatCard::for_scenario(CardId::ReachHeaven, 992);
    reach_plus.upgrades = 1;
    let reach = resolve_card_play(CardId::ReachHeaven, &state, &reach_plus, Some(7));
    assert_eq!(reach.len(), 2);
    match &reach[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 15);
            assert_eq!(info.output, 15);
        }
        other => panic!("Reach Heaven+ should first emit DamageAction, got {other:?}"),
    }
    assert_eq!(
        reach[1].action,
        Action::MakeTempCardInDrawPile {
            card_id: CardId::ThroughViolence,
            amount: 1,
            random_spot: true,
            to_bottom: false,
            upgraded: false,
        },
        "Java ReachHeaven uses MakeTempCardInDrawPileAction(ThroughViolence, 1, true, true)"
    );

    let mut through_plus = CombatCard::for_scenario(CardId::ThroughViolence, 993);
    through_plus.upgrades = 1;
    assert!(crate::content::cards::is_self_retain(&through_plus));
    assert!(crate::content::cards::exhausts_when_played(&through_plus));
    let through = resolve_card_play(CardId::ThroughViolence, &state, &through_plus, Some(7));
    match &through[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 30);
            assert_eq!(info.output, 30);
        }
        other => panic!("Through Violence+ should emit DamageAction, got {other:?}"),
    }
}

#[test]
fn watcher_energy_retain_and_block_hook_powers_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();

    let mut flurry_plus = CombatCard::for_scenario(CardId::FlurryOfBlows, 1000);
    flurry_plus.upgrades = 1;
    let flurry = resolve_card_play(CardId::FlurryOfBlows, &state, &flurry_plus, Some(7));
    match &flurry[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
        }
        other => panic!("Flurry of Blows+ should emit DamageAction, got {other:?}"),
    }

    let mut stance_state = crate::test_support::blank_test_combat();
    stance_state.zones.discard_pile =
        vec![CombatCard::for_scenario(CardId::FlurryOfBlows, 1001)].into();
    crate::engine::action_handlers::execute_action(
        Action::EnterStance("Wrath".to_string()),
        &mut stance_state,
    );
    assert_eq!(
        stance_state.pop_next_action(),
        Some(Action::DiscardToHand {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(1001),
            cost_for_turn: None,
        }),
        "Java ChangeStanceAction calls triggerExhaustedCardsOnStanceChange for discard pile cards"
    );

    let mut establishment_plus = CombatCard::for_scenario(CardId::Establishment, 1002);
    establishment_plus.upgrades = 1;
    assert!(crate::content::cards::is_innate_card(&establishment_plus));
    let establishment = resolve_card_play(CardId::Establishment, &state, &establishment_plus, None);
    assert_eq!(
        establishment[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::EstablishmentPower,
            amount: 1,
        }
    );
    assert_eq!(
        crate::content::powers::resolve_power_at_end_of_turn(
            &Power {
                power_type: PowerId::EstablishmentPower,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
            &mut state,
            0,
        )
        .as_slice(),
        &[Action::ReduceRetainedHandCosts { amount: 1 }]
    );

    let mut priority_state = crate::test_support::blank_test_combat();
    priority_state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 1007)];
    crate::content::powers::store::set_powers_for(
        &mut priority_state,
        0,
        vec![
            Power {
                power_type: PowerId::EstablishmentPower,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::RetainCards,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
        ],
    );
    crate::engine::action_handlers::execute_action(Action::EndTurnTrigger, &mut priority_state);
    crate::engine::action_handlers::cards::handle_end_turn_post_cards(&mut priority_state);
    assert_eq!(
        priority_state.pop_next_action(),
        Some(Action::SuspendForHandSelect {
            min: 0,
            max: 1,
            can_cancel: true,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::Retain,
        }),
        "Java sorts powers by priority, so RetainCardsPower queues before EstablishmentPower"
    );
    assert_eq!(
        priority_state.pop_next_action(),
        Some(Action::ReduceRetainedHandCosts { amount: 1 }),
        "EstablishmentPowerAction must run after the retain choice so selected cards can be discounted"
    );

    let mut reduce_state = crate::test_support::blank_test_combat();
    let mut retained = CombatCard::for_scenario(CardId::Strike, 1003);
    retained.retain_override = Some(true);
    let normal = CombatCard::for_scenario(CardId::Bash, 1004);
    let self_retain = CombatCard::for_scenario(CardId::Protect, 1005);
    reduce_state.zones.hand = vec![retained, normal, self_retain];
    crate::engine::action_handlers::execute_action(
        Action::ReduceRetainedHandCosts { amount: 1 },
        &mut reduce_state,
    );
    assert_eq!(reduce_state.zones.hand[0].get_cost(), 0);
    assert_eq!(
        reduce_state.zones.hand[1].get_cost(),
        2,
        "Java EstablishmentPowerAction ignores non-retained hand cards"
    );
    assert_eq!(reduce_state.zones.hand[2].get_cost(), 1);

    let mut wave_plus = CombatCard::for_scenario(CardId::WaveOfTheHand, 1006);
    wave_plus.upgrades = 1;
    let wave = resolve_card_play(CardId::WaveOfTheHand, &state, &wave_plus, None);
    assert_eq!(
        wave[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::WaveOfTheHandPower,
            amount: 2,
        }
    );
    let mut block_state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    block_state.entities.monsters = vec![first, second];
    crate::content::powers::store::set_powers_for(
        &mut block_state,
        0,
        vec![Power {
            power_type: PowerId::WaveOfTheHandPower,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::GainBlock {
            target: 0,
            amount: 5,
        },
        &mut block_state,
    );
    assert_eq!(
        block_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Weak,
            amount: 2,
        })
    );
    assert_eq!(
        block_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 8,
            power_id: PowerId::Weak,
            amount: 2,
        })
    );
    assert_eq!(
        crate::content::powers::resolve_power_at_end_of_round(
            PowerId::WaveOfTheHandPower,
            &state,
            0,
            2,
            false,
        )
        .as_slice(),
        &[Action::RemovePower {
            target: 0,
            power_id: PowerId::WaveOfTheHandPower,
        }]
    );

    let mut fasting_plus = CombatCard::for_scenario(CardId::Fasting, 1007);
    fasting_plus.upgrades = 1;
    assert_eq!(java_id(CardId::Fasting), "Fasting2");
    assert!(crate::content::powers::is_debuff(
        PowerId::EnergyDownPower,
        1
    ));
    let fasting = resolve_card_play(CardId::Fasting, &state, &fasting_plus, None);
    assert_eq!(
        fasting.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Strength,
                amount: 4,
            },
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Dexterity,
                amount: 4,
            },
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::EnergyDownPower,
                amount: 1,
            },
        ]
    );
    assert_eq!(
        crate::content::powers::resolve_power_at_turn_start(
            PowerId::EnergyDownPower,
            &mut crate::test_support::blank_test_combat(),
            0,
            1,
        )
        .as_slice(),
        &[Action::GainEnergy { amount: -1 }]
    );

    let mut deva_plus = CombatCard::for_scenario(CardId::DevaForm, 1008);
    deva_plus.upgrades = 1;
    assert!(crate::content::cards::is_ethereal(
        &CombatCard::for_scenario(CardId::DevaForm, 1009)
    ));
    assert!(!crate::content::cards::is_ethereal(&deva_plus));
    let deva = resolve_card_play(CardId::DevaForm, &state, &deva_plus, None);
    assert_eq!(
        deva[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::DevaForm,
            amount: 1,
        }
    );
    let mut deva_state = crate::test_support::blank_test_combat();
    crate::engine::action_handlers::execute_action(deva[0].action.clone(), &mut deva_state);
    crate::engine::action_handlers::execute_action(deva[0].action.clone(), &mut deva_state);
    let deva_power = crate::content::powers::store::powers_for(&deva_state, 0)
        .unwrap()
        .iter()
        .find(|power| power.power_type == PowerId::DevaForm)
        .unwrap();
    assert_eq!(deva_power.amount, 2);
    assert_eq!(
        deva_power.extra_data, 2,
        "Java DevaPower.stackPower increments energyGainAmount separately from amount"
    );
    deva_state.turn.energy = 3;
    crate::engine::action_handlers::powers::apply_player_turn_energy_recharge_hooks(
        &mut deva_state,
    );
    assert_eq!(deva_state.turn.energy, 5);
    let deva_power = crate::content::powers::store::powers_for(&deva_state, 0)
        .unwrap()
        .iter()
        .find(|power| power.power_type == PowerId::DevaForm)
        .unwrap();
    assert_eq!(deva_power.extra_data, 4);
}

#[test]
fn watcher_delayed_energy_batch_matches_java_sources() {
    let java_map = build_java_id_map();
    for (id, java) in [
        (CardId::Blasphemy, "Blasphemy"),
        (CardId::Collect, "Collect"),
        (CardId::DeusExMachina, "DeusExMachina"),
    ] {
        assert_eq!(java_id(id), java);
        assert_eq!(java_map.get(java), Some(&id));
    }

    let blasphemy = get_card_definition(CardId::Blasphemy);
    assert_eq!(blasphemy.name, "Blasphemy");
    assert_eq!(blasphemy.card_type, CardType::Skill);
    assert_eq!(blasphemy.rarity, CardRarity::Rare);
    assert_eq!(blasphemy.cost, 1);
    assert_eq!(blasphemy.target, CardTarget::SelfTarget);
    assert!(blasphemy.exhaust);
    assert!(!crate::content::cards::is_self_retain(
        &CombatCard::for_scenario(CardId::Blasphemy, 1010)
    ));
    let mut blasphemy_plus = CombatCard::for_scenario(CardId::Blasphemy, 1011);
    blasphemy_plus.upgrades = 1;
    assert!(crate::content::cards::is_self_retain(&blasphemy_plus));

    let collect = get_card_definition(CardId::Collect);
    assert_eq!(collect.name, "Collect");
    assert_eq!(collect.card_type, CardType::Skill);
    assert_eq!(collect.rarity, CardRarity::Uncommon);
    assert_eq!(collect.cost, -1);
    assert_eq!(collect.target, CardTarget::SelfTarget);
    assert!(collect.exhaust);

    let deus = get_card_definition(CardId::DeusExMachina);
    assert_eq!(deus.name, "Deus Ex Machina");
    assert_eq!(deus.card_type, CardType::Skill);
    assert_eq!(deus.rarity, CardRarity::Rare);
    assert_eq!(deus.cost, -2);
    assert_eq!(deus.base_magic, 2);
    assert_eq!(deus.upgrade_magic, 1);
    assert_eq!(deus.target, CardTarget::SelfTarget);
    assert!(deus.exhaust);

    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Collect));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Blasphemy));
    assert!(WATCHER_RARE_POOL.contains(&CardId::DeusExMachina));

    let state = crate::test_support::blank_test_combat();
    let blasphemy_actions = resolve_card_play(CardId::Blasphemy, &state, &blasphemy_plus, None);
    assert_eq!(
        blasphemy_actions
            .iter()
            .map(|info| &info.action)
            .collect::<Vec<_>>(),
        vec![
            &Action::EnterStance("Divinity".to_string()),
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::EndTurnDeathPower,
                amount: -1,
            },
        ]
    );
    assert_eq!(
        crate::content::powers::resolve_power_at_turn_start(
            PowerId::EndTurnDeathPower,
            &mut crate::test_support::blank_test_combat(),
            0,
            -1,
        )
        .as_slice(),
        &[
            Action::LoseHp {
                target: 0,
                amount: 99_999,
                triggers_rupture: true,
            },
            Action::RemovePower {
                target: 0,
                power_id: PowerId::EndTurnDeathPower,
            },
        ]
    );

    let mut collect_card = CombatCard::for_scenario(CardId::Collect, 1012);
    collect_card.upgrades = 1;
    collect_card.energy_on_use = 2;
    let collect_actions = resolve_card_play(CardId::Collect, &state, &collect_card, None);
    assert_eq!(
        collect_actions[0].action,
        Action::Collect {
            upgraded: true,
            free_to_play_once: false,
            energy_on_use: 2,
        }
    );

    let mut collect_state = crate::test_support::blank_test_combat();
    collect_state.turn.energy = 2;
    crate::engine::action_handlers::execute_action(
        collect_actions[0].action.clone(),
        &mut collect_state,
    );
    assert_eq!(
        collect_state.turn.energy, 0,
        "Java CollectAction only spends current energy after a positive effect"
    );
    let apply_collect = collect_state
        .pop_next_action()
        .expect("Collect should queue CollectPower when effect is positive");
    assert_eq!(
        apply_collect,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::CollectPower,
            amount: 3,
        }
    );
    crate::engine::action_handlers::execute_action(apply_collect, &mut collect_state);
    assert_eq!(
        crate::content::powers::store::power_amount(&collect_state, 0, PowerId::CollectPower),
        3
    );

    crate::engine::action_handlers::powers::apply_player_turn_energy_recharge_hooks(
        &mut collect_state,
    );
    match collect_state.pop_next_action() {
        Some(Action::MakeConstructedCopyInHand { original, amount }) => {
            assert_eq!(original.id, CardId::Miracle);
            assert_eq!(original.upgrades, 1);
            assert_eq!(amount, 1);
        }
        other => {
            panic!("Java CollectPower.onEnergyRecharge creates an upgraded Miracle, got {other:?}")
        }
    }
    assert_eq!(
        collect_state.pop_next_action(),
        Some(Action::ReducePower {
            target: 0,
            power_id: PowerId::CollectPower,
            amount: 1,
        })
    );

    let mut collect_remove_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut collect_remove_state,
        0,
        vec![Power {
            power_type: PowerId::CollectPower,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::powers::apply_player_turn_energy_recharge_hooks(
        &mut collect_remove_state,
    );
    match collect_remove_state.pop_next_action() {
        Some(Action::MakeConstructedCopyInHand { original, amount }) => {
            assert_eq!(original.id, CardId::Miracle);
            assert_eq!(original.upgrades, 1);
            assert_eq!(amount, 1);
        }
        other => {
            panic!("Java CollectPower.onEnergyRecharge creates an upgraded Miracle, got {other:?}")
        }
    }
    assert_eq!(
        collect_remove_state.pop_next_action(),
        Some(Action::RemovePower {
            target: 0,
            power_id: PowerId::CollectPower,
        })
    );

    let miracle = CombatCard::for_scenario(CardId::Miracle, 1016);
    assert_eq!(
        resolve_card_play(CardId::Miracle, &state, &miracle, None)[0].action,
        Action::GainEnergy { amount: 1 }
    );
    let mut miracle_plus = CombatCard::for_scenario(CardId::Miracle, 1017);
    miracle_plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::Miracle, &state, &miracle_plus, None)[0].action,
        Action::GainEnergy { amount: 2 },
        "Java Miracle.use gains 2 energy when the Miracle card itself is upgraded"
    );

    let mut deus_plus = CombatCard::for_scenario(CardId::DeusExMachina, 1013);
    deus_plus.upgrades = 1;
    assert!(
        can_play_card(&deus_plus, &state).is_err(),
        "Java DeusExMachina.canUse always returns false"
    );
    let deus_play = resolve_card_play(CardId::DeusExMachina, &state, &deus_plus, None);
    assert!(deus_play.is_empty());

    let mut draw_state = crate::test_support::blank_test_combat();
    draw_state.zones.draw_pile = (vec![deus_plus.clone()]).into();
    crate::engine::action_handlers::execute_action(Action::DrawCards(1), &mut draw_state);
    assert_eq!(draw_state.zones.hand.len(), 1);
    assert_eq!(draw_state.zones.hand[0].id, CardId::DeusExMachina);
    let exhaust = draw_state
        .pop_next_action()
        .expect("Deus Ex Machina should queue ExhaustSpecificCardAction first");
    assert_eq!(
        exhaust,
        Action::ExhaustCard {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(1013),
            source_pile: crate::state::PileType::Hand,
        },
        "Java DeusExMachina.triggerWhenDrawn addToTop order makes ExhaustSpecificCardAction execute first"
    );
    let make_miracles = draw_state
        .pop_next_action()
        .expect("Deus Ex Machina should queue MakeTempCardInHandAction after exhaust");
    match &make_miracles {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Miracle);
            assert_eq!(original.upgrades, 0);
            assert_eq!(*amount, 3);
        }
        other => panic!("Deus Ex Machina should generate constructed Miracles, got {other:?}"),
    }
    crate::engine::action_handlers::execute_action(exhaust, &mut draw_state);
    crate::engine::action_handlers::execute_action(make_miracles, &mut draw_state);
    assert_eq!(draw_state.zones.exhaust_pile.len(), 1);
    assert_eq!(draw_state.zones.exhaust_pile[0].id, CardId::DeusExMachina);
    assert_eq!(draw_state.zones.hand.len(), 3);
    assert!(draw_state
        .zones
        .hand
        .iter()
        .all(|card| card.id == CardId::Miracle && card.upgrades == 0));
}

#[test]
fn watcher_alpha_beta_omega_chain_matches_java_sources() {
    let java_map = build_java_id_map();
    for (id, java) in [
        (CardId::Alpha, "Alpha"),
        (CardId::Beta, "Beta"),
        (CardId::Omega, "Omega"),
    ] {
        assert_eq!(java_id(id), java);
        assert_eq!(java_map.get(java), Some(&id));
    }

    let alpha = get_card_definition(CardId::Alpha);
    assert_eq!(alpha.name, "Alpha");
    assert_eq!(alpha.card_type, CardType::Skill);
    assert_eq!(alpha.rarity, CardRarity::Rare);
    assert_eq!(alpha.cost, 1);
    assert_eq!(alpha.target, CardTarget::None);
    assert!(alpha.exhaust);
    assert!(WATCHER_RARE_POOL.contains(&CardId::Alpha));
    assert!(!crate::content::cards::is_innate_card(
        &CombatCard::for_scenario(CardId::Alpha, 1020)
    ));
    let mut alpha_plus = CombatCard::for_scenario(CardId::Alpha, 1021);
    alpha_plus.upgrades = 1;
    assert!(
        crate::content::cards::is_innate_card(&alpha_plus),
        "Java Alpha.upgrade sets isInnate"
    );

    let beta = get_card_definition(CardId::Beta);
    assert_eq!(beta.name, "Beta");
    assert_eq!(beta.card_type, CardType::Skill);
    assert_eq!(beta.rarity, CardRarity::Special);
    assert_eq!(beta.cost, 2);
    assert_eq!(beta.target, CardTarget::None);
    assert!(beta.exhaust);
    let mut beta_plus = CombatCard::for_scenario(CardId::Beta, 1022);
    beta_plus.upgrades = 1;
    assert_eq!(
        beta_plus.get_cost(),
        1,
        "Java Beta.upgrade uses upgradeBaseCost(1)"
    );

    let omega = get_card_definition(CardId::Omega);
    assert_eq!(omega.name, "Omega");
    assert_eq!(omega.card_type, CardType::Power);
    assert_eq!(omega.rarity, CardRarity::Special);
    assert_eq!(omega.cost, 3);
    assert_eq!(omega.base_magic, 50);
    assert_eq!(omega.upgrade_magic, 10);
    assert_eq!(omega.target, CardTarget::SelfTarget);

    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    state.entities.monsters = vec![first, second];

    let alpha_actions = resolve_card_play(CardId::Alpha, &state, &alpha_plus, None);
    assert_eq!(
        alpha_actions[0].action,
        Action::MakeTempCardInDrawPile {
            card_id: CardId::Beta,
            amount: 1,
            random_spot: true,
            to_bottom: false,
            upgraded: false,
        },
        "Java Alpha.use queues MakeTempCardInDrawPileAction(Beta, 1, true, true)"
    );

    let beta_actions = resolve_card_play(CardId::Beta, &state, &beta_plus, None);
    assert_eq!(
        beta_actions[0].action,
        Action::MakeTempCardInDrawPile {
            card_id: CardId::Omega,
            amount: 1,
            random_spot: true,
            to_bottom: false,
            upgraded: false,
        },
        "Java Beta.use queues MakeTempCardInDrawPileAction(Omega, 1, true, true)"
    );

    let mut omega_plus = CombatCard::for_scenario(CardId::Omega, 1023);
    omega_plus.upgrades = 1;
    let omega_actions = resolve_card_play(CardId::Omega, &state, &omega_plus, None);
    assert_eq!(
        omega_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::OmegaPower,
            amount: 60,
        }
    );
    let omega_power = Power {
        power_type: PowerId::OmegaPower,
        instance_id: None,
        amount: 60,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    };
    let end_turn =
        crate::content::powers::resolve_power_at_end_of_turn(&omega_power, &mut state, 0);
    assert_eq!(end_turn.len(), 1);
    assert_eq!(
        end_turn[0],
        Action::DamageAllEnemies {
            source: crate::runtime::action::NO_SOURCE,
            damages: smallvec::smallvec![60, 60],
            damage_type: DamageType::Thorns,
            is_modified: false,
        },
        "Java OmegaPower.atEndOfTurn queues DamageAllEnemiesAction(null, ...) with THORNS damage"
    );
}

#[test]
fn watcher_conjure_blade_and_expunger_match_java_sources() {
    let java_map = build_java_id_map();
    for (id, java) in [
        (CardId::ConjureBlade, "ConjureBlade"),
        (CardId::Expunger, "Expunger"),
    ] {
        assert_eq!(java_id(id), java);
        assert_eq!(java_map.get(java), Some(&id));
    }

    let conjure = get_card_definition(CardId::ConjureBlade);
    assert_eq!(conjure.name, "Conjure Blade");
    assert_eq!(conjure.card_type, CardType::Skill);
    assert_eq!(conjure.rarity, CardRarity::Rare);
    assert_eq!(conjure.cost, -1);
    assert_eq!(conjure.target, CardTarget::SelfTarget);
    assert!(conjure.exhaust);
    assert!(WATCHER_RARE_POOL.contains(&CardId::ConjureBlade));

    let expunger = get_card_definition(CardId::Expunger);
    assert_eq!(expunger.name, "Expunger");
    assert_eq!(expunger.card_type, CardType::Attack);
    assert_eq!(expunger.rarity, CardRarity::Special);
    assert_eq!(expunger.cost, 1);
    assert_eq!(expunger.base_damage, 9);
    assert_eq!(expunger.upgrade_damage, 6);
    assert_eq!(expunger.target, CardTarget::Enemy);

    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 2;
    let mut conjure_plus = CombatCard::for_scenario(CardId::ConjureBlade, 1030);
    conjure_plus.upgrades = 1;
    conjure_plus.energy_on_use = 2;
    let conjure_actions = resolve_card_play(CardId::ConjureBlade, &state, &conjure_plus, None);
    assert_eq!(
        conjure_actions[0].action,
        Action::ConjureBlade {
            free_to_play_once: false,
            energy_on_use: 3,
        },
        "Java ConjureBlade+ passes energyOnUse + 1 into ConjureBladeAction"
    );

    crate::engine::action_handlers::execute_action(conjure_actions[0].action.clone(), &mut state);
    assert_eq!(
        state.turn.energy, 0,
        "Java ConjureBladeAction spends current energy even though generated Expunger stores the boosted X"
    );
    assert_eq!(state.zones.draw_pile.len(), 1);
    assert_eq!(state.zones.draw_pile[0].id, CardId::Expunger);
    assert_eq!(
        state.zones.draw_pile[0].misc_value, 3,
        "Rust stores Java Expunger.setX(effect) in misc_value so stat-equivalent copies preserve it"
    );

    let mut expunge_state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    expunge_state.entities.monsters = vec![target];
    let expunger_card = state.zones.draw_pile[0].clone();
    let expunger_actions =
        resolve_card_play(CardId::Expunger, &expunge_state, &expunger_card, Some(7));
    assert_eq!(expunger_actions.len(), 3);
    for action in expunger_actions {
        match action.action {
            Action::Damage(info) => {
                assert_eq!(info.target, 7);
                assert_eq!(info.base, 9);
                assert_eq!(info.output, 9);
            }
            other => panic!("Expunger should emit repeated DamageAction, got {other:?}"),
        }
    }

    let mut master_state = crate::test_support::blank_test_combat();
    master_state.turn.energy = 1;
    crate::content::powers::store::set_powers_for(
        &mut master_state,
        0,
        vec![Power {
            power_type: PowerId::MasterRealityPower,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::ConjureBlade {
            free_to_play_once: false,
            energy_on_use: 1,
        },
        &mut master_state,
    );
    assert_eq!(master_state.zones.draw_pile[0].id, CardId::Expunger);
    assert_eq!(master_state.zones.draw_pile[0].upgrades, 1);
    assert_eq!(
        master_state.zones.draw_pile[0].misc_value, 1,
        "Java Master Reality upgrades Expunger damage after setX; it does not add a hit"
    );
}

#[test]
fn watcher_meditate_matches_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::Meditate), "Meditate");
    assert_eq!(java_map.get("Meditate"), Some(&CardId::Meditate));

    let meditate = get_card_definition(CardId::Meditate);
    assert_eq!(meditate.name, "Meditate");
    assert_eq!(meditate.card_type, CardType::Skill);
    assert_eq!(meditate.rarity, CardRarity::Uncommon);
    assert_eq!(meditate.cost, 1);
    assert_eq!(meditate.base_magic, 1);
    assert_eq!(meditate.upgrade_magic, 1);
    assert_eq!(meditate.target, CardTarget::None);
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::Meditate));

    let state = crate::test_support::blank_test_combat();
    let mut meditate_plus = CombatCard::for_scenario(CardId::Meditate, 1040);
    meditate_plus.upgrades = 1;
    let actions = resolve_card_play(CardId::Meditate, &state, &meditate_plus, None);
    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::Meditate { amount: 2 },
            &Action::EnterStance("Calm".to_string()),
            &Action::QueueEarlyEndTurn,
        ],
        "Java Meditate.use queues MeditateAction, ChangeStanceAction(Calm), then PressEndTurnButtonAction"
    );

    let mut all_state = crate::test_support::blank_test_combat();
    all_state.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::StrikeP, 1041),
        CombatCard::for_scenario(CardId::DefendP, 1042),
    ]
    .into();
    crate::engine::action_handlers::execute_action(Action::Meditate { amount: 2 }, &mut all_state);
    assert!(all_state.zones.discard_pile.is_empty());
    assert_eq!(all_state.zones.hand.len(), 2);
    assert!(
        all_state
            .zones
            .hand
            .iter()
            .all(|card| card.retain_override == Some(true)),
        "Java MeditateAction marks returned cards retain=true"
    );

    let mut select_state = crate::test_support::blank_test_combat();
    select_state.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::StrikeP, 1043),
        CombatCard::for_scenario(CardId::DefendP, 1044),
    ]
    .into();
    crate::engine::action_handlers::execute_action(
        Action::Meditate { amount: 1 },
        &mut select_state,
    );
    assert_eq!(
        select_state.pop_next_action(),
        Some(Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Discard,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Any,
            reason: crate::state::GridSelectReason::DiscardToHandRetain,
        })
    );
}

#[test]
fn watcher_vault_matches_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::Vault), "Vault");
    assert_eq!(java_map.get("Vault"), Some(&CardId::Vault));

    let vault = get_card_definition(CardId::Vault);
    assert_eq!(vault.name, "Vault");
    assert_eq!(vault.card_type, CardType::Skill);
    assert_eq!(vault.rarity, CardRarity::Rare);
    assert_eq!(vault.cost, 3);
    assert_eq!(vault.target, CardTarget::All);
    assert!(vault.exhaust);
    assert!(WATCHER_RARE_POOL.contains(&CardId::Vault));

    let state = crate::test_support::blank_test_combat();
    let mut vault_plus = CombatCard::for_scenario(CardId::Vault, 1045);
    vault_plus.upgrades = 1;
    assert_eq!(vault_plus.get_cost(), 2);

    let actions = resolve_card_play(CardId::Vault, &state, &vault_plus, None);
    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![&Action::SkipEnemiesTurn, &Action::QueueEarlyEndTurn],
        "Java Vault queues SkipEnemiesTurnAction before PressEndTurnButtonAction; VFX is intentionally not simulated"
    );

    let mut action_state = crate::test_support::blank_test_combat();
    crate::engine::action_handlers::execute_action(Action::SkipEnemiesTurn, &mut action_state);
    assert!(
        action_state.turn.counters.skip_monster_turn_pending,
        "SkipEnemiesTurnAction sets AbstractRoom.skipMonsterTurn=true"
    );
}

#[test]
fn watcher_omniscience_matches_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::Omniscience), "Omniscience");
    assert_eq!(java_map.get("Omniscience"), Some(&CardId::Omniscience));

    let omni = get_card_definition(CardId::Omniscience);
    assert_eq!(omni.name, "Omniscience");
    assert_eq!(omni.card_type, CardType::Skill);
    assert_eq!(omni.rarity, CardRarity::Rare);
    assert_eq!(omni.cost, 4);
    assert_eq!(omni.base_magic, 2);
    assert_eq!(omni.target, CardTarget::None);
    assert!(omni.exhaust);
    assert!(WATCHER_RARE_POOL.contains(&CardId::Omniscience));

    let mut omni_plus = CombatCard::for_scenario(CardId::Omniscience, 1046);
    omni_plus.upgrades = 1;
    assert_eq!(omni_plus.get_cost(), 3);

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(CardId::Omniscience, &state, &omni_plus, None);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Draw,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Any,
            reason: crate::state::GridSelectReason::Omniscience { play_amount: 2 },
        },
        "Java Omniscience queues OmniscienceAction(magicNumber), which opens draw-pile grid select"
    );
}

#[test]
fn watcher_foreign_influence_matches_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::ForeignInfluence), "ForeignInfluence");
    assert_eq!(
        java_map.get("ForeignInfluence"),
        Some(&CardId::ForeignInfluence)
    );

    let foreign = get_card_definition(CardId::ForeignInfluence);
    assert_eq!(foreign.name, "Foreign Influence");
    assert_eq!(foreign.card_type, CardType::Skill);
    assert_eq!(foreign.rarity, CardRarity::Uncommon);
    assert_eq!(foreign.cost, 0);
    assert_eq!(foreign.target, CardTarget::None);
    assert!(foreign.exhaust);
    assert!(WATCHER_UNCOMMON_POOL.contains(&CardId::ForeignInfluence));

    let state = crate::test_support::blank_test_combat();
    let mut foreign_plus = CombatCard::for_scenario(CardId::ForeignInfluence, 1048);
    foreign_plus.upgrades = 1;
    let actions = resolve_card_play(CardId::ForeignInfluence, &state, &foreign_plus, None);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::SuspendForForeignInfluence { upgraded: true },
        "Java ForeignInfluence.use queues ForeignInfluenceAction(this.upgraded)"
    );
}

#[test]
fn watcher_lesson_learned_matches_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::LessonLearned), "LessonLearned");
    assert_eq!(java_map.get("LessonLearned"), Some(&CardId::LessonLearned));

    let lesson = get_card_definition(CardId::LessonLearned);
    assert_eq!(lesson.name, "Lesson Learned");
    assert_eq!(lesson.card_type, CardType::Attack);
    assert_eq!(lesson.rarity, CardRarity::Rare);
    assert_eq!(lesson.cost, 2);
    assert_eq!(lesson.base_damage, 10);
    assert_eq!(lesson.upgrade_damage, 3);
    assert_eq!(lesson.target, CardTarget::Enemy);
    assert!(lesson.exhaust);
    assert!(lesson.tags.contains(&CardTag::Healing));
    assert!(WATCHER_RARE_POOL.contains(&CardId::LessonLearned));

    let state = crate::test_support::blank_test_combat();
    let mut lesson_plus = CombatCard::for_scenario(CardId::LessonLearned, 1049);
    lesson_plus.upgrades = 1;
    let actions = resolve_card_play(CardId::LessonLearned, &state, &lesson_plus, Some(7));
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::LessonLearned {
            target: 7,
            damage_info: DamageInfo {
                source: 0,
                target: 7,
                base: 13,
                output: 13,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
        },
        "Java LessonLearned.use queues LessonLearnedAction with this.damage"
    );
}

#[test]
fn watcher_wish_and_option_cards_match_java_sources() {
    let java_map = build_java_id_map();
    assert_eq!(java_id(CardId::Wish), "Wish");
    assert_eq!(java_map.get("Wish"), Some(&CardId::Wish));
    assert_eq!(
        java_map.get("BecomeAlmighty"),
        Some(&CardId::BecomeAlmighty)
    );
    assert_eq!(
        java_map.get("FameAndFortune"),
        Some(&CardId::FameAndFortune)
    );
    assert_eq!(java_map.get("LiveForever"), Some(&CardId::LiveForever));

    let wish = get_card_definition(CardId::Wish);
    assert_eq!(wish.name, "Wish");
    assert_eq!(wish.card_type, CardType::Skill);
    assert_eq!(wish.rarity, CardRarity::Rare);
    assert_eq!(wish.cost, 3);
    assert_eq!(wish.base_damage, 3);
    assert_eq!(wish.base_magic, 25);
    assert_eq!(wish.base_block, 6);
    assert_eq!(wish.target, CardTarget::None);
    assert!(wish.exhaust);
    assert!(wish.tags.contains(&CardTag::Healing));
    assert!(WATCHER_RARE_POOL.contains(&CardId::Wish));

    let almighty = get_card_definition(CardId::BecomeAlmighty);
    assert_eq!(almighty.card_type, CardType::Power);
    assert_eq!(almighty.rarity, CardRarity::Special);
    assert_eq!(almighty.cost, -2);
    assert_eq!(almighty.base_magic, 3);
    assert_eq!(almighty.upgrade_magic, 1);

    let fortune = get_card_definition(CardId::FameAndFortune);
    assert_eq!(fortune.card_type, CardType::Skill);
    assert_eq!(fortune.rarity, CardRarity::Special);
    assert_eq!(fortune.cost, -2);
    assert_eq!(fortune.base_magic, 25);
    assert_eq!(fortune.upgrade_magic, 5);

    let live = get_card_definition(CardId::LiveForever);
    assert_eq!(live.card_type, CardType::Power);
    assert_eq!(live.rarity, CardRarity::Special);
    assert_eq!(live.cost, -2);
    assert_eq!(live.base_magic, 6);
    assert_eq!(live.upgrade_magic, 2);

    let state = crate::test_support::blank_test_combat();
    let mut wish_plus = CombatCard::for_scenario(CardId::Wish, 1047);
    wish_plus.upgrades = 1;
    let actions = resolve_card_play(CardId::Wish, &state, &wish_plus, None);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::SuspendForChooseOne {
            choices: vec![
                crate::state::ChooseOneCardChoice {
                    card_id: CardId::BecomeAlmighty,
                    upgrades: 1,
                },
                crate::state::ChooseOneCardChoice {
                    card_id: CardId::FameAndFortune,
                    upgrades: 1,
                },
                crate::state::ChooseOneCardChoice {
                    card_id: CardId::LiveForever,
                    upgrades: 1,
                },
            ],
        },
        "Java Wish upgrades the three option cards before ChooseOneAction when Wish is upgraded"
    );
}

#[test]
fn watcher_wish_evaluation_matches_empty_java_apply_powers() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![
            Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 9,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::Dexterity,
                instance_id: None,
                amount: 9,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
        ],
    );

    let mut wish_plus = CombatCard::for_scenario(CardId::Wish, 1048);
    wish_plus.upgrades = 1;
    let evaluated = evaluate_card_for_play(&wish_plus, &state, None);

    assert_eq!(
        evaluated.base_damage_mut, 4,
        "Java Wish.applyPowers is empty, so Strength must not modify its option damage value"
    );
    assert_eq!(
        evaluated.base_block_mut, 8,
        "Java Wish.applyPowers is empty, so Dexterity must not modify its option block value"
    );
    assert_eq!(evaluated.base_magic_num_mut, 30);
}

#[test]
fn watcher_wish_option_callbacks_match_java_on_chose_this_option() {
    let state = crate::test_support::blank_test_combat();

    let almighty = resolve_choose_one_option(CardId::BecomeAlmighty, 1, &state);
    assert_eq!(
        almighty[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 4,
        },
        "BecomeAlmighty+ applies 4 Strength"
    );

    let fortune = resolve_choose_one_option(CardId::FameAndFortune, 1, &state);
    assert_eq!(
        fortune[0].action,
        Action::GainGold { amount: 30 },
        "FameAndFortune+ gains 30 gold"
    );

    let live = resolve_choose_one_option(CardId::LiveForever, 1, &state);
    assert_eq!(
        live[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::PlatedArmor,
            amount: 8,
        },
        "LiveForever+ applies 8 Plated Armor"
    );
}

#[test]
fn watcher_scry_card_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let mut just_lucky_plus = CombatCard::for_scenario(CardId::JustLucky, 310);
    just_lucky_plus.upgrades = 1;
    let just_lucky = resolve_card_play(CardId::JustLucky, &state, &just_lucky_plus, Some(7));
    assert_eq!(just_lucky.len(), 3);
    assert_eq!(just_lucky[0].action, Action::Scry(2));
    assert_eq!(
        just_lucky[1].action,
        Action::GainBlock {
            target: 0,
            amount: 3,
        }
    );
    match &just_lucky[2].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 4);
            assert_eq!(info.output, 4);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Just Lucky final action should be DamageAction, got {other:?}"),
    }

    let cut = resolve_card_play(
        CardId::CutThroughFate,
        &state,
        &CombatCard::for_scenario(CardId::CutThroughFate, 311),
        Some(7),
    );
    assert_eq!(cut.len(), 3);
    match &cut[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 7);
        }
        other => panic!("Cut Through Fate first action should be DamageAction, got {other:?}"),
    }
    assert_eq!(cut[1].action, Action::Scry(2));
    assert_eq!(cut[2].action, Action::DrawCards(1));

    let mut third_eye_plus = CombatCard::for_scenario(CardId::ThirdEye, 312);
    third_eye_plus.upgrades = 1;
    let third_eye = resolve_card_play(CardId::ThirdEye, &state, &third_eye_plus, None);
    assert_eq!(
        third_eye
            .iter()
            .map(|info| &info.action)
            .collect::<Vec<_>>(),
        vec![
            &Action::GainBlock {
                target: 0,
                amount: 9,
            },
            &Action::Scry(5),
        ]
    );
}

#[test]
fn watcher_prostrate_runtime_actions_match_java_use_method() {
    let state = crate::test_support::blank_test_combat();
    let mut prostrate_plus = CombatCard::for_scenario(CardId::Prostrate, 320);
    prostrate_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::Prostrate, &state, &prostrate_plus, None);

    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Mantra,
                amount: 3,
            },
            &Action::GainBlock {
                target: 0,
                amount: 4,
            },
        ]
    );
}

#[test]
fn watcher_wheel_kick_runtime_actions_match_java_use_method() {
    let state = crate::test_support::blank_test_combat();
    let mut wheel_kick_plus = CombatCard::for_scenario(CardId::WheelKick, 321);
    wheel_kick_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::WheelKick, &state, &wheel_kick_plus, Some(7));

    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 20);
            assert_eq!(info.output, 20);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Wheel Kick first action should be DamageAction, got {other:?}"),
    }
    assert_eq!(actions[1].action, Action::DrawCards(2));
}

#[test]
fn watcher_inner_peace_runtime_action_reads_stance_when_it_resolves() {
    let state = crate::test_support::blank_test_combat();
    let mut inner_peace_plus = CombatCard::for_scenario(CardId::InnerPeace, 322);
    inner_peace_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::InnerPeace, &state, &inner_peace_plus, None);

    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![&Action::InnerPeace { draw_amount: 4 }]
    );

    let mut calm_state = crate::test_support::blank_test_combat();
    calm_state.entities.player.stance = StanceId::Calm;
    crate::engine::action_handlers::execute_action(
        Action::InnerPeace { draw_amount: 4 },
        &mut calm_state,
    );
    assert_eq!(calm_state.pop_next_action(), Some(Action::DrawCards(4)));

    let mut neutral_state = crate::test_support::blank_test_combat();
    crate::engine::action_handlers::execute_action(
        Action::InnerPeace { draw_amount: 4 },
        &mut neutral_state,
    );
    assert_eq!(
        neutral_state.pop_next_action(),
        Some(Action::EnterStance("Calm".to_string()))
    );
}

#[test]
fn watcher_fear_no_evil_runtime_action_reads_target_intent_when_it_resolves() {
    let state = crate::test_support::blank_test_combat();
    let mut fear_no_evil_plus = CombatCard::for_scenario(CardId::FearNoEvil, 323);
    fear_no_evil_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::FearNoEvil, &state, &fear_no_evil_plus, Some(7));

    assert_eq!(actions.len(), 1);
    let damage_info = match &actions[0].action {
        Action::FearNoEvil {
            target,
            damage_info,
        } => {
            assert_eq!(*target, 7);
            assert_eq!(damage_info.source, 0);
            assert_eq!(damage_info.target, 7);
            assert_eq!(damage_info.base, 11);
            assert_eq!(damage_info.output, 11);
            damage_info.clone()
        }
        other => panic!("Fear No Evil should emit FearNoEvilAction, got {other:?}"),
    };

    let mut attack_state = crate::test_support::blank_test_combat();
    attack_state
        .set_monster_protocol_visible_intent(7, Intent::AttackDefend { damage: 9, hits: 1 });
    crate::engine::action_handlers::execute_action(
        Action::FearNoEvil {
            target: 7,
            damage_info: damage_info.clone(),
        },
        &mut attack_state,
    );
    assert_eq!(
        attack_state.pop_next_action(),
        Some(Action::Damage(damage_info.clone())),
        "Java adds ChangeStance then Damage to top, so Damage resolves first"
    );
    assert_eq!(
        attack_state.pop_next_action(),
        Some(Action::EnterStance("Calm".to_string()))
    );

    let mut defend_state = crate::test_support::blank_test_combat();
    defend_state.set_monster_protocol_visible_intent(7, Intent::Defend);
    crate::engine::action_handlers::execute_action(
        Action::FearNoEvil {
            target: 7,
            damage_info: damage_info.clone(),
        },
        &mut defend_state,
    );
    assert_eq!(
        defend_state.pop_next_action(),
        Some(Action::Damage(damage_info))
    );
    assert_eq!(defend_state.pop_next_action(), None);
}

#[test]
fn watcher_indignation_runtime_action_reads_stance_when_it_resolves() {
    let state = crate::test_support::blank_test_combat();
    let mut indignation_plus = CombatCard::for_scenario(CardId::Indignation, 324);
    indignation_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::Indignation, &state, &indignation_plus, None);

    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![&Action::Indignation { amount: 5 }]
    );

    let mut wrath_state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    wrath_state.entities.monsters = vec![first, second];
    wrath_state.entities.player.stance = StanceId::Wrath;
    crate::engine::action_handlers::execute_action(
        Action::Indignation { amount: 5 },
        &mut wrath_state,
    );
    assert_eq!(
        wrath_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Vulnerable,
            amount: 5,
        })
    );
    assert_eq!(
        wrath_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 8,
            power_id: PowerId::Vulnerable,
            amount: 5,
        })
    );

    let mut neutral_state = crate::test_support::blank_test_combat();
    crate::engine::action_handlers::execute_action(
        Action::Indignation { amount: 5 },
        &mut neutral_state,
    );
    assert_eq!(
        neutral_state.pop_next_action(),
        Some(Action::EnterStance("Wrath".to_string()))
    );
}

#[test]
fn watcher_follow_up_runtime_action_reads_previous_played_card_when_it_resolves() {
    let state = crate::test_support::blank_test_combat();
    let mut follow_up_plus = CombatCard::for_scenario(CardId::FollowUp, 325);
    follow_up_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::FollowUp, &state, &follow_up_plus, Some(7));

    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 11);
            assert_eq!(info.output, 11);
        }
        other => panic!("Follow-Up first action should be DamageAction, got {other:?}"),
    }
    assert_eq!(actions[1].action, Action::FollowUp);

    let mut attack_previous = crate::test_support::blank_test_combat();
    attack_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::StrikeP, CardId::FollowUp];
    crate::engine::action_handlers::execute_action(Action::FollowUp, &mut attack_previous);
    assert_eq!(
        attack_previous.pop_next_action(),
        Some(Action::GainEnergy { amount: 1 })
    );

    let mut skill_previous = crate::test_support::blank_test_combat();
    skill_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::DefendP, CardId::FollowUp];
    crate::engine::action_handlers::execute_action(Action::FollowUp, &mut skill_previous);
    assert_eq!(skill_previous.pop_next_action(), None);
}

#[test]
fn watcher_sanctity_runtime_action_reads_previous_played_card_when_it_resolves() {
    let state = crate::test_support::blank_test_combat();
    let mut sanctity_plus = CombatCard::for_scenario(CardId::Sanctity, 326);
    sanctity_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::Sanctity, &state, &sanctity_plus, None);

    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::GainBlock {
                target: 0,
                amount: 9,
            },
            &Action::Sanctity { draw_amount: 2 },
        ]
    );

    let mut skill_previous = crate::test_support::blank_test_combat();
    skill_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::DefendP, CardId::Sanctity];
    crate::engine::action_handlers::execute_action(
        Action::Sanctity { draw_amount: 2 },
        &mut skill_previous,
    );
    assert_eq!(skill_previous.pop_next_action(), Some(Action::DrawCards(2)));

    let mut attack_previous = crate::test_support::blank_test_combat();
    attack_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::StrikeP, CardId::Sanctity];
    crate::engine::action_handlers::execute_action(
        Action::Sanctity { draw_amount: 2 },
        &mut attack_previous,
    );
    assert_eq!(attack_previous.pop_next_action(), None);
}

#[test]
fn watcher_crush_joints_runtime_action_reads_previous_played_card_when_it_resolves() {
    let state = crate::test_support::blank_test_combat();
    let mut crush_joints_plus = CombatCard::for_scenario(CardId::CrushJoints, 327);
    crush_joints_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::CrushJoints, &state, &crush_joints_plus, Some(7));

    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 10);
            assert_eq!(info.output, 10);
        }
        other => panic!("Crush Joints first action should be DamageAction, got {other:?}"),
    }
    assert_eq!(
        actions[1].action,
        Action::CrushJoints {
            target: 7,
            amount: 2,
        }
    );

    let mut skill_previous = crate::test_support::blank_test_combat();
    skill_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::DefendP, CardId::CrushJoints];
    crate::engine::action_handlers::execute_action(
        Action::CrushJoints {
            target: 7,
            amount: 2,
        },
        &mut skill_previous,
    );
    assert_eq!(
        skill_previous.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Vulnerable,
            amount: 2,
        })
    );

    let mut attack_previous = crate::test_support::blank_test_combat();
    attack_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::StrikeP, CardId::CrushJoints];
    crate::engine::action_handlers::execute_action(
        Action::CrushJoints {
            target: 7,
            amount: 2,
        },
        &mut attack_previous,
    );
    assert_eq!(attack_previous.pop_next_action(), None);
}

#[test]
fn watcher_sash_whip_runtime_action_reads_previous_played_card_when_it_resolves() {
    let state = crate::test_support::blank_test_combat();
    let mut sash_whip_plus = CombatCard::for_scenario(CardId::SashWhip, 328);
    sash_whip_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::SashWhip, &state, &sash_whip_plus, Some(7));

    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 10);
            assert_eq!(info.output, 10);
        }
        other => panic!("Sash Whip first action should be DamageAction, got {other:?}"),
    }
    assert_eq!(
        actions[1].action,
        Action::SashWhip {
            target: 7,
            amount: 2,
        }
    );

    let mut attack_previous = crate::test_support::blank_test_combat();
    attack_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::StrikeP, CardId::SashWhip];
    crate::engine::action_handlers::execute_action(
        Action::SashWhip {
            target: 7,
            amount: 2,
        },
        &mut attack_previous,
    );
    assert_eq!(
        attack_previous.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 7,
            power_id: PowerId::Weak,
            amount: 2,
        })
    );

    let mut skill_previous = crate::test_support::blank_test_combat();
    skill_previous.turn.counters.card_ids_played_this_combat =
        vec![CardId::DefendP, CardId::SashWhip];
    crate::engine::action_handlers::execute_action(
        Action::SashWhip {
            target: 7,
            amount: 2,
        },
        &mut skill_previous,
    );
    assert_eq!(skill_previous.pop_next_action(), None);
}

#[test]
fn watcher_conclude_runtime_actions_match_java_use_method() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 7;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 8;
    state.entities.monsters = vec![first, second];
    let mut conclude_plus = CombatCard::for_scenario(CardId::Conclude, 329);
    conclude_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::Conclude, &state, &conclude_plus, None);

    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            ..
        } => {
            assert_eq!(*source, 0);
            assert_eq!(damages.as_slice(), &[16, 16]);
            assert_eq!(*damage_type, DamageType::Normal);
        }
        other => panic!("Conclude first action should be DamageAllEnemiesAction, got {other:?}"),
    }
    assert_eq!(actions[1].action, Action::QueueEarlyEndTurn);

    crate::content::powers::store::set_powers_for(
        &mut state,
        8,
        vec![Power {
            power_type: PowerId::Vulnerable,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let vulnerable_actions = resolve_card_play(CardId::Conclude, &state, &conclude_plus, None);
    match &vulnerable_actions[0].action {
        Action::DamageAllEnemies { damages, .. } => {
            assert_eq!(
                damages.as_slice(),
                &[16, 24],
                "Java Conclude passes this.multiDamage, so target-specific damage modifiers must be preserved"
            );
        }
        other => {
            panic!("Conclude first action should remain DamageAllEnemiesAction, got {other:?}")
        }
    }
}

#[test]
fn watcher_brilliance_uses_combat_mantra_gained_before_damage_modifiers() {
    let mut counter_state = crate::test_support::blank_test_combat();
    crate::engine::action_handlers::execute_action(
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Mantra,
            amount: 4,
        },
        &mut counter_state,
    );
    assert_eq!(counter_state.turn.counters.mantra_gained_this_combat, 4);

    let mut state = crate::test_support::blank_test_combat();
    state.turn.counters.mantra_gained_this_combat = 4;
    state.entities.player.stance = StanceId::Wrath;
    let mut brilliance_plus = CombatCard::for_scenario(CardId::Brilliance, 330);
    brilliance_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::Brilliance, &state, &brilliance_plus, Some(7));

    assert_eq!(actions.len(), 1);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(
                info.base, 40,
                "Java adds mantra gained to Brilliance base damage before Wrath"
            );
            assert_eq!(info.output, 40);
        }
        other => panic!("Brilliance should emit DamageAction, got {other:?}"),
    }

    let mut hand_state = state.clone();
    hand_state.zones.hand = vec![brilliance_plus.clone()];
    hand_state.update_hand_cards();
    assert_eq!(
        hand_state.zones.hand[0].base_damage_mut, 40,
        "Java Brilliance.applyPowers also updates rendered/observed damage, not only play-time damage"
    );
    assert_eq!(
        hand_state.zones.hand[0].base_magic_num_mut, 4,
        "Java Brilliance.applyPowers mirrors actionManager.mantraGained into baseMagicNumber"
    );
}

#[test]
fn watcher_ragnarok_runtime_actions_match_java_use_method() {
    let state = crate::test_support::blank_test_combat();
    let mut ragnarok_plus = CombatCard::for_scenario(CardId::Ragnarok, 331);
    ragnarok_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::Ragnarok, &state, &ragnarok_plus, None);

    assert_eq!(
        actions.len(),
        6,
        "Java Ragnarok queues one AttackDamageRandomEnemyAction per magicNumber"
    );
    for action in &actions {
        match &action.action {
            Action::AttackDamageRandomEnemyCard { card } => {
                assert_eq!(card.id, CardId::Ragnarok);
                assert_eq!(card.upgrades, 1);
            }
            other => panic!("Ragnarok should emit AttackDamageRandomEnemyCard, got {other:?}"),
        }
    }
}

#[test]
fn watcher_wreath_of_flame_runtime_actions_match_java_use_method() {
    let state = crate::test_support::blank_test_combat();
    let mut wreath_plus = CombatCard::for_scenario(CardId::WreathOfFlame, 332);
    wreath_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::WreathOfFlame, &state, &wreath_plus, None);

    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![&Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Vigor,
            amount: 8,
        }]
    );
}

#[test]
fn watcher_signature_move_can_use_requires_no_other_attacks_and_use_matches_java() {
    let mut allowed = crate::test_support::blank_test_combat();
    allowed.zones.hand = vec![
        CombatCard::for_scenario(CardId::SignatureMove, 333),
        CombatCard::for_scenario(CardId::DefendP, 334),
    ];
    assert!(can_play_card(&allowed.zones.hand[0], &allowed).is_ok());

    let mut blocked = allowed.clone();
    blocked
        .zones
        .hand
        .push(CombatCard::for_scenario(CardId::StrikeP, 335));
    assert!(
        can_play_card(&blocked.zones.hand[0], &blocked).is_err(),
        "Java SignatureMove.canUse rejects other Attack cards in hand"
    );

    let mut signature_plus = CombatCard::for_scenario(CardId::SignatureMove, 336);
    signature_plus.upgrades = 1;
    let mut playable_plus = crate::test_support::blank_test_combat();
    playable_plus.zones.hand = vec![
        signature_plus.clone(),
        CombatCard::for_scenario(CardId::DefendP, 337),
    ];
    assert!(can_play_card(&playable_plus.zones.hand[0], &playable_plus).is_ok());
    let play_actions = resolve_card_play(
        CardId::SignatureMove,
        &playable_plus,
        &signature_plus,
        Some(7),
    );
    assert_eq!(play_actions.len(), 1);
    match &play_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 40);
            assert_eq!(info.output, 40);
        }
        other => panic!("Signature Move should emit DamageAction, got {other:?}"),
    }

    let mut autoplay_blocked = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    autoplay_blocked.entities.monsters = vec![target];
    autoplay_blocked.zones.hand = vec![CombatCard::for_scenario(CardId::StrikeP, 338)];
    autoplay_blocked.enqueue_card_play(
        crate::runtime::combat::QueuedCardPlay {
            card: CombatCard::for_scenario(CardId::SignatureMove, 339),
            target: Some(7),
            energy_on_use: 0,
            ignore_energy_total: true,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: crate::runtime::combat::QueuedCardSource::Normal,
        },
        false,
    );
    let flush = autoplay_blocked
        .pop_next_action()
        .expect("queued autoplay Signature Move should schedule queue flush");
    crate::engine::action_handlers::execute_action(flush, &mut autoplay_blocked);
    assert!(
        matches!(
            autoplay_blocked.pop_next_action(),
            Some(Action::UseCardDone {
                should_exhaust: false,
                trigger_after_use_hooks: false,
            })
        ),
        "Java queued/autoplay cards still call canUse; Signature Move is blocked by another Attack in hand"
    );
}

#[test]
fn watcher_simmering_fury_runtime_actions_match_java_use_and_power_hooks() {
    let state = crate::test_support::blank_test_combat();
    let mut simmering_plus = CombatCard::for_scenario(CardId::SimmeringFury, 340);
    simmering_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::SimmeringFury, &state, &simmering_plus, None);

    assert_eq!(
        actions.iter().map(|info| &info.action).collect::<Vec<_>>(),
        vec![
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::WrathNextTurn,
                amount: -1,
            },
            &Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::DrawCardNextTurn,
                amount: 3,
            },
        ]
    );

    let mut hook_state = crate::test_support::blank_test_combat();
    let hook_actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::WrathNextTurn,
        &mut hook_state,
        0,
        -1,
    );
    assert_eq!(
        hook_actions.as_slice(),
        &[
            Action::EnterStance("Wrath".to_string()),
            Action::RemovePower {
                target: 0,
                power_id: PowerId::WrathNextTurn,
            },
        ]
    );
}

#[test]
fn watcher_spirit_shield_runtime_actions_match_java_apply_powers_count() {
    let mut state = crate::test_support::blank_test_combat();
    let mut spirit_plus = CombatCard::for_scenario(CardId::SpiritShield, 341);
    spirit_plus.upgrades = 1;
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeP, 342),
        spirit_plus.clone(),
        CombatCard::for_scenario(CardId::DefendP, 343),
        CombatCard::for_scenario(CardId::Vigilance, 344),
    ];

    let actions = resolve_card_play(CardId::SpiritShield, &state, &spirit_plus, None);

    assert_eq!(
        actions.as_slice(),
        &[ActionInfo {
            action: Action::GainBlock {
                target: 0,
                amount: 12,
            },
            insertion_mode: AddTo::Bottom,
        }],
        "Java SpiritShield.applyPowers counts cards in hand except itself, then multiplies by magic"
    );

    let mut already_removed = state.clone();
    already_removed.zones.hand.retain(|card| {
        card.lineage != crate::runtime::combat::CardLineageId::constructed_scenario(341)
    });
    let actions_after_hand_removal =
        resolve_card_play(CardId::SpiritShield, &already_removed, &spirit_plus, None);
    assert_eq!(actions_after_hand_removal[0].action, actions[0].action);

    let mut dex_state = state.clone();
    dex_state.entities.power_db.insert(
        0,
        vec![Power {
            power_type: PowerId::Dexterity,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: PowerPayload::None,
            just_applied: false,
        }],
    );
    let dex_actions = resolve_card_play(CardId::SpiritShield, &dex_state, &spirit_plus, None);
    assert_eq!(
        dex_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 14,
        },
        "Java SpiritShield.applyPowers sets dynamic baseBlock, then super.applyPowers applies Dexterity"
    );
    let mut shield_hand_state = dex_state.clone();
    shield_hand_state.update_hand_cards();
    assert_eq!(
        shield_hand_state.zones.hand[1].base_block_mut, 14,
        "Java SpiritShield.applyPowers also updates rendered/observed block"
    );
}

#[test]
fn watcher_study_and_insight_match_java_use_power_and_self_retain() {
    let state = crate::test_support::blank_test_combat();
    let study = CombatCard::for_scenario(CardId::Study, 345);
    let study_actions = resolve_card_play(CardId::Study, &state, &study, None);
    assert_eq!(
        study_actions.as_slice(),
        &[ActionInfo {
            action: Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Study,
                amount: 1,
            },
            insertion_mode: AddTo::Bottom,
        }]
    );

    let mut study_plus = CombatCard::for_scenario(CardId::Study, 346);
    study_plus.upgrades = 1;
    assert_eq!(study_plus.get_cost(), 1);

    let mut hook_state = crate::test_support::blank_test_combat();
    let power = Power {
        power_type: PowerId::Study,
        instance_id: None,
        amount: 2,
        extra_data: 0,
        payload: PowerPayload::None,
        just_applied: false,
    };
    let hook_actions =
        crate::content::powers::resolve_power_at_end_of_turn(&power, &mut hook_state, 0);
    assert_eq!(
        hook_actions.as_slice(),
        &[Action::MakeTempCardInDrawPile {
            card_id: CardId::Insight,
            amount: 2,
            random_spot: true,
            to_bottom: false,
            upgraded: false,
        }]
    );

    let mut insight_plus = CombatCard::for_scenario(CardId::Insight, 347);
    insight_plus.upgrades = 1;
    let insight_actions = resolve_card_play(CardId::Insight, &hook_state, &insight_plus, None);
    assert_eq!(insight_actions[0].action, Action::DrawCards(3));

    assert!(crate::content::cards::is_self_retain(
        &CombatCard::for_scenario(CardId::Insight, 349)
    ));
    assert!(crate::content::cards::is_self_retain(
        &CombatCard::for_scenario(CardId::Miracle, 350)
    ));
}

#[test]
fn watcher_swivel_matches_java_free_attack_power() {
    let mut state = crate::test_support::blank_test_combat();
    let mut swivel_plus = CombatCard::for_scenario(CardId::Swivel, 351);
    swivel_plus.upgrades = 1;

    let actions = resolve_card_play(CardId::Swivel, &state, &swivel_plus, None);

    assert_eq!(
        actions.as_slice(),
        &[
            ActionInfo {
                action: Action::GainBlock {
                    target: 0,
                    amount: 11,
                },
                insertion_mode: AddTo::Bottom,
            },
            ActionInfo {
                action: Action::ApplyPower {
                    source: 0,
                    target: 0,
                    power_id: PowerId::FreeAttackPower,
                    amount: 1,
                },
                insertion_mode: AddTo::Bottom,
            },
        ]
    );

    crate::engine::action_handlers::execute_action(
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::FreeAttackPower,
            amount: 1,
        },
        &mut state,
    );
    state.turn.energy = 0;
    state.zones.hand = vec![CombatCard::for_scenario(CardId::StrikeP, 352)];
    assert!(
        can_play_card(&state.zones.hand[0], &state).is_ok(),
        "Java AbstractCard.freeToPlay treats attacks as playable while FreeAttackPower is active"
    );
    let attack = state.zones.hand[0].clone();
    let mut exhaust_override = false;
    crate::content::powers::resolve_power_on_use_card(
        PowerId::FreeAttackPower,
        &mut state,
        &attack,
        &mut exhaust_override,
        false,
        Some(7),
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::RemovePower {
            target: 0,
            power_id: PowerId::FreeAttackPower,
        }),
        "Java FreeAttackPower.onUseCard decrements immediately and addToTop removes it at zero"
    );

    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![crate::runtime::combat::Power {
            power_type: PowerId::FreeAttackPower,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::content::powers::resolve_power_on_use_card(
        PowerId::FreeAttackPower,
        &mut state,
        &attack,
        &mut exhaust_override,
        false,
        Some(7),
    );
    assert_eq!(
        crate::content::powers::store::power_amount(&state, 0, PowerId::FreeAttackPower),
        1,
        "Java FreeAttackPower decrements its amount before later queued card effects resolve"
    );
    assert_eq!(state.pop_next_action(), None);
}

#[test]
fn dualcast_runtime_evoke_without_removing_preserves_front_orb_until_second_evoke() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.max_orbs = 1;
    state.entities.player.orbs = vec![OrbEntity::new(OrbId::Lightning)];

    crate::engine::action_handlers::execute_action(Action::EvokeOrbWithoutRemoving, &mut state);
    assert_eq!(state.entities.player.orbs[0].id, OrbId::Lightning);
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::OrbDamageRandomEnemy { base_damage: 8, .. })
    ));

    crate::engine::action_handlers::execute_action(Action::EvokeOrb, &mut state);
    assert_eq!(state.entities.player.orbs[0].id, OrbId::Empty);
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::OrbDamageRandomEnemy { base_damage: 8, .. })
    ));
}

#[test]
fn defect_first_common_batch_definitions_match_java_sources() {
    let cases = [
        (
            CardId::BallLightning,
            "Ball Lightning",
            CardType::Attack,
            1,
            7,
            0,
            1,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::BeamCell,
            "Beam Cell",
            CardType::Attack,
            0,
            3,
            0,
            1,
            CardTarget::Enemy,
            1,
            0,
            1,
        ),
        (
            CardId::ColdSnap,
            "Cold Snap",
            CardType::Attack,
            1,
            6,
            0,
            1,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::ConserveBattery,
            "Conserve Battery",
            CardType::Skill,
            1,
            0,
            7,
            0,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::Coolheaded,
            "Coolheaded",
            CardType::Skill,
            1,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::Leap,
            "Leap",
            CardType::Skill,
            1,
            0,
            9,
            0,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::Turbo,
            "Turbo",
            CardType::Skill,
            0,
            0,
            0,
            2,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::SweepingBeam,
            "Sweeping Beam",
            CardType::Attack,
            1,
            6,
            0,
            1,
            CardTarget::AllEnemy,
            3,
            0,
            0,
        ),
        (
            CardId::Hologram,
            "Hologram",
            CardType::Skill,
            1,
            0,
            3,
            0,
            CardTarget::SelfTarget,
            0,
            2,
            0,
        ),
        (
            CardId::Stack,
            "Stack",
            CardType::Skill,
            1,
            0,
            0,
            0,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::CompileDriver,
            "Compile Driver",
            CardType::Attack,
            1,
            7,
            0,
            1,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::Barrage,
            "Barrage",
            CardType::Attack,
            1,
            4,
            0,
            0,
            CardTarget::Enemy,
            2,
            0,
            0,
        ),
        (
            CardId::GoForTheEyes,
            "Go for the Eyes",
            CardType::Attack,
            0,
            3,
            0,
            1,
            CardTarget::Enemy,
            1,
            0,
            1,
        ),
        (
            CardId::Recursion,
            "Redo",
            CardType::Skill,
            1,
            0,
            0,
            0,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Streamline,
            "Streamline",
            CardType::Attack,
            2,
            15,
            0,
            1,
            CardTarget::Enemy,
            5,
            0,
            0,
        ),
        (
            CardId::Rebound,
            "Rebound",
            CardType::Attack,
            1,
            9,
            0,
            0,
            CardTarget::Enemy,
            3,
            0,
            0,
        ),
        (
            CardId::Claw,
            "Gash",
            CardType::Attack,
            0,
            3,
            0,
            2,
            CardTarget::Enemy,
            2,
            0,
            0,
        ),
        (
            CardId::SteamBarrier,
            "Steam",
            CardType::Skill,
            0,
            0,
            6,
            0,
            CardTarget::SelfTarget,
            0,
            2,
            0,
        ),
    ];

    let java_map = build_java_id_map();
    for (
        id,
        java_id_value,
        card_type,
        cost,
        base_damage,
        base_block,
        base_magic,
        target,
        upgrade_damage,
        upgrade_block,
        upgrade_magic,
    ) in cases
    {
        assert_eq!(java_id(id), java_id_value);
        assert_eq!(java_map.get(java_id_value), Some(&id));
        let def = get_card_definition(id);
        match id {
            CardId::Recursion => assert_eq!(def.name, "Recursion"),
            CardId::Claw => assert_eq!(def.name, "Claw"),
            CardId::SteamBarrier => assert_eq!(def.name, "Steam Barrier"),
            _ => assert_eq!(def.name, java_id_value),
        }
        assert_eq!(def.card_type, card_type);
        assert_eq!(def.rarity, CardRarity::Common);
        assert_eq!(def.cost, cost);
        assert_eq!(def.base_damage, base_damage);
        assert_eq!(def.base_block, base_block);
        assert_eq!(def.base_magic, base_magic);
        assert_eq!(def.target, target);
        assert_eq!(def.upgrade_damage, upgrade_damage);
        assert_eq!(def.upgrade_block, upgrade_block);
        assert_eq!(def.upgrade_magic, upgrade_magic);
    }
}

#[test]
fn defect_first_common_batch_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let ball = resolve_card_play(
        CardId::BallLightning,
        &state,
        &CombatCard::for_scenario(CardId::BallLightning, 110),
        Some(7),
    );
    assert_eq!(ball.len(), 2);
    match &ball[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 7);
            assert_eq!(info.output, 7);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Ball Lightning first action should damage, got {other:?}"),
    }
    assert_eq!(ball[1].action, Action::ChannelOrb(OrbId::Lightning));

    let cold = resolve_card_play(
        CardId::ColdSnap,
        &state,
        &CombatCard::for_scenario(CardId::ColdSnap, 111),
        Some(8),
    );
    assert_eq!(cold.len(), 2);
    match &cold[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 8);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Cold Snap first action should damage, got {other:?}"),
    }
    assert_eq!(cold[1].action, Action::ChannelOrb(OrbId::Frost));

    let beam = resolve_card_play(
        CardId::BeamCell,
        &state,
        &CombatCard::for_scenario(CardId::BeamCell, 112),
        Some(9),
    );
    assert_eq!(beam.len(), 2);
    match &beam[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 9);
            assert_eq!(info.base, 3);
            assert_eq!(info.output, 3);
        }
        other => panic!("Beam Cell first action should damage, got {other:?}"),
    }
    assert_eq!(
        beam[1].action,
        Action::ApplyPower {
            source: 0,
            target: 9,
            power_id: PowerId::Vulnerable,
            amount: 1,
        }
    );

    let coolheaded = resolve_card_play(
        CardId::Coolheaded,
        &state,
        &CombatCard::for_scenario(CardId::Coolheaded, 113),
        None,
    );
    assert_eq!(coolheaded.len(), 2);
    assert_eq!(coolheaded[0].action, Action::ChannelOrb(OrbId::Frost));
    assert_eq!(coolheaded[1].action, Action::DrawCards(1));

    let leap = resolve_card_play(
        CardId::Leap,
        &state,
        &CombatCard::for_scenario(CardId::Leap, 114),
        None,
    );
    assert_eq!(
        leap[0].action,
        Action::GainBlock {
            target: 0,
            amount: 9,
        }
    );

    let battery = resolve_card_play(
        CardId::ConserveBattery,
        &state,
        &CombatCard::for_scenario(CardId::ConserveBattery, 115),
        None,
    );
    assert_eq!(battery.len(), 2);
    assert_eq!(
        battery[0].action,
        Action::GainBlock {
            target: 0,
            amount: 7,
        }
    );
    assert_eq!(
        battery[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::EnergizedBlue,
            amount: 1,
        }
    );

    let turbo = resolve_card_play(
        CardId::Turbo,
        &state,
        &CombatCard::for_scenario(CardId::Turbo, 116),
        None,
    );
    assert_eq!(turbo.len(), 2);
    assert_eq!(turbo[0].action, Action::GainEnergy { amount: 2 });
    assert_eq!(
        turbo[1].action,
        Action::MakeTempCardInDiscard {
            card_id: CardId::Void,
            amount: 1,
            upgraded: false,
        }
    );

    let mut turbo_plus = CombatCard::for_scenario(CardId::Turbo, 117);
    turbo_plus.upgrades = 1;
    let turbo_plus_actions = resolve_card_play(CardId::Turbo, &state, &turbo_plus, None);
    assert_eq!(
        turbo_plus_actions[0].action,
        Action::GainEnergy { amount: 3 }
    );
    assert_eq!(turbo_plus_actions[1].action, turbo[1].action);

    let sweeping = resolve_card_play(
        CardId::SweepingBeam,
        &state,
        &CombatCard::for_scenario(CardId::SweepingBeam, 118),
        None,
    );
    assert_eq!(sweeping.len(), 2);
    match &sweeping[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, 0);
            assert!(damages.iter().all(|d| *d == 6));
            assert_eq!(*damage_type, DamageType::Normal);
            assert!(!is_modified);
        }
        other => panic!("Sweeping Beam first action should damage all enemies, got {other:?}"),
    }
    assert_eq!(sweeping[1].action, Action::DrawCards(1));

    let mut sweeping_plus = CombatCard::for_scenario(CardId::SweepingBeam, 119);
    sweeping_plus.upgrades = 1;
    let sweeping_plus_actions =
        resolve_card_play(CardId::SweepingBeam, &state, &sweeping_plus, None);
    match &sweeping_plus_actions[0].action {
        Action::DamageAllEnemies { damages, .. } => {
            assert!(damages.iter().all(|d| *d == 9));
        }
        other => panic!("Sweeping Beam+ first action should damage all enemies, got {other:?}"),
    }
    assert_eq!(sweeping_plus_actions[1].action, Action::DrawCards(1));

    let hologram = resolve_card_play(
        CardId::Hologram,
        &state,
        &CombatCard::for_scenario(CardId::Hologram, 120),
        None,
    );
    assert_eq!(hologram.len(), 2);
    assert_eq!(
        hologram[0].action,
        Action::GainBlock {
            target: 0,
            amount: 3,
        }
    );
    assert_eq!(
        hologram[1].action,
        Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Discard,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Any,
            reason: crate::state::GridSelectReason::DiscardToHandNoCostChange,
        }
    );
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Hologram,
        121
    )));

    let mut hologram_plus = CombatCard::for_scenario(CardId::Hologram, 122);
    hologram_plus.upgrades = 1;
    let hologram_plus_actions = resolve_card_play(CardId::Hologram, &state, &hologram_plus, None);
    assert_eq!(
        hologram_plus_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 5,
        }
    );
    assert!(!exhausts_when_played(&hologram_plus));

    let mut stack_state = state.clone();
    stack_state.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::StrikeB, 123),
        CombatCard::for_scenario(CardId::DefendB, 124),
    ]
    .into();
    let stack = resolve_card_play(
        CardId::Stack,
        &stack_state,
        &CombatCard::for_scenario(CardId::Stack, 125),
        None,
    );
    assert_eq!(
        stack[0].action,
        Action::GainBlock {
            target: 0,
            amount: 2,
        }
    );

    let mut stack_plus = CombatCard::for_scenario(CardId::Stack, 126);
    stack_plus.upgrades = 1;
    let stack_plus_actions = resolve_card_play(CardId::Stack, &stack_state, &stack_plus, None);
    assert_eq!(
        stack_plus_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 5,
        }
    );

    crate::content::powers::store::set_powers_for(
        &mut stack_state,
        0,
        vec![Power {
            power_type: PowerId::Dexterity,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let stack_with_dex = resolve_card_play(
        CardId::Stack,
        &stack_state,
        &CombatCard::for_scenario(CardId::Stack, 127),
        None,
    );
    assert_eq!(
        stack_with_dex[0].action,
        Action::GainBlock {
            target: 0,
            amount: 4,
        },
        "Stack's discard-count base block must still flow through normal block modifiers"
    );

    let compile = resolve_card_play(
        CardId::CompileDriver,
        &state,
        &CombatCard::for_scenario(CardId::CompileDriver, 128),
        Some(13),
    );
    assert_eq!(compile.len(), 2);
    match &compile[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 13);
            assert_eq!(info.base, 7);
            assert_eq!(info.output, 7);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Compile Driver first action should damage, got {other:?}"),
    }
    assert_eq!(
        compile[1].action,
        Action::DrawForUniqueOrbTypes {
            amount_per_orb_type: 1,
        }
    );

    let mut compile_plus = CombatCard::for_scenario(CardId::CompileDriver, 129);
    compile_plus.upgrades = 1;
    let compile_plus_actions =
        resolve_card_play(CardId::CompileDriver, &state, &compile_plus, Some(13));
    match &compile_plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 10);
            assert_eq!(info.output, 10);
        }
        other => panic!("Compile Driver+ first action should damage, got {other:?}"),
    }
    assert_eq!(compile_plus_actions[1].action, compile[1].action);

    let barrage = resolve_card_play(
        CardId::Barrage,
        &state,
        &CombatCard::for_scenario(CardId::Barrage, 130),
        Some(14),
    );
    assert_eq!(barrage.len(), 1);
    match &barrage[0].action {
        Action::Barrage { damage } => {
            assert_eq!(damage.target, 14);
            assert_eq!(damage.base, 4);
            assert_eq!(damage.output, 4);
            assert_eq!(damage.damage_type, DamageType::Normal);
        }
        other => panic!("Barrage should emit BarrageAction equivalent, got {other:?}"),
    }

    let mut barrage_plus = CombatCard::for_scenario(CardId::Barrage, 131);
    barrage_plus.upgrades = 1;
    let barrage_plus_actions = resolve_card_play(CardId::Barrage, &state, &barrage_plus, Some(14));
    match &barrage_plus_actions[0].action {
        Action::Barrage { damage } => {
            assert_eq!(damage.base, 6);
            assert_eq!(damage.output, 6);
        }
        other => panic!("Barrage+ should emit BarrageAction equivalent, got {other:?}"),
    }

    let go_for = resolve_card_play(
        CardId::GoForTheEyes,
        &state,
        &CombatCard::for_scenario(CardId::GoForTheEyes, 132),
        Some(15),
    );
    assert_eq!(go_for.len(), 2);
    match &go_for[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 15);
            assert_eq!(info.base, 3);
            assert_eq!(info.output, 3);
        }
        other => panic!("Go for the Eyes first action should damage, got {other:?}"),
    }
    assert_eq!(
        go_for[1].action,
        Action::ApplyWeakIfTargetAttacking {
            target: 15,
            amount: 1,
        }
    );

    let mut go_for_plus = CombatCard::for_scenario(CardId::GoForTheEyes, 133);
    go_for_plus.upgrades = 1;
    let go_for_plus_actions =
        resolve_card_play(CardId::GoForTheEyes, &state, &go_for_plus, Some(15));
    match &go_for_plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 4);
            assert_eq!(info.output, 4);
        }
        other => panic!("Go for the Eyes+ first action should damage, got {other:?}"),
    }
    assert_eq!(
        go_for_plus_actions[1].action,
        Action::ApplyWeakIfTargetAttacking {
            target: 15,
            amount: 2,
        }
    );

    let recursion = resolve_card_play(
        CardId::Recursion,
        &state,
        &CombatCard::for_scenario(CardId::Recursion, 134),
        None,
    );
    assert_eq!(recursion.len(), 1);
    assert_eq!(recursion[0].action, Action::RedoOrb);
    let mut recursion_plus = CombatCard::for_scenario(CardId::Recursion, 135);
    recursion_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&recursion_plus), Some(0));

    let streamline = resolve_card_play(
        CardId::Streamline,
        &state,
        &CombatCard::for_scenario(CardId::Streamline, 136),
        Some(16),
    );
    assert_eq!(streamline.len(), 2);
    match &streamline[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 16);
            assert_eq!(info.base, 15);
            assert_eq!(info.output, 15);
        }
        other => panic!("Streamline first action should damage, got {other:?}"),
    }
    assert_eq!(
        streamline[1].action,
        Action::ReduceCardCostForCombat {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(136),
            amount: 1,
        }
    );

    let mut streamline_plus = CombatCard::for_scenario(CardId::Streamline, 137);
    streamline_plus.upgrades = 1;
    let streamline_plus_actions =
        resolve_card_play(CardId::Streamline, &state, &streamline_plus, Some(16));
    match &streamline_plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 20);
            assert_eq!(info.output, 20);
        }
        other => panic!("Streamline+ first action should damage, got {other:?}"),
    }

    let rebound = resolve_card_play(
        CardId::Rebound,
        &state,
        &CombatCard::for_scenario(CardId::Rebound, 138),
        Some(17),
    );
    assert_eq!(rebound.len(), 2);
    match &rebound[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 17);
            assert_eq!(info.base, 9);
            assert_eq!(info.output, 9);
        }
        other => panic!("Rebound first action should damage, got {other:?}"),
    }
    assert_eq!(
        rebound[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Rebound,
            amount: 1,
        }
    );

    let mut rebound_plus = CombatCard::for_scenario(CardId::Rebound, 139);
    rebound_plus.upgrades = 1;
    let rebound_plus_actions = resolve_card_play(CardId::Rebound, &state, &rebound_plus, Some(17));
    match &rebound_plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 12);
            assert_eq!(info.output, 12);
        }
        other => panic!("Rebound+ first action should damage, got {other:?}"),
    }

    let claw = resolve_card_play(
        CardId::Claw,
        &state,
        &CombatCard::for_scenario(CardId::Claw, 140),
        Some(18),
    );
    assert_eq!(claw.len(), 2);
    match &claw[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 18);
            assert_eq!(info.base, 3);
            assert_eq!(info.output, 3);
        }
        other => panic!("Claw first action should damage, got {other:?}"),
    }
    assert_eq!(
        claw[1].action,
        Action::Gash {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(140),
            amount: 2,
        }
    );

    let mut claw_plus = CombatCard::for_scenario(CardId::Claw, 141);
    claw_plus.upgrades = 1;
    let claw_plus_actions = resolve_card_play(CardId::Claw, &state, &claw_plus, Some(18));
    match &claw_plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 5);
            assert_eq!(info.output, 5);
        }
        other => panic!("Claw+ first action should damage, got {other:?}"),
    }
    assert_eq!(
        claw_plus_actions[1].action,
        Action::Gash {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(141),
            amount: 2,
        }
    );

    let steam = resolve_card_play(
        CardId::SteamBarrier,
        &state,
        &CombatCard::for_scenario(CardId::SteamBarrier, 142),
        None,
    );
    assert_eq!(steam.len(), 2);
    assert_eq!(
        steam[0].action,
        Action::GainBlock {
            target: 0,
            amount: 6,
        }
    );
    assert_eq!(
        steam[1].action,
        Action::ModifyCardBlock {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(142),
            amount: -1,
        }
    );

    let mut steam_plus = CombatCard::for_scenario(CardId::SteamBarrier, 143);
    steam_plus.upgrades = 1;
    let steam_plus_actions = resolve_card_play(CardId::SteamBarrier, &state, &steam_plus, None);
    assert_eq!(
        steam_plus_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 8,
        }
    );
}

#[test]
fn compile_driver_action_draws_for_unique_non_empty_orb_types_at_execution_time() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Lightning),
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Lightning),
        OrbEntity::new(OrbId::Empty),
    ];

    crate::engine::action_handlers::execute_action(
        Action::DrawForUniqueOrbTypes {
            amount_per_orb_type: 1,
        },
        &mut state,
    );
    assert_eq!(state.pop_next_action(), Some(Action::DrawCards(2)));

    state.entities.player.orbs = vec![OrbEntity::new(OrbId::Empty)];
    crate::engine::action_handlers::execute_action(
        Action::DrawForUniqueOrbTypes {
            amount_per_orb_type: 1,
        },
        &mut state,
    );
    assert_eq!(state.pop_next_action(), None);
}

#[test]
fn barrage_action_queues_damage_for_each_non_empty_orb_at_execution_time() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Lightning),
        OrbEntity::new(OrbId::Empty),
        OrbEntity::new(OrbId::Frost),
    ];

    crate::engine::action_handlers::execute_action(
        Action::Barrage {
            damage: DamageInfo {
                source: 0,
                target: 42,
                base: 4,
                output: 4,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
        },
        &mut state,
    );

    assert!(matches!(
        state.pop_next_action(),
        Some(Action::Damage(DamageInfo {
            target: 42,
            base: 4,
            ..
        }))
    ));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::Damage(DamageInfo {
            target: 42,
            base: 4,
            ..
        }))
    ));
    assert_eq!(state.pop_next_action(), None);
}

#[test]
fn go_for_the_eyes_action_checks_target_attack_intent_when_it_resolves() {
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 43;
    monster.set_planned_move_id(1);

    let mut state = crate::test_support::blank_test_combat();
    state.entities.monsters = vec![monster];

    crate::engine::action_handlers::execute_action(
        Action::ApplyWeakIfTargetAttacking {
            target: 43,
            amount: 2,
        },
        &mut state,
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 43,
            power_id: PowerId::Weak,
            amount: 2,
        })
    );

    state.entities.monsters[0].set_planned_move_id(2);
    crate::engine::action_handlers::execute_action(
        Action::ApplyWeakIfTargetAttacking {
            target: 43,
            amount: 2,
        },
        &mut state,
    );
    assert_eq!(state.pop_next_action(), None);
}

#[test]
fn recursion_redo_action_evokes_then_rechannels_same_orb_instance() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::Cultist);
    target.id = 44;
    target.current_hp = 10;
    state.entities.monsters = vec![target];
    state.entities.player.max_orbs = 1;
    let mut dark = OrbEntity::new(OrbId::Dark);
    dark.evoke_amount = 18;
    state.entities.player.orbs = vec![dark.clone()];

    crate::engine::action_handlers::execute_action(Action::RedoOrb, &mut state);
    assert_eq!(state.pop_next_action(), Some(Action::EvokeOrb));
    let channel = state.pop_next_action();
    assert_eq!(
        channel,
        Some(Action::ChannelOrbEntity { orb: dark.clone() }),
        "Java RedoAction channels the same orb object it captured before evoking"
    );

    crate::content::orbs::hooks::evoke_next_orb_now(&mut state);
    assert_eq!(state.entities.player.orbs[0].id, OrbId::Empty);
    let queued_dark_damage = state.pop_next_action();
    assert!(matches!(
        queued_dark_damage,
        Some(Action::OrbDamage {
            target: 44,
            base_damage: 18,
            ..
        })
    ));

    crate::engine::action_handlers::execute_action(channel.unwrap(), &mut state);
    assert_eq!(state.entities.player.orbs[0], dark);
}

#[test]
fn streamline_reduce_cost_action_mutates_matching_combat_instances() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Streamline, 200)];
    state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Streamline, 200)]).into();
    state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Streamline, 201)].into();

    crate::engine::action_handlers::execute_action(
        Action::ReduceCardCostForCombat {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(200),
            amount: 1,
        },
        &mut state,
    );

    assert_eq!(
        state.zones.hand[0].combat_cost_without_turn_override_java(),
        1
    );
    assert_eq!(
        state.zones.draw_pile[0].combat_cost_without_turn_override_java(),
        1
    );
    assert_eq!(
        state.zones.discard_pile[0].combat_cost_without_turn_override_java(),
        2,
        "ReduceCostAction targets GetAllInBattleInstances for the used card lineage only"
    );
}

#[test]
fn rebound_power_skips_card_that_created_it_like_java_just_evoked() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.limbo = vec![CombatCard::for_scenario(CardId::Rebound, 210)];
    crate::engine::action_handlers::execute_action(
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Rebound,
            amount: 1,
        },
        &mut state,
    );
    let rebound_power = crate::content::powers::store::powers_for(&state, 0)
        .unwrap()
        .iter()
        .find(|p| p.power_type == PowerId::Rebound)
        .unwrap();
    assert_eq!(rebound_power.extra_data, 1);

    crate::engine::action_handlers::execute_action(
        Action::UseCardDone {
            should_exhaust: false,
            trigger_after_use_hooks: true,
        },
        &mut state,
    );

    assert!(state.zones.draw_pile.is_empty());
    assert_eq!(state.zones.discard_pile.len(), 1);
    assert_eq!(state.zones.discard_pile[0].id, CardId::Rebound);
    assert_eq!(state.pop_next_action(), None);
    let rebound_power = crate::content::powers::store::powers_for(&state, 0)
        .unwrap()
        .iter()
        .find(|p| p.power_type == PowerId::Rebound)
        .unwrap();
    assert_eq!(rebound_power.extra_data, 0);
}

#[test]
fn rebound_power_moves_next_non_power_card_to_draw_pile_top() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.limbo = vec![CombatCard::for_scenario(CardId::StrikeB, 211)];
    state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::DefendB, 212)]).into();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Rebound,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::execute_action(
        Action::UseCardDone {
            should_exhaust: false,
            trigger_after_use_hooks: true,
        },
        &mut state,
    );

    assert!(state.zones.discard_pile.is_empty());
    assert_eq!(state.zones.draw_pile[0].id, CardId::StrikeB);
    assert_eq!(
        state.zones.draw_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(211)
    );
    assert_eq!(state.zones.draw_pile[1].id, CardId::DefendB);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ReducePower {
            target: 0,
            power_id: PowerId::Rebound,
            amount: 1,
        })
    );
}

#[test]
fn rebound_power_consumes_on_power_card_without_rebounding_it() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Rebound,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::execute_action(
        Action::UseCardAfterUseHooks {
            card: Box::new(CombatCard::for_scenario(CardId::Inflame, 213)),
        },
        &mut state,
    );

    assert!(state.zones.draw_pile.is_empty());
    assert!(state.zones.discard_pile.is_empty());
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ReducePower {
            target: 0,
            power_id: PowerId::Rebound,
            amount: 1,
        })
    );
}

#[test]
fn gash_action_increases_current_and_visible_claw_cards_only() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Claw, 221),
        CombatCard::for_scenario(CardId::StrikeB, 222),
    ];
    let mut upgraded_claw = CombatCard::for_scenario(CardId::Claw, 223);
    upgraded_claw.upgrades = 1;
    state.zones.draw_pile = (vec![upgraded_claw]).into();
    let mut damaged_claw = CombatCard::for_scenario(CardId::Claw, 224);
    damaged_claw.base_damage_override = Some(10);
    state.zones.discard_pile = vec![damaged_claw].into();
    state.zones.exhaust_pile = (vec![CombatCard::for_scenario(CardId::Claw, 225)]).into();
    state.zones.limbo = vec![CombatCard::for_scenario(CardId::Claw, 226)];

    crate::engine::action_handlers::execute_action(
        Action::Gash {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(226),
            amount: 2,
        },
        &mut state,
    );

    assert_eq!(state.zones.hand[0].base_damage_override, Some(5));
    assert_eq!(state.zones.hand[1].base_damage_override, None);
    assert_eq!(
        state.zones.draw_pile[0].base_damage_override,
        Some(7),
        "upgraded Claw should grow from its upgraded base damage"
    );
    assert_eq!(
        state.zones.discard_pile[0].base_damage_override,
        Some(12),
        "existing combat damage mutations should keep stacking"
    );
    assert_eq!(
        state.zones.exhaust_pile[0].base_damage_override, None,
        "Java GashAction does not loop the exhaust pile"
    );
    assert_eq!(state.zones.limbo[0].base_damage_override, Some(5));
}

#[test]
fn steam_barrier_modify_block_action_persists_combat_base_block_changes() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::SteamBarrier, 231)];
    let mut upgraded = CombatCard::for_scenario(CardId::SteamBarrier, 231);
    upgraded.upgrades = 1;
    state.zones.draw_pile = (vec![upgraded]).into();
    state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::SteamBarrier, 232)].into();
    state.zones.limbo = vec![CombatCard::for_scenario(CardId::SteamBarrier, 231)];

    crate::engine::action_handlers::execute_action(
        Action::ModifyCardBlock {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(231),
            amount: -1,
        },
        &mut state,
    );

    assert_eq!(state.zones.hand[0].base_block_override, Some(5));
    assert_eq!(
        state.zones.draw_pile[0].base_block_override,
        Some(7),
        "upgraded Steam Barrier should decay from its upgraded base block"
    );
    assert_eq!(
        state.zones.discard_pile[0].base_block_override, None,
        "ModifyBlockAction targets matching UUID instances only"
    );
    assert_eq!(state.zones.limbo[0].base_block_override, Some(5));

    let evaluated = evaluate_card_for_play(&state.zones.hand[0], &state, None);
    assert_eq!(evaluated.base_block_mut, 5);

    state.zones.hand[0].base_block_override = Some(-1);
    let depleted = evaluate_card_for_play(&state.zones.hand[0], &state, None);
    assert_eq!(
        depleted.base_block_mut, 0,
        "Java applyPowersToBlock floors negative baseBlock to zero output"
    );
}

#[test]
fn defect_low_risk_uncommon_definitions_match_java_sources() {
    let cases = [
        (
            CardId::Defragment,
            "Defragment",
            CardType::Power,
            1,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::Fusion,
            "Fusion",
            CardType::Skill,
            2,
            0,
            0,
            1,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Glacier,
            "Glacier",
            CardType::Skill,
            2,
            0,
            7,
            2,
            CardTarget::SelfTarget,
            0,
            3,
            0,
        ),
        (
            CardId::Skim,
            "Skim",
            CardType::Skill,
            1,
            0,
            0,
            3,
            CardTarget::None,
            0,
            0,
            1,
        ),
        (
            CardId::Overclock,
            "Steam Power",
            CardType::Skill,
            0,
            0,
            0,
            2,
            CardTarget::SelfTarget,
            0,
            0,
            1,
        ),
        (
            CardId::DoubleEnergy,
            "Double Energy",
            CardType::Skill,
            1,
            0,
            0,
            0,
            CardTarget::SelfTarget,
            0,
            0,
            0,
        ),
        (
            CardId::Reprogram,
            "Reprogram",
            CardType::Skill,
            1,
            0,
            0,
            1,
            CardTarget::None,
            0,
            0,
            1,
        ),
        (
            CardId::Melter,
            "Melter",
            CardType::Attack,
            1,
            10,
            0,
            0,
            CardTarget::Enemy,
            4,
            0,
            0,
        ),
        (
            CardId::Ftl,
            "FTL",
            CardType::Attack,
            0,
            5,
            0,
            3,
            CardTarget::Enemy,
            1,
            0,
            1,
        ),
        (
            CardId::RipAndTear,
            "Rip and Tear",
            CardType::Attack,
            1,
            7,
            0,
            2,
            CardTarget::AllEnemy,
            2,
            0,
            0,
        ),
    ];

    let java_map = build_java_id_map();
    for (
        id,
        java_id_value,
        card_type,
        cost,
        base_damage,
        base_block,
        base_magic,
        target,
        upgrade_damage,
        upgrade_block,
        upgrade_magic,
    ) in cases
    {
        assert_eq!(java_id(id), java_id_value);
        assert_eq!(java_map.get(java_id_value), Some(&id));
        let def = get_card_definition(id);
        if id == CardId::Overclock {
            assert_eq!(def.name, "Overclock");
        } else {
            assert_eq!(def.name, java_id_value);
        }
        assert_eq!(def.card_type, card_type);
        assert_eq!(def.rarity, CardRarity::Uncommon);
        assert_eq!(def.cost, cost);
        assert_eq!(def.base_damage, base_damage);
        assert_eq!(def.base_block, base_block);
        assert_eq!(def.base_magic, base_magic);
        assert_eq!(def.target, target);
        assert_eq!(def.upgrade_damage, upgrade_damage);
        assert_eq!(def.upgrade_block, upgrade_block);
        assert_eq!(def.upgrade_magic, upgrade_magic);
    }

    let mut fusion_plus = CombatCard::for_scenario(CardId::Fusion, 241);
    fusion_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&fusion_plus), Some(1));
    let mut double_energy_plus = CombatCard::for_scenario(CardId::DoubleEnergy, 2411);
    double_energy_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&double_energy_plus), Some(0));
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::DoubleEnergy,
        2412
    )));
}

#[test]
fn defect_low_risk_uncommon_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let defrag = resolve_card_play(
        CardId::Defragment,
        &state,
        &CombatCard::for_scenario(CardId::Defragment, 242),
        None,
    );
    assert_eq!(
        defrag[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 1,
        }
    );
    let mut defrag_plus = CombatCard::for_scenario(CardId::Defragment, 243);
    defrag_plus.upgrades = 1;
    let defrag_plus_actions = resolve_card_play(CardId::Defragment, &state, &defrag_plus, None);
    assert_eq!(
        defrag_plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 2,
        }
    );

    let fusion = resolve_card_play(
        CardId::Fusion,
        &state,
        &CombatCard::for_scenario(CardId::Fusion, 244),
        None,
    );
    assert_eq!(fusion.len(), 1);
    assert_eq!(fusion[0].action, Action::ChannelOrb(OrbId::Plasma));

    let glacier = resolve_card_play(
        CardId::Glacier,
        &state,
        &CombatCard::for_scenario(CardId::Glacier, 245),
        None,
    );
    assert_eq!(glacier.len(), 3);
    assert_eq!(
        glacier[0].action,
        Action::GainBlock {
            target: 0,
            amount: 7,
        }
    );
    assert_eq!(glacier[1].action, Action::ChannelOrb(OrbId::Frost));
    assert_eq!(glacier[2].action, Action::ChannelOrb(OrbId::Frost));
    let mut glacier_plus = CombatCard::for_scenario(CardId::Glacier, 246);
    glacier_plus.upgrades = 1;
    let glacier_plus_actions = resolve_card_play(CardId::Glacier, &state, &glacier_plus, None);
    assert_eq!(
        glacier_plus_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 10,
        }
    );

    let skim = resolve_card_play(
        CardId::Skim,
        &state,
        &CombatCard::for_scenario(CardId::Skim, 247),
        None,
    );
    assert_eq!(skim[0].action, Action::DrawCards(3));
    let mut skim_plus = CombatCard::for_scenario(CardId::Skim, 248);
    skim_plus.upgrades = 1;
    let skim_plus_actions = resolve_card_play(CardId::Skim, &state, &skim_plus, None);
    assert_eq!(skim_plus_actions[0].action, Action::DrawCards(4));

    let overclock = resolve_card_play(
        CardId::Overclock,
        &state,
        &CombatCard::for_scenario(CardId::Overclock, 249),
        None,
    );
    assert_eq!(overclock.len(), 2);
    assert_eq!(overclock[0].action, Action::DrawCards(2));
    assert_eq!(
        overclock[1].action,
        Action::MakeTempCardInDiscard {
            card_id: CardId::Burn,
            amount: 1,
            upgraded: false,
        }
    );
    let mut overclock_plus = CombatCard::for_scenario(CardId::Overclock, 250);
    overclock_plus.upgrades = 1;
    let overclock_plus_actions =
        resolve_card_play(CardId::Overclock, &state, &overclock_plus, None);
    assert_eq!(overclock_plus_actions[0].action, Action::DrawCards(3));
    assert_eq!(overclock_plus_actions[1].action, overclock[1].action);

    let double_energy = resolve_card_play(
        CardId::DoubleEnergy,
        &state,
        &CombatCard::for_scenario(CardId::DoubleEnergy, 251),
        None,
    );
    assert_eq!(double_energy.len(), 1);
    assert_eq!(double_energy[0].action, Action::DoubleEnergy);

    let reprogram = resolve_card_play(
        CardId::Reprogram,
        &state,
        &CombatCard::for_scenario(CardId::Reprogram, 252),
        None,
    );
    assert_eq!(reprogram.len(), 3);
    assert_eq!(
        reprogram[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: -1,
        }
    );
    assert_eq!(
        reprogram[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 1,
        }
    );
    assert_eq!(
        reprogram[2].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Dexterity,
            amount: 1,
        }
    );
    let mut reprogram_plus = CombatCard::for_scenario(CardId::Reprogram, 253);
    reprogram_plus.upgrades = 1;
    let reprogram_plus_actions =
        resolve_card_play(CardId::Reprogram, &state, &reprogram_plus, None);
    assert_eq!(
        reprogram_plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: -2,
        }
    );
    assert_eq!(
        reprogram_plus_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 2,
        }
    );
    assert_eq!(
        reprogram_plus_actions[2].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Dexterity,
            amount: 2,
        }
    );

    let melter = resolve_card_play(
        CardId::Melter,
        &state,
        &CombatCard::for_scenario(CardId::Melter, 254),
        Some(19),
    );
    assert_eq!(melter.len(), 2);
    assert_eq!(melter[0].action, Action::RemoveAllBlock { target: 19 });
    match &melter[1].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 19);
            assert_eq!(info.base, 10);
            assert_eq!(info.output, 10);
        }
        other => panic!("Melter second action should damage, got {other:?}"),
    }
    let mut melter_plus = CombatCard::for_scenario(CardId::Melter, 255);
    melter_plus.upgrades = 1;
    let melter_plus_actions = resolve_card_play(CardId::Melter, &state, &melter_plus, Some(19));
    match &melter_plus_actions[1].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 14);
            assert_eq!(info.output, 14);
        }
        other => panic!("Melter+ second action should damage, got {other:?}"),
    }

    let ftl = resolve_card_play(
        CardId::Ftl,
        &state,
        &CombatCard::for_scenario(CardId::Ftl, 257),
        Some(20),
    );
    assert_eq!(ftl.len(), 1);
    match &ftl[0].action {
        Action::Ftl {
            target,
            damage_info,
            card_play_count,
        } => {
            assert_eq!(*target, 20);
            assert_eq!(damage_info.base, 5);
            assert_eq!(damage_info.output, 5);
            assert_eq!(*card_play_count, 3);
        }
        other => panic!("FTL should emit FTLAction equivalent, got {other:?}"),
    }
    let mut ftl_plus = CombatCard::for_scenario(CardId::Ftl, 258);
    ftl_plus.upgrades = 1;
    let ftl_plus_actions = resolve_card_play(CardId::Ftl, &state, &ftl_plus, Some(20));
    match &ftl_plus_actions[0].action {
        Action::Ftl {
            damage_info,
            card_play_count,
            ..
        } => {
            assert_eq!(damage_info.base, 6);
            assert_eq!(*card_play_count, 4);
        }
        other => panic!("FTL+ should emit FTLAction equivalent, got {other:?}"),
    }

    let rip = resolve_card_play(
        CardId::RipAndTear,
        &state,
        &CombatCard::for_scenario(CardId::RipAndTear, 259),
        None,
    );
    assert_eq!(rip.len(), 2);
    for action in &rip {
        match &action.action {
            Action::AttackDamageRandomEnemyCard { card } => {
                assert_eq!(card.id, CardId::RipAndTear);
                assert_eq!(
                    card.lineage,
                    crate::runtime::combat::CardLineageId::constructed_scenario(259)
                );
            }
            other => panic!("Rip and Tear should queue random enemy attacks, got {other:?}"),
        }
    }
}

#[test]
fn defect_energy_and_remove_block_actions_read_execution_state() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 2;
    crate::engine::action_handlers::execute_action(Action::DoubleEnergy, &mut state);
    assert_eq!(state.turn.energy, 4);

    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.id = 256;
    monster.block = 18;
    state.entities.monsters = vec![monster];
    crate::engine::action_handlers::execute_action(
        Action::RemoveAllBlock { target: 256 },
        &mut state,
    );
    assert_eq!(state.entities.monsters[0].block, 0);
}

#[test]
fn defect_tempest_and_darkness_definitions_match_java_sources() {
    let tempest = get_card_definition(CardId::Tempest);
    assert_eq!(tempest.name, "Tempest");
    assert_eq!(tempest.card_type, CardType::Skill);
    assert_eq!(tempest.rarity, CardRarity::Uncommon);
    assert_eq!(tempest.cost, -1);
    assert_eq!(tempest.base_magic, 0);
    assert_eq!(tempest.target, CardTarget::SelfTarget);
    assert!(tempest.exhaust);
    assert_eq!(tempest.upgrade_damage, 0);
    assert_eq!(tempest.upgrade_block, 0);
    assert_eq!(tempest.upgrade_magic, 0);

    let darkness = get_card_definition(CardId::Darkness);
    assert_eq!(darkness.name, "Darkness");
    assert_eq!(darkness.card_type, CardType::Skill);
    assert_eq!(darkness.rarity, CardRarity::Uncommon);
    assert_eq!(darkness.cost, 1);
    assert_eq!(darkness.base_magic, 1);
    assert_eq!(darkness.target, CardTarget::SelfTarget);
    assert!(!darkness.exhaust);
    assert_eq!(darkness.upgrade_damage, 0);
    assert_eq!(darkness.upgrade_block, 0);
    assert_eq!(darkness.upgrade_magic, 0);

    assert_eq!(java_id(CardId::Tempest), "Tempest");
    assert_eq!(java_id(CardId::Darkness), "Darkness");
    assert_eq!(build_java_id_map().get("Tempest"), Some(&CardId::Tempest));
    assert_eq!(build_java_id_map().get("Darkness"), Some(&CardId::Darkness));
}

#[test]
fn defect_tempest_and_darkness_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let mut tempest_card = CombatCard::for_scenario(CardId::Tempest, 261);
    tempest_card.energy_on_use = 2;
    let tempest = resolve_card_play(CardId::Tempest, &state, &tempest_card, None);
    assert_eq!(tempest.len(), 1);
    assert_eq!(
        tempest[0].action,
        Action::Tempest {
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 2,
        }
    );

    let darkness = resolve_card_play(
        CardId::Darkness,
        &state,
        &CombatCard::for_scenario(CardId::Darkness, 262),
        None,
    );
    assert_eq!(darkness.len(), 1);
    assert_eq!(darkness[0].action, Action::ChannelOrb(OrbId::Dark));

    let mut darkness_plus = CombatCard::for_scenario(CardId::Darkness, 263);
    darkness_plus.upgrades = 1;
    let darkness_plus_actions = resolve_card_play(CardId::Darkness, &state, &darkness_plus, None);
    assert_eq!(darkness_plus_actions.len(), 2);
    assert_eq!(
        darkness_plus_actions[0].action,
        Action::ChannelOrb(OrbId::Dark)
    );
    assert_eq!(
        darkness_plus_actions[1].action,
        Action::TriggerDarkImpulseOrbs
    );
}

#[test]
fn tempest_action_uses_x_cost_relic_upgrade_and_free_to_play_semantics() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 2;
    crate::engine::action_handlers::execute_action(
        Action::Tempest {
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 2,
        },
        &mut state,
    );
    assert_eq!(state.turn.energy, 0);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
    assert_eq!(state.pop_next_action(), None);

    let mut upgraded_zero = crate::test_support::blank_test_combat();
    upgraded_zero.turn.energy = 0;
    crate::engine::action_handlers::execute_action(
        Action::Tempest {
            upgraded: true,
            free_to_play_once: false,
            energy_on_use: 0,
        },
        &mut upgraded_zero,
    );
    assert_eq!(
        upgraded_zero.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning)),
        "Java TempestAction adds the upgrade bonus after reading zero X energy"
    );
    assert_eq!(upgraded_zero.pop_next_action(), None);

    let mut chemical_x = crate::test_support::blank_test_combat();
    chemical_x
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ChemicalX,
        ));
    crate::engine::action_handlers::execute_action(
        Action::Tempest {
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 0,
        },
        &mut chemical_x,
    );
    assert_eq!(
        chemical_x.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
    assert_eq!(
        chemical_x.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
    assert_eq!(chemical_x.pop_next_action(), None);

    let mut free_to_play = crate::test_support::blank_test_combat();
    free_to_play.turn.energy = 3;
    crate::engine::action_handlers::execute_action(
        Action::Tempest {
            upgraded: false,
            free_to_play_once: true,
            energy_on_use: 3,
        },
        &mut free_to_play,
    );
    assert_eq!(free_to_play.turn.energy, 3);
}

#[test]
fn dark_impulse_action_only_triggers_dark_orbs_and_dark_first_cables() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.max_orbs = 3;
    state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Dark),
        OrbEntity::new(OrbId::Plasma),
        OrbEntity::new(OrbId::Dark),
    ];
    state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::GoldPlatedCables,
        ));

    crate::engine::action_handlers::execute_action(Action::TriggerDarkImpulseOrbs, &mut state);
    assert_eq!(state.entities.player.orbs[0].evoke_amount, 18);
    assert_eq!(state.entities.player.orbs[1].evoke_amount, 2);
    assert_eq!(state.entities.player.orbs[2].evoke_amount, 12);
    assert_eq!(
        state.pop_next_action(),
        None,
        "DarkImpulseAction must not trigger Plasma/Frost/Lightning passive effects"
    );

    let mut first_not_dark = crate::test_support::blank_test_combat();
    first_not_dark.entities.player.max_orbs = 2;
    first_not_dark.entities.player.orbs =
        vec![OrbEntity::new(OrbId::Frost), OrbEntity::new(OrbId::Dark)];
    first_not_dark
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::GoldPlatedCables,
        ));

    crate::engine::action_handlers::execute_action(
        Action::TriggerDarkImpulseOrbs,
        &mut first_not_dark,
    );
    assert_eq!(first_not_dark.entities.player.orbs[0].evoke_amount, 5);
    assert_eq!(first_not_dark.entities.player.orbs[1].evoke_amount, 12);
    assert_eq!(first_not_dark.pop_next_action(), None);
}

#[test]
fn defect_more_uncommon_definitions_match_java_sources() {
    let aggregate = get_card_definition(CardId::Aggregate);
    assert_eq!(aggregate.name, "Aggregate");
    assert_eq!(aggregate.card_type, CardType::Skill);
    assert_eq!(aggregate.rarity, CardRarity::Uncommon);
    assert_eq!(aggregate.cost, 1);
    assert_eq!(aggregate.base_magic, 4);
    assert_eq!(aggregate.upgrade_magic, -1);
    assert_eq!(java_id(CardId::Aggregate), "Aggregate");

    let auto = get_card_definition(CardId::AutoShields);
    assert_eq!(auto.name, "Auto-Shields");
    assert_eq!(auto.card_type, CardType::Skill);
    assert_eq!(auto.rarity, CardRarity::Uncommon);
    assert_eq!(auto.cost, 1);
    assert_eq!(auto.base_block, 11);
    assert_eq!(auto.upgrade_block, 4);
    assert_eq!(java_id(CardId::AutoShields), "Auto Shields");

    let boot = get_card_definition(CardId::BootSequence);
    assert_eq!(boot.name, "Boot Sequence");
    assert_eq!(boot.card_type, CardType::Skill);
    assert_eq!(boot.rarity, CardRarity::Uncommon);
    assert_eq!(boot.cost, 0);
    assert_eq!(boot.base_block, 10);
    assert_eq!(boot.upgrade_block, 3);
    assert!(boot.exhaust);
    assert!(boot.innate);
    assert_eq!(java_id(CardId::BootSequence), "BootSequence");

    let capacitor = get_card_definition(CardId::Capacitor);
    assert_eq!(capacitor.name, "Capacitor");
    assert_eq!(capacitor.card_type, CardType::Power);
    assert_eq!(capacitor.rarity, CardRarity::Uncommon);
    assert_eq!(capacitor.cost, 1);
    assert_eq!(capacitor.base_magic, 2);
    assert_eq!(capacitor.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Capacitor), "Capacitor");

    let chill = get_card_definition(CardId::Chill);
    assert_eq!(chill.name, "Chill");
    assert_eq!(chill.card_type, CardType::Skill);
    assert_eq!(chill.rarity, CardRarity::Uncommon);
    assert_eq!(chill.cost, 0);
    assert_eq!(chill.base_magic, 1);
    assert!(chill.exhaust);
    assert!(!chill.innate);
    assert_eq!(java_id(CardId::Chill), "Chill");

    let mut chill_plus = CombatCard::for_scenario(CardId::Chill, 264);
    chill_plus.upgrades = 1;
    assert!(is_innate_card(&chill_plus));
    assert_eq!(
        build_java_id_map().get("Auto Shields"),
        Some(&CardId::AutoShields)
    );
    assert_eq!(
        build_java_id_map().get("BootSequence"),
        Some(&CardId::BootSequence)
    );
}

#[test]
fn defect_more_uncommon_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();

    let aggregate = resolve_card_play(
        CardId::Aggregate,
        &state,
        &CombatCard::for_scenario(CardId::Aggregate, 265),
        None,
    );
    assert_eq!(
        aggregate[0].action,
        Action::AggregateEnergy { divide_amount: 4 }
    );
    let mut aggregate_plus = CombatCard::for_scenario(CardId::Aggregate, 266);
    aggregate_plus.upgrades = 1;
    let aggregate_plus_actions =
        resolve_card_play(CardId::Aggregate, &state, &aggregate_plus, None);
    assert_eq!(
        aggregate_plus_actions[0].action,
        Action::AggregateEnergy { divide_amount: 3 }
    );

    let auto = resolve_card_play(
        CardId::AutoShields,
        &state,
        &CombatCard::for_scenario(CardId::AutoShields, 267),
        None,
    );
    assert_eq!(
        auto[0].action,
        Action::GainBlock {
            target: 0,
            amount: 11,
        }
    );
    state.entities.player.block = 1;
    let blocked_auto = resolve_card_play(
        CardId::AutoShields,
        &state,
        &CombatCard::for_scenario(CardId::AutoShields, 268),
        None,
    );
    assert!(
        blocked_auto.is_empty(),
        "Java AutoShields.use queues no GainBlockAction when currentBlock is nonzero"
    );
    state.entities.player.block = 0;
    let mut auto_plus = CombatCard::for_scenario(CardId::AutoShields, 269);
    auto_plus.upgrades = 1;
    let auto_plus_actions = resolve_card_play(CardId::AutoShields, &state, &auto_plus, None);
    assert_eq!(
        auto_plus_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 15,
        }
    );

    let boot = resolve_card_play(
        CardId::BootSequence,
        &state,
        &CombatCard::for_scenario(CardId::BootSequence, 270),
        None,
    );
    assert_eq!(
        boot[0].action,
        Action::GainBlock {
            target: 0,
            amount: 10,
        }
    );

    let capacitor = resolve_card_play(
        CardId::Capacitor,
        &state,
        &CombatCard::for_scenario(CardId::Capacitor, 271),
        None,
    );
    assert_eq!(capacitor[0].action, Action::IncreaseMaxOrb(2));
    let mut capacitor_plus = CombatCard::for_scenario(CardId::Capacitor, 272);
    capacitor_plus.upgrades = 1;
    let capacitor_plus_actions =
        resolve_card_play(CardId::Capacitor, &state, &capacitor_plus, None);
    assert_eq!(capacitor_plus_actions[0].action, Action::IncreaseMaxOrb(3));

    let mut alive = crate::test_support::test_monster(EnemyId::JawWorm);
    alive.id = 273;
    let mut escaped = crate::test_support::test_monster(EnemyId::Cultist);
    escaped.id = 274;
    escaped.is_escaped = true;
    let mut half_dead = crate::test_support::test_monster(EnemyId::Darkling);
    half_dead.id = 275;
    half_dead.half_dead = true;
    state.entities.monsters = vec![alive, escaped, half_dead];
    let chill = resolve_card_play(
        CardId::Chill,
        &state,
        &CombatCard::for_scenario(CardId::Chill, 276),
        None,
    );
    assert_eq!(chill.len(), 1);
    assert_eq!(chill[0].action, Action::ChannelOrb(OrbId::Frost));
}

#[test]
fn aggregate_energy_reads_draw_pile_size_when_action_executes() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 0;
    state.zones.draw_pile = ((0..9)
        .map(|offset| CombatCard::for_scenario(CardId::StrikeB, 300 + offset))
        .collect::<Vec<_>>())
    .into();

    crate::engine::action_handlers::execute_action(
        Action::AggregateEnergy { divide_amount: 4 },
        &mut state,
    );
    assert_eq!(state.turn.energy, 2);

    state
        .zones
        .draw_pile
        .push(CombatCard::for_scenario(CardId::StrikeB, 400));
    crate::engine::action_handlers::execute_action(
        Action::AggregateEnergy { divide_amount: 3 },
        &mut state,
    );
    assert_eq!(state.turn.energy, 5);
}

#[test]
fn defect_x_block_dark_attack_and_consume_definitions_match_java_sources() {
    let body = get_card_definition(CardId::ReinforcedBody);
    assert_eq!(body.name, "Reinforced Body");
    assert_eq!(body.card_type, CardType::Skill);
    assert_eq!(body.rarity, CardRarity::Uncommon);
    assert_eq!(body.cost, -1);
    assert_eq!(body.base_block, 7);
    assert_eq!(body.upgrade_block, 2);
    assert_eq!(java_id(CardId::ReinforcedBody), "Reinforced Body");

    let doom = get_card_definition(CardId::DoomAndGloom);
    assert_eq!(doom.name, "Doom and Gloom");
    assert_eq!(doom.card_type, CardType::Attack);
    assert_eq!(doom.rarity, CardRarity::Uncommon);
    assert_eq!(doom.cost, 2);
    assert_eq!(doom.base_damage, 10);
    assert_eq!(doom.base_magic, 1);
    assert_eq!(doom.target, CardTarget::AllEnemy);
    assert!(doom.is_multi_damage);
    assert_eq!(doom.upgrade_damage, 4);
    assert_eq!(java_id(CardId::DoomAndGloom), "Doom and Gloom");

    let consume = get_card_definition(CardId::Consume);
    assert_eq!(consume.name, "Consume");
    assert_eq!(consume.card_type, CardType::Skill);
    assert_eq!(consume.rarity, CardRarity::Uncommon);
    assert_eq!(consume.cost, 2);
    assert_eq!(consume.base_magic, 2);
    assert_eq!(consume.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Consume), "Consume");
}

#[test]
fn defect_x_block_dark_attack_and_consume_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 280;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 281;
    state.entities.monsters = vec![first, second];

    let mut body_card = CombatCard::for_scenario(CardId::ReinforcedBody, 282);
    body_card.energy_on_use = 2;
    let body = resolve_card_play(CardId::ReinforcedBody, &state, &body_card, None);
    assert_eq!(
        body[0].action,
        Action::ReinforcedBody {
            block_amount: 7,
            free_to_play_once: false,
            energy_on_use: 2,
        }
    );
    let mut body_plus = CombatCard::for_scenario(CardId::ReinforcedBody, 283);
    body_plus.upgrades = 1;
    body_plus.energy_on_use = 2;
    let body_plus_actions = resolve_card_play(CardId::ReinforcedBody, &state, &body_plus, None);
    assert_eq!(
        body_plus_actions[0].action,
        Action::ReinforcedBody {
            block_amount: 9,
            free_to_play_once: false,
            energy_on_use: 2,
        }
    );

    let doom = resolve_card_play(
        CardId::DoomAndGloom,
        &state,
        &CombatCard::for_scenario(CardId::DoomAndGloom, 284),
        None,
    );
    assert_eq!(doom.len(), 2);
    match &doom[0].action {
        Action::DamageAllEnemies { damages, .. } => {
            assert_eq!(damages.as_slice(), &[10, 10]);
        }
        other => panic!("Doom and Gloom should damage all enemies first, got {other:?}"),
    }
    assert_eq!(doom[1].action, Action::ChannelOrb(OrbId::Dark));
    let mut doom_plus = CombatCard::for_scenario(CardId::DoomAndGloom, 285);
    doom_plus.upgrades = 1;
    let doom_plus_actions = resolve_card_play(CardId::DoomAndGloom, &state, &doom_plus, None);
    match &doom_plus_actions[0].action {
        Action::DamageAllEnemies { damages, .. } => {
            assert_eq!(damages.as_slice(), &[14, 14]);
        }
        other => panic!("Doom and Gloom+ should damage all enemies first, got {other:?}"),
    }

    let consume = resolve_card_play(
        CardId::Consume,
        &state,
        &CombatCard::for_scenario(CardId::Consume, 286),
        None,
    );
    assert_eq!(consume.len(), 2);
    assert_eq!(
        consume[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 2,
        }
    );
    assert_eq!(consume[1].action, Action::DecreaseMaxOrb(1));
    let mut consume_plus = CombatCard::for_scenario(CardId::Consume, 287);
    consume_plus.upgrades = 1;
    let consume_plus_actions = resolve_card_play(CardId::Consume, &state, &consume_plus, None);
    assert_eq!(
        consume_plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 3,
        }
    );
}

#[test]
fn reinforced_body_and_decrease_max_orb_actions_match_java_execution_semantics() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 2;
    crate::engine::action_handlers::execute_action(
        Action::ReinforcedBody {
            block_amount: 7,
            free_to_play_once: false,
            energy_on_use: 2,
        },
        &mut state,
    );
    assert_eq!(state.turn.energy, 0);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 7,
        })
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 7,
        })
    );
    assert_eq!(state.pop_next_action(), None);

    let mut chemical_x = crate::test_support::blank_test_combat();
    chemical_x
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ChemicalX,
        ));
    crate::engine::action_handlers::execute_action(
        Action::ReinforcedBody {
            block_amount: 9,
            free_to_play_once: true,
            energy_on_use: 0,
        },
        &mut chemical_x,
    );
    assert_eq!(chemical_x.turn.energy, 3);
    assert_eq!(
        chemical_x.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 9,
        })
    );
    assert_eq!(
        chemical_x.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 9,
        })
    );
    assert_eq!(chemical_x.pop_next_action(), None);

    let mut orbs = crate::test_support::blank_test_combat();
    orbs.entities.player.max_orbs = 3;
    orbs.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Lightning),
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Dark),
    ];
    crate::engine::action_handlers::execute_action(Action::DecreaseMaxOrb(1), &mut orbs);
    assert_eq!(orbs.entities.player.max_orbs, 2);
    assert_eq!(
        orbs.entities.player.orbs,
        vec![
            OrbEntity::new(OrbId::Lightning),
            OrbEntity::new(OrbId::Frost)
        ],
        "Java decreaseMaxOrbSlots removes the last orb slot without evoking it"
    );
}

#[test]
fn sunder_definition_and_runtime_action_match_java_sources() {
    let sunder = get_card_definition(CardId::Sunder);
    assert_eq!(sunder.name, "Sunder");
    assert_eq!(sunder.card_type, CardType::Attack);
    assert_eq!(sunder.rarity, CardRarity::Uncommon);
    assert_eq!(sunder.cost, 3);
    assert_eq!(sunder.base_damage, 24);
    assert_eq!(sunder.target, CardTarget::Enemy);
    assert_eq!(sunder.upgrade_damage, 8);
    assert_eq!(java_id(CardId::Sunder), "Sunder");

    let state = crate::test_support::blank_test_combat();
    let sunder_actions = resolve_card_play(
        CardId::Sunder,
        &state,
        &CombatCard::for_scenario(CardId::Sunder, 288),
        Some(289),
    );
    assert_eq!(sunder_actions.len(), 1);
    match &sunder_actions[0].action {
        Action::Sunder {
            target,
            damage_info,
            energy_gain,
        } => {
            assert_eq!(*target, 289);
            assert_eq!(damage_info.base, 24);
            assert_eq!(damage_info.output, 24);
            assert_eq!(*energy_gain, 3);
        }
        other => panic!("Sunder should emit SunderAction equivalent, got {other:?}"),
    }
    let mut sunder_plus = CombatCard::for_scenario(CardId::Sunder, 290);
    sunder_plus.upgrades = 1;
    let sunder_plus_actions = resolve_card_play(CardId::Sunder, &state, &sunder_plus, Some(289));
    match &sunder_plus_actions[0].action {
        Action::Sunder { damage_info, .. } => {
            assert_eq!(damage_info.base, 32);
            assert_eq!(damage_info.output, 32);
        }
        other => panic!("Sunder+ should emit SunderAction equivalent, got {other:?}"),
    }
}

#[test]
fn sunder_action_grants_energy_only_when_target_dies_and_not_cleared_by_combat_end() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 291;
    target.current_hp = 10;
    let mut other = crate::test_support::test_monster(EnemyId::Cultist);
    other.id = 292;
    other.current_hp = 30;
    state.entities.monsters = vec![target, other];

    crate::engine::action_handlers::execute_action(
        Action::Sunder {
            target: 291,
            damage_info: DamageInfo {
                source: 0,
                target: 291,
                base: 24,
                output: 24,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
            energy_gain: 3,
        },
        &mut state,
    );
    assert_eq!(state.entities.monsters[0].current_hp, 0);
    let queued: Vec<_> = std::iter::from_fn(|| state.pop_next_action()).collect();
    assert!(
        queued.contains(&Action::GainEnergy { amount: 3 }),
        "Java SunderAction queues GainEnergyAction when the target dies and combat continues"
    );

    let mut final_enemy = crate::test_support::blank_test_combat();
    let mut only = crate::test_support::test_monster(EnemyId::JawWorm);
    only.id = 293;
    only.current_hp = 10;
    final_enemy.entities.monsters = vec![only];
    crate::engine::action_handlers::execute_action(
        Action::Sunder {
            target: 293,
            damage_info: DamageInfo {
                source: 0,
                target: 293,
                base: 24,
                output: 24,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
            energy_gain: 3,
        },
        &mut final_enemy,
    );
    assert_eq!(
        final_enemy.pop_next_action(),
        None,
        "Java clearPostCombatActions runs after Sunder queues energy, so final-kill energy is not retained"
    );
}

#[test]
fn chaos_definition_and_runtime_action_match_java_sources() {
    let chaos = get_card_definition(CardId::Chaos);
    assert_eq!(chaos.name, "Chaos");
    assert_eq!(chaos.card_type, CardType::Skill);
    assert_eq!(chaos.rarity, CardRarity::Uncommon);
    assert_eq!(chaos.cost, 1);
    assert_eq!(chaos.base_magic, 1);
    assert_eq!(chaos.target, CardTarget::SelfTarget);
    assert_eq!(chaos.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Chaos), "Chaos");

    let state = crate::test_support::blank_test_combat();
    let chaos_actions = resolve_card_play(
        CardId::Chaos,
        &state,
        &CombatCard::for_scenario(CardId::Chaos, 294),
        None,
    );
    assert_eq!(chaos_actions.len(), 1);
    assert_eq!(
        chaos_actions[0].action,
        Action::ChannelRandomOrbs { amount: 1 }
    );

    let mut chaos_plus = CombatCard::for_scenario(CardId::Chaos, 295);
    chaos_plus.upgrades = 1;
    let chaos_plus_actions = resolve_card_play(CardId::Chaos, &state, &chaos_plus, None);
    assert_eq!(
        chaos_plus_actions[0].action,
        Action::ChannelRandomOrbs { amount: 2 }
    );
}

#[test]
fn chaos_materializes_all_random_orbs_before_channeling_any_of_them() {
    use crate::runtime::combat::OrbId;

    let mut state = crate::test_support::blank_test_combat();
    state.rng.card_random_rng = crate::runtime::rng::StsRng::new(42);
    let mut expected_rng = state.rng.card_random_rng.clone();
    let expected: Vec<OrbId> = (0..2)
        .map(|_| match expected_rng.random(3) {
            0 => OrbId::Dark,
            1 => OrbId::Frost,
            2 => OrbId::Lightning,
            _ => OrbId::Plasma,
        })
        .collect();

    crate::engine::action_handlers::execute_action(
        Action::ChannelRandomOrbs { amount: 2 },
        &mut state,
    );
    assert_eq!(
        state.rng.card_random_rng.counter, expected_rng.counter,
        "Java Chaos calls AbstractOrb.getRandomOrb(true) once per channel during card use"
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ChannelOrb(expected[0]))
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ChannelOrb(expected[1]))
    );
}

#[test]
fn buffer_definition_and_runtime_action_match_java_sources() {
    let buffer = get_card_definition(CardId::Buffer);
    assert_eq!(buffer.name, "Buffer");
    assert_eq!(buffer.card_type, CardType::Power);
    assert_eq!(buffer.rarity, CardRarity::Rare);
    assert_eq!(buffer.cost, 2);
    assert_eq!(buffer.base_magic, 1);
    assert_eq!(buffer.target, CardTarget::SelfTarget);
    assert_eq!(buffer.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Buffer), "Buffer");

    let state = crate::test_support::blank_test_combat();
    let buffer_actions = resolve_card_play(
        CardId::Buffer,
        &state,
        &CombatCard::for_scenario(CardId::Buffer, 296),
        None,
    );
    assert_eq!(buffer_actions.len(), 1);
    assert_eq!(
        buffer_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Buffer,
            amount: 1,
        }
    );

    let mut buffer_plus = CombatCard::for_scenario(CardId::Buffer, 297);
    buffer_plus.upgrades = 1;
    let buffer_plus_actions = resolve_card_play(CardId::Buffer, &state, &buffer_plus, None);
    assert_eq!(
        buffer_plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Buffer,
            amount: 2,
        }
    );
}

#[test]
fn equilibrium_definition_and_runtime_action_match_java_sources() {
    let equilibrium = get_card_definition(CardId::Equilibrium);
    assert_eq!(equilibrium.name, "Equilibrium");
    assert_eq!(equilibrium.card_type, CardType::Skill);
    assert_eq!(equilibrium.rarity, CardRarity::Uncommon);
    assert_eq!(equilibrium.cost, 2);
    assert_eq!(equilibrium.base_block, 13);
    assert_eq!(equilibrium.base_magic, 1);
    assert_eq!(equilibrium.target, CardTarget::SelfTarget);
    assert_eq!(equilibrium.upgrade_block, 3);
    assert_eq!(equilibrium.upgrade_magic, 0);
    assert_eq!(java_id(CardId::Equilibrium), "Undo");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Equilibrium,
        &state,
        &CombatCard::for_scenario(CardId::Equilibrium, 298),
        None,
    );
    assert_eq!(actions.len(), 2);
    assert_eq!(
        actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 13,
        }
    );
    assert_eq!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Equilibrium,
            amount: 1,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::Equilibrium, 299);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::Equilibrium, &state, &plus, None);
    assert_eq!(
        plus_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 16,
        }
    );
    assert_eq!(
        plus_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: crate::content::powers::PowerId::Equilibrium,
            amount: 1,
        }
    );
}

#[test]
fn equilibrium_power_retains_non_ethereal_hand_cards_and_ticks_at_round_end() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeB, 300),
        CombatCard::for_scenario(CardId::GhostlyArmor, 301),
        CombatCard::for_scenario(CardId::DefendB, 302),
    ];

    let power = crate::runtime::combat::Power {
        power_type: crate::content::powers::PowerId::Equilibrium,
        instance_id: None,
        amount: 1,
        extra_data: 0,
        payload: crate::runtime::combat::PowerPayload::None,
        just_applied: false,
    };
    let end_turn_actions =
        crate::content::powers::resolve_power_at_end_of_turn(&power, &mut state, 0);
    assert!(end_turn_actions.is_empty());
    let retain_flags: Vec<_> = state
        .zones
        .hand
        .iter()
        .map(|card| (card.id, card.retain_override))
        .collect();
    assert_eq!(
        retain_flags,
        vec![
            (CardId::StrikeB, Some(true)),
            (CardId::GhostlyArmor, None),
            (CardId::DefendB, Some(true)),
        ],
        "Java EquilibriumPower marks every non-ethereal hand card retained at end of turn"
    );

    let end_round_actions = crate::content::powers::resolve_power_at_end_of_round(
        crate::content::powers::PowerId::Equilibrium,
        &state,
        0,
        1,
        false,
    );
    assert_eq!(
        end_round_actions.as_slice(),
        &[Action::ReducePower {
            target: 0,
            power_id: crate::content::powers::PowerId::Equilibrium,
            amount: 1,
        }]
    );
}

#[test]
fn self_repair_definition_runtime_and_victory_power_match_java_sources() {
    let self_repair = get_card_definition(CardId::SelfRepair);
    assert_eq!(self_repair.name, "Self Repair");
    assert_eq!(self_repair.card_type, CardType::Power);
    assert_eq!(self_repair.rarity, CardRarity::Uncommon);
    assert_eq!(self_repair.cost, 1);
    assert_eq!(self_repair.base_magic, 7);
    assert_eq!(self_repair.target, CardTarget::SelfTarget);
    assert_eq!(self_repair.upgrade_magic, 3);
    assert!(self_repair.tags.contains(&CardTag::Healing));
    assert_eq!(java_id(CardId::SelfRepair), "Self Repair");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::SelfRepair,
        &state,
        &CombatCard::for_scenario(CardId::SelfRepair, 303),
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Repair,
            amount: 7,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::SelfRepair, 304);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::SelfRepair, &state, &plus, None);
    assert_eq!(
        plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Repair,
            amount: 10,
        }
    );

    let mut wounded = crate::test_support::blank_test_combat();
    wounded.entities.player.current_hp = 40;
    wounded.entities.player.max_hp = 80;
    let victory_actions =
        crate::content::powers::resolve_power_on_victory(PowerId::Repair, &wounded, 0, 7);
    assert_eq!(
        victory_actions.as_slice(),
        &[Action::Heal {
            target: 0,
            amount: 7,
        }],
        "Java RepairPower.onVictory heals the living player by its amount"
    );

    let mut dead = wounded;
    dead.entities.player.current_hp = 0;
    assert!(
        crate::content::powers::resolve_power_on_victory(PowerId::Repair, &dead, 0, 7).is_empty(),
        "Java RepairPower checks p.currentHealth > 0 before healing"
    );
}

#[test]
fn lock_on_definition_runtime_and_power_decay_match_java_sources() {
    let lock_on = get_card_definition(CardId::LockOn);
    assert_eq!(lock_on.name, "Lock-On");
    assert_eq!(lock_on.card_type, CardType::Attack);
    assert_eq!(lock_on.rarity, CardRarity::Uncommon);
    assert_eq!(lock_on.cost, 1);
    assert_eq!(lock_on.base_damage, 8);
    assert_eq!(lock_on.base_magic, 2);
    assert_eq!(lock_on.target, CardTarget::Enemy);
    assert_eq!(lock_on.upgrade_damage, 3);
    assert_eq!(lock_on.upgrade_magic, 1);
    assert_eq!(java_id(CardId::LockOn), "Lockon");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::LockOn,
        &state,
        &CombatCard::for_scenario(CardId::LockOn, 305),
        Some(306),
    );
    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 306);
            assert_eq!(info.base, 8);
            assert_eq!(info.output, 8);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Lock-On should damage before applying Lock-On, got {other:?}"),
    }
    assert_eq!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 306,
            power_id: PowerId::LockOn,
            amount: 2,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::LockOn, 307);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::LockOn, &state, &plus, Some(306));
    match &plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 11);
            assert_eq!(info.output, 11);
        }
        other => panic!("Lock-On+ should damage before applying Lock-On, got {other:?}"),
    }
    assert_eq!(
        plus_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 306,
            power_id: PowerId::LockOn,
            amount: 3,
        }
    );

    assert!(crate::content::powers::is_debuff(PowerId::LockOn, 2));
    assert!(crate::content::powers::is_debuff_application(
        PowerId::LockOn,
        2
    ));
    let decay = crate::content::powers::resolve_power_at_end_of_round(
        PowerId::LockOn,
        &state,
        306,
        2,
        false,
    );
    assert_eq!(
        decay.as_slice(),
        &[Action::ReducePower {
            target: 306,
            power_id: PowerId::LockOn,
            amount: 1,
        }]
    );
}

#[test]
fn lock_on_multiplier_applies_only_to_orb_damage_paths() {
    let mut state = crate::test_support::blank_test_combat();
    let mut locked = crate::test_support::test_monster(EnemyId::JawWorm);
    locked.id = 308;
    locked.current_hp = 50;
    let mut plain = crate::test_support::test_monster(EnemyId::Cultist);
    plain.id = 309;
    plain.current_hp = 50;
    state.entities.monsters = vec![locked, plain];
    crate::content::powers::store::set_powers_for(
        &mut state,
        308,
        vec![Power {
            power_type: PowerId::LockOn,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::execute_action(
        Action::OrbDamage {
            source: 0,
            target: 308,
            base_damage: 8,
        },
        &mut state,
    );
    assert_eq!(state.entities.monsters[0].current_hp, 38);

    crate::engine::action_handlers::execute_action(
        Action::OrbDamageAllEnemies {
            source: 0,
            base_damage: 8,
        },
        &mut state,
    );
    let all_enemy_actions: Vec<_> = std::iter::from_fn(|| state.pop_next_action()).collect();
    assert!(all_enemy_actions.contains(&Action::Damage(DamageInfo {
        source: 0,
        target: 308,
        base: 8,
        output: 12,
        damage_type: DamageType::Thorns,
        is_modified: true,
    })));
    assert!(all_enemy_actions.contains(&Action::Damage(DamageInfo {
        source: 0,
        target: 309,
        base: 8,
        output: 8,
        damage_type: DamageType::Thorns,
        is_modified: false,
    })));

    crate::engine::action_handlers::execute_action(
        Action::DamageRandomEnemy {
            source: 0,
            base_damage: 8,
            damage_type: DamageType::Thorns,
        },
        &mut state,
    );
    match state.pop_next_action() {
        Some(Action::Damage(info)) => {
            assert_eq!(
                info.output, 8,
                "non-orb THORNS actions such as Tingsha/Juggernaut must not inherit Lock-On"
            );
        }
        other => panic!("expected queued non-orb random damage, got {other:?}"),
    }
}

#[test]
fn core_surge_definition_and_runtime_action_match_java_sources() {
    let core_surge = get_card_definition(CardId::CoreSurge);
    assert_eq!(core_surge.name, "Core Surge");
    assert_eq!(core_surge.card_type, CardType::Attack);
    assert_eq!(core_surge.rarity, CardRarity::Rare);
    assert_eq!(core_surge.cost, 1);
    assert_eq!(core_surge.base_damage, 11);
    assert_eq!(core_surge.base_magic, 1);
    assert_eq!(core_surge.target, CardTarget::Enemy);
    assert!(core_surge.exhaust);
    assert_eq!(core_surge.upgrade_damage, 4);
    assert_eq!(core_surge.upgrade_magic, 0);
    assert_eq!(java_id(CardId::CoreSurge), "Core Surge");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::CoreSurge,
        &state,
        &CombatCard::for_scenario(CardId::CoreSurge, 310),
        Some(311),
    );
    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 311);
            assert_eq!(info.base, 11);
            assert_eq!(info.output, 11);
        }
        other => panic!("Core Surge should damage before Artifact, got {other:?}"),
    }
    assert_eq!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Artifact,
            amount: 1,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::CoreSurge, 312);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::CoreSurge, &state, &plus, Some(311));
    match &plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 15);
            assert_eq!(info.output, 15);
        }
        other => panic!("Core Surge+ should damage before Artifact, got {other:?}"),
    }
    assert_eq!(
        plus_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Artifact,
            amount: 1,
        }
    );
}

#[test]
fn biased_cognition_definition_runtime_and_bias_power_match_java_sources() {
    let biased = get_card_definition(CardId::BiasedCognition);
    assert_eq!(biased.name, "Biased Cognition");
    assert_eq!(biased.card_type, CardType::Power);
    assert_eq!(biased.rarity, CardRarity::Rare);
    assert_eq!(biased.cost, 1);
    assert_eq!(biased.base_magic, 4);
    assert_eq!(biased.target, CardTarget::SelfTarget);
    assert_eq!(biased.upgrade_magic, 1);
    assert_eq!(java_id(CardId::BiasedCognition), "Biased Cognition");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::BiasedCognition,
        &state,
        &CombatCard::for_scenario(CardId::BiasedCognition, 313),
        None,
    );
    assert_eq!(actions.len(), 2);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 4,
        }
    );
    assert_eq!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Bias,
            amount: 1,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::BiasedCognition, 314);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::BiasedCognition, &state, &plus, None);
    assert_eq!(
        plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 5,
        }
    );
    assert_eq!(plus_actions[1].action, actions[1].action);

    assert!(crate::content::powers::is_debuff(PowerId::Bias, 1));
    assert!(crate::content::powers::is_debuff_application(
        PowerId::Bias,
        1
    ));

    let mut start_state = crate::test_support::blank_test_combat();
    let start_actions =
        crate::content::powers::resolve_power_at_turn_start(PowerId::Bias, &mut start_state, 0, 1);
    assert_eq!(
        start_actions.as_slice(),
        &[Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: -1,
        }],
        "Java BiasPower.atStartOfTurn applies negative Focus through ApplyPowerAction"
    );

    let mut artifact_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut artifact_state,
        0,
        vec![Power {
            power_type: PowerId::Artifact,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Bias,
            amount: 1,
        },
        &mut artifact_state,
    );
    assert_eq!(
        crate::content::powers::store::power_amount(&artifact_state, 0, PowerId::Bias),
        0,
        "BiasPower is a Java DEBUFF and should be blockable by Artifact"
    );
    assert_eq!(
        crate::content::powers::store::power_amount(&artifact_state, 0, PowerId::Artifact),
        0,
        "blocking Bias consumes one Artifact stack"
    );
}

#[test]
fn loop_definition_runtime_and_first_orb_trigger_match_java_sources() {
    let loop_card = get_card_definition(CardId::Loop);
    assert_eq!(loop_card.name, "Loop");
    assert_eq!(loop_card.card_type, CardType::Power);
    assert_eq!(loop_card.rarity, CardRarity::Uncommon);
    assert_eq!(loop_card.cost, 1);
    assert_eq!(loop_card.base_magic, 1);
    assert_eq!(loop_card.target, CardTarget::SelfTarget);
    assert_eq!(loop_card.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Loop), "Loop");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Loop,
        &state,
        &CombatCard::for_scenario(CardId::Loop, 315),
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Loop,
            amount: 1,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::Loop, 316);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::Loop, &state, &plus, None);
    assert_eq!(
        plus_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Loop,
            amount: 2,
        }
    );

    let mut frost_state = crate::test_support::blank_test_combat();
    frost_state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Lightning),
    ];
    let frost_actions =
        crate::content::powers::resolve_power_at_turn_start(PowerId::Loop, &mut frost_state, 0, 2);
    assert!(frost_actions.is_empty());
    assert_eq!(
        std::iter::from_fn(|| frost_state.pop_next_action()).collect::<Vec<_>>(),
        vec![
            Action::GainBlock {
                target: 0,
                amount: 2,
            },
            Action::GainBlock {
                target: 0,
                amount: 2,
            },
        ],
        "Java LoopPower triggers only the first orb, so the second Lightning orb should not fire"
    );

    let mut dark_state = crate::test_support::blank_test_combat();
    dark_state.entities.player.orbs = vec![OrbEntity::new(OrbId::Dark)];
    crate::content::powers::resolve_power_at_turn_start(PowerId::Loop, &mut dark_state, 0, 2);
    assert_eq!(
        dark_state.entities.player.orbs[0].evoke_amount, 18,
        "Dark orb onEndOfTurn runs once per Loop stack"
    );
}

#[test]
fn meteor_strike_definition_and_runtime_action_match_java_sources() {
    let meteor = get_card_definition(CardId::MeteorStrike);
    assert_eq!(meteor.name, "Meteor Strike");
    assert_eq!(meteor.card_type, CardType::Attack);
    assert_eq!(meteor.rarity, CardRarity::Rare);
    assert_eq!(meteor.cost, 5);
    assert_eq!(meteor.base_damage, 24);
    assert_eq!(meteor.base_magic, 3);
    assert_eq!(meteor.target, CardTarget::Enemy);
    assert_eq!(meteor.upgrade_damage, 6);
    assert_eq!(meteor.upgrade_magic, 0);
    assert!(meteor.tags.contains(&CardTag::Strike));
    assert_eq!(java_id(CardId::MeteorStrike), "Meteor Strike");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::MeteorStrike,
        &state,
        &CombatCard::for_scenario(CardId::MeteorStrike, 317),
        Some(318),
    );
    assert_eq!(actions.len(), 4);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 318);
            assert_eq!(info.base, 24);
            assert_eq!(info.output, 24);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Meteor Strike should damage before channeling Plasma, got {other:?}"),
    }
    assert_eq!(actions[1].action, Action::ChannelOrb(OrbId::Plasma));
    assert_eq!(actions[2].action, Action::ChannelOrb(OrbId::Plasma));
    assert_eq!(actions[3].action, Action::ChannelOrb(OrbId::Plasma));

    let mut plus = CombatCard::for_scenario(CardId::MeteorStrike, 319);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::MeteorStrike, &state, &plus, Some(318));
    match &plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.base, 30);
            assert_eq!(info.output, 30);
        }
        other => panic!("Meteor Strike+ should still damage before Plasma, got {other:?}"),
    }
    assert_eq!(
        plus_actions[1..]
            .iter()
            .map(|info| &info.action)
            .collect::<Vec<_>>(),
        vec![
            &Action::ChannelOrb(OrbId::Plasma),
            &Action::ChannelOrb(OrbId::Plasma),
            &Action::ChannelOrb(OrbId::Plasma),
        ],
        "Java Meteor Strike upgrades damage only; it still channels exactly three Plasma"
    );
}

#[test]
fn hyperbeam_definition_and_runtime_action_match_java_sources() {
    let hyperbeam = get_card_definition(CardId::Hyperbeam);
    assert_eq!(hyperbeam.name, "Hyperbeam");
    assert_eq!(hyperbeam.card_type, CardType::Attack);
    assert_eq!(hyperbeam.rarity, CardRarity::Rare);
    assert_eq!(hyperbeam.cost, 2);
    assert_eq!(hyperbeam.base_damage, 26);
    assert_eq!(hyperbeam.base_magic, 3);
    assert_eq!(hyperbeam.target, CardTarget::AllEnemy);
    assert!(hyperbeam.is_multi_damage);
    assert_eq!(hyperbeam.upgrade_damage, 8);
    assert_eq!(hyperbeam.upgrade_magic, 0);
    assert_eq!(java_id(CardId::Hyperbeam), "Hyperbeam");

    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 320;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 321;
    state.entities.monsters = vec![first, second];

    let actions = resolve_card_play(
        CardId::Hyperbeam,
        &state,
        &CombatCard::for_scenario(CardId::Hyperbeam, 322),
        None,
    );
    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(damages.as_slice(), &[26, 26]);
            assert_eq!(*damage_type, DamageType::Normal);
            assert!(!is_modified);
        }
        other => panic!("Hyperbeam should damage all enemies first, got {other:?}"),
    }
    assert_eq!(
        actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: -3,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::Hyperbeam, 323);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::Hyperbeam, &state, &plus, None);
    match &plus_actions[0].action {
        Action::DamageAllEnemies { damages, .. } => {
            assert_eq!(damages.as_slice(), &[34, 34]);
        }
        other => panic!("Hyperbeam+ should damage all enemies first, got {other:?}"),
    }
    assert_eq!(
        plus_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: -3,
        },
        "Java Hyperbeam upgrades damage only; Focus loss remains 3"
    );
}

#[test]
fn electrodynamics_definition_runtime_and_sentinel_power_match_java_sources() {
    let electro = get_card_definition(CardId::Electrodynamics);
    assert_eq!(electro.name, "Electrodynamics");
    assert_eq!(electro.card_type, CardType::Power);
    assert_eq!(electro.rarity, CardRarity::Rare);
    assert_eq!(electro.cost, 2);
    assert_eq!(electro.base_magic, 2);
    assert_eq!(electro.target, CardTarget::SelfTarget);
    assert_eq!(electro.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Electrodynamics), "Electrodynamics");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Electrodynamics,
        &state,
        &CombatCard::for_scenario(CardId::Electrodynamics, 324),
        None,
    );
    assert_eq!(actions.len(), 3);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Electro,
            amount: -1,
        },
        "Java ElectroPower uses AbstractPower's sentinel amount -1"
    );
    assert_eq!(actions[1].action, Action::ChannelOrb(OrbId::Lightning));
    assert_eq!(actions[2].action, Action::ChannelOrb(OrbId::Lightning));

    let mut plus = CombatCard::for_scenario(CardId::Electrodynamics, 325);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::Electrodynamics, &state, &plus, None);
    assert_eq!(plus_actions.len(), 4);
    assert_eq!(plus_actions[1].action, Action::ChannelOrb(OrbId::Lightning));
    assert_eq!(plus_actions[2].action, Action::ChannelOrb(OrbId::Lightning));
    assert_eq!(plus_actions[3].action, Action::ChannelOrb(OrbId::Lightning));

    let mut apply_state = crate::test_support::blank_test_combat();
    crate::engine::action_handlers::execute_action(actions[0].action.clone(), &mut apply_state);
    assert_eq!(
        crate::content::powers::store::power_amount(&apply_state, 0, PowerId::Electro),
        -1
    );
    crate::engine::action_handlers::execute_action(actions[0].action.clone(), &mut apply_state);
    assert_eq!(
        crate::content::powers::store::power_amount(&apply_state, 0, PowerId::Electro),
        -1,
        "AbstractPower.stackPower returns without changing the existing -1 sentinel"
    );

    apply_state.entities.player.orbs = vec![OrbEntity::new(OrbId::Lightning)];
    crate::content::orbs::hooks::trigger_end_of_turn_orbs_now(&mut apply_state);
    assert_eq!(
        apply_state.pop_next_action(),
        Some(Action::OrbDamageAllEnemies {
            source: 0,
            base_damage: 3,
        }),
        "ElectroPower is amount -1, so lightning must check power presence, not amount > 0"
    );
}

#[test]
fn rainbow_definition_runtime_and_upgrade_exhaust_match_java_sources() {
    let rainbow = get_card_definition(CardId::Rainbow);
    assert_eq!(rainbow.name, "Rainbow");
    assert_eq!(rainbow.card_type, CardType::Skill);
    assert_eq!(rainbow.rarity, CardRarity::Rare);
    assert_eq!(rainbow.cost, 2);
    assert_eq!(rainbow.target, CardTarget::SelfTarget);
    assert!(rainbow.exhaust);
    assert_eq!(java_id(CardId::Rainbow), "Rainbow");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Rainbow,
        &state,
        &CombatCard::for_scenario(CardId::Rainbow, 326),
        None,
    );
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0].action, Action::ChannelOrb(OrbId::Lightning));
    assert_eq!(actions[1].action, Action::ChannelOrb(OrbId::Frost));
    assert_eq!(actions[2].action, Action::ChannelOrb(OrbId::Dark));

    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Rainbow,
        327
    )));
    let mut rainbow_plus = CombatCard::for_scenario(CardId::Rainbow, 328);
    rainbow_plus.upgrades = 1;
    assert!(
        !exhausts_when_played(&rainbow_plus),
        "Java Rainbow+ changes exhaust only; channel order is unchanged"
    );
    let plus_actions = resolve_card_play(CardId::Rainbow, &state, &rainbow_plus, None);
    assert_eq!(
        plus_actions
            .iter()
            .map(|info| &info.action)
            .collect::<Vec<_>>(),
        vec![
            &Action::ChannelOrb(OrbId::Lightning),
            &Action::ChannelOrb(OrbId::Frost),
            &Action::ChannelOrb(OrbId::Dark),
        ]
    );
}

#[test]
fn impulse_definition_runtime_and_upgrade_exhaust_match_java_sources() {
    let impulse = get_card_definition(CardId::Impulse);
    assert_eq!(impulse.name, "Impulse");
    assert_eq!(impulse.card_type, CardType::Skill);
    assert_eq!(impulse.rarity, CardRarity::Uncommon);
    assert_eq!(impulse.cost, 1);
    assert_eq!(impulse.target, CardTarget::SelfTarget);
    assert!(impulse.exhaust);
    assert_eq!(java_id(CardId::Impulse), "Impulse");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Impulse,
        &state,
        &CombatCard::for_scenario(CardId::Impulse, 329),
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(actions[0].action, Action::TriggerImpulseOrbs);

    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Impulse,
        330
    )));
    let mut plus = CombatCard::for_scenario(CardId::Impulse, 331);
    plus.upgrades = 1;
    assert!(
        !exhausts_when_played(&plus),
        "Java Impulse+ changes exhaust only; the orb trigger action is unchanged"
    );
    let plus_actions = resolve_card_play(CardId::Impulse, &state, &plus, None);
    assert_eq!(plus_actions[0].action, Action::TriggerImpulseOrbs);

    let mut orb_state = crate::test_support::blank_test_combat();
    orb_state.entities.player.orbs =
        vec![OrbEntity::new(OrbId::Frost), OrbEntity::new(OrbId::Dark)];
    crate::engine::action_handlers::execute_action(Action::TriggerImpulseOrbs, &mut orb_state);
    assert_eq!(
        orb_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 2,
        }),
        "Java ImpulseAction triggers each orb's start/end callbacks"
    );
    assert_eq!(orb_state.entities.player.orbs[1].evoke_amount, 12);
}

#[test]
fn machine_learning_definition_runtime_and_draw_power_match_java_sources() {
    let machine = get_card_definition(CardId::MachineLearning);
    assert_eq!(machine.name, "Machine Learning");
    assert_eq!(machine.card_type, CardType::Power);
    assert_eq!(machine.rarity, CardRarity::Rare);
    assert_eq!(machine.cost, 1);
    assert_eq!(machine.base_magic, 1);
    assert_eq!(machine.target, CardTarget::SelfTarget);
    assert!(!machine.innate);
    assert_eq!(machine.upgrade_magic, 0);
    assert_eq!(java_id(CardId::MachineLearning), "Machine Learning");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::MachineLearning,
        &state,
        &CombatCard::for_scenario(CardId::MachineLearning, 332),
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Draw,
            amount: 1,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::MachineLearning, 333);
    plus.upgrades = 1;
    assert!(
        is_innate_card(&plus),
        "Java Machine Learning+ changes innate only; DrawPower amount remains 1"
    );
    assert_eq!(
        resolve_card_play(CardId::MachineLearning, &state, &plus, None)[0].action,
        actions[0].action
    );

    let mut draw_state = crate::test_support::blank_test_combat();
    assert_eq!(draw_state.turn.turn_start_draw_modifier, 0);
    crate::engine::action_handlers::execute_action(actions[0].action.clone(), &mut draw_state);
    assert_eq!(
        crate::content::powers::store::power_amount(&draw_state, 0, PowerId::Draw),
        1
    );
    assert_eq!(
        draw_state.turn.turn_start_draw_modifier, 1,
        "Java DrawPower mutates player.gameHandSize immediately"
    );
    crate::engine::action_handlers::execute_action(actions[0].action.clone(), &mut draw_state);
    assert_eq!(
        crate::content::powers::store::power_amount(&draw_state, 0, PowerId::Draw),
        2
    );
    assert_eq!(
        draw_state.turn.turn_start_draw_modifier, 2,
        "Stacking DrawPower should add to the derived turn-start draw modifier"
    );
}

#[test]
fn static_discharge_definition_runtime_and_on_attacked_hook_match_java_sources() {
    let static_discharge = get_card_definition(CardId::StaticDischarge);
    assert_eq!(static_discharge.name, "Static Discharge");
    assert_eq!(static_discharge.card_type, CardType::Power);
    assert_eq!(static_discharge.rarity, CardRarity::Uncommon);
    assert_eq!(static_discharge.cost, 1);
    assert_eq!(static_discharge.base_magic, 1);
    assert_eq!(static_discharge.target, CardTarget::SelfTarget);
    assert_eq!(static_discharge.upgrade_magic, 1);
    assert_eq!(java_id(CardId::StaticDischarge), "Static Discharge");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::StaticDischarge,
        &state,
        &CombatCard::for_scenario(CardId::StaticDischarge, 334),
        None,
    );
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::StaticDischarge,
            amount: 1,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::StaticDischarge, 335);
    plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::StaticDischarge, &state, &plus, None)[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::StaticDischarge,
            amount: 2,
        }
    );

    let mut hit_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut hit_state,
        0,
        vec![Power {
            power_type: PowerId::StaticDischarge,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 336,
            target: 0,
            base: 5,
            output: 5,
            damage_type: DamageType::Normal,
            is_modified: true,
        }),
        &mut hit_state,
    );
    assert_eq!(hit_state.entities.player.current_hp, 75);
    assert_eq!(
        hit_state.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
    assert_eq!(
        hit_state.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
    assert_eq!(hit_state.pop_next_action(), None);

    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: 336,
            target: 0,
            base: 5,
            output: 5,
            damage_type: DamageType::Thorns,
            is_modified: false,
        }),
        &mut hit_state,
    );
    assert_eq!(
        hit_state.pop_next_action(),
        None,
        "Java StaticDischargePower ignores THORNS damage"
    );

    crate::engine::action_handlers::execute_action(
        Action::Damage(DamageInfo {
            source: NO_SOURCE,
            target: 0,
            base: 5,
            output: 5,
            damage_type: DamageType::Normal,
            is_modified: false,
        }),
        &mut hit_state,
    );
    assert_eq!(
        hit_state.pop_next_action(),
        None,
        "Java StaticDischargePower requires info.owner != null"
    );
}

#[test]
fn heatsinks_and_storm_power_use_card_hooks_match_java_queue_order() {
    let heatsinks = get_card_definition(CardId::Heatsinks);
    assert_eq!(heatsinks.name, "Heatsinks");
    assert_eq!(heatsinks.card_type, CardType::Power);
    assert_eq!(heatsinks.rarity, CardRarity::Uncommon);
    assert_eq!(heatsinks.cost, 1);
    assert_eq!(heatsinks.base_magic, 1);
    assert_eq!(heatsinks.target, CardTarget::SelfTarget);
    assert_eq!(heatsinks.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Heatsinks), "Heatsinks");

    let storm = get_card_definition(CardId::Storm);
    assert_eq!(storm.name, "Storm");
    assert_eq!(storm.card_type, CardType::Power);
    assert_eq!(storm.rarity, CardRarity::Uncommon);
    assert_eq!(storm.cost, 1);
    assert_eq!(storm.base_magic, 1);
    assert_eq!(storm.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::Storm), "Storm");

    let state = crate::test_support::blank_test_combat();
    assert_eq!(
        resolve_card_play(
            CardId::Heatsinks,
            &state,
            &CombatCard::for_scenario(CardId::Heatsinks, 337),
            None,
        )[0]
        .action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Heatsink,
            amount: 1,
        }
    );
    let mut heatsinks_plus = CombatCard::for_scenario(CardId::Heatsinks, 338);
    heatsinks_plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::Heatsinks, &state, &heatsinks_plus, None)[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Heatsink,
            amount: 2,
        }
    );

    assert_eq!(
        resolve_card_play(
            CardId::Storm,
            &state,
            &CombatCard::for_scenario(CardId::Storm, 339),
            None,
        )[0]
        .action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Storm,
            amount: 1,
        }
    );
    let mut storm_plus = CombatCard::for_scenario(CardId::Storm, 340);
    storm_plus.upgrades = 1;
    assert!(
        is_innate_card(&storm_plus),
        "Java Storm+ changes innate only; StormPower amount remains 1"
    );

    let mut heatsink_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut heatsink_state,
        0,
        vec![Power {
            power_type: PowerId::Heatsink,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::PlayCardDirect {
            card: Box::new(CombatCard::for_scenario(CardId::Defragment, 341)),
            target: None,
            purge: false,
        },
        &mut heatsink_state,
    );
    assert_eq!(
        heatsink_state.pop_next_action(),
        Some(Action::DrawCards(2)),
        "Java HeatsinkPower.addToTop draws before the played power's queued ApplyPowerAction"
    );
    assert_eq!(
        heatsink_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 1,
        })
    );

    let mut storm_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut storm_state,
        0,
        vec![Power {
            power_type: PowerId::Storm,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::PlayCardDirect {
            card: Box::new(CombatCard::for_scenario(CardId::Defragment, 342)),
            target: None,
            purge: false,
        },
        &mut storm_state,
    );
    assert_eq!(
        storm_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 1,
        })
    );
    assert_eq!(
        storm_state.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
    assert_eq!(
        storm_state.pop_next_action(),
        Some(Action::ChannelOrb(OrbId::Lightning))
    );
}

#[test]
fn amplify_definition_runtime_and_power_copy_hook_match_java_sources() {
    let amplify = get_card_definition(CardId::Amplify);
    assert_eq!(amplify.name, "Amplify");
    assert_eq!(amplify.card_type, CardType::Skill);
    assert_eq!(amplify.rarity, CardRarity::Rare);
    assert_eq!(amplify.cost, 1);
    assert_eq!(amplify.base_magic, 1);
    assert_eq!(amplify.target, CardTarget::SelfTarget);
    assert_eq!(amplify.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Amplify), "Amplify");

    let mut state = crate::test_support::blank_test_combat();
    assert_eq!(
        resolve_card_play(
            CardId::Amplify,
            &state,
            &CombatCard::for_scenario(CardId::Amplify, 343),
            None,
        )[0]
        .action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Amplify,
            amount: 1,
        }
    );
    let mut plus = CombatCard::for_scenario(CardId::Amplify, 344);
    plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::Amplify, &state, &plus, None)[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Amplify,
            amount: 2,
        }
    );

    let mut copy_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut copy_state,
        0,
        vec![Power {
            power_type: PowerId::Amplify,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::PlayCardDirect {
            card: Box::new(CombatCard::for_scenario(CardId::Defragment, 345)),
            target: None,
            purge: false,
        },
        &mut copy_state,
    );
    assert_eq!(
        copy_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Focus,
            amount: 1,
        })
    );
    match copy_state.pop_next_action() {
        Some(Action::EnqueueCardPlay { item, in_front }) => {
            assert!(in_front);
            assert_eq!(item.card.id, CardId::Defragment);
            assert_eq!(
                item.card.lineage,
                crate::runtime::combat::CardLineageId::constructed_scenario(345),
                "Java AmplifyPower uses makeSameInstanceOf(), preserving UUID"
            );
            assert!(item.purge_on_use);
            assert_eq!(
                item.source,
                crate::runtime::combat::QueuedCardSource::Amplify
            );
        }
        other => panic!("Amplify should enqueue same-instance power copy, got {other:?}"),
    }
    assert_eq!(
        crate::content::powers::store::power_amount(&copy_state, 0, PowerId::Amplify),
        0,
        "AmplifyPower consumes one stack when it queues a copied power"
    );

    let turn_end_power = Power {
        power_type: PowerId::Amplify,
        instance_id: None,
        amount: 2,
        extra_data: 0,
        payload: crate::runtime::combat::PowerPayload::None,
        just_applied: false,
    };
    assert_eq!(
        crate::content::powers::resolve_power_at_end_of_turn(&turn_end_power, &mut state, 0)
            .as_slice(),
        &[Action::RemovePower {
            target: 0,
            power_id: PowerId::Amplify,
        }],
        "Java AmplifyPower removes itself at player end of turn"
    );
}

#[test]
fn echo_form_definition_runtime_and_power_copy_hook_match_java_sources() {
    let echo = get_card_definition(CardId::EchoForm);
    assert_eq!(echo.name, "Echo Form");
    assert_eq!(echo.card_type, CardType::Power);
    assert_eq!(echo.rarity, CardRarity::Rare);
    assert_eq!(echo.cost, 3);
    assert_eq!(echo.target, CardTarget::SelfTarget);
    assert!(echo.ethereal);
    assert_eq!(java_id(CardId::EchoForm), "Echo Form");

    let state = crate::test_support::blank_test_combat();
    assert!(crate::content::cards::is_ethereal(
        &CombatCard::for_scenario(CardId::EchoForm, 346)
    ));
    let mut upgraded = CombatCard::for_scenario(CardId::EchoForm, 347);
    upgraded.upgrades = 1;
    assert!(
        !crate::content::cards::is_ethereal(&upgraded),
        "Java EchoForm.upgrade() sets isEthereal=false without changing card stats"
    );
    assert_eq!(
        resolve_card_play(
            CardId::EchoForm,
            &state,
            &CombatCard::for_scenario(CardId::EchoForm, 348),
            None,
        )[0]
        .action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::EchoForm,
            amount: 1,
        }
    );

    let mut copy_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut copy_state,
        0,
        vec![Power {
            power_type: PowerId::EchoForm,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::execute_action(
        Action::PlayCardDirect {
            card: Box::new(CombatCard::for_scenario(CardId::DefendB, 349)),
            target: None,
            purge: false,
        },
        &mut copy_state,
    );
    assert_eq!(
        copy_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 5,
        })
    );
    match copy_state.pop_next_action() {
        Some(Action::EnqueueCardPlay { item, in_front }) => {
            assert!(in_front);
            assert_eq!(item.card.id, CardId::DefendB);
            assert_eq!(
                item.card.lineage,
                crate::runtime::combat::CardLineageId::constructed_scenario(349),
                "Java EchoPower uses makeSameInstanceOf(), preserving UUID"
            );
            assert!(item.purge_on_use);
            assert_eq!(
                item.source,
                crate::runtime::combat::QueuedCardSource::EchoForm
            );
        }
        other => panic!("Echo Form should enqueue same-instance copied card, got {other:?}"),
    }
    assert_eq!(
        crate::content::powers::store::powers_for(&copy_state, 0)
            .unwrap()
            .iter()
            .find(|p| p.power_type == PowerId::EchoForm)
            .unwrap()
            .extra_data,
        1,
        "Rust stores Java EchoPower.cardsDoubledThisTurn in Power.extra_data"
    );

    let mut limited_state = crate::test_support::blank_test_combat();
    limited_state.turn.counters.cards_played_this_turn = 2;
    crate::content::powers::store::set_powers_for(
        &mut limited_state,
        0,
        vec![Power {
            power_type: PowerId::EchoForm,
            instance_id: None,
            amount: 1,
            extra_data: 1,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::content::powers::defect::echo_form::on_use_card(
        &mut limited_state,
        &CombatCard::for_scenario(CardId::DefendB, 350),
        false,
        None,
    );
    assert_eq!(
        limited_state.pop_next_action(),
        None,
        "after one original and its purge copy, Echo Form amount=1 must not copy the next original"
    );

    let mut purge_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut purge_state,
        0,
        vec![Power {
            power_type: PowerId::EchoForm,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::content::powers::defect::echo_form::on_use_card(
        &mut purge_state,
        &CombatCard::for_scenario(CardId::DefendB, 351),
        true,
        None,
    );
    assert_eq!(
        purge_state.pop_next_action(),
        None,
        "Java EchoPower ignores purgeOnUse copied cards"
    );

    let mut reset_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut reset_state,
        0,
        vec![Power {
            power_type: PowerId::EchoForm,
            instance_id: None,
            amount: 2,
            extra_data: 2,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let power_snapshot =
        crate::content::powers::store::powers_snapshot_for(&reset_state, 0)[0].clone();
    let actions = crate::content::powers::resolve_power_instance_at_turn_start(
        &power_snapshot,
        &mut reset_state,
        0,
    );
    assert!(actions.is_empty());
    assert_eq!(
        crate::content::powers::store::powers_for(&reset_state, 0)
            .unwrap()
            .iter()
            .find(|p| p.power_type == PowerId::EchoForm)
            .unwrap()
            .extra_data,
        0,
        "Java EchoPower.atStartOfTurn resets cardsDoubledThisTurn"
    );
}

#[test]
fn force_field_definition_runtime_and_cost_hooks_match_java_sources() {
    let force = get_card_definition(CardId::ForceField);
    assert_eq!(force.name, "Force Field");
    assert_eq!(force.card_type, CardType::Skill);
    assert_eq!(force.rarity, CardRarity::Uncommon);
    assert_eq!(force.cost, 4);
    assert_eq!(force.base_block, 12);
    assert_eq!(force.target, CardTarget::SelfTarget);
    assert_eq!(force.upgrade_block, 4);
    assert_eq!(java_id(CardId::ForceField), "Force Field");

    let state = crate::test_support::blank_test_combat();
    assert_eq!(
        resolve_card_play(
            CardId::ForceField,
            &state,
            &CombatCard::for_scenario(CardId::ForceField, 352),
            None,
        )[0]
        .action,
        Action::GainBlock {
            target: 0,
            amount: 12,
        }
    );
    let mut plus = CombatCard::for_scenario(CardId::ForceField, 353);
    plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::ForceField, &state, &plus, None)[0].action,
        Action::GainBlock {
            target: 0,
            amount: 16,
        }
    );

    let mut generated_state = crate::test_support::blank_test_combat();
    generated_state.turn.counters.card_ids_played_this_combat =
        vec![CardId::Defragment, CardId::StrikeB, CardId::EchoForm];
    let generated = crate::content::cards::make_fresh_card_copy_for_combat(
        CardId::ForceField,
        crate::runtime::combat::CardLineageId::constructed_scenario(354),
        &generated_state,
    );
    assert_eq!(
        generated.combat_cost_without_turn_override_java(),
        2,
        "Java ForceField.configureCostsOnNewCard calls updateCost(-1) for each Power played this combat"
    );
    assert_eq!(generated.cost_for_turn_java(), 2);

    let mut hook_state = crate::test_support::blank_test_combat();
    hook_state.zones.hand = vec![CombatCard::for_scenario(CardId::ForceField, 355)];
    hook_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::ForceField, 356)].into();
    hook_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::ForceField, 357)]).into();
    crate::engine::action_handlers::execute_action(
        Action::PlayCardDirect {
            card: Box::new(CombatCard::for_scenario(CardId::Defragment, 358)),
            target: None,
            purge: false,
        },
        &mut hook_state,
    );
    assert_eq!(
        hook_state.zones.hand[0].combat_cost_without_turn_override_java(),
        3
    );
    assert_eq!(
        hook_state.zones.discard_pile[0].combat_cost_without_turn_override_java(),
        3
    );
    assert_eq!(
        hook_state.zones.draw_pile[0].combat_cost_without_turn_override_java(),
        3
    );
    assert_eq!(
        hook_state.turn.counters.card_ids_played_this_combat,
        vec![CardId::Defragment],
        "Rust combat-wide played-card history mirrors Java cardsPlayedThisCombat for Force Field"
    );

    crate::engine::action_handlers::execute_action(
        Action::PlayCardDirect {
            card: Box::new(CombatCard::for_scenario(CardId::StrikeB, 359)),
            target: Some(7),
            purge: false,
        },
        &mut hook_state,
    );
    assert_eq!(
        hook_state.zones.hand[0].combat_cost_without_turn_override_java(),
        3,
        "ForceField.triggerOnCardPlayed only reacts to Power cards"
    );
}

#[test]
fn reboot_definition_runtime_and_shuffle_actions_match_java_sources() {
    let reboot = get_card_definition(CardId::Reboot);
    assert_eq!(reboot.name, "Reboot");
    assert_eq!(reboot.card_type, CardType::Skill);
    assert_eq!(reboot.rarity, CardRarity::Rare);
    assert_eq!(reboot.cost, 0);
    assert_eq!(reboot.base_magic, 4);
    assert!(reboot.exhaust);
    assert_eq!(reboot.target, CardTarget::SelfTarget);
    assert_eq!(reboot.upgrade_magic, 2);
    assert_eq!(java_id(CardId::Reboot), "Reboot");
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Reboot,
        360
    )));

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Reboot,
        &state,
        &CombatCard::for_scenario(CardId::Reboot, 361),
        None,
    );
    assert_eq!(actions.len(), 3);
    assert_eq!(actions[0].action, Action::ShuffleAllIntoDraw);
    assert_eq!(
        actions[1].action,
        Action::ShuffleDrawPile {
            trigger_relics: false
        }
    );
    assert_eq!(actions[2].action, Action::DrawCards(4));

    let mut plus = CombatCard::for_scenario(CardId::Reboot, 362);
    plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::Reboot, &state, &plus, None)[2].action,
        Action::DrawCards(6)
    );

    let mut shuffle_all_state = crate::test_support::blank_test_combat();
    shuffle_all_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeB, 363),
        CombatCard::for_scenario(CardId::DefendB, 364),
    ];
    shuffle_all_state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Zap, 365)]).into();
    shuffle_all_state.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Dualcast, 366),
        CombatCard::for_scenario(CardId::BallLightning, 367),
    ]
    .into();
    let mut expected_rng = shuffle_all_state.rng.clone();
    let mut expected_discard = shuffle_all_state.zones.discard_pile.clone().into_vec();
    crate::runtime::rng::shuffle_with_random_long(
        &mut expected_discard,
        &mut expected_rng.shuffle_rng,
    );
    expected_discard.reverse();
    let mut expected_draw = expected_discard;
    expected_draw.extend(shuffle_all_state.zones.draw_pile.iter().cloned());

    crate::engine::action_handlers::execute_action(
        Action::ShuffleAllIntoDraw,
        &mut shuffle_all_state,
    );
    assert_eq!(
        shuffle_all_state.pop_next_action(),
        Some(Action::PutOnDeck {
            amount: 99,
            random: true,
        }),
        "Java ShuffleAllAction queues PutOnDeckAction(99, true) to top"
    );
    assert!(shuffle_all_state.zones.discard_pile.is_empty());
    assert_eq!(
        shuffle_all_state
            .zones
            .draw_pile
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        expected_draw.iter().map(|card| card.lineage).collect::<Vec<_>>(),
        "ShuffleAllAction shuffles discard with shuffleRng, then adds those cards to Java draw-pile top"
    );
    assert_eq!(
        shuffle_all_state.rng.shuffle_rng.counter,
        expected_rng.shuffle_rng.counter
    );

    let mut shuffle_draw_state = crate::test_support::blank_test_combat();
    shuffle_draw_state.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::Zap, 368),
        CombatCard::for_scenario(CardId::Dualcast, 369),
        CombatCard::for_scenario(CardId::StrikeB, 370),
    ])
    .into();
    let mut expected_rng = shuffle_draw_state.rng.clone();
    let mut expected_java_draw = shuffle_draw_state.zones.draw_pile.clone();
    expected_java_draw.reverse();
    crate::runtime::rng::shuffle_with_random_long(
        expected_java_draw.as_mut_slice(),
        &mut expected_rng.shuffle_rng,
    );
    expected_java_draw.reverse();

    crate::engine::action_handlers::execute_action(
        Action::ShuffleDrawPile {
            trigger_relics: false,
        },
        &mut shuffle_draw_state,
    );
    assert_eq!(
        shuffle_draw_state
            .zones
            .draw_pile
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        expected_java_draw
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        "Java ShuffleAction shuffles drawPile.group, whose top maps to Rust index 0 after reversing"
    );
    assert_eq!(
        shuffle_draw_state.rng.shuffle_rng.counter,
        expected_rng.shuffle_rng.counter
    );
}

#[test]
fn scrape_definition_runtime_and_follow_up_match_java_sources() {
    let scrape = get_card_definition(CardId::Scrape);
    assert_eq!(scrape.name, "Scrape");
    assert_eq!(scrape.card_type, CardType::Attack);
    assert_eq!(scrape.rarity, CardRarity::Uncommon);
    assert_eq!(scrape.cost, 1);
    assert_eq!(scrape.base_damage, 7);
    assert_eq!(scrape.base_magic, 4);
    assert_eq!(scrape.target, CardTarget::Enemy);
    assert_eq!(scrape.upgrade_damage, 3);
    assert_eq!(scrape.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Scrape), "Scrape");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::Scrape,
        &state,
        &CombatCard::for_scenario(CardId::Scrape, 371),
        Some(77),
    );
    assert_eq!(actions.len(), 3);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 77);
            assert_eq!(info.base, 7);
            assert_eq!(info.output, 7);
        }
        other => panic!("Scrape should damage before drawing, got {other:?}"),
    }
    assert_eq!(
        actions[1].action,
        Action::DrawCardsWithHistory {
            amount: 4,
            clear_history: true,
        }
    );
    assert_eq!(actions[2].action, Action::ScrapeFollowUp);

    let mut plus = CombatCard::for_scenario(CardId::Scrape, 372);
    plus.upgrades = 1;
    let plus_actions = resolve_card_play(CardId::Scrape, &state, &plus, Some(78));
    match &plus_actions[0].action {
        Action::Damage(info) => assert_eq!(info.base, 10),
        other => panic!("Scrape+ should still damage first, got {other:?}"),
    }
    assert_eq!(
        plus_actions[1].action,
        Action::DrawCardsWithHistory {
            amount: 5,
            clear_history: true,
        }
    );

    let mut follow_up = crate::test_support::blank_test_combat();
    let discard_me = CombatCard::for_scenario(CardId::DefendB, 373);
    let same_id_not_drawn = CombatCard::for_scenario(CardId::DefendB, 374);
    let zero_cost = CombatCard::for_scenario(CardId::Turbo, 375);
    let mut free_nonzero = CombatCard::for_scenario(CardId::StrikeB, 376);
    free_nonzero.free_to_play_once = true;
    follow_up.zones.hand = vec![
        discard_me.clone(),
        same_id_not_drawn.clone(),
        zero_cost.clone(),
        free_nonzero.clone(),
    ];
    follow_up.runtime.last_drawn_cards = vec![
        DrawnCardRecord {
            card_lineage: discard_me.lineage,
            card_id: discard_me.id,
        },
        DrawnCardRecord {
            card_lineage: zero_cost.lineage,
            card_id: zero_cost.id,
        },
        DrawnCardRecord {
            card_lineage: free_nonzero.lineage,
            card_id: free_nonzero.id,
        },
        DrawnCardRecord {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(9999),
            card_id: CardId::StrikeB,
        },
    ];
    crate::engine::action_handlers::execute_action(Action::ScrapeFollowUp, &mut follow_up);
    assert_eq!(
        follow_up
            .zones
            .hand
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        vec![
            same_id_not_drawn.lineage,
            zero_cost.lineage,
            free_nonzero.lineage
        ],
        "Java ScrapeFollowUpAction iterates DrawCardAction.drawnCards instances, not CardId names"
    );
    assert_eq!(
        follow_up
            .zones
            .discard_pile
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        vec![discard_me.lineage]
    );
    assert_eq!(follow_up.turn.counters.cards_discarded_this_turn, 1);

    let mut draw_history = crate::test_support::blank_test_combat();
    draw_history.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::DefendB, 377),
        CombatCard::for_scenario(CardId::Turbo, 378),
    ])
    .into();
    draw_history.runtime.last_drawn_cards = vec![DrawnCardRecord {
        card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(9998),
        card_id: CardId::StrikeB,
    }];
    crate::engine::action_handlers::execute_action(
        Action::DrawCardsWithHistory {
            amount: 2,
            clear_history: true,
        },
        &mut draw_history,
    );
    assert_eq!(
        draw_history.runtime.last_drawn_cards,
        vec![
            DrawnCardRecord {
                card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(377),
                card_id: CardId::DefendB,
            },
            DrawnCardRecord {
                card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(378),
                card_id: CardId::Turbo,
            },
        ],
        "DrawCardAction.drawnCards must preserve concrete drawn card instances for Scrape"
    );
}

#[test]
fn seek_definition_runtime_and_draw_pile_selection_match_java_sources() {
    let seek = get_card_definition(CardId::Seek);
    assert_eq!(seek.name, "Seek");
    assert_eq!(seek.card_type, CardType::Skill);
    assert_eq!(seek.rarity, CardRarity::Rare);
    assert_eq!(seek.cost, 0);
    assert_eq!(seek.base_magic, 1);
    assert_eq!(seek.target, CardTarget::None);
    assert!(seek.exhaust);
    assert_eq!(seek.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Seek), "Seek");
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Seek,
        379
    )));

    let mut empty = crate::test_support::blank_test_combat();
    let empty_actions = resolve_card_play(
        CardId::Seek,
        &empty,
        &CombatCard::for_scenario(CardId::Seek, 380),
        None,
    );
    empty.queue_actions(empty_actions);
    let mut engine = crate::state::EngineState::CombatProcessing;
    assert!(crate::engine::core::tick_engine(
        &mut engine,
        &mut empty,
        None
    ));
    assert!(empty.pop_next_action().is_none());
    assert!(empty
        .take_engine_diagnostics()
        .iter()
        .all(|d| d.severity != crate::state::selection::EngineDiagnosticSeverity::Error));

    let mut choose_state = crate::test_support::blank_test_combat();
    choose_state.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::StrikeB, 381),
        CombatCard::for_scenario(CardId::DefendB, 382),
    ])
    .into();
    let choose_actions = resolve_card_play(
        CardId::Seek,
        &choose_state,
        &CombatCard::for_scenario(CardId::Seek, 383),
        None,
    );
    assert_eq!(
        choose_actions[0].action,
        Action::SuspendForGridSelect {
            source_pile: crate::state::PileType::Draw,
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::GridSelectFilter::Any,
            reason: crate::state::GridSelectReason::DrawPileToHand,
        }
    );

    let mut auto_state = crate::test_support::blank_test_combat();
    auto_state.zones.draw_pile = (vec![
        CombatCard::for_scenario(CardId::StrikeB, 384),
        CombatCard::for_scenario(CardId::DefendB, 385),
    ])
    .into();
    let mut plus = CombatCard::for_scenario(CardId::Seek, 386);
    plus.upgrades = 1;
    let auto_actions = resolve_card_play(CardId::Seek, &auto_state, &plus, None);
    auto_state.queue_actions(auto_actions);
    let mut engine = crate::state::EngineState::CombatProcessing;
    assert!(crate::engine::core::tick_engine(
        &mut engine,
        &mut auto_state,
        None
    ));
    assert!(auto_state.zones.draw_pile.is_empty());
    assert_eq!(
        auto_state.zones.hand.iter().map(|card| card.lineage).collect::<Vec<_>>(),
        [385, 384].map(crate::test_support::scenario_lineage),
        "Java BetterDrawPileToHandAction auto-moves the whole drawPile.group in bottom-to-top order"
    );
}

#[test]
fn recycle_definition_runtime_and_energy_gain_match_java_sources() {
    let recycle = get_card_definition(CardId::Recycle);
    assert_eq!(recycle.name, "Recycle");
    assert_eq!(recycle.card_type, CardType::Skill);
    assert_eq!(recycle.rarity, CardRarity::Uncommon);
    assert_eq!(recycle.cost, 1);
    assert_eq!(recycle.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::Recycle), "Recycle");

    let state = crate::test_support::blank_test_combat();
    assert_eq!(
        resolve_card_play(
            CardId::Recycle,
            &state,
            &CombatCard::for_scenario(CardId::Recycle, 387),
            None,
        )[0]
        .action,
        Action::Recycle
    );
    let mut plus = CombatCard::for_scenario(CardId::Recycle, 388);
    plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&plus), Some(0));
    assert_eq!(plus.get_cost(), 0);

    let mut empty = crate::test_support::blank_test_combat();
    crate::engine::action_handlers::execute_action(Action::Recycle, &mut empty);
    assert_eq!(empty.pop_next_action(), None);

    let mut one_card = crate::test_support::blank_test_combat();
    one_card.zones.hand = vec![CombatCard::for_scenario(CardId::Bash, 389)];
    crate::engine::action_handlers::execute_action(Action::Recycle, &mut one_card);
    assert!(one_card.zones.hand.is_empty());
    assert_eq!(
        one_card.zones.exhaust_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(389)
    );
    assert_eq!(
        one_card.pop_next_action(),
        Some(Action::GainEnergy { amount: 2 }),
        "Java RecycleAction gains positive costForTurn before exhausting the selected card"
    );

    let mut x_cost = crate::test_support::blank_test_combat();
    x_cost.turn.energy = 3;
    x_cost.zones.hand = vec![CombatCard::for_scenario(CardId::Whirlwind, 390)];
    crate::engine::action_handlers::execute_action(Action::Recycle, &mut x_cost);
    assert_eq!(
        x_cost.zones.exhaust_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(390)
    );
    assert_eq!(
        x_cost.pop_next_action(),
        Some(Action::GainEnergy { amount: 3 }),
        "Java RecycleAction gives current EnergyPanel energy for X-cost cards"
    );

    let mut multi = crate::test_support::blank_test_combat();
    multi.zones.hand = vec![
        CombatCard::for_scenario(CardId::StrikeB, 391),
        CombatCard::for_scenario(CardId::DefendB, 392),
    ];
    crate::engine::action_handlers::execute_action(Action::Recycle, &mut multi);
    assert_eq!(
        multi.pop_next_action(),
        Some(Action::SuspendForHandSelect {
            min: 1,
            max: 1,
            can_cancel: false,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::Recycle,
        })
    );
}

#[test]
fn defect_random_generation_cards_and_powers_match_java_sources() {
    let creative = get_card_definition(CardId::CreativeAI);
    assert_eq!(creative.name, "Creative AI");
    assert_eq!(creative.card_type, CardType::Power);
    assert_eq!(creative.rarity, CardRarity::Rare);
    assert_eq!(creative.cost, 3);
    assert_eq!(creative.base_magic, 1);
    assert_eq!(creative.target, CardTarget::SelfTarget);
    assert_eq!(java_id(CardId::CreativeAI), "Creative AI");
    let mut creative_plus = CombatCard::for_scenario(CardId::CreativeAI, 393);
    creative_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&creative_plus), Some(2));
    assert_eq!(
        resolve_card_play(
            CardId::CreativeAI,
            &crate::test_support::blank_test_combat(),
            &creative_plus,
            None
        )[0]
        .action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::CreativeAI,
            amount: 1,
        }
    );

    let hello = get_card_definition(CardId::HelloWorld);
    assert_eq!(hello.name, "Hello World");
    assert_eq!(hello.card_type, CardType::Power);
    assert_eq!(hello.rarity, CardRarity::Uncommon);
    assert_eq!(hello.cost, 1);
    assert_eq!(java_id(CardId::HelloWorld), "Hello World");
    let mut hello_plus = CombatCard::for_scenario(CardId::HelloWorld, 394);
    hello_plus.upgrades = 1;
    assert!(is_innate_card(&hello_plus));
    assert_eq!(
        resolve_card_play(
            CardId::HelloWorld,
            &crate::test_support::blank_test_combat(),
            &hello_plus,
            None
        )[0]
        .action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Hello,
            amount: 1,
        }
    );

    let white = get_card_definition(CardId::WhiteNoise);
    assert_eq!(white.name, "White Noise");
    assert_eq!(white.card_type, CardType::Skill);
    assert_eq!(white.rarity, CardRarity::Uncommon);
    assert_eq!(white.cost, 1);
    assert!(white.exhaust);
    assert_eq!(white.target, CardTarget::None);
    assert_eq!(java_id(CardId::WhiteNoise), "White Noise");
    let mut white_plus = CombatCard::for_scenario(CardId::WhiteNoise, 395);
    white_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&white_plus), Some(0));
    assert_eq!(
        resolve_card_play(
            CardId::WhiteNoise,
            &crate::test_support::blank_test_combat(),
            &white_plus,
            None
        )[0]
        .action,
        Action::MakeRandomCardInHand {
            card_type: Some(CardType::Power),
            cost_for_turn: Some(0),
        }
    );
}

#[test]
fn defect_random_card_pools_are_complete_and_exclude_unregistered_impulse() {
    assert_eq!(DEFECT_COMMON_POOL.len(), 18);
    assert_eq!(DEFECT_UNCOMMON_POOL.len(), 36);
    assert_eq!(DEFECT_RARE_POOL.len(), 17);
    assert_eq!(
        DEFECT_COMMON_POOL,
        &[
            CardId::GoForTheEyes,
            CardId::BallLightning,
            CardId::Streamline,
            CardId::Recursion,
            CardId::CompileDriver,
            CardId::Barrage,
            CardId::Stack,
            CardId::Rebound,
            CardId::Claw,
            CardId::Coolheaded,
            CardId::Turbo,
            CardId::SweepingBeam,
            CardId::ConserveBattery,
            CardId::Hologram,
            CardId::BeamCell,
            CardId::Leap,
            CardId::ColdSnap,
            CardId::SteamBarrier,
        ],
        "Defect common pool order must match Java CardLibrary HashMap traversal"
    );
    assert_eq!(
        DEFECT_UNCOMMON_POOL,
        &[
            CardId::Storm,
            CardId::GeneticAlgorithm,
            CardId::Overclock,
            CardId::HelloWorld,
            CardId::Sunder,
            CardId::Glacier,
            CardId::Consume,
            CardId::Fusion,
            CardId::Aggregate,
            CardId::Blizzard,
            CardId::Chaos,
            CardId::Melter,
            CardId::SelfRepair,
            CardId::Loop,
            CardId::Chill,
            CardId::BootSequence,
            CardId::StaticDischarge,
            CardId::Heatsinks,
            CardId::Tempest,
            CardId::Equilibrium,
            CardId::ForceField,
            CardId::Ftl,
            CardId::RipAndTear,
            CardId::Darkness,
            CardId::DoubleEnergy,
            CardId::ReinforcedBody,
            CardId::AutoShields,
            CardId::Reprogram,
            CardId::LockOn,
            CardId::Scrape,
            CardId::Recycle,
            CardId::Skim,
            CardId::WhiteNoise,
            CardId::Capacitor,
            CardId::Defragment,
            CardId::DoomAndGloom,
        ],
        "Defect uncommon pool order must match Java CardLibrary HashMap traversal"
    );
    assert_eq!(
        DEFECT_RARE_POOL,
        &[
            CardId::CoreSurge,
            CardId::Fission,
            CardId::CreativeAI,
            CardId::Amplify,
            CardId::Reboot,
            CardId::AllForOne,
            CardId::EchoForm,
            CardId::MeteorStrike,
            CardId::Seek,
            CardId::Rainbow,
            CardId::Buffer,
            CardId::Electrodynamics,
            CardId::MachineLearning,
            CardId::BiasedCognition,
            CardId::ThunderStrike,
            CardId::Hyperbeam,
            CardId::MultiCast,
        ],
        "Defect rare pool order must match Java CardLibrary HashMap traversal"
    );
    assert!(!DEFECT_COMMON_POOL.contains(&CardId::Impulse));
    assert!(!DEFECT_UNCOMMON_POOL.contains(&CardId::Impulse));
    assert!(!DEFECT_RARE_POOL.contains(&CardId::Impulse));
    assert!(DEFECT_RARE_POOL.contains(&CardId::CreativeAI));
    assert!(DEFECT_UNCOMMON_POOL.contains(&CardId::HelloWorld));
    assert!(DEFECT_UNCOMMON_POOL.contains(&CardId::WhiteNoise));

    for (rarity, pool) in &[
        (CardRarity::Common, DEFECT_COMMON_POOL),
        (CardRarity::Uncommon, DEFECT_UNCOMMON_POOL),
        (CardRarity::Rare, DEFECT_RARE_POOL),
    ] {
        for &id in *pool {
            let def = get_card_definition(id);
            assert_eq!(
                def.rarity, *rarity,
                "{id:?} is in the wrong Defect rarity pool"
            );
            assert_ne!(def.rarity, CardRarity::Basic);
        }
    }
}

#[test]
fn creative_ai_and_hello_world_powers_sample_defect_random_pools() {
    let mut creative_state = crate::test_support::blank_test_combat();
    creative_state.meta.player_class = "Defect".to_string();
    let creative_actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::CreativeAI,
        &mut creative_state,
        0,
        2,
    );
    assert_eq!(creative_actions.len(), 2);
    for action in creative_actions {
        match action {
            Action::MakeConstructedCopyInHand { original, amount } => {
                assert_eq!(amount, 1);
                assert_eq!(get_card_definition(original.id).card_type, CardType::Power);
                assert!(
                    DEFECT_RARE_POOL.contains(&original.id)
                        || DEFECT_UNCOMMON_POOL.contains(&original.id)
                        || DEFECT_COMMON_POOL.contains(&original.id)
                );
            }
            other => panic!("Creative AI should queue random Power copies, got {other:?}"),
        }
    }

    let mut hello_state = crate::test_support::blank_test_combat();
    hello_state.meta.player_class = "Defect".to_string();
    hello_state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    let hello_actions =
        crate::content::powers::resolve_power_at_turn_start(PowerId::Hello, &mut hello_state, 0, 1);
    assert_eq!(hello_actions.len(), 1);
    match &hello_actions[0] {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(*amount, 1);
            assert!(
                DEFECT_COMMON_POOL.contains(&original.id),
                "Java HelloPower samples AbstractDungeon.getCard(COMMON, cardRandomRng)"
            );
        }
        other => panic!("Hello World should queue a random common card, got {other:?}"),
    }

    hello_state.entities.monsters[0].is_dying = true;
    assert!(
        crate::content::powers::resolve_power_at_turn_start(
            PowerId::Hello,
            &mut hello_state,
            0,
            1,
        )
        .is_empty(),
        "Java HelloPower skips generation when monsters are basically dead"
    );
}

#[test]
fn fission_definition_runtime_and_orb_actions_match_java_sources() {
    let fission = get_card_definition(CardId::Fission);
    assert_eq!(fission.name, "Fission");
    assert_eq!(fission.card_type, CardType::Skill);
    assert_eq!(fission.rarity, CardRarity::Rare);
    assert_eq!(fission.cost, 0);
    assert_eq!(fission.base_magic, 1);
    assert_eq!(fission.target, CardTarget::None);
    assert!(fission.exhaust);
    assert_eq!(fission.upgrade_magic, 0);
    assert_eq!(java_id(CardId::Fission), "Fission");

    let state = crate::test_support::blank_test_combat();
    assert_eq!(
        resolve_card_play(
            CardId::Fission,
            &state,
            &CombatCard::for_scenario(CardId::Fission, 346),
            None,
        )[0]
        .action,
        Action::Fission { upgraded: false }
    );
    assert!(exhausts_when_played(&CombatCard::for_scenario(
        CardId::Fission,
        347
    )));

    let mut plus = CombatCard::for_scenario(CardId::Fission, 348);
    plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::Fission, &state, &plus, None)[0].action,
        Action::Fission { upgraded: true },
        "Java Fission+ changes FissionAction mode only; card still exhausts"
    );
    assert!(exhausts_when_played(&plus));

    let mut remove_state = crate::test_support::blank_test_combat();
    remove_state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Lightning),
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Empty),
    ];
    crate::engine::action_handlers::execute_action(
        Action::Fission { upgraded: false },
        &mut remove_state,
    );
    assert_eq!(remove_state.pop_next_action(), Some(Action::RemoveAllOrbs));
    crate::engine::action_handlers::execute_action(Action::RemoveAllOrbs, &mut remove_state);
    assert_eq!(
        remove_state.entities.player.orbs,
        vec![
            OrbEntity::new(OrbId::Empty),
            OrbEntity::new(OrbId::Empty),
            OrbEntity::new(OrbId::Empty),
        ],
        "Java RemoveAllOrbsAction removes filled orb slots without evoking"
    );
    assert_eq!(
        remove_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 2 })
    );
    assert_eq!(remove_state.pop_next_action(), Some(Action::DrawCards(2)));

    let mut evoke_state = crate::test_support::blank_test_combat();
    evoke_state.entities.player.orbs = vec![
        OrbEntity::new(OrbId::Frost),
        OrbEntity::new(OrbId::Lightning),
        OrbEntity::new(OrbId::Empty),
    ];
    crate::engine::action_handlers::execute_action(
        Action::Fission { upgraded: true },
        &mut evoke_state,
    );
    assert_eq!(evoke_state.pop_next_action(), Some(Action::EvokeAllOrbs));
    crate::engine::action_handlers::execute_action(Action::EvokeAllOrbs, &mut evoke_state);
    assert_eq!(
        std::iter::from_fn(|| evoke_state.pop_next_action()).collect::<Vec<_>>(),
        vec![
            Action::EvokeOrb,
            Action::EvokeOrb,
            Action::EvokeOrb,
            Action::GainEnergy { amount: 2 },
            Action::DrawCards(2),
        ],
        "Java EvokeAllOrbsAction queues one EvokeOrbAction per orb slot, not per filled orb"
    );
}

#[test]
fn multi_cast_definition_runtime_and_x_cost_orb_actions_match_java_sources() {
    let multi_cast = get_card_definition(CardId::MultiCast);
    assert_eq!(multi_cast.name, "Multi-Cast");
    assert_eq!(multi_cast.card_type, CardType::Skill);
    assert_eq!(multi_cast.rarity, CardRarity::Rare);
    assert_eq!(multi_cast.cost, -1);
    assert_eq!(multi_cast.base_magic, 0);
    assert_eq!(multi_cast.target, CardTarget::None);
    assert!(!multi_cast.exhaust);
    assert_eq!(multi_cast.upgrade_magic, 0);
    assert_eq!(java_id(CardId::MultiCast), "Multi-Cast");

    let state = crate::test_support::blank_test_combat();
    let mut card = CombatCard::for_scenario(CardId::MultiCast, 349);
    card.energy_on_use = 2;
    assert_eq!(
        resolve_card_play(CardId::MultiCast, &state, &card, None)[0].action,
        Action::MultiCast {
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 2,
        }
    );
    let mut plus = CombatCard::for_scenario(CardId::MultiCast, 350);
    plus.upgrades = 1;
    plus.free_to_play_once = true;
    plus.energy_on_use = 0;
    assert_eq!(
        resolve_card_play(CardId::MultiCast, &state, &plus, None)[0].action,
        Action::MultiCast {
            upgraded: true,
            free_to_play_once: true,
            energy_on_use: 0,
        }
    );

    let mut basic = crate::test_support::blank_test_combat();
    basic.turn.energy = 2;
    basic.entities.player.orbs = vec![OrbEntity::new(OrbId::Dark), OrbEntity::new(OrbId::Empty)];
    crate::engine::action_handlers::execute_action(
        Action::MultiCast {
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 2,
        },
        &mut basic,
    );
    assert_eq!(basic.turn.energy, 0);
    assert_eq!(
        std::iter::from_fn(|| basic.pop_next_action()).collect::<Vec<_>>(),
        vec![Action::EvokeOrbWithoutRemoving, Action::EvokeOrb],
        "Java MulticastAction evokes without removing effect-1 times, then evokes/removes once"
    );

    let mut upgraded_zero = crate::test_support::blank_test_combat();
    upgraded_zero.entities.player.orbs = vec![OrbEntity::new(OrbId::Lightning)];
    crate::engine::action_handlers::execute_action(
        Action::MultiCast {
            upgraded: true,
            free_to_play_once: false,
            energy_on_use: 0,
        },
        &mut upgraded_zero,
    );
    assert_eq!(
        upgraded_zero.pop_next_action(),
        Some(Action::EvokeOrb),
        "Java Multi-Cast+ adds one evoke after reading zero X energy"
    );
    assert_eq!(upgraded_zero.pop_next_action(), None);

    let mut chemical_x = crate::test_support::blank_test_combat();
    chemical_x
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ChemicalX,
        ));
    chemical_x.entities.player.orbs = vec![OrbEntity::new(OrbId::Frost)];
    crate::engine::action_handlers::execute_action(
        Action::MultiCast {
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 0,
        },
        &mut chemical_x,
    );
    assert_eq!(
        std::iter::from_fn(|| chemical_x.pop_next_action()).collect::<Vec<_>>(),
        vec![Action::EvokeOrbWithoutRemoving, Action::EvokeOrb],
        "Chemical X adds two Multi-Cast evokes even when X is zero"
    );

    let mut free_to_play = crate::test_support::blank_test_combat();
    free_to_play.turn.energy = 3;
    free_to_play.entities.player.orbs = vec![OrbEntity::new(OrbId::Plasma)];
    crate::engine::action_handlers::execute_action(
        Action::MultiCast {
            upgraded: false,
            free_to_play_once: true,
            energy_on_use: 2,
        },
        &mut free_to_play,
    );
    assert_eq!(free_to_play.turn.energy, 3);
    assert_eq!(
        std::iter::from_fn(|| free_to_play.pop_next_action()).collect::<Vec<_>>(),
        vec![Action::EvokeOrbWithoutRemoving, Action::EvokeOrb],
        "freeToPlayOnce keeps current energy but still uses energyOnUse for X"
    );

    let mut no_orb = crate::test_support::blank_test_combat();
    no_orb.turn.energy = 3;
    no_orb.entities.player.orbs = vec![OrbEntity::new(OrbId::Empty)];
    crate::engine::action_handlers::execute_action(
        Action::MultiCast {
            upgraded: false,
            free_to_play_once: false,
            energy_on_use: 3,
        },
        &mut no_orb,
    );
    assert_eq!(no_orb.turn.energy, 3);
    assert_eq!(
        no_orb.pop_next_action(),
        None,
        "Java MulticastAction does nothing and spends no energy when player.hasOrb() is false"
    );
}

#[test]
fn all_for_one_definition_runtime_and_discard_to_hand_match_java_sources() {
    let all_for_one = get_card_definition(CardId::AllForOne);
    assert_eq!(all_for_one.name, "All For One");
    assert_eq!(all_for_one.card_type, CardType::Attack);
    assert_eq!(all_for_one.rarity, CardRarity::Rare);
    assert_eq!(all_for_one.cost, 2);
    assert_eq!(all_for_one.base_damage, 10);
    assert_eq!(all_for_one.target, CardTarget::Enemy);
    assert_eq!(all_for_one.upgrade_damage, 4);
    assert_eq!(java_id(CardId::AllForOne), "All For One");

    let state = crate::test_support::blank_test_combat();
    let actions = resolve_card_play(
        CardId::AllForOne,
        &state,
        &CombatCard::for_scenario(CardId::AllForOne, 351),
        Some(352),
    );
    assert_eq!(actions.len(), 2);
    match &actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.target, 352);
            assert_eq!(info.base, 10);
            assert_eq!(info.output, 10);
        }
        other => panic!("All For One should damage before retrieving cards, got {other:?}"),
    }
    assert_eq!(actions[1].action, Action::AllCostToHand { cost_target: 0 });

    let mut plus = CombatCard::for_scenario(CardId::AllForOne, 353);
    plus.upgrades = 1;
    match &resolve_card_play(CardId::AllForOne, &state, &plus, Some(354))[0].action {
        Action::Damage(info) => assert_eq!(info.base, 14),
        other => panic!("All For One+ should still emit damage first, got {other:?}"),
    }

    let mut retrieval = crate::test_support::blank_test_combat();
    let zero_cost = CombatCard::for_scenario(CardId::Deflect, 355);
    let mut free_nonzero = CombatCard::for_scenario(CardId::StrikeG, 356);
    free_nonzero.free_to_play_once = true;
    let mut only_turn_free = CombatCard::for_scenario(CardId::StrikeG, 357);
    only_turn_free.set_cost_for_turn_java(0);
    retrieval.zones.discard_pile = vec![
        zero_cost.clone(),
        free_nonzero.clone(),
        only_turn_free.clone(),
    ]
    .into();
    crate::engine::action_handlers::execute_action(
        Action::AllCostToHand { cost_target: 0 },
        &mut retrieval,
    );
    assert_eq!(
        retrieval.pop_next_action(),
        Some(Action::DiscardToHand {
            card_lineage: zero_cost.lineage,
            cost_for_turn: None,
        })
    );
    assert_eq!(
        retrieval.pop_next_action(),
        Some(Action::DiscardToHand {
            card_lineage: free_nonzero.lineage,
            cost_for_turn: None,
        })
    );
    assert_eq!(retrieval.pop_next_action(), None);

    crate::engine::action_handlers::execute_action(
        Action::DiscardToHand {
            card_lineage: zero_cost.lineage,
            cost_for_turn: None,
        },
        &mut retrieval,
    );
    crate::engine::action_handlers::execute_action(
        Action::DiscardToHand {
            card_lineage: free_nonzero.lineage,
            cost_for_turn: None,
        },
        &mut retrieval,
    );
    assert_eq!(
        retrieval
            .zones
            .hand
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        vec![zero_cost.lineage, free_nonzero.lineage]
    );
    assert_eq!(
        retrieval
            .zones
            .discard_pile
            .iter()
            .map(|card| card.lineage)
            .collect::<Vec<_>>(),
        vec![only_turn_free.lineage],
        "Java AllCostToHandAction checks card.cost, not temporary costForTurn"
    );

    let mut full_hand = crate::test_support::blank_test_combat();
    full_hand.zones.hand = (0..10)
        .map(|idx| CombatCard::for_scenario(CardId::StrikeG, 400 + idx))
        .collect();
    full_hand.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Deflect, 411)].into();
    crate::engine::action_handlers::execute_action(
        Action::DiscardToHand {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(411),
            cost_for_turn: None,
        },
        &mut full_hand,
    );
    assert_eq!(full_hand.zones.hand.len(), 10);
    assert_eq!(
        full_hand.zones.discard_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(411),
        "Java DiscardToHandAction leaves the card in discard when the hand is full"
    );
}

#[test]
fn genetic_algorithm_definition_runtime_and_misc_growth_match_java_sources() {
    let genetic = get_card_definition(CardId::GeneticAlgorithm);
    assert_eq!(genetic.name, "Genetic Algorithm");
    assert_eq!(genetic.card_type, CardType::Skill);
    assert_eq!(genetic.rarity, CardRarity::Uncommon);
    assert_eq!(genetic.cost, 1);
    assert_eq!(genetic.base_block, 1);
    assert_eq!(genetic.base_magic, 2);
    assert_eq!(genetic.target, CardTarget::SelfTarget);
    assert!(genetic.exhaust);
    assert_eq!(genetic.upgrade_magic, 1);
    assert_eq!(java_id(CardId::GeneticAlgorithm), "Genetic Algorithm");

    let state = crate::test_support::blank_test_combat();
    let card = CombatCard::for_scenario(CardId::GeneticAlgorithm, 412);
    assert_eq!(
        card.misc_value, 1,
        "Java GeneticAlgorithm constructor initializes misc/baseBlock to 1"
    );
    let actions = resolve_card_play(CardId::GeneticAlgorithm, &state, &card, None);
    assert_eq!(
        actions[0].action,
        Action::ModifyCardMisc {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(412),
            amount: 2,
        }
    );
    assert_eq!(
        actions[1].action,
        Action::GainBlock {
            target: 0,
            amount: 1,
        }
    );

    let mut plus = CombatCard::for_scenario(CardId::GeneticAlgorithm, 413);
    plus.upgrades = 1;
    assert_eq!(
        resolve_card_play(CardId::GeneticAlgorithm, &state, &plus, None)[0].action,
        Action::ModifyCardMisc {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(413),
            amount: 3,
        }
    );

    let mut grown = crate::test_support::blank_test_combat();
    grown.zones.hand = vec![CombatCard::for_scenario(CardId::GeneticAlgorithm, 414)];
    grown.meta.master_deck_snapshot = vec![grown.zones.hand[0].clone()].into();
    grown.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::GeneticAlgorithm, 414)]).into();
    crate::engine::action_handlers::execute_action(
        Action::ModifyCardMisc {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(414),
            amount: 2,
        },
        &mut grown,
    );
    assert_eq!(grown.zones.hand[0].misc_value, 3);
    assert_eq!(grown.zones.draw_pile[0].misc_value, 3);
    assert_eq!(
        grown.meta.meta_changes,
        vec![crate::runtime::combat::MetaChange::ModifyCardMisc {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(414),
            amount: 2,
        }],
        "Java IncreaseMiscAction mutates the matching masterDeck card before battle instances"
    );

    let evaluated =
        crate::content::cards::evaluate_card_for_play(&grown.zones.hand[0], &grown, None);
    assert_eq!(
        evaluated.base_block_mut, 3,
        "Java GeneticAlgorithm.applyPowers uses misc as baseBlock"
    );
}

#[test]
fn defect_channel_history_blizzard_and_thunder_strike_match_java_sources() {
    let mut history_state = crate::test_support::blank_test_combat();
    history_state.entities.player.max_orbs = 2;
    history_state.entities.player.orbs =
        vec![OrbEntity::new(OrbId::Empty), OrbEntity::new(OrbId::Empty)];
    crate::engine::action_handlers::execute_action(
        Action::ChannelOrb(OrbId::Frost),
        &mut history_state,
    );
    crate::engine::action_handlers::execute_action(
        Action::ChannelOrbEntity {
            orb: OrbEntity::new(OrbId::Lightning),
        },
        &mut history_state,
    );
    assert_eq!(
        history_state.turn.counters.orbs_channeled_this_turn,
        vec![OrbId::Frost, OrbId::Lightning]
    );
    assert_eq!(
        history_state.turn.counters.orbs_channeled_this_combat,
        vec![OrbId::Frost, OrbId::Lightning]
    );
    history_state.turn.begin_next_player_turn(3);
    assert!(history_state
        .turn
        .counters
        .orbs_channeled_this_turn
        .is_empty());
    assert_eq!(
        history_state.turn.counters.orbs_channeled_this_combat,
        vec![OrbId::Frost, OrbId::Lightning],
        "Java GameActionManager clears orbsChanneledThisTurn each turn but keeps combat history"
    );

    let blizzard = get_card_definition(CardId::Blizzard);
    assert_eq!(blizzard.name, "Blizzard");
    assert_eq!(blizzard.card_type, CardType::Attack);
    assert_eq!(blizzard.rarity, CardRarity::Uncommon);
    assert_eq!(blizzard.cost, 1);
    assert_eq!(blizzard.base_damage, 0);
    assert_eq!(blizzard.base_magic, 2);
    assert_eq!(blizzard.target, CardTarget::AllEnemy);
    assert!(blizzard.is_multi_damage);
    assert_eq!(blizzard.upgrade_magic, 1);
    assert_eq!(java_id(CardId::Blizzard), "Blizzard");

    let thunder = get_card_definition(CardId::ThunderStrike);
    assert_eq!(thunder.name, "Thunder Strike");
    assert_eq!(thunder.card_type, CardType::Attack);
    assert_eq!(thunder.rarity, CardRarity::Rare);
    assert_eq!(thunder.cost, 3);
    assert_eq!(thunder.base_damage, 7);
    assert_eq!(thunder.target, CardTarget::AllEnemy);
    assert!(thunder.tags.contains(&CardTag::Strike));
    assert_eq!(thunder.upgrade_damage, 2);
    assert_eq!(java_id(CardId::ThunderStrike), "Thunder Strike");

    let mut state = crate::test_support::blank_test_combat();
    let mut left = crate::test_support::test_monster(EnemyId::JawWorm);
    left.id = 501;
    let mut right = crate::test_support::test_monster(EnemyId::Cultist);
    right.id = 502;
    state.entities.monsters = vec![left, right];
    state.turn.counters.orbs_channeled_this_combat =
        vec![OrbId::Frost, OrbId::Lightning, OrbId::Frost];

    match &resolve_card_play(
        CardId::Blizzard,
        &state,
        &CombatCard::for_scenario(CardId::Blizzard, 415),
        None,
    )[0]
    .action
    {
        Action::DamageAllEnemies { damages, .. } => {
            assert_eq!(damages.as_slice(), &[4, 4]);
        }
        other => panic!("Blizzard should emit DamageAllEnemiesAction, got {other:?}"),
    }

    let mut blizzard_plus = CombatCard::for_scenario(CardId::Blizzard, 416);
    blizzard_plus.upgrades = 1;
    match &resolve_card_play(CardId::Blizzard, &state, &blizzard_plus, None)[0].action {
        Action::DamageAllEnemies { damages, .. } => {
            assert_eq!(damages.as_slice(), &[6, 6]);
        }
        other => panic!("Blizzard+ should emit DamageAllEnemiesAction, got {other:?}"),
    }

    let thunder_actions = resolve_card_play(
        CardId::ThunderStrike,
        &state,
        &CombatCard::for_scenario(CardId::ThunderStrike, 417),
        None,
    );
    assert_eq!(
        thunder_actions.len(),
        1,
        "Java Thunder Strike queues one NewThunderStrikeAction per Lightning channeled this combat"
    );
    match &thunder_actions[0].action {
        Action::AttackDamageRandomEnemyCard { card } => {
            assert_eq!(card.id, CardId::ThunderStrike);
            assert_eq!(card.upgrades, 0);
        }
        other => panic!("Thunder Strike should emit AttackDamageRandomEnemyCard, got {other:?}"),
    }

    let mut thunder_plus = CombatCard::for_scenario(CardId::ThunderStrike, 418);
    thunder_plus.upgrades = 1;
    match &resolve_card_play(CardId::ThunderStrike, &state, &thunder_plus, None)[0].action {
        Action::AttackDamageRandomEnemyCard { card } => {
            assert_eq!(card.id, CardId::ThunderStrike);
            assert_eq!(card.upgrades, 1);
        }
        other => {
            panic!("Thunder Strike+ should preserve upgraded damage in queued card, got {other:?}")
        }
    }
}

#[test]
fn ftl_action_draws_before_damage_only_below_play_count_threshold() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.counters.cards_played_this_turn = 3;
    crate::engine::action_handlers::execute_action(
        Action::Ftl {
            target: 260,
            damage_info: DamageInfo {
                source: 0,
                target: 260,
                base: 5,
                output: 5,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
            card_play_count: 3,
        },
        &mut state,
    );
    assert_eq!(state.pop_next_action(), Some(Action::DrawCards(1)));
    assert!(matches!(state.pop_next_action(), Some(Action::Damage(_))));

    state.turn.counters.cards_played_this_turn = 4;
    crate::engine::action_handlers::execute_action(
        Action::Ftl {
            target: 260,
            damage_info: DamageInfo {
                source: 0,
                target: 260,
                base: 5,
                output: 5,
                damage_type: DamageType::Normal,
                is_modified: true,
            },
            card_play_count: 3,
        },
        &mut state,
    );
    assert!(matches!(state.pop_next_action(), Some(Action::Damage(_))));
    assert_eq!(state.pop_next_action(), None);
}

#[test]
fn ironclad_common_utility_definitions_match_java_sources() {
    let anger = get_card_definition(CardId::Anger);
    assert_eq!(anger.name, "Anger");
    assert_eq!(anger.card_type, CardType::Attack);
    assert_eq!(anger.rarity, CardRarity::Common);
    assert_eq!(anger.cost, 0);
    assert_eq!(anger.base_damage, 6);
    assert_eq!(anger.target, CardTarget::Enemy);
    assert_eq!(anger.upgrade_damage, 2);

    let armaments = get_card_definition(CardId::Armaments);
    assert_eq!(armaments.name, "Armaments");
    assert_eq!(armaments.card_type, CardType::Skill);
    assert_eq!(armaments.rarity, CardRarity::Common);
    assert_eq!(armaments.cost, 1);
    assert_eq!(armaments.base_block, 5);
    assert_eq!(armaments.target, CardTarget::SelfTarget);
    assert_eq!(armaments.upgrade_damage, 0);
    assert_eq!(armaments.upgrade_block, 0);
    assert_eq!(armaments.upgrade_magic, 0);

    let barricade = get_card_definition(CardId::Barricade);
    assert_eq!(barricade.name, "Barricade");
    assert_eq!(barricade.card_type, CardType::Power);
    assert_eq!(barricade.rarity, CardRarity::Rare);
    assert_eq!(barricade.cost, 3);
    assert_eq!(barricade.target, CardTarget::SelfTarget);
    let mut barricade_plus = CombatCard::for_scenario(CardId::Barricade, 41);
    barricade_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&barricade_plus), Some(2));

    let battle_trance = get_card_definition(CardId::BattleTrance);
    assert_eq!(battle_trance.name, "Battle Trance");
    assert_eq!(battle_trance.card_type, CardType::Skill);
    assert_eq!(battle_trance.rarity, CardRarity::Uncommon);
    assert_eq!(battle_trance.cost, 0);
    assert_eq!(battle_trance.base_magic, 3);
    assert_eq!(battle_trance.target, CardTarget::None);
    assert_eq!(battle_trance.upgrade_magic, 1);
}

#[test]
fn ironclad_common_utility_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();

    let anger_actions = resolve_card_play(
        CardId::Anger,
        &state,
        &CombatCard::for_scenario(CardId::Anger, 10),
        Some(7),
    );
    assert_eq!(anger_actions.len(), 2);
    match &anger_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Anger first action should be DamageAction, got {other:?}"),
    }
    match &anger_actions[1].action {
        Action::MakeCopyInDiscard { original, amount } => {
            assert_eq!(original.id, CardId::Anger);
            assert_eq!(
                original.lineage,
                crate::runtime::combat::CardLineageId::constructed_scenario(10)
            );
            assert_eq!(original.upgrades, 0);
            assert_eq!(*amount, 1);
        }
        other => panic!(
            "Anger second action should be MakeTempCardInDiscardAction equivalent, got {other:?}"
        ),
    }

    let mut mutated_anger = CombatCard::for_scenario(CardId::Anger, 77);
    mutated_anger.upgrades = 1;
    mutated_anger.misc_value = 9;
    mutated_anger.base_damage_override = Some(21);
    mutated_anger.cost_modifier = -1;
    mutated_anger.cost_for_turn = Some(0);
    mutated_anger.free_to_play_once = true;
    mutated_anger.base_damage_mut = 99;
    mutated_anger.base_block_mut = 88;
    mutated_anger.base_magic_num_mut = 77;
    mutated_anger.multi_damage = smallvec::smallvec![1, 2, 3];
    mutated_anger.exhaust_override = Some(true);
    mutated_anger.retain_override = Some(true);
    mutated_anger.energy_on_use = 5;

    let mutated_anger_actions = resolve_card_play(CardId::Anger, &state, &mutated_anger, Some(7));
    let Action::MakeCopyInDiscard { original, amount } = &mutated_anger_actions[1].action else {
        panic!("Anger should queue a stat-equivalent source copy for MakeTempCardInDiscardAction");
    };
    assert_eq!(*amount, 1);
    assert_eq!(
        original.lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(77)
    );
    assert_eq!(original.id, CardId::Anger);
    assert_eq!(original.upgrades, 1);
    assert_eq!(original.misc_value, 9);
    assert_eq!(original.base_damage_override, Some(21));
    assert_eq!(original.cost_modifier, -1);
    assert_eq!(original.cost_for_turn, Some(0));
    assert!(original.free_to_play_once);
    assert_eq!(
        original.base_damage_mut, 0,
        "Java Anger queues this.makeStatEquivalentCopy(), not the rendered damage snapshot"
    );
    assert_eq!(original.base_block_mut, 0);
    assert_eq!(original.base_magic_num_mut, 0);
    assert!(original.multi_damage.is_empty());
    assert_eq!(original.exhaust_override, None);
    assert_eq!(original.retain_override, None);
    assert_eq!(original.energy_on_use, 0);

    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 11),
        {
            let mut upgraded = CombatCard::for_scenario(CardId::Defend, 12);
            upgraded.upgrades = 1;
            upgraded
        },
        CombatCard::for_scenario(CardId::Wound, 13),
    ];
    let mut armaments_plus = CombatCard::for_scenario(CardId::Armaments, 14);
    armaments_plus.upgrades = 1;
    let armaments_plus_actions =
        resolve_card_play(CardId::Armaments, &state, &armaments_plus, None);
    assert_eq!(armaments_plus_actions.len(), 2);
    assert!(matches!(
        armaments_plus_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 5
        }
    ));
    crate::engine::action_handlers::execute_action(
        armaments_plus_actions[1].action.clone(),
        &mut state,
    );
    assert_eq!(
        state
            .zones
            .hand
            .iter()
            .map(|c| c.upgrades)
            .collect::<Vec<_>>(),
        [1, 1, 0],
        "Armaments+ only upgrades eligible cards in the resolving hand"
    );

    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 21),
        CombatCard::for_scenario(CardId::Defend, 22),
    ];
    let armaments_actions = resolve_card_play(
        CardId::Armaments,
        &state,
        &CombatCard::for_scenario(CardId::Armaments, 23),
        None,
    );
    assert_eq!(armaments_actions.len(), 2);
    match &armaments_actions[1].action {
        Action::SuspendForHandSelect {
            min,
            max,
            can_cancel,
            filter,
            reason,
        } => {
            assert_eq!(*min, 1);
            assert_eq!(*max, 1);
            assert!(!*can_cancel);
            assert_eq!(*filter, crate::state::HandSelectFilter::Upgradeable);
            assert_eq!(*reason, crate::state::HandSelectReason::Upgrade);
        }
        other => panic!("Armaments should open upgrade hand select, got {other:?}"),
    }

    let barricade_actions = resolve_card_play(
        CardId::Barricade,
        &state,
        &CombatCard::for_scenario(CardId::Barricade, 30),
        None,
    );
    assert_eq!(barricade_actions.len(), 1);
    match &barricade_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Barricade);
            assert_eq!(*amount, -1);
        }
        other => panic!("Barricade should apply BarricadePower, got {other:?}"),
    }
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Barricade,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let duplicate_barricade_actions = resolve_card_play(
        CardId::Barricade,
        &state,
        &CombatCard::for_scenario(CardId::Barricade, 31),
        None,
    );
    assert!(duplicate_barricade_actions.is_empty());

    let battle_trance_actions = resolve_card_play(
        CardId::BattleTrance,
        &state,
        &CombatCard::for_scenario(CardId::BattleTrance, 40),
        None,
    );
    assert_eq!(battle_trance_actions.len(), 2);
    assert!(matches!(
        battle_trance_actions[0].action,
        Action::DrawCards(3)
    ));
    match &battle_trance_actions[1].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::NoDraw);
            assert_eq!(*amount, -1);
        }
        other => panic!("Battle Trance should apply NoDrawPower, got {other:?}"),
    }

    let mut battle_trance_plus = CombatCard::for_scenario(CardId::BattleTrance, 41);
    battle_trance_plus.upgrades = 1;
    let battle_trance_plus_actions =
        resolve_card_play(CardId::BattleTrance, &state, &battle_trance_plus, None);
    assert!(matches!(
        battle_trance_plus_actions[0].action,
        Action::DrawCards(4)
    ));
}

#[test]
fn ironclad_cost_and_hp_cards_definitions_match_java_sources() {
    let berserk = get_card_definition(CardId::Berserk);
    assert_eq!(berserk.name, "Berserk");
    assert_eq!(berserk.card_type, CardType::Power);
    assert_eq!(berserk.rarity, CardRarity::Rare);
    assert_eq!(berserk.cost, 0);
    assert_eq!(berserk.base_magic, 2);
    assert_eq!(berserk.target, CardTarget::SelfTarget);
    assert_eq!(berserk.upgrade_magic, -1);

    let blood_for_blood = get_card_definition(CardId::BloodForBlood);
    assert_eq!(blood_for_blood.name, "Blood for Blood");
    assert_eq!(blood_for_blood.card_type, CardType::Attack);
    assert_eq!(blood_for_blood.rarity, CardRarity::Uncommon);
    assert_eq!(blood_for_blood.cost, 4);
    assert_eq!(blood_for_blood.base_damage, 18);
    assert_eq!(blood_for_blood.target, CardTarget::Enemy);
    assert_eq!(blood_for_blood.upgrade_damage, 4);
    let mut blood_for_blood_plus = CombatCard::for_scenario(CardId::BloodForBlood, 50);
    blood_for_blood_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&blood_for_blood_plus), Some(3));

    let bloodletting = get_card_definition(CardId::Bloodletting);
    assert_eq!(bloodletting.name, "Bloodletting");
    assert_eq!(bloodletting.card_type, CardType::Skill);
    assert_eq!(bloodletting.rarity, CardRarity::Uncommon);
    assert_eq!(bloodletting.cost, 0);
    assert_eq!(bloodletting.base_magic, 2);
    assert_eq!(bloodletting.target, CardTarget::SelfTarget);
    assert_eq!(bloodletting.upgrade_magic, 1);

    let bludgeon = get_card_definition(CardId::Bludgeon);
    assert_eq!(bludgeon.name, "Bludgeon");
    assert_eq!(bludgeon.card_type, CardType::Attack);
    assert_eq!(bludgeon.rarity, CardRarity::Rare);
    assert_eq!(bludgeon.cost, 3);
    assert_eq!(bludgeon.base_damage, 32);
    assert_eq!(bludgeon.target, CardTarget::Enemy);
    assert_eq!(bludgeon.upgrade_damage, 10);
}

#[test]
fn ironclad_cost_and_hp_cards_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let berserk_actions = resolve_card_play(
        CardId::Berserk,
        &state,
        &CombatCard::for_scenario(CardId::Berserk, 51),
        None,
    );
    assert_eq!(berserk_actions.len(), 2);
    match &berserk_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Vulnerable);
            assert_eq!(*amount, 2);
        }
        other => panic!("Berserk first action should apply Vulnerable, got {other:?}"),
    }
    match &berserk_actions[1].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Berserk);
            assert_eq!(*amount, 1);
        }
        other => panic!("Berserk second action should apply BerserkPower, got {other:?}"),
    }
    let mut berserk_plus = CombatCard::for_scenario(CardId::Berserk, 52);
    berserk_plus.upgrades = 1;
    let berserk_plus_actions = resolve_card_play(CardId::Berserk, &state, &berserk_plus, None);
    match &berserk_plus_actions[0].action {
        Action::ApplyPower { amount, .. } => assert_eq!(*amount, 1),
        other => panic!("Berserk+ first action should apply 1 Vulnerable, got {other:?}"),
    }

    let blood_for_blood_actions = resolve_card_play(
        CardId::BloodForBlood,
        &state,
        &CombatCard::for_scenario(CardId::BloodForBlood, 53),
        Some(7),
    );
    assert_eq!(blood_for_blood_actions.len(), 1);
    match &blood_for_blood_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 18);
            assert_eq!(info.output, 18);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Blood for Blood should emit DamageAction, got {other:?}"),
    }

    let bloodletting_actions = resolve_card_play(
        CardId::Bloodletting,
        &state,
        &CombatCard::for_scenario(CardId::Bloodletting, 54),
        None,
    );
    assert_eq!(bloodletting_actions.len(), 2);
    match &bloodletting_actions[0].action {
        Action::LoseHp {
            target,
            amount,
            triggers_rupture,
        } => {
            assert_eq!(*target, 0);
            assert_eq!(*amount, 3);
            assert!(*triggers_rupture);
        }
        other => panic!("Bloodletting first action should be LoseHPAction, got {other:?}"),
    }
    assert!(matches!(
        bloodletting_actions[1].action,
        Action::GainEnergy { amount: 2 }
    ));
    let mut bloodletting_plus = CombatCard::for_scenario(CardId::Bloodletting, 55);
    bloodletting_plus.upgrades = 1;
    let bloodletting_plus_actions =
        resolve_card_play(CardId::Bloodletting, &state, &bloodletting_plus, None);
    assert!(matches!(
        bloodletting_plus_actions[1].action,
        Action::GainEnergy { amount: 3 }
    ));

    let bludgeon_actions = resolve_card_play(
        CardId::Bludgeon,
        &state,
        &CombatCard::for_scenario(CardId::Bludgeon, 56),
        Some(7),
    );
    assert_eq!(bludgeon_actions.len(), 1);
    match &bludgeon_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 32);
            assert_eq!(info.output, 32);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Bludgeon should emit DamageAction, got {other:?}"),
    }
    let mut bludgeon_plus = CombatCard::for_scenario(CardId::Bludgeon, 57);
    bludgeon_plus.upgrades = 1;
    let bludgeon_plus_actions =
        resolve_card_play(CardId::Bludgeon, &state, &bludgeon_plus, Some(7));
    match &bludgeon_plus_actions[0].action {
        Action::Damage(info) => assert_eq!(info.output, 42),
        other => panic!("Bludgeon+ should emit upgraded DamageAction, got {other:?}"),
    }
}

#[test]
fn blood_for_blood_cost_updates_when_player_takes_hp_loss() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::BloodForBlood, 60)];
    state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::BloodForBlood, 61)].into();
    let mut upgraded = CombatCard::for_scenario(CardId::BloodForBlood, 62);
    upgraded.upgrades = 1;
    state.zones.draw_pile = (vec![upgraded]).into();

    crate::engine::action_handlers::damage::handle_lose_hp(0, 1, true, &mut state);

    assert_eq!(state.zones.hand[0].cost_modifier, -1);
    assert_eq!(state.zones.hand[0].get_cost(), 3);
    assert_eq!(state.zones.discard_pile[0].cost_modifier, -1);
    assert_eq!(state.zones.discard_pile[0].get_cost(), 3);
    assert_eq!(state.zones.draw_pile[0].cost_modifier, -1);
    assert_eq!(state.zones.draw_pile[0].get_cost(), 2);
    assert_eq!(state.turn.counters.times_damaged_this_combat, 1);
}

#[test]
fn upgraded_base_cost_is_used_when_spending_energy() {
    let mut state = crate::test_support::blank_test_combat();
    let mut barricade_plus = CombatCard::for_scenario(CardId::Barricade, 70);
    barricade_plus.upgrades = 1;
    state.zones.hand = vec![barricade_plus];
    state.turn.energy = 3;

    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut state)
        .expect("Barricade+ should be playable");

    assert_eq!(state.turn.energy, 1);
}

#[test]
fn ironclad_block_exhaust_and_ethereal_definitions_match_java_sources() {
    let body_slam = get_card_definition(CardId::BodySlam);
    assert_eq!(body_slam.name, "Body Slam");
    assert_eq!(body_slam.card_type, CardType::Attack);
    assert_eq!(body_slam.rarity, CardRarity::Common);
    assert_eq!(body_slam.cost, 1);
    assert_eq!(body_slam.base_damage, 0);
    assert_eq!(body_slam.target, CardTarget::Enemy);
    let mut body_slam_plus = CombatCard::for_scenario(CardId::BodySlam, 80);
    body_slam_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&body_slam_plus), Some(0));

    let brutality = get_card_definition(CardId::Brutality);
    assert_eq!(brutality.name, "Brutality");
    assert_eq!(brutality.card_type, CardType::Power);
    assert_eq!(brutality.rarity, CardRarity::Rare);
    assert_eq!(brutality.cost, 0);
    assert_eq!(brutality.target, CardTarget::SelfTarget);
    assert!(!brutality.innate);
    let mut brutality_plus = CombatCard::for_scenario(CardId::Brutality, 81);
    brutality_plus.upgrades = 1;
    assert!(is_innate_card(&brutality_plus));

    let burning_pact = get_card_definition(CardId::BurningPact);
    assert_eq!(burning_pact.name, "Burning Pact");
    assert_eq!(burning_pact.card_type, CardType::Skill);
    assert_eq!(burning_pact.rarity, CardRarity::Uncommon);
    assert_eq!(burning_pact.cost, 1);
    assert_eq!(burning_pact.base_magic, 2);
    assert_eq!(burning_pact.target, CardTarget::None);
    assert_eq!(burning_pact.upgrade_magic, 1);

    let carnage = get_card_definition(CardId::Carnage);
    assert_eq!(carnage.name, "Carnage");
    assert_eq!(carnage.card_type, CardType::Attack);
    assert_eq!(carnage.rarity, CardRarity::Uncommon);
    assert_eq!(carnage.cost, 2);
    assert_eq!(carnage.base_damage, 20);
    assert_eq!(carnage.target, CardTarget::Enemy);
    assert!(carnage.ethereal);
    assert_eq!(carnage.upgrade_damage, 8);
}

#[test]
fn ironclad_block_exhaust_and_ethereal_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    state.entities.player.block = 13;

    let body_slam_actions = resolve_card_play(
        CardId::BodySlam,
        &state,
        &CombatCard::for_scenario(CardId::BodySlam, 82),
        Some(7),
    );
    assert_eq!(body_slam_actions.len(), 1);
    match &body_slam_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 13);
            assert_eq!(info.output, 13);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Body Slam should emit block-based DamageAction, got {other:?}"),
    }

    let brutality_actions = resolve_card_play(
        CardId::Brutality,
        &state,
        &CombatCard::for_scenario(CardId::Brutality, 83),
        None,
    );
    assert_eq!(brutality_actions.len(), 1);
    match &brutality_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Brutality);
            assert_eq!(*amount, 1);
        }
        other => panic!("Brutality should apply BrutalityPower, got {other:?}"),
    }

    state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 84)];
    let burning_pact_actions = resolve_card_play(
        CardId::BurningPact,
        &state,
        &CombatCard::for_scenario(CardId::BurningPact, 85),
        None,
    );
    assert_eq!(burning_pact_actions.len(), 2);
    assert_eq!(
        burning_pact_actions[0].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: false,
            any_number: false,
            can_pick_zero: false,
        },
        "Java Burning Pact queues ExhaustAction; it reads hand size when the action executes"
    );
    assert!(matches!(
        burning_pact_actions[1].action,
        Action::DrawCards(2)
    ));

    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 86),
        CombatCard::for_scenario(CardId::Defend, 87),
    ];
    let burning_pact_select_actions = resolve_card_play(
        CardId::BurningPact,
        &state,
        &CombatCard::for_scenario(CardId::BurningPact, 88),
        None,
    );
    assert_eq!(
        burning_pact_select_actions[0].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: false,
            any_number: false,
            can_pick_zero: false,
        }
    );
    let mut burning_pact_plus = CombatCard::for_scenario(CardId::BurningPact, 89);
    burning_pact_plus.upgrades = 1;
    let burning_pact_plus_actions =
        resolve_card_play(CardId::BurningPact, &state, &burning_pact_plus, None);
    assert!(matches!(
        burning_pact_plus_actions[1].action,
        Action::DrawCards(3)
    ));

    let carnage_actions = resolve_card_play(
        CardId::Carnage,
        &state,
        &CombatCard::for_scenario(CardId::Carnage, 90),
        Some(7),
    );
    assert_eq!(carnage_actions.len(), 1);
    match &carnage_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 20);
            assert_eq!(info.output, 20);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Carnage should emit DamageAction, got {other:?}"),
    }
    let mut carnage_plus = CombatCard::for_scenario(CardId::Carnage, 91);
    carnage_plus.upgrades = 1;
    let carnage_plus_actions = resolve_card_play(CardId::Carnage, &state, &carnage_plus, Some(7));
    match &carnage_plus_actions[0].action {
        Action::Damage(info) => assert_eq!(info.output, 28),
        other => panic!("Carnage+ should emit upgraded DamageAction, got {other:?}"),
    }
}

#[test]
fn mind_blast_shared_evaluation_tracks_the_current_draw_pile() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.draw_pile = (0..7)
        .map(|index| CombatCard::for_scenario(CardId::Strike, 20_000 + index))
        .collect();
    let card = CombatCard::for_scenario(CardId::MindBlast, 20_100);

    let evaluated = evaluate_card_for_play(&card, &state, Some(7));
    assert_eq!(evaluated.base_damage_mut, 7);

    let actions = resolve_card_play(CardId::MindBlast, &state, &card, Some(7));
    let Action::Damage(info) = &actions[0].action else {
        panic!("Mind Blast should produce one damage action");
    };
    assert_eq!(info.base, 7);
    assert_eq!(info.output, 7);
}

#[test]
fn ironclad_attack_condition_and_dot_power_definitions_match_java_sources() {
    let clash = get_card_definition(CardId::Clash);
    assert_eq!(clash.name, "Clash");
    assert_eq!(clash.card_type, CardType::Attack);
    assert_eq!(clash.rarity, CardRarity::Common);
    assert_eq!(clash.cost, 0);
    assert_eq!(clash.base_damage, 14);
    assert_eq!(clash.target, CardTarget::Enemy);
    assert_eq!(clash.upgrade_damage, 4);

    let cleave = get_card_definition(CardId::Cleave);
    assert_eq!(cleave.name, "Cleave");
    assert_eq!(cleave.card_type, CardType::Attack);
    assert_eq!(cleave.rarity, CardRarity::Common);
    assert_eq!(cleave.cost, 1);
    assert_eq!(cleave.base_damage, 8);
    assert!(cleave.is_multi_damage);
    assert_eq!(cleave.target, CardTarget::AllEnemy);
    assert_eq!(cleave.upgrade_damage, 3);

    let clothesline = get_card_definition(CardId::Clothesline);
    assert_eq!(clothesline.name, "Clothesline");
    assert_eq!(clothesline.card_type, CardType::Attack);
    assert_eq!(clothesline.rarity, CardRarity::Common);
    assert_eq!(clothesline.cost, 2);
    assert_eq!(clothesline.base_damage, 12);
    assert_eq!(clothesline.base_magic, 2);
    assert_eq!(clothesline.target, CardTarget::Enemy);
    assert_eq!(clothesline.upgrade_damage, 2);
    assert_eq!(clothesline.upgrade_magic, 1);

    let combust = get_card_definition(CardId::Combust);
    assert_eq!(combust.name, "Combust");
    assert_eq!(combust.card_type, CardType::Power);
    assert_eq!(combust.rarity, CardRarity::Uncommon);
    assert_eq!(combust.cost, 1);
    assert_eq!(combust.base_magic, 5);
    assert_eq!(combust.target, CardTarget::SelfTarget);
    assert_eq!(combust.upgrade_magic, 2);
}

#[test]
fn ironclad_attack_condition_and_dot_power_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();

    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Clash, 100),
        CombatCard::for_scenario(CardId::Strike, 101),
    ];
    assert!(can_play_card(&state.zones.hand[0], &state).is_ok());
    let mut clash_plus = CombatCard::for_scenario(CardId::Clash, 102);
    clash_plus.upgrades = 1;
    let clash_plus_actions = resolve_card_play(CardId::Clash, &state, &clash_plus, Some(7));
    assert_eq!(clash_plus_actions.len(), 1);
    match &clash_plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 7);
            assert_eq!(info.base, 18);
            assert_eq!(info.output, 18);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Clash+ should emit upgraded DamageAction, got {other:?}"),
    }

    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Clash, 103),
        CombatCard::for_scenario(CardId::Defend, 104),
    ];
    assert!(can_play_card(&state.zones.hand[0], &state).is_err());

    let mut autoplay_clash_state = crate::test_support::blank_test_combat();
    let mut clash_target = crate::test_support::test_monster(EnemyId::JawWorm);
    clash_target.id = 7;
    autoplay_clash_state.entities.monsters = vec![clash_target.clone()];
    autoplay_clash_state.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 106)];
    autoplay_clash_state.enqueue_card_play(
        crate::runtime::combat::QueuedCardPlay {
            card: CombatCard::for_scenario(CardId::Clash, 107),
            target: Some(7),
            energy_on_use: 0,
            ignore_energy_total: true,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: crate::runtime::combat::QueuedCardSource::Normal,
        },
        false,
    );
    let flush_autoplay_clash = autoplay_clash_state
        .pop_next_action()
        .expect("autoplay Clash should schedule queue flush");
    crate::engine::action_handlers::execute_action(flush_autoplay_clash, &mut autoplay_clash_state);
    let fizzled_clash_cleanup = autoplay_clash_state.pop_next_action();
    assert!(
        matches!(
            fizzled_clash_cleanup,
            Some(Action::UseCardDone {
                should_exhaust: false,
                trigger_after_use_hooks: false,
            })
        ),
        "Java queued/autoplay cards still call canUse; failed autoplay still resolves UseCardAction cleanup"
    );
    crate::engine::action_handlers::execute_action(
        fizzled_clash_cleanup.unwrap(),
        &mut autoplay_clash_state,
    );
    assert_eq!(autoplay_clash_state.zones.discard_pile.len(), 1);
    assert_eq!(autoplay_clash_state.zones.discard_pile[0].id, CardId::Clash);

    let mut fizzle_then_continue_state = crate::test_support::blank_test_combat();
    fizzle_then_continue_state.entities.monsters = vec![clash_target.clone()];
    fizzle_then_continue_state.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 109)];
    fizzle_then_continue_state.enqueue_card_play(
        crate::runtime::combat::QueuedCardPlay {
            card: CombatCard::for_scenario(CardId::Clash, 110),
            target: Some(7),
            energy_on_use: 0,
            ignore_energy_total: true,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: crate::runtime::combat::QueuedCardSource::Normal,
        },
        false,
    );
    fizzle_then_continue_state.enqueue_card_play(
        crate::runtime::combat::QueuedCardPlay {
            card: CombatCard::for_scenario(CardId::Strike, 111),
            target: Some(7),
            energy_on_use: 0,
            ignore_energy_total: true,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: crate::runtime::combat::QueuedCardSource::Normal,
        },
        false,
    );
    let first_flush = fizzle_then_continue_state
        .pop_next_action()
        .expect("first queued card should schedule flush");
    crate::engine::action_handlers::execute_action(first_flush, &mut fizzle_then_continue_state);
    let cleanup = fizzle_then_continue_state
        .pop_next_action()
        .expect("failed autoplay should clean up before later queued cards");
    assert!(matches!(cleanup, Action::UseCardDone { .. }));
    crate::engine::action_handlers::execute_action(cleanup, &mut fizzle_then_continue_state);
    assert_eq!(
        fizzle_then_continue_state.pop_next_action(),
        Some(Action::FlushNextQueuedCard),
        "a failed autoplay card must not strand later queued cards"
    );

    let mut autoplay_no_energy_state = crate::test_support::blank_test_combat();
    autoplay_no_energy_state.turn.energy = 0;
    autoplay_no_energy_state.entities.monsters = vec![clash_target];
    autoplay_no_energy_state.enqueue_card_play(
        crate::runtime::combat::QueuedCardPlay {
            card: CombatCard::for_scenario(CardId::Strike, 108),
            target: Some(7),
            energy_on_use: 0,
            ignore_energy_total: true,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: crate::runtime::combat::QueuedCardSource::Normal,
        },
        false,
    );
    let flush_autoplay_strike = autoplay_no_energy_state
        .pop_next_action()
        .expect("autoplay Strike should schedule queue flush");
    crate::engine::action_handlers::execute_action(
        flush_autoplay_strike,
        &mut autoplay_no_energy_state,
    );
    assert!(
        matches!(
            autoplay_no_energy_state.pop_next_action(),
            Some(Action::PlayCardDirect { .. })
        ),
        "Java isInAutoplay bypasses only the final energy check"
    );

    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 11;
    first.slot = 0;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 12;
    second.slot = 1;
    state.entities.monsters = vec![first, second];

    let mut cleave_plus = CombatCard::for_scenario(CardId::Cleave, 105);
    cleave_plus.upgrades = 1;
    let cleave_plus_actions = resolve_card_play(CardId::Cleave, &state, &cleave_plus, None);
    assert_eq!(cleave_plus_actions.len(), 1);
    match &cleave_plus_actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(damages.as_slice(), &[11, 11]);
            assert_eq!(*damage_type, DamageType::Normal);
            assert!(!*is_modified);
        }
        other => panic!("Cleave+ should emit DamageAllEnemiesAction, got {other:?}"),
    }

    let mut clothesline_plus = CombatCard::for_scenario(CardId::Clothesline, 106);
    clothesline_plus.upgrades = 1;
    let clothesline_plus_actions =
        resolve_card_play(CardId::Clothesline, &state, &clothesline_plus, Some(11));
    assert_eq!(clothesline_plus_actions.len(), 2);
    match &clothesline_plus_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 11);
            assert_eq!(info.base, 14);
            assert_eq!(info.output, 14);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Clothesline+ first action should be DamageAction, got {other:?}"),
    }
    match &clothesline_plus_actions[1].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 11);
            assert_eq!(*power_id, PowerId::Weak);
            assert_eq!(*amount, 3);
        }
        other => panic!("Clothesline+ second action should apply Weak, got {other:?}"),
    }

    let mut combust_plus = CombatCard::for_scenario(CardId::Combust, 107);
    combust_plus.upgrades = 1;
    let combust_plus_actions = resolve_card_play(CardId::Combust, &state, &combust_plus, None);
    assert_eq!(combust_plus_actions.len(), 1);
    match &combust_plus_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Combust);
            assert_eq!(*amount, 7);
        }
        other => panic!("Combust+ should apply CombustPower with upgraded damage, got {other:?}"),
    }
}

#[test]
fn combust_power_stacks_damage_and_hp_loss_like_java_source() {
    let mut state = crate::test_support::blank_test_combat();

    crate::engine::action_handlers::powers::handle_apply_power(
        0,
        0,
        PowerId::Combust,
        5,
        &mut state,
    );
    crate::engine::action_handlers::powers::handle_apply_power(
        0,
        0,
        PowerId::Combust,
        7,
        &mut state,
    );

    let combust_power = crate::content::powers::store::powers_for(&state, 0)
        .and_then(|powers| powers.iter().find(|p| p.power_type == PowerId::Combust))
        .cloned()
        .expect("Combust power should be stored on the player");
    assert_eq!(combust_power.amount, 12);
    assert_eq!(combust_power.extra_data, 2);

    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 21;
    first.slot = 0;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 22;
    second.slot = 1;
    state.entities.monsters = vec![first, second];

    let actions =
        crate::content::powers::resolve_power_at_end_of_turn(&combust_power, &mut state, 0);
    assert_eq!(actions.len(), 2);
    match &actions[0] {
        Action::LoseHp {
            target,
            amount,
            triggers_rupture,
        } => {
            assert_eq!(*target, 0);
            assert_eq!(*amount, 2);
            assert!(*triggers_rupture);
        }
        other => panic!("Combust should lose stored hpLoss first, got {other:?}"),
    }
    match &actions[1] {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, NO_SOURCE);
            assert_eq!(damages.as_slice(), &[12, 12]);
            assert_eq!(*damage_type, DamageType::Thorns);
            assert!(!*is_modified);
        }
        other => panic!("Combust should damage all enemies second, got {other:?}"),
    }

    for monster in &mut state.entities.monsters {
        monster.current_hp = 0;
        monster.is_dying = false;
    }
    let zero_hp_not_dying_actions =
        crate::content::powers::resolve_power_at_end_of_turn(&combust_power, &mut state, 0);
    assert_eq!(
        zero_hp_not_dying_actions.len(),
        2,
        "Java areMonstersBasicallyDead ignores currentHealth; Combust still fires until monsters are isDying/isEscaping"
    );

    for monster in &mut state.entities.monsters {
        monster.current_hp = 0;
        monster.is_dying = true;
    }
    let no_monster_actions =
        crate::content::powers::resolve_power_at_end_of_turn(&combust_power, &mut state, 0);
    assert!(
        no_monster_actions.is_empty(),
        "Java skips Combust atEndOfTurn when monsters are basically dead"
    );
}

#[test]
fn ironclad_power_and_debuff_definitions_match_java_sources() {
    let corruption = get_card_definition(CardId::Corruption);
    assert_eq!(corruption.name, "Corruption");
    assert_eq!(corruption.card_type, CardType::Power);
    assert_eq!(corruption.rarity, CardRarity::Rare);
    assert_eq!(corruption.cost, 3);
    assert_eq!(corruption.base_magic, 3);
    assert_eq!(corruption.target, CardTarget::SelfTarget);
    let mut corruption_plus = CombatCard::for_scenario(CardId::Corruption, 110);
    corruption_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&corruption_plus), Some(2));

    let dark_embrace = get_card_definition(CardId::DarkEmbrace);
    assert_eq!(dark_embrace.name, "Dark Embrace");
    assert_eq!(dark_embrace.card_type, CardType::Power);
    assert_eq!(dark_embrace.rarity, CardRarity::Uncommon);
    assert_eq!(dark_embrace.cost, 2);
    assert_eq!(dark_embrace.target, CardTarget::SelfTarget);
    let mut dark_embrace_plus = CombatCard::for_scenario(CardId::DarkEmbrace, 111);
    dark_embrace_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&dark_embrace_plus), Some(1));

    let demon_form = get_card_definition(CardId::DemonForm);
    assert_eq!(demon_form.name, "Demon Form");
    assert_eq!(demon_form.card_type, CardType::Power);
    assert_eq!(demon_form.rarity, CardRarity::Rare);
    assert_eq!(demon_form.cost, 3);
    assert_eq!(demon_form.base_magic, 2);
    assert_eq!(demon_form.target, CardTarget::None);
    assert_eq!(demon_form.upgrade_magic, 1);

    let disarm = get_card_definition(CardId::Disarm);
    assert_eq!(disarm.name, "Disarm");
    assert_eq!(disarm.card_type, CardType::Skill);
    assert_eq!(disarm.rarity, CardRarity::Uncommon);
    assert_eq!(disarm.cost, 1);
    assert_eq!(disarm.base_magic, 2);
    assert_eq!(disarm.target, CardTarget::Enemy);
    assert!(disarm.exhaust);
    assert_eq!(disarm.upgrade_magic, 1);
}

#[test]
fn ironclad_power_and_debuff_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();

    let corruption_actions = resolve_card_play(
        CardId::Corruption,
        &state,
        &CombatCard::for_scenario(CardId::Corruption, 112),
        None,
    );
    assert_eq!(corruption_actions.len(), 1);
    match &corruption_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Corruption);
            assert_eq!(*amount, -1);
        }
        other => panic!("Corruption should apply CorruptionPower once, got {other:?}"),
    }

    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Corruption,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let duplicate_corruption_actions = resolve_card_play(
        CardId::Corruption,
        &state,
        &CombatCard::for_scenario(CardId::Corruption, 113),
        None,
    );
    assert!(duplicate_corruption_actions.is_empty());

    let mut corruption_apply_state = crate::test_support::blank_test_combat();
    let mut hand_skill = CombatCard::for_scenario(CardId::Defend, 910);
    hand_skill.cost_modifier = 2;
    corruption_apply_state.zones.hand = vec![hand_skill];
    corruption_apply_state.zones.draw_pile =
        (vec![CombatCard::for_scenario(CardId::ShrugItOff, 911)]).into();
    corruption_apply_state.zones.discard_pile =
        vec![CombatCard::for_scenario(CardId::BurningPact, 912)].into();
    corruption_apply_state.zones.exhaust_pile =
        (vec![CombatCard::for_scenario(CardId::PowerThrough, 913)]).into();
    corruption_apply_state.zones.limbo = vec![CombatCard::for_scenario(CardId::TrueGrit, 914)];

    crate::content::cards::ironclad::corruption::corruption_on_apply(&mut corruption_apply_state);

    assert_eq!(corruption_apply_state.zones.hand[0].cost_for_turn, Some(0));
    assert_eq!(
        corruption_apply_state.zones.hand[0].cost_modifier, -1,
        "Java ApplyPowerAction applies Corruption with modifyCostForCombat(-9), mutating combat cost too"
    );
    assert_eq!(corruption_apply_state.zones.draw_pile[0].cost_modifier, -1);
    assert_eq!(
        corruption_apply_state.zones.discard_pile[0].cost_modifier,
        -1
    );
    assert_eq!(
        corruption_apply_state.zones.exhaust_pile[0].cost_modifier,
        -1
    );
    assert_eq!(
        corruption_apply_state.zones.draw_pile[0].cost_for_turn,
        Some(0)
    );
    assert_eq!(
        corruption_apply_state.zones.discard_pile[0].cost_for_turn,
        Some(0)
    );
    assert_eq!(
        corruption_apply_state.zones.exhaust_pile[0].cost_for_turn,
        Some(0)
    );
    assert_eq!(
        corruption_apply_state.zones.limbo[0].cost_for_turn, None,
        "Java ApplyPowerAction's Corruption constructor scans hand/draw/discard/exhaust, not limbo"
    );

    let dark_embrace_actions = resolve_card_play(
        CardId::DarkEmbrace,
        &state,
        &CombatCard::for_scenario(CardId::DarkEmbrace, 114),
        None,
    );
    assert_eq!(dark_embrace_actions.len(), 1);
    match &dark_embrace_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::DarkEmbrace);
            assert_eq!(*amount, 1);
        }
        other => panic!("Dark Embrace should apply DarkEmbracePower(1), got {other:?}"),
    }

    let mut demon_form_plus = CombatCard::for_scenario(CardId::DemonForm, 115);
    demon_form_plus.upgrades = 1;
    let demon_form_plus_actions =
        resolve_card_play(CardId::DemonForm, &state, &demon_form_plus, None);
    assert_eq!(demon_form_plus_actions.len(), 1);
    match &demon_form_plus_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::DemonForm);
            assert_eq!(*amount, 3);
        }
        other => panic!("Demon Form+ should apply upgraded DemonFormPower, got {other:?}"),
    }

    let mut disarm_plus = CombatCard::for_scenario(CardId::Disarm, 116);
    disarm_plus.upgrades = 1;
    let disarm_plus_actions = resolve_card_play(CardId::Disarm, &state, &disarm_plus, Some(23));
    assert_eq!(disarm_plus_actions.len(), 1);
    match &disarm_plus_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 23);
            assert_eq!(*power_id, PowerId::Strength);
            assert_eq!(*amount, -3);
        }
        other => panic!("Disarm+ should apply negative Strength to target, got {other:?}"),
    }
}

#[test]
fn dark_embrace_and_demon_form_power_hooks_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();
    let no_monster_dark_embrace = crate::content::powers::resolve_power_on_exhaust(
        PowerId::DarkEmbrace,
        &state,
        0,
        1,
        crate::runtime::combat::CardLineageId::constructed_scenario(117),
        CardId::Strike,
    );
    assert!(
        no_monster_dark_embrace.is_empty(),
        "Java skips Dark Embrace draw when monsters are basically dead"
    );

    let mut zero_hp_not_dying = crate::test_support::test_monster(EnemyId::JawWorm);
    zero_hp_not_dying.current_hp = 0;
    zero_hp_not_dying.is_dying = false;
    state.entities.monsters.push(zero_hp_not_dying);
    let zero_hp_dark_embrace = crate::content::powers::resolve_power_on_exhaust(
        PowerId::DarkEmbrace,
        &state,
        0,
        1,
        crate::runtime::combat::CardLineageId::constructed_scenario(118),
        CardId::Strike,
    );
    assert_eq!(
        zero_hp_dark_embrace.len(),
        1,
        "Java areMonstersBasicallyDead ignores currentHealth; Dark Embrace still draws until monsters are isDying/isEscaping"
    );

    state.entities.monsters[0].current_hp = 20;
    state.entities.monsters[0].is_dying = false;
    let dark_embrace_actions = crate::content::powers::resolve_power_on_exhaust(
        PowerId::DarkEmbrace,
        &state,
        0,
        1,
        crate::runtime::combat::CardLineageId::constructed_scenario(119),
        CardId::Strike,
    );
    assert_eq!(dark_embrace_actions.len(), 1);
    assert!(matches!(dark_embrace_actions[0], Action::DrawCards(1)));

    let demon_form_actions =
        crate::content::powers::resolve_power_on_post_draw(PowerId::DemonForm, &state, 0, 3);
    assert_eq!(demon_form_actions.len(), 1);
    match &demon_form_actions[0] {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Strength);
            assert_eq!(*amount, 3);
        }
        other => panic!("Demon Form post-draw hook should apply Strength, got {other:?}"),
    }
}

#[test]
fn corruption_action_construction_modifies_skill_costs_in_java_piles() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Defend, 120),
        CombatCard::for_scenario(CardId::Strike, 121),
    ];
    state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Armaments, 122)]).into();
    state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Disarm, 123)].into();
    state.zones.exhaust_pile = (vec![CombatCard::for_scenario(CardId::BurningPact, 124)]).into();

    state.queue_action_back(Action::ApplyPower {
        source: 0,
        target: 0,
        power_id: PowerId::Corruption,
        amount: -1,
    });

    assert_eq!(state.zones.hand[0].get_cost(), 0);
    assert_eq!(state.zones.hand[1].get_cost(), 1);
    assert_eq!(state.zones.draw_pile[0].get_cost(), 0);
    assert_eq!(state.zones.discard_pile[0].get_cost(), 0);
    assert_eq!(state.zones.exhaust_pile[0].get_cost(), 0);
}

#[test]
fn ironclad_copy_and_block_definitions_match_java_sources() {
    let double_tap = get_card_definition(CardId::DoubleTap);
    assert_eq!(double_tap.name, "Double Tap");
    assert_eq!(double_tap.card_type, CardType::Skill);
    assert_eq!(double_tap.rarity, CardRarity::Rare);
    assert_eq!(double_tap.cost, 1);
    assert_eq!(double_tap.base_magic, 1);
    assert_eq!(double_tap.target, CardTarget::SelfTarget);
    assert_eq!(double_tap.upgrade_magic, 1);

    let dropkick = get_card_definition(CardId::Dropkick);
    assert_eq!(dropkick.name, "Dropkick");
    assert_eq!(dropkick.card_type, CardType::Attack);
    assert_eq!(dropkick.rarity, CardRarity::Uncommon);
    assert_eq!(dropkick.cost, 1);
    assert_eq!(dropkick.base_damage, 5);
    assert_eq!(dropkick.target, CardTarget::Enemy);
    assert_eq!(dropkick.upgrade_damage, 3);

    let dual_wield = get_card_definition(CardId::DualWield);
    assert_eq!(dual_wield.name, "Dual Wield");
    assert_eq!(dual_wield.card_type, CardType::Skill);
    assert_eq!(dual_wield.rarity, CardRarity::Uncommon);
    assert_eq!(dual_wield.cost, 1);
    assert_eq!(dual_wield.base_magic, 1);
    assert_eq!(dual_wield.target, CardTarget::None);
    assert_eq!(dual_wield.upgrade_magic, 1);

    let entrench = get_card_definition(CardId::Entrench);
    assert_eq!(entrench.name, "Entrench");
    assert_eq!(entrench.card_type, CardType::Skill);
    assert_eq!(entrench.rarity, CardRarity::Uncommon);
    assert_eq!(entrench.cost, 2);
    assert_eq!(entrench.target, CardTarget::SelfTarget);
    let mut entrench_plus = CombatCard::for_scenario(CardId::Entrench, 130);
    entrench_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&entrench_plus), Some(1));
}

#[test]
fn ironclad_copy_and_block_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();

    let mut double_tap_plus = CombatCard::for_scenario(CardId::DoubleTap, 131);
    double_tap_plus.upgrades = 1;
    let double_tap_plus_actions =
        resolve_card_play(CardId::DoubleTap, &state, &double_tap_plus, None);
    assert_eq!(double_tap_plus_actions.len(), 1);
    match &double_tap_plus_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::DoubleTap);
            assert_eq!(*amount, 2);
        }
        other => panic!("Double Tap+ should apply DoubleTapPower(2), got {other:?}"),
    }

    let mut dropkick_plus = CombatCard::for_scenario(CardId::Dropkick, 132);
    dropkick_plus.upgrades = 1;
    let dropkick_plus_actions =
        resolve_card_play(CardId::Dropkick, &state, &dropkick_plus, Some(7));
    assert_eq!(dropkick_plus_actions.len(), 1);
    match &dropkick_plus_actions[0].action {
        Action::DropkickDamageAndEffect {
            target,
            damage_info,
        } => {
            assert_eq!(*target, 7);
            assert_eq!(damage_info.source, 0);
            assert_eq!(damage_info.target, 7);
            assert_eq!(damage_info.base, 8);
            assert_eq!(damage_info.output, 8);
            assert_eq!(damage_info.damage_type, DamageType::Normal);
        }
        other => panic!("Dropkick+ should emit DropkickAction equivalent, got {other:?}"),
    }

    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 133),
        CombatCard::for_scenario(CardId::Defend, 134),
    ];
    let mut dual_wield_plus = CombatCard::for_scenario(CardId::DualWield, 135);
    dual_wield_plus.upgrades = 1;
    let dual_wield_auto_actions =
        resolve_card_play(CardId::DualWield, &state, &dual_wield_plus, None);
    assert_eq!(dual_wield_auto_actions.len(), 1);
    match &dual_wield_auto_actions[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Strike);
            assert_eq!(*amount, 2);
        }
        other => panic!("Dual Wield+ should auto-copy sole valid card, got {other:?}"),
    }

    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 136),
        CombatCard::for_scenario(CardId::Inflame, 137),
        CombatCard::for_scenario(CardId::Defend, 138),
    ];
    let dual_wield_select_actions =
        resolve_card_play(CardId::DualWield, &state, &dual_wield_plus, None);
    assert_eq!(dual_wield_select_actions.len(), 1);
    match &dual_wield_select_actions[0].action {
        Action::SuspendForHandSelect {
            min,
            max,
            can_cancel,
            filter,
            reason,
        } => {
            assert_eq!(*min, 1);
            assert_eq!(*max, 1);
            assert!(!*can_cancel);
            assert_eq!(*filter, crate::state::HandSelectFilter::AttackOrPower);
            assert_eq!(*reason, crate::state::HandSelectReason::Copy { amount: 3 });
        }
        other => panic!("Dual Wield should open Attack/Power hand select, got {other:?}"),
    }

    state.entities.player.block = 14;
    let entrench_actions = resolve_card_play(
        CardId::Entrench,
        &state,
        &CombatCard::for_scenario(CardId::Entrench, 139),
        None,
    );
    assert_eq!(entrench_actions.len(), 1);
    assert!(matches!(
        entrench_actions[0].action,
        Action::DoubleBlock { target: 0 }
    ));

    state.entities.player.block = 9;
    crate::engine::action_handlers::damage::handle_double_block(0, &mut state);
    assert_eq!(
        state.entities.player.block, 18,
        "Java DoubleYourBlockAction reads currentBlock when the action updates"
    );
}

#[test]
fn dropkick_and_double_tap_action_hooks_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        7,
        vec![Power {
            power_type: PowerId::Vulnerable,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::damage::handle_dropkick(
        7,
        crate::runtime::action::DamageInfo {
            source: 0,
            target: 7,
            base: 5,
            output: 5,
            damage_type: DamageType::Normal,
            is_modified: false,
        },
        &mut state,
    );
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::Damage(crate::runtime::action::DamageInfo {
            target: 7,
            ..
        }))
    ));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::GainEnergy { amount: 1 })
    ));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::DrawCards(1))
    ));

    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::DoubleTap,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let mut strike = CombatCard::for_scenario(CardId::Strike, 140);
    strike.base_damage_mut = 99;
    crate::content::powers::ironclad::double_tap::on_use_card(&mut state, &strike, false, Some(7));
    assert_eq!(
        crate::content::powers::store::power_amount(&state, 0, PowerId::DoubleTap),
        0
    );
    match state.pop_next_action() {
        Some(Action::EnqueueCardPlay { item, in_front }) => {
            assert!(in_front);
            assert_eq!(item.card.id, CardId::Strike);
            assert_eq!(item.target, Some(7));
            assert!(item.ignore_energy_total);
            assert!(item.autoplay);
            assert!(item.purge_on_use);
            assert_eq!(
                item.card.lineage,
                crate::runtime::combat::CardLineageId::constructed_scenario(140),
                "Java DoubleTapPower uses makeSameInstanceOf(), preserving UUID"
            );
            assert_eq!(
                item.card.base_damage_mut, 0,
                "makeSameInstanceOf is a stat-equivalent copy, not a raw clone of transient evaluated damage"
            );
            assert_eq!(
                item.source,
                crate::runtime::combat::QueuedCardSource::DoubleTap
            );
        }
        other => panic!("Double Tap should enqueue a purge-on-use copy, got {other:?}"),
    }
}

#[test]
fn same_instance_replay_powers_preserve_card_lineage_like_java() {
    let mut burst_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut burst_state,
        0,
        vec![Power {
            power_type: PowerId::Burst,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let mut shrug = CombatCard::for_scenario(CardId::ShrugItOff, 920);
    shrug.base_block_mut = 99;
    crate::content::powers::silent::burst::on_use_card(&mut burst_state, &shrug, false, None);
    match burst_state.pop_next_action() {
        Some(Action::EnqueueCardPlay { item, .. }) => {
            assert_eq!(
                item.card.lineage,
                crate::runtime::combat::CardLineageId::constructed_scenario(920)
            );
            assert_eq!(item.card.base_block_mut, 0);
            assert_eq!(item.source, crate::runtime::combat::QueuedCardSource::Burst);
        }
        other => panic!("Burst should enqueue same-instance skill copy, got {other:?}"),
    }

    let mut duplication_state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut duplication_state,
        0,
        vec![Power {
            power_type: PowerId::DuplicationPower,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let mut strike = CombatCard::for_scenario(CardId::Strike, 921);
    strike.base_damage_mut = 99;
    crate::content::powers::core::duplication_power::on_use_card(
        &mut duplication_state,
        &strike,
        false,
        Some(7),
    );
    match duplication_state.pop_next_action() {
        Some(Action::EnqueueCardPlay { item, .. }) => {
            assert_eq!(
                item.card.lineage,
                crate::runtime::combat::CardLineageId::constructed_scenario(921)
            );
            assert_eq!(item.card.base_damage_mut, 0);
            assert_eq!(
                item.source,
                crate::runtime::combat::QueuedCardSource::Duplication
            );
        }
        other => panic!("DuplicationPower should enqueue same-instance copy, got {other:?}"),
    }
}

#[test]
fn double_tap_replays_x_cost_energy_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 0;
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::DoubleTap,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let mut whirlwind = CombatCard::for_scenario(CardId::Whirlwind, 922);
    whirlwind.energy_on_use = 3;
    crate::content::powers::ironclad::double_tap::on_use_card(&mut state, &whirlwind, false, None);

    let enqueue = state
        .pop_next_action()
        .expect("Double Tap should enqueue a same-instance Whirlwind");
    match &enqueue {
        Action::EnqueueCardPlay { item, .. } => {
            assert_eq!(item.card.energy_on_use, 0);
            assert_eq!(item.energy_on_use, 3);
        }
        other => panic!("Double Tap should enqueue Whirlwind, got {other:?}"),
    }
    crate::engine::action_handlers::execute_action(enqueue, &mut state);

    let flush = state
        .pop_next_action()
        .expect("queued replay should schedule a queue flush");
    assert!(matches!(flush, Action::FlushNextQueuedCard));
    crate::engine::action_handlers::execute_action(flush, &mut state);

    let direct_play = state
        .pop_next_action()
        .expect("queue flush should emit a direct card play");
    crate::engine::action_handlers::execute_action(direct_play, &mut state);

    let replayed_whirlwind = state
        .pop_next_action()
        .expect("replayed Whirlwind should retain its original X value");
    match replayed_whirlwind {
        Action::Whirlwind { energy_on_use, .. } => assert_eq!(energy_on_use, 3),
        other => panic!("replayed Whirlwind should emit WhirlwindAction, got {other:?}"),
    }
}

#[test]
fn rampage_growth_reaches_each_same_instance_replay_before_its_damage_is_calculated() {
    for (upgrades, growth) in [(0, 5), (1, 8)] {
        for powers in [
            vec![PowerId::DoubleTap],
            vec![PowerId::DuplicationPower],
            vec![PowerId::DoubleTap, PowerId::DuplicationPower],
        ] {
            let mut state = crate::test_support::blank_test_combat();
            state.turn.energy = 3;
            let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
            target.id = 7;
            target.current_hp = 200;
            target.max_hp = 200;
            state.entities.monsters = vec![target];
            let mut rampage = CombatCard::for_scenario(CardId::Rampage, 934);
            rampage.upgrades = upgrades;
            state.zones.hand = vec![rampage, CombatCard::for_scenario(CardId::Rampage, 935)];
            crate::content::powers::store::set_powers_for(
                &mut state,
                0,
                powers
                    .iter()
                    .map(|power_type| Power {
                        power_type: *power_type,
                        instance_id: None,
                        amount: 1,
                        extra_data: 0,
                        payload: PowerPayload::None,
                        just_applied: false,
                    })
                    .collect(),
            );
            crate::engine::action_handlers::cards::handle_play_card_from_hand(
                0,
                Some(7),
                &mut state,
            )
            .unwrap();
            let mut hits = Vec::new();
            let mut steps = 0;
            while let Some(action) = state.pop_next_action() {
                if let Action::Damage(info) = &action {
                    hits.push(info.output);
                }
                crate::engine::action_handlers::execute_action(action, &mut state);
                steps += 1;
                assert!(steps < 100, "same-instance replays must settle");
            }
            let plays = powers.len() as i32 + 1;
            let expected: Vec<_> = (0..plays).map(|index| 8 + index * growth).collect();
            assert_eq!(hits, expected, "upgrades={upgrades}, powers={powers:?}");
            assert_eq!(
                state.entities.monsters[0].current_hp,
                200 - expected.iter().sum::<i32>()
            );
            assert_eq!(state.turn.energy, 2);
            assert_eq!(
                state.zones.discard_pile.len(),
                1,
                "replay copies are purged"
            );
            assert_eq!(
                state.zones.discard_pile[0].base_damage_override,
                Some(8 + plays * growth)
            );
            assert_eq!(
                state.zones.hand[0].base_damage_override, None,
                "a different lineage must not grow"
            );
        }
    }
}

#[test]
fn double_tap_replays_fiend_fire_only_after_original_exhausts_the_hand_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 7;
    target.current_hp = 100;
    target.max_hp = 100;
    state.entities.monsters = vec![target];
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 930),
        CombatCard::for_scenario(CardId::Defend, 931),
        CombatCard::for_scenario(CardId::Bash, 932),
    ];
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::DoubleTap,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let fiend_fire = CombatCard::for_scenario(CardId::FiendFire, 933);
    state.queue_actions(resolve_card_play(
        CardId::FiendFire,
        &state,
        &fiend_fire,
        Some(7),
    ));
    crate::content::powers::ironclad::double_tap::on_use_card(
        &mut state,
        &fiend_fire,
        false,
        Some(7),
    );

    let mut fiend_fire_actions = 0;
    let mut damage_actions = 0;
    let mut steps = 0;
    while let Some(action) = state.pop_next_action() {
        match &action {
            Action::FiendFire { .. } => fiend_fire_actions += 1,
            Action::Damage(_) => damage_actions += 1,
            _ => {}
        }
        crate::engine::action_handlers::execute_action(action, &mut state);
        steps += 1;
        assert!(steps < 64, "Double Tap replay queue should settle");
    }

    assert_eq!(fiend_fire_actions, 2, "Double Tap still replays the Attack");
    assert_eq!(
        damage_actions, 3,
        "Java drains original FiendFireAction before replaying the card, so only the original sees the three-card hand"
    );
    assert!(state.zones.hand.is_empty());
    assert_eq!(state.entities.monsters[0].current_hp, 79);
}

#[test]
fn ironclad_exhaust_and_growth_definitions_match_java_sources() {
    let evolve = get_card_definition(CardId::Evolve);
    assert_eq!(evolve.card_type, CardType::Power);
    assert_eq!(evolve.rarity, CardRarity::Uncommon);
    assert_eq!(evolve.cost, 1);
    assert_eq!(evolve.base_magic, 1);
    assert_eq!(evolve.target, CardTarget::SelfTarget);
    assert_eq!(evolve.upgrade_magic, 1);

    let exhume = get_card_definition(CardId::Exhume);
    assert_eq!(exhume.card_type, CardType::Skill);
    assert_eq!(exhume.rarity, CardRarity::Rare);
    assert_eq!(exhume.cost, 1);
    assert_eq!(exhume.target, CardTarget::None);
    assert!(exhume.exhaust);
    let mut exhume_plus = CombatCard::for_scenario(CardId::Exhume, 150);
    exhume_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&exhume_plus), Some(0));

    let feed = get_card_definition(CardId::Feed);
    assert_eq!(feed.card_type, CardType::Attack);
    assert_eq!(feed.rarity, CardRarity::Rare);
    assert_eq!(feed.cost, 1);
    assert_eq!(feed.base_damage, 10);
    assert_eq!(feed.base_magic, 3);
    assert_eq!(feed.target, CardTarget::Enemy);
    assert!(feed.exhaust);
    assert!(feed.tags.contains(&CardTag::Healing));
    assert_eq!(feed.upgrade_damage, 2);
    assert_eq!(feed.upgrade_magic, 1);

    let feel_no_pain = get_card_definition(CardId::FeelNoPain);
    assert_eq!(feel_no_pain.card_type, CardType::Power);
    assert_eq!(feel_no_pain.rarity, CardRarity::Uncommon);
    assert_eq!(feel_no_pain.cost, 1);
    assert_eq!(feel_no_pain.base_magic, 3);
    assert_eq!(feel_no_pain.target, CardTarget::SelfTarget);
    assert_eq!(feel_no_pain.upgrade_magic, 1);
}

#[test]
fn ironclad_exhaust_and_growth_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();

    let mut evolve_plus = CombatCard::for_scenario(CardId::Evolve, 151);
    evolve_plus.upgrades = 1;
    let evolve_actions = resolve_card_play(CardId::Evolve, &state, &evolve_plus, None);
    assert_eq!(evolve_actions.len(), 1);
    match &evolve_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Evolve);
            assert_eq!(*amount, 2);
        }
        other => panic!("Evolve+ should apply upgraded EvolvePower, got {other:?}"),
    }

    let mut feel_no_pain_plus = CombatCard::for_scenario(CardId::FeelNoPain, 152);
    feel_no_pain_plus.upgrades = 1;
    let feel_no_pain_actions =
        resolve_card_play(CardId::FeelNoPain, &state, &feel_no_pain_plus, None);
    assert_eq!(feel_no_pain_actions.len(), 1);
    match &feel_no_pain_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::FeelNoPain);
            assert_eq!(*amount, 4);
        }
        other => panic!("Feel No Pain+ should apply upgraded power, got {other:?}"),
    }

    let mut feed_plus = CombatCard::for_scenario(CardId::Feed, 153);
    feed_plus.upgrades = 1;
    let feed_actions = resolve_card_play(CardId::Feed, &state, &feed_plus, Some(7));
    assert_eq!(feed_actions.len(), 1);
    match &feed_actions[0].action {
        Action::Feed {
            target,
            damage_info,
            max_hp_amount,
        } => {
            assert_eq!(*target, 7);
            assert_eq!(damage_info.source, 0);
            assert_eq!(damage_info.target, 7);
            assert_eq!(damage_info.base, 12);
            assert_eq!(damage_info.output, 12);
            assert_eq!(damage_info.damage_type, DamageType::Normal);
            assert_eq!(*max_hp_amount, 4);
        }
        other => panic!("Feed+ should emit upgraded FeedAction, got {other:?}"),
    }

    state.zones.exhaust_pile = (vec![CombatCard::for_scenario(CardId::Strike, 154)]).into();
    let exhume_actions = resolve_card_play(
        CardId::Exhume,
        &state,
        &CombatCard::for_scenario(CardId::Exhume, 155),
        None,
    );
    assert_eq!(exhume_actions.len(), 1);

    state.zones.exhaust_pile = (vec![
        CombatCard::for_scenario(CardId::Exhume, 156),
        CombatCard::for_scenario(CardId::Strike, 157),
    ])
    .into();
    let mut exhume_plus = CombatCard::for_scenario(CardId::Exhume, 158);
    exhume_plus.upgrades = 1;
    let select_actions = resolve_card_play(CardId::Exhume, &state, &exhume_plus, None);
    assert_eq!(
        exhume_actions, select_actions,
        "Exhume.use defers the singleton decision to action resolution"
    );
    assert_eq!(select_actions.len(), 1);
    match &select_actions[0].action {
        Action::SuspendForGridSelect {
            source_pile,
            min,
            max,
            can_cancel,
            filter,
            reason,
        } => {
            assert_eq!(*source_pile, crate::state::PileType::Exhaust);
            assert_eq!(*min, 1);
            assert_eq!(*max, 1);
            assert!(!*can_cancel);
            assert_eq!(*filter, crate::state::GridSelectFilter::NonExhume);
            assert_eq!(
                *reason,
                crate::state::GridSelectReason::Exhume { upgrade: false }
            );
        }
        other => panic!("Exhume should open non-cancellable exhaust grid select, got {other:?}"),
    }

    state.zones.hand = (0..10)
        .map(|offset| CombatCard::for_scenario(CardId::Defend, 170 + offset))
        .collect();
    state.zones.exhaust_pile = (vec![CombatCard::for_scenario(CardId::Strike, 181)]).into();
    let full_hand_actions = resolve_card_play(
        CardId::Exhume,
        &state,
        &CombatCard::for_scenario(CardId::Exhume, 182),
        None,
    );
    assert_eq!(
        full_hand_actions, select_actions,
        "Exhume.use defers the full-hand check to action resolution"
    );
}

#[test]
fn evolve_exhume_feed_and_feel_no_pain_hooks_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Wound, 190)];
    let evolve_draw = crate::content::powers::resolve_power_on_card_drawn(
        PowerId::Evolve,
        &state,
        0,
        2,
        crate::runtime::combat::CardLineageId::constructed_scenario(190),
    );
    assert_eq!(evolve_draw.as_slice(), &[Action::DrawCards(2)]);

    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::NoDraw,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let blocked_evolve_draw = crate::content::powers::resolve_power_on_card_drawn(
        PowerId::Evolve,
        &state,
        0,
        2,
        crate::runtime::combat::CardLineageId::constructed_scenario(190),
    );
    assert!(blocked_evolve_draw.is_empty());

    let feel_no_pain_block = crate::content::powers::resolve_power_on_exhaust(
        PowerId::FeelNoPain,
        &state,
        0,
        4,
        crate::runtime::combat::CardLineageId::constructed_scenario(191),
        CardId::Strike,
    );
    assert_eq!(
        feel_no_pain_block.as_slice(),
        &[Action::GainBlock {
            target: 0,
            amount: 4
        }]
    );

    let mut exhume_state = crate::test_support::blank_test_combat();
    exhume_state.zones.exhaust_pile = (vec![CombatCard::for_scenario(CardId::Defend, 192)]).into();
    crate::content::powers::store::set_powers_for(
        &mut exhume_state,
        0,
        vec![Power {
            power_type: PowerId::Corruption,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::cards::handle_exhume_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(192),
        false,
        &mut exhume_state,
    );
    assert!(exhume_state.zones.exhaust_pile.is_empty());
    assert_eq!(exhume_state.zones.hand.len(), 1);
    assert_eq!(exhume_state.zones.hand[0].id, CardId::Defend);
    assert_eq!(exhume_state.zones.hand[0].cost_for_turn, Some(0));

    let mut normal_feed_state = crate::test_support::blank_test_combat();
    let mut jaw_worm = crate::test_support::test_monster(EnemyId::JawWorm);
    jaw_worm.id = 31;
    jaw_worm.current_hp = 5;
    normal_feed_state.entities.monsters = vec![jaw_worm];
    crate::engine::action_handlers::damage::handle_feed(
        31,
        crate::runtime::action::DamageInfo {
            source: 0,
            target: 31,
            base: 10,
            output: 10,
            damage_type: DamageType::Normal,
            is_modified: false,
        },
        3,
        &mut normal_feed_state,
    );
    assert_eq!(normal_feed_state.entities.player.max_hp, 83);
    assert_eq!(normal_feed_state.entities.player.current_hp, 83);

    let mut minion_feed_state = crate::test_support::blank_test_combat();
    let mut minion = crate::test_support::test_monster(EnemyId::JawWorm);
    minion.id = 32;
    minion.current_hp = 5;
    minion_feed_state.entities.monsters = vec![minion];
    crate::content::powers::store::set_powers_for(
        &mut minion_feed_state,
        32,
        vec![Power {
            power_type: PowerId::Minion,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::damage::handle_feed(
        32,
        crate::runtime::action::DamageInfo {
            source: 0,
            target: 32,
            base: 10,
            output: 10,
            damage_type: DamageType::Normal,
            is_modified: false,
        },
        3,
        &mut minion_feed_state,
    );
    assert_eq!(minion_feed_state.entities.player.max_hp, 80);
    assert_eq!(minion_feed_state.entities.player.current_hp, 80);
}

#[test]
fn on_kill_card_rewards_ignore_minions_and_half_dead_targets_like_java_actions() {
    fn test_damage(target: usize) -> DamageInfo {
        DamageInfo {
            source: 0,
            target,
            base: 10,
            output: 10,
            damage_type: DamageType::Normal,
            is_modified: false,
        }
    }

    let mut greed_normal = crate::test_support::blank_test_combat();
    let starting_gold = greed_normal.entities.player.gold;
    let mut normal = crate::test_support::test_monster(EnemyId::JawWorm);
    normal.id = 41;
    normal.current_hp = 5;
    greed_normal.entities.monsters = vec![normal];
    crate::engine::action_handlers::damage::handle_hand_of_greed(
        41,
        test_damage(41),
        20,
        &mut greed_normal,
    );
    assert_eq!(greed_normal.entities.player.gold, starting_gold + 20);
    assert_eq!(greed_normal.entities.player.gold_delta_this_combat, 20);
    assert_eq!(
        greed_normal.pop_next_action(),
        None,
        "Java GreedAction calls player.gainGold inside the damage action, before clearPostCombatActions"
    );

    let mut greed_minion = crate::test_support::blank_test_combat();
    let minion_starting_gold = greed_minion.entities.player.gold;
    let mut minion = crate::test_support::test_monster(EnemyId::JawWorm);
    minion.id = 42;
    minion.current_hp = 5;
    greed_minion.entities.monsters = vec![minion];
    crate::content::powers::store::set_powers_for(
        &mut greed_minion,
        42,
        vec![Power {
            power_type: PowerId::Minion,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::damage::handle_hand_of_greed(
        42,
        test_damage(42),
        20,
        &mut greed_minion,
    );
    assert_eq!(greed_minion.entities.player.gold, minion_starting_gold);
    assert_eq!(greed_minion.pop_next_action(), None);

    let mut dagger_normal = crate::test_support::blank_test_combat();
    let mut dagger_target = crate::test_support::test_monster(EnemyId::JawWorm);
    dagger_target.id = 44;
    dagger_target.current_hp = 5;
    dagger_normal.entities.monsters = vec![dagger_target];
    dagger_normal.zones.hand = vec![CombatCard::for_scenario(CardId::RitualDagger, 900)];
    dagger_normal.meta.master_deck_snapshot = vec![dagger_normal.zones.hand[0].clone()].into();
    dagger_normal.zones.draw_pile =
        (vec![CombatCard::for_scenario(CardId::RitualDagger, 900)]).into();
    dagger_normal.zones.discard_pile =
        vec![CombatCard::for_scenario(CardId::RitualDagger, 900)].into();
    dagger_normal.zones.exhaust_pile =
        (vec![CombatCard::for_scenario(CardId::RitualDagger, 900)]).into();
    dagger_normal.zones.limbo = vec![CombatCard::for_scenario(CardId::RitualDagger, 900)];
    crate::engine::action_handlers::damage::handle_ritual_dagger(
        44,
        test_damage(44),
        3,
        crate::runtime::combat::CardLineageId::constructed_scenario(900),
        &mut dagger_normal,
    );
    assert_eq!(dagger_normal.zones.hand[0].misc_value, 18);
    assert_eq!(dagger_normal.zones.draw_pile[0].misc_value, 18);
    assert_eq!(dagger_normal.zones.discard_pile[0].misc_value, 18);
    assert_eq!(dagger_normal.zones.exhaust_pile[0].misc_value, 18);
    assert_eq!(dagger_normal.zones.limbo[0].misc_value, 18);
    assert_eq!(
        dagger_normal.meta.meta_changes,
        vec![crate::runtime::combat::MetaChange::ModifyCardMisc {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(900),
            amount: 3,
        }],
        "Java RitualDaggerAction also mutates the matching masterDeck card"
    );
    assert_eq!(
        dagger_normal.pop_next_action(),
        None,
        "Java RitualDaggerAction mutates every matching GetAllInBattleInstances card inside the damage action"
    );

    let mut dagger_half_dead = crate::test_support::blank_test_combat();
    let mut half_dead = crate::test_support::test_monster(EnemyId::AwakenedOne);
    half_dead.id = 43;
    half_dead.current_hp = 5;
    half_dead.half_dead = true;
    dagger_half_dead.entities.monsters = vec![half_dead];
    crate::engine::action_handlers::damage::handle_ritual_dagger(
        43,
        test_damage(43),
        3,
        crate::runtime::combat::CardLineageId::constructed_scenario(900),
        &mut dagger_half_dead,
    );
    assert_eq!(dagger_half_dead.pop_next_action(), None);
}

#[test]
fn ironclad_fire_and_strength_definitions_match_java_sources() {
    let fiend_fire = get_card_definition(CardId::FiendFire);
    assert_eq!(fiend_fire.card_type, CardType::Attack);
    assert_eq!(fiend_fire.rarity, CardRarity::Rare);
    assert_eq!(fiend_fire.cost, 2);
    assert_eq!(fiend_fire.base_damage, 7);
    assert_eq!(fiend_fire.target, CardTarget::Enemy);
    assert!(fiend_fire.exhaust);
    assert_eq!(fiend_fire.upgrade_damage, 3);

    let fire_breathing = get_card_definition(CardId::FireBreathing);
    assert_eq!(fire_breathing.card_type, CardType::Power);
    assert_eq!(fire_breathing.rarity, CardRarity::Uncommon);
    assert_eq!(fire_breathing.cost, 1);
    assert_eq!(fire_breathing.base_magic, 6);
    assert_eq!(fire_breathing.target, CardTarget::SelfTarget);
    assert_eq!(fire_breathing.upgrade_magic, 4);

    let flame_barrier = get_card_definition(CardId::FlameBarrier);
    assert_eq!(flame_barrier.card_type, CardType::Skill);
    assert_eq!(flame_barrier.rarity, CardRarity::Uncommon);
    assert_eq!(flame_barrier.cost, 2);
    assert_eq!(flame_barrier.base_block, 12);
    assert_eq!(flame_barrier.base_magic, 4);
    assert_eq!(flame_barrier.target, CardTarget::SelfTarget);
    assert_eq!(flame_barrier.upgrade_block, 4);
    assert_eq!(flame_barrier.upgrade_magic, 2);

    let flex = get_card_definition(CardId::Flex);
    assert_eq!(flex.card_type, CardType::Skill);
    assert_eq!(flex.rarity, CardRarity::Common);
    assert_eq!(flex.cost, 0);
    assert_eq!(flex.base_magic, 2);
    assert_eq!(flex.target, CardTarget::SelfTarget);
    assert_eq!(flex.upgrade_magic, 2);
}

#[test]
fn ironclad_fire_and_strength_runtime_actions_match_java_use_methods() {
    let state = crate::test_support::blank_test_combat();

    let mut fiend_fire_plus = CombatCard::for_scenario(CardId::FiendFire, 210);
    fiend_fire_plus.upgrades = 1;
    let fiend_fire_actions =
        resolve_card_play(CardId::FiendFire, &state, &fiend_fire_plus, Some(9));
    assert_eq!(fiend_fire_actions.len(), 1);
    match &fiend_fire_actions[0].action {
        Action::FiendFire {
            target,
            damage_info,
        } => {
            assert_eq!(*target, 9);
            assert_eq!(damage_info.source, 0);
            assert_eq!(damage_info.target, 9);
            assert_eq!(damage_info.base, 10);
            assert_eq!(damage_info.output, 10);
            assert_eq!(damage_info.damage_type, DamageType::Normal);
        }
        other => panic!("Fiend Fire+ should emit upgraded FiendFireAction, got {other:?}"),
    }

    let mut fire_breathing_plus = CombatCard::for_scenario(CardId::FireBreathing, 211);
    fire_breathing_plus.upgrades = 1;
    let fire_breathing_actions =
        resolve_card_play(CardId::FireBreathing, &state, &fire_breathing_plus, None);
    assert_eq!(fire_breathing_actions.len(), 1);
    match &fire_breathing_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::FireBreathing);
            assert_eq!(*amount, 10);
        }
        other => panic!("Fire Breathing+ should apply upgraded power, got {other:?}"),
    }

    let mut flame_barrier_plus = CombatCard::for_scenario(CardId::FlameBarrier, 212);
    flame_barrier_plus.upgrades = 1;
    let flame_barrier_actions =
        resolve_card_play(CardId::FlameBarrier, &state, &flame_barrier_plus, None);
    assert_eq!(flame_barrier_actions.len(), 2);
    assert!(matches!(
        flame_barrier_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 16
        }
    ));
    match &flame_barrier_actions[1].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::FlameBarrier);
            assert_eq!(*amount, 6);
        }
        other => panic!("Flame Barrier+ should apply upgraded power, got {other:?}"),
    }

    let mut flex_plus = CombatCard::for_scenario(CardId::Flex, 213);
    flex_plus.upgrades = 1;
    let flex_actions = resolve_card_play(CardId::Flex, &state, &flex_plus, None);
    assert_eq!(flex_actions.len(), 2);
    match &flex_actions[0].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::Strength);
            assert_eq!(*amount, 4);
        }
        other => panic!("Flex+ first action should apply Strength, got {other:?}"),
    }
    match &flex_actions[1].action {
        Action::ApplyPower {
            source,
            target,
            power_id,
            amount,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*target, 0);
            assert_eq!(*power_id, PowerId::LoseStrength);
            assert_eq!(*amount, 4);
        }
        other => panic!("Flex+ second action should apply LoseStrength, got {other:?}"),
    }
}

#[test]
fn fire_breathing_flame_barrier_and_fiend_fire_hooks_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 41;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 42;
    second.slot = 1;
    state.entities.monsters = vec![first, second];
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Injury, 220)];

    let fire_breathing_damage = crate::content::powers::resolve_power_on_card_drawn(
        PowerId::FireBreathing,
        &state,
        0,
        10,
        crate::runtime::combat::CardLineageId::constructed_scenario(220),
    );
    assert_eq!(fire_breathing_damage.len(), 1);
    match &fire_breathing_damage[0] {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, NO_SOURCE);
            assert_eq!(damages.as_slice(), &[10, 10]);
            assert_eq!(*damage_type, DamageType::Thorns);
            assert!(!*is_modified);
        }
        other => panic!("Fire Breathing should damage all enemies with THORNS, got {other:?}"),
    }

    state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 221)];
    let non_status_draw = crate::content::powers::resolve_power_on_card_drawn(
        PowerId::FireBreathing,
        &state,
        0,
        10,
        crate::runtime::combat::CardLineageId::constructed_scenario(221),
    );
    assert!(non_status_draw.is_empty());

    let flame_barrier_damage = crate::content::powers::resolve_power_on_attacked(
        PowerId::FlameBarrier,
        &state,
        0,
        7,
        41,
        DamageType::Normal,
        6,
    );
    assert_eq!(flame_barrier_damage.len(), 1);
    match &flame_barrier_damage[0] {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 41);
            assert_eq!(info.base, 6);
            assert_eq!(info.output, 6);
            assert_eq!(info.damage_type, DamageType::Thorns);
        }
        other => panic!("Flame Barrier should retaliate with THORNS damage, got {other:?}"),
    }
    assert!(crate::content::powers::resolve_power_on_attacked(
        PowerId::FlameBarrier,
        &state,
        0,
        7,
        41,
        DamageType::Thorns,
        6,
    )
    .is_empty());
    assert!(crate::content::powers::resolve_power_on_attacked(
        PowerId::FlameBarrier,
        &state,
        0,
        7,
        NO_SOURCE,
        DamageType::Normal,
        6,
    )
    .is_empty());

    let lose_strength_turn_end = crate::content::powers::resolve_power_at_end_of_turn(
        &Power {
            power_type: PowerId::LoseStrength,
            instance_id: None,
            amount: 4,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        },
        &mut state,
        0,
    );
    assert_eq!(
        lose_strength_turn_end.as_slice(),
        &[
            Action::ApplyPower {
                source: 0,
                target: 0,
                power_id: PowerId::Strength,
                amount: -4
            },
            Action::RemovePower {
                target: 0,
                power_id: PowerId::LoseStrength
            }
        ]
    );

    let mut fiend_fire_state = state.clone();
    fiend_fire_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 230),
        CombatCard::for_scenario(CardId::Defend, 231),
        CombatCard::for_scenario(CardId::Bash, 232),
    ];
    fiend_fire_state.entities.monsters[0].current_hp = 40;
    crate::engine::action_handlers::damage::handle_fiend_fire(
        41,
        crate::runtime::action::DamageInfo {
            source: 0,
            target: 41,
            base: 7,
            output: 7,
            damage_type: DamageType::Normal,
            is_modified: false,
        },
        &mut fiend_fire_state,
    );
    assert_eq!(
        fiend_fire_state.zones.hand.len(),
        3,
        "FiendFireAction queues random ExhaustAction instances; it does not drain hand immediately"
    );
    for _ in 0..3 {
        assert_eq!(
            fiend_fire_state.pop_next_action(),
            Some(Action::ExhaustRandomCard { amount: 1 })
        );
    }
    for _ in 0..3 {
        assert!(matches!(
            fiend_fire_state.pop_next_action(),
            Some(Action::Damage(crate::runtime::action::DamageInfo {
                target: 41,
                output: 7,
                ..
            }))
        ));
    }
}

#[test]
fn fiend_fire_executes_one_strength_scaled_hit_and_exhaust_hook_per_remaining_card() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 41;
    target.current_hp = 100;
    target.max_hp = 100;
    state.entities.monsters = vec![target];
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::FiendFire, 229),
        CombatCard::for_scenario(CardId::Strike, 230),
        CombatCard::for_scenario(CardId::Defend, 231),
        CombatCard::for_scenario(CardId::Bash, 232),
    ];
    state.zones.draw_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 240),
        CombatCard::for_scenario(CardId::Defend, 241),
        CombatCard::for_scenario(CardId::Strike, 242),
    ]
    .into();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![
            Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 3,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::FeelNoPain,
                instance_id: None,
                amount: 3,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::DarkEmbrace,
                instance_id: None,
                amount: 1,
                extra_data: 0,
                payload: PowerPayload::None,
                just_applied: false,
            },
        ],
    );

    let fiend_fire = state.zones.hand[0].clone();
    let actions = resolve_card_play(CardId::FiendFire, &state, &fiend_fire, Some(41));
    state.zones.hand.remove(0);
    state.queue_actions(actions);

    while let Some(action) = state.pop_next_action() {
        crate::engine::action_handlers::execute_action(action, &mut state);
    }

    assert_eq!(state.entities.monsters[0].current_hp, 70);
    assert_eq!(state.zones.exhaust_pile.len(), 3);
    assert_eq!(state.entities.player.block, 9);
    assert_eq!(state.zones.hand.len(), 3);
}

#[test]
fn ironclad_topdeck_and_strength_scaling_definitions_match_java_sources() {
    let ghostly_armor = get_card_definition(CardId::GhostlyArmor);
    assert_eq!(ghostly_armor.card_type, CardType::Skill);
    assert_eq!(ghostly_armor.rarity, CardRarity::Uncommon);
    assert_eq!(ghostly_armor.cost, 1);
    assert_eq!(ghostly_armor.base_block, 10);
    assert_eq!(ghostly_armor.target, CardTarget::SelfTarget);
    assert!(ghostly_armor.ethereal);
    assert_eq!(ghostly_armor.upgrade_block, 3);

    let havoc = get_card_definition(CardId::Havoc);
    assert_eq!(havoc.card_type, CardType::Skill);
    assert_eq!(havoc.rarity, CardRarity::Common);
    assert_eq!(havoc.cost, 1);
    assert_eq!(havoc.target, CardTarget::None);
    let mut havoc_plus = CombatCard::for_scenario(CardId::Havoc, 240);
    havoc_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&havoc_plus), Some(0));

    let headbutt = get_card_definition(CardId::Headbutt);
    assert_eq!(headbutt.card_type, CardType::Attack);
    assert_eq!(headbutt.rarity, CardRarity::Common);
    assert_eq!(headbutt.cost, 1);
    assert_eq!(headbutt.base_damage, 9);
    assert_eq!(headbutt.target, CardTarget::Enemy);
    assert_eq!(headbutt.upgrade_damage, 3);

    let heavy_blade = get_card_definition(CardId::HeavyBlade);
    assert_eq!(heavy_blade.card_type, CardType::Attack);
    assert_eq!(heavy_blade.rarity, CardRarity::Common);
    assert_eq!(heavy_blade.cost, 2);
    assert_eq!(heavy_blade.base_damage, 14);
    assert_eq!(heavy_blade.base_magic, 3);
    assert_eq!(heavy_blade.target, CardTarget::Enemy);
    assert_eq!(heavy_blade.upgrade_magic, 2);
}

#[test]
fn ironclad_topdeck_and_strength_scaling_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let mut ghostly_armor_plus = CombatCard::for_scenario(CardId::GhostlyArmor, 241);
    ghostly_armor_plus.upgrades = 1;
    let ghostly_armor_actions =
        resolve_card_play(CardId::GhostlyArmor, &state, &ghostly_armor_plus, None);
    assert_eq!(ghostly_armor_actions.len(), 1);
    assert!(matches!(
        ghostly_armor_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 13
        }
    ));

    let havoc_actions = resolve_card_play(
        CardId::Havoc,
        &state,
        &CombatCard::for_scenario(CardId::Havoc, 242),
        None,
    );
    assert_eq!(havoc_actions.len(), 1);
    assert!(matches!(
        havoc_actions[0].action,
        Action::PlayTopCard {
            target: None,
            exhaust: true
        }
    ));

    let mut headbutt_plus = CombatCard::for_scenario(CardId::Headbutt, 243);
    headbutt_plus.upgrades = 1;
    let headbutt_actions = resolve_card_play(CardId::Headbutt, &state, &headbutt_plus, Some(51));
    assert_eq!(headbutt_actions.len(), 2);
    match &headbutt_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 51);
            assert_eq!(info.base, 14);
            assert_eq!(info.output, 14);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Headbutt+ first action should be damage, got {other:?}"),
    }
    assert!(matches!(
        headbutt_actions[1].action,
        Action::DiscardPileToTopOfDeck
    ));

    let mut heavy_blade_plus = CombatCard::for_scenario(CardId::HeavyBlade, 244);
    heavy_blade_plus.upgrades = 1;
    let heavy_blade_actions =
        resolve_card_play(CardId::HeavyBlade, &state, &heavy_blade_plus, Some(51));
    assert_eq!(heavy_blade_actions.len(), 1);
    match &heavy_blade_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 51);
            assert_eq!(info.base, 24);
            assert_eq!(info.output, 24);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Heavy Blade+ should multiply Strength by 5, got {other:?}"),
    }
}

#[test]
fn headbutt_and_havoc_execution_helpers_match_java_sources() {
    let mut headbutt_state = crate::test_support::blank_test_combat();
    let mut jaw_worm = crate::test_support::test_monster(EnemyId::JawWorm);
    jaw_worm.id = 61;
    jaw_worm.current_hp = 0;
    jaw_worm.is_dying = true;
    headbutt_state.entities.monsters = vec![jaw_worm];
    headbutt_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Strike, 250)].into();
    crate::engine::action_handlers::cards::handle_discard_pile_to_top_of_deck(&mut headbutt_state);
    assert_eq!(headbutt_state.zones.discard_pile.len(), 1);
    assert!(headbutt_state.zones.draw_pile.is_empty());

    headbutt_state.entities.monsters[0].current_hp = 20;
    headbutt_state.entities.monsters[0].is_dying = false;
    crate::engine::action_handlers::cards::handle_discard_pile_to_top_of_deck(&mut headbutt_state);
    assert!(headbutt_state.zones.discard_pile.is_empty());
    assert_eq!(headbutt_state.zones.draw_pile.len(), 1);
    assert_eq!(
        headbutt_state.zones.draw_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(250)
    );

    headbutt_state.zones.discard_pile = vec![
        CombatCard::for_scenario(CardId::Strike, 251),
        CombatCard::for_scenario(CardId::Defend, 252),
    ]
    .into();
    crate::engine::action_handlers::cards::handle_discard_pile_to_top_of_deck(&mut headbutt_state);
    match headbutt_state.pop_next_action() {
        Some(Action::SuspendForGridSelect {
            source_pile,
            min,
            max,
            can_cancel,
            filter,
            reason,
        }) => {
            assert_eq!(source_pile, crate::state::PileType::Discard);
            assert_eq!(min, 1);
            assert_eq!(max, 1);
            assert!(!can_cancel);
            assert_eq!(filter, crate::state::GridSelectFilter::Any);
            assert_eq!(reason, crate::state::GridSelectReason::MoveToDrawPile);
        }
        other => {
            panic!("Headbutt should defer multi-card discard choice to grid select, got {other:?}")
        }
    }

    let mut havoc_state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 71;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 72;
    second.slot = 1;
    havoc_state.entities.monsters = vec![first, second];
    havoc_state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Strike, 260)].into();
    crate::engine::action_handlers::cards::handle_play_top_card(None, true, &mut havoc_state);
    assert!(matches!(
        havoc_state.pop_next_action(),
        Some(Action::EmptyDeckShuffle)
    ));
    match havoc_state.pop_next_action() {
        Some(Action::PlayTopCard { target, exhaust }) => {
            assert!(matches!(target, Some(71 | 72)));
            assert!(exhaust);
        }
        other => panic!("Havoc should lock random target before empty-deck shuffle, got {other:?}"),
    }

    let mut played_havoc_state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 73;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 74;
    second.slot = 1;
    played_havoc_state.entities.monsters = vec![first, second];
    played_havoc_state.zones.hand = vec![CombatCard::for_scenario(CardId::Havoc, 270)];
    played_havoc_state.zones.draw_pile =
        (vec![CombatCard::for_scenario(CardId::Clash, 271)]).into();
    assert_eq!(played_havoc_state.rng.card_random_rng.counter, 0);

    crate::engine::action_handlers::cards::handle_play_card_from_hand(
        0,
        None,
        &mut played_havoc_state,
    )
    .expect("Havoc should be playable");

    assert_eq!(
        played_havoc_state.rng.card_random_rng.counter, 1,
        "Havoc.use chooses its random monster target immediately"
    );
    match played_havoc_state.pop_next_action() {
        Some(Action::PlayTopCard { target, exhaust }) => {
            assert!(matches!(target, Some(73 | 74)));
            assert!(exhaust);
        }
        other => {
            panic!("played Havoc should enqueue PlayTopCard with locked target, got {other:?}")
        }
    }

    let mut mayhem_state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 75;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 76;
    second.slot = 1;
    mayhem_state.entities.monsters = vec![first, second];
    let mayhem_actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::MayhemPower,
        &mut mayhem_state,
        0,
        1,
    );
    assert_eq!(mayhem_actions.len(), 1);
    assert!(matches!(
        mayhem_actions[0],
        Action::QueuePlayTopCardToBottom {
            target: None,
            exhaust: false
        }
    ));
    assert_eq!(mayhem_state.rng.card_random_rng.counter, 0);
    crate::engine::action_handlers::execute_action(mayhem_actions[0].clone(), &mut mayhem_state);
    assert_eq!(
        mayhem_state.rng.card_random_rng.counter, 1,
        "MayhemPower wrapper chooses its random monster target before queuing PlayTopCardAction"
    );
    match mayhem_state.pop_next_action() {
        Some(Action::PlayTopCard { target, exhaust }) => {
            assert!(matches!(target, Some(75 | 76)));
            assert!(!exhaust);
        }
        other => {
            panic!("Mayhem wrapper should enqueue PlayTopCard with locked target, got {other:?}")
        }
    }
}

#[test]
fn ironclad_hp_loss_and_generated_attack_definitions_match_java_sources() {
    let hemokinesis = get_card_definition(CardId::Hemokinesis);
    assert_eq!(hemokinesis.card_type, CardType::Attack);
    assert_eq!(hemokinesis.rarity, CardRarity::Uncommon);
    assert_eq!(hemokinesis.cost, 1);
    assert_eq!(hemokinesis.base_damage, 15);
    assert_eq!(hemokinesis.base_magic, 2);
    assert_eq!(hemokinesis.target, CardTarget::Enemy);
    assert_eq!(hemokinesis.upgrade_damage, 5);

    let immolate = get_card_definition(CardId::Immolate);
    assert_eq!(immolate.card_type, CardType::Attack);
    assert_eq!(immolate.rarity, CardRarity::Rare);
    assert_eq!(immolate.cost, 2);
    assert_eq!(immolate.base_damage, 21);
    assert_eq!(immolate.target, CardTarget::AllEnemy);
    assert!(immolate.is_multi_damage);
    assert_eq!(immolate.upgrade_damage, 7);

    let impervious = get_card_definition(CardId::Impervious);
    assert_eq!(impervious.card_type, CardType::Skill);
    assert_eq!(impervious.rarity, CardRarity::Rare);
    assert_eq!(impervious.cost, 2);
    assert_eq!(impervious.base_block, 30);
    assert_eq!(impervious.target, CardTarget::SelfTarget);
    assert!(impervious.exhaust);
    assert_eq!(impervious.upgrade_block, 10);

    let infernal_blade = get_card_definition(CardId::InfernalBlade);
    assert_eq!(infernal_blade.card_type, CardType::Skill);
    assert_eq!(infernal_blade.rarity, CardRarity::Uncommon);
    assert_eq!(infernal_blade.cost, 1);
    assert_eq!(infernal_blade.target, CardTarget::None);
    assert!(infernal_blade.exhaust);
    let mut infernal_blade_plus = CombatCard::for_scenario(CardId::InfernalBlade, 270);
    infernal_blade_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&infernal_blade_plus), Some(0));
}

#[test]
fn ironclad_hp_loss_and_generated_attack_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 81;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 82;
    second.slot = 1;
    state.entities.monsters = vec![first, second];
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let mut hemokinesis_plus = CombatCard::for_scenario(CardId::Hemokinesis, 271);
    hemokinesis_plus.upgrades = 1;
    let hemokinesis_actions =
        resolve_card_play(CardId::Hemokinesis, &state, &hemokinesis_plus, Some(81));
    assert_eq!(hemokinesis_actions.len(), 2);
    assert!(matches!(
        hemokinesis_actions[0].action,
        Action::LoseHp {
            target: 0,
            amount: 2,
            triggers_rupture: true
        }
    ));
    match &hemokinesis_actions[1].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 81);
            assert_eq!(info.base, 22);
            assert_eq!(info.output, 22);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Hemokinesis+ should damage after HP loss, got {other:?}"),
    }

    let mut immolate_plus = CombatCard::for_scenario(CardId::Immolate, 272);
    immolate_plus.upgrades = 1;
    let immolate_actions = resolve_card_play(CardId::Immolate, &state, &immolate_plus, None);
    assert_eq!(immolate_actions.len(), 2);
    match &immolate_actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(damages.as_slice(), &[30, 30]);
            assert_eq!(*damage_type, DamageType::Normal);
            assert!(!*is_modified);
        }
        other => panic!("Immolate+ should damage all enemies, got {other:?}"),
    }
    assert!(matches!(
        immolate_actions[1].action,
        Action::MakeTempCardInDiscard {
            card_id: CardId::Burn,
            amount: 1,
            upgraded: false
        }
    ));

    let mut impervious_plus = CombatCard::for_scenario(CardId::Impervious, 273);
    impervious_plus.upgrades = 1;
    let impervious_actions = resolve_card_play(CardId::Impervious, &state, &impervious_plus, None);
    assert_eq!(impervious_actions.len(), 1);
    assert!(matches!(
        impervious_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 40
        }
    ));

    let infernal_blade_actions = resolve_card_play(
        CardId::InfernalBlade,
        &state,
        &CombatCard::for_scenario(CardId::InfernalBlade, 274),
        None,
    );
    assert_eq!(infernal_blade_actions.len(), 1);
    assert!(matches!(
        infernal_blade_actions[0].action,
        Action::MakeRandomCardInHand {
            card_type: Some(CardType::Attack),
            cost_for_turn: Some(0)
        }
    ));

    let mut play_state = state.clone();
    play_state.zones.hand = vec![CombatCard::for_scenario(CardId::InfernalBlade, 275)];
    assert_eq!(play_state.rng.card_random_rng.counter, 0);
    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut play_state)
        .expect("Infernal Blade should be playable");
    assert_eq!(
        play_state.rng.card_random_rng.counter, 1,
        "Java InfernalBlade.use samples the random attack before queuing MakeTempCardInHandAction"
    );
    match play_state.pop_next_action() {
        Some(Action::MakeConstructedCopyInHand { original, amount }) => {
            assert_eq!(amount, 1);
            assert_eq!(
                crate::content::cards::get_card_definition(original.id).card_type,
                CardType::Attack
            );
            assert_eq!(original.cost_for_turn_java(), 0);
        }
        other => panic!("Infernal Blade should queue a concrete generated attack, got {other:?}"),
    }
}

#[test]
fn ironclad_power_and_hybrid_attack_definitions_match_java_sources() {
    let inflame = get_card_definition(CardId::Inflame);
    assert_eq!(inflame.card_type, CardType::Power);
    assert_eq!(inflame.rarity, CardRarity::Uncommon);
    assert_eq!(inflame.cost, 1);
    assert_eq!(inflame.base_magic, 2);
    assert_eq!(inflame.target, CardTarget::SelfTarget);
    assert_eq!(inflame.upgrade_magic, 1);

    let intimidate = get_card_definition(CardId::Intimidate);
    assert_eq!(intimidate.card_type, CardType::Skill);
    assert_eq!(intimidate.rarity, CardRarity::Uncommon);
    assert_eq!(intimidate.cost, 0);
    assert_eq!(intimidate.base_magic, 1);
    assert_eq!(intimidate.target, CardTarget::AllEnemy);
    assert!(intimidate.exhaust);
    assert_eq!(intimidate.upgrade_magic, 1);

    let iron_wave = get_card_definition(CardId::IronWave);
    assert_eq!(iron_wave.card_type, CardType::Attack);
    assert_eq!(iron_wave.rarity, CardRarity::Common);
    assert_eq!(iron_wave.cost, 1);
    assert_eq!(iron_wave.base_damage, 5);
    assert_eq!(iron_wave.base_block, 5);
    assert_eq!(iron_wave.target, CardTarget::Enemy);
    assert_eq!(iron_wave.upgrade_damage, 2);
    assert_eq!(iron_wave.upgrade_block, 2);

    let juggernaut = get_card_definition(CardId::Juggernaut);
    assert_eq!(juggernaut.card_type, CardType::Power);
    assert_eq!(juggernaut.rarity, CardRarity::Rare);
    assert_eq!(juggernaut.cost, 2);
    assert_eq!(juggernaut.base_magic, 5);
    assert_eq!(juggernaut.target, CardTarget::SelfTarget);
    assert_eq!(juggernaut.upgrade_magic, 2);
}

#[test]
fn ironclad_power_and_hybrid_attack_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 91;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 92;
    second.slot = 1;
    state.entities.monsters = vec![first, second];
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let mut inflame_plus = CombatCard::for_scenario(CardId::Inflame, 280);
    inflame_plus.upgrades = 1;
    let inflame_actions = resolve_card_play(CardId::Inflame, &state, &inflame_plus, None);
    assert_eq!(inflame_actions.len(), 1);
    assert!(matches!(
        inflame_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 3
        }
    ));

    let mut intimidate_plus = CombatCard::for_scenario(CardId::Intimidate, 281);
    intimidate_plus.upgrades = 1;
    let intimidate_actions = resolve_card_play(CardId::Intimidate, &state, &intimidate_plus, None);
    assert_eq!(intimidate_actions.len(), 2);
    for (action, expected_target) in intimidate_actions.iter().zip([91, 92]) {
        assert!(matches!(
            action.action,
            Action::ApplyPower {
                source: 0,
                target,
                power_id: PowerId::Weak,
                amount: 2
            } if target == expected_target
        ));
    }

    let mut iron_wave_plus = CombatCard::for_scenario(CardId::IronWave, 282);
    iron_wave_plus.upgrades = 1;
    let iron_wave_actions = resolve_card_play(CardId::IronWave, &state, &iron_wave_plus, Some(91));
    assert_eq!(iron_wave_actions.len(), 2);
    assert!(matches!(
        iron_wave_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 7
        }
    ));
    match &iron_wave_actions[1].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 91);
            assert_eq!(info.base, 9);
            assert_eq!(info.output, 9);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Iron Wave+ should gain block then deal damage, got {other:?}"),
    }

    let mut juggernaut_plus = CombatCard::for_scenario(CardId::Juggernaut, 283);
    juggernaut_plus.upgrades = 1;
    let juggernaut_actions = resolve_card_play(CardId::Juggernaut, &state, &juggernaut_plus, None);
    assert_eq!(juggernaut_actions.len(), 1);
    assert!(matches!(
        juggernaut_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Juggernaut,
            amount: 7
        }
    ));
}

#[test]
fn intimidate_and_shockwave_enqueue_apply_power_for_every_monster_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    let mut dying = crate::test_support::test_monster(EnemyId::JawWorm);
    dying.id = 910;
    dying.current_hp = 0;
    dying.is_dying = true;
    let mut escaped = crate::test_support::test_monster(EnemyId::Cultist);
    escaped.id = 911;
    escaped.is_escaped = true;
    state.entities.monsters = vec![dying, escaped];

    let mut intimidate_plus = CombatCard::for_scenario(CardId::Intimidate, 912);
    intimidate_plus.upgrades = 1;
    let intimidate_actions = resolve_card_play(CardId::Intimidate, &state, &intimidate_plus, None);
    assert_eq!(
        intimidate_actions.len(),
        2,
        "Java Intimidate loops over monsters.monsters and leaves dead/escaped filtering to ApplyPowerAction"
    );
    assert!(matches!(
        intimidate_actions[0].action,
        Action::ApplyPower {
            target: 910,
            power_id: PowerId::Weak,
            amount: 2,
            ..
        }
    ));
    assert!(matches!(
        intimidate_actions[1].action,
        Action::ApplyPower {
            target: 911,
            power_id: PowerId::Weak,
            amount: 2,
            ..
        }
    ));

    let mut shockwave_plus = CombatCard::for_scenario(CardId::Shockwave, 913);
    shockwave_plus.upgrades = 1;
    let shockwave_actions = resolve_card_play(CardId::Shockwave, &state, &shockwave_plus, None);
    assert_eq!(
        shockwave_actions.len(),
        4,
        "Java Shockwave loops over monsters.monsters and leaves dead/escaped filtering to ApplyPowerAction"
    );
    assert!(matches!(
        shockwave_actions[0].action,
        Action::ApplyPower {
            target: 910,
            power_id: PowerId::Weak,
            amount: 5,
            ..
        }
    ));
    assert!(matches!(
        shockwave_actions[1].action,
        Action::ApplyPower {
            target: 910,
            power_id: PowerId::Vulnerable,
            amount: 5,
            ..
        }
    ));
    assert!(matches!(
        shockwave_actions[2].action,
        Action::ApplyPower {
            target: 911,
            power_id: PowerId::Weak,
            amount: 5,
            ..
        }
    ));
    assert!(matches!(
        shockwave_actions[3].action,
        Action::ApplyPower {
            target: 911,
            power_id: PowerId::Vulnerable,
            amount: 5,
            ..
        }
    ));
}

#[test]
fn juggernaut_block_hook_matches_java_source() {
    let state = crate::test_support::blank_test_combat();
    let actions =
        crate::content::powers::resolve_power_on_block_gained(PowerId::Juggernaut, &state, 0, 7, 5);
    assert_eq!(actions.len(), 1);
    match &actions[0] {
        Action::DamageRandomEnemy {
            source,
            base_damage,
            damage_type,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(*base_damage, 7);
            assert_eq!(*damage_type, DamageType::Thorns);
        }
        other => {
            panic!("Juggernaut should queue random THORNS damage on block gain, got {other:?}")
        }
    }
}

#[test]
fn ironclad_limit_and_strike_scaling_definitions_match_java_sources() {
    let limit_break = get_card_definition(CardId::LimitBreak);
    assert_eq!(limit_break.card_type, CardType::Skill);
    assert_eq!(limit_break.rarity, CardRarity::Rare);
    assert_eq!(limit_break.cost, 1);
    assert_eq!(limit_break.target, CardTarget::SelfTarget);
    assert!(limit_break.exhaust);
    let mut limit_break_plus = CombatCard::for_scenario(CardId::LimitBreak, 290);
    limit_break_plus.upgrades = 1;
    assert!(!exhausts_when_played(&limit_break_plus));

    let metallicize = get_card_definition(CardId::Metallicize);
    assert_eq!(metallicize.card_type, CardType::Power);
    assert_eq!(metallicize.rarity, CardRarity::Uncommon);
    assert_eq!(metallicize.cost, 1);
    assert_eq!(metallicize.base_magic, 3);
    assert_eq!(metallicize.target, CardTarget::SelfTarget);
    assert_eq!(metallicize.upgrade_magic, 1);

    let offering = get_card_definition(CardId::Offering);
    assert_eq!(offering.card_type, CardType::Skill);
    assert_eq!(offering.rarity, CardRarity::Rare);
    assert_eq!(offering.cost, 0);
    assert_eq!(offering.base_magic, 3);
    assert_eq!(offering.target, CardTarget::SelfTarget);
    assert!(offering.exhaust);
    assert_eq!(offering.upgrade_magic, 2);

    let perfected_strike = get_card_definition(CardId::PerfectedStrike);
    assert_eq!(perfected_strike.card_type, CardType::Attack);
    assert_eq!(perfected_strike.rarity, CardRarity::Common);
    assert_eq!(perfected_strike.cost, 2);
    assert_eq!(perfected_strike.base_damage, 6);
    assert_eq!(perfected_strike.base_magic, 2);
    assert_eq!(perfected_strike.target, CardTarget::Enemy);
    assert!(perfected_strike.tags.contains(&CardTag::Strike));
    assert_eq!(perfected_strike.upgrade_magic, 1);
}

#[test]
fn ironclad_limit_and_strike_scaling_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    let mut perfected_strike_plus = CombatCard::for_scenario(CardId::PerfectedStrike, 291);
    perfected_strike_plus.upgrades = 1;
    state.zones.hand = vec![
        perfected_strike_plus.clone(),
        CombatCard::for_scenario(CardId::Strike, 292),
    ];
    state.zones.draw_pile = (vec![CombatCard::for_scenario(CardId::Strike, 293)]).into();
    state.zones.discard_pile = vec![CombatCard::for_scenario(CardId::Strike, 294)].into();
    state.zones.limbo = vec![CombatCard::for_scenario(CardId::Strike, 295)];

    let perfected_actions = resolve_card_play(
        CardId::PerfectedStrike,
        &state,
        &perfected_strike_plus,
        Some(101),
    );
    assert_eq!(perfected_actions.len(), 1);
    match &perfected_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 101);
            assert_eq!(info.base, 18);
            assert_eq!(info.output, 18);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!(
            "Perfected Strike+ should count hand/draw/discard Strike cards only, got {other:?}"
        ),
    }

    let limit_break_actions = resolve_card_play(
        CardId::LimitBreak,
        &state,
        &CombatCard::for_scenario(CardId::LimitBreak, 296),
        None,
    );
    assert_eq!(limit_break_actions.len(), 1);
    assert!(matches!(limit_break_actions[0].action, Action::LimitBreak));

    let mut metallicize_plus = CombatCard::for_scenario(CardId::Metallicize, 297);
    metallicize_plus.upgrades = 1;
    let metallicize_actions =
        resolve_card_play(CardId::Metallicize, &state, &metallicize_plus, None);
    assert_eq!(metallicize_actions.len(), 1);
    assert!(matches!(
        metallicize_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Metallicize,
            amount: 4
        }
    ));

    let mut offering_plus = CombatCard::for_scenario(CardId::Offering, 298);
    offering_plus.upgrades = 1;
    let offering_actions = resolve_card_play(CardId::Offering, &state, &offering_plus, None);
    assert_eq!(offering_actions.len(), 3);
    assert!(matches!(
        offering_actions[0].action,
        Action::LoseHp {
            target: 0,
            amount: 6,
            triggers_rupture: true
        }
    ));
    assert!(matches!(
        offering_actions[1].action,
        Action::GainEnergy { amount: 2 }
    ));
    assert!(matches!(offering_actions[2].action, Action::DrawCards(5)));
}

#[test]
fn limit_break_and_metallicize_hooks_match_java_sources() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 3,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::damage::handle_limit_break(&mut state);
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 3,
        }),
        "Java LimitBreakAction queues ApplyPowerAction to top instead of applying Strength inline"
    );
    crate::engine::action_handlers::powers::handle_apply_power(
        0,
        0,
        PowerId::Strength,
        3,
        &mut state,
    );
    assert_eq!(
        crate::content::powers::store::power_amount(&state, 0, PowerId::Strength),
        6
    );

    let metallicize_block = crate::content::powers::resolve_power_at_end_of_turn(
        &Power {
            power_type: PowerId::Metallicize,
            instance_id: None,
            amount: 4,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        },
        &mut state,
        0,
    );
    assert_eq!(
        metallicize_block.as_slice(),
        &[Action::GainBlock {
            target: 0,
            amount: 4
        }]
    );
}

#[test]
fn ironclad_multi_hit_and_rage_definitions_match_java_sources() {
    let pommel = get_card_definition(CardId::PommelStrike);
    assert_eq!(pommel.card_type, CardType::Attack);
    assert_eq!(pommel.rarity, CardRarity::Common);
    assert_eq!(pommel.cost, 1);
    assert_eq!(pommel.base_damage, 9);
    assert_eq!(pommel.base_magic, 1);
    assert_eq!(pommel.target, CardTarget::Enemy);
    assert!(pommel.tags.contains(&CardTag::Strike));
    assert_eq!(pommel.upgrade_damage, 1);
    assert_eq!(pommel.upgrade_magic, 1);

    let power_through = get_card_definition(CardId::PowerThrough);
    assert_eq!(power_through.card_type, CardType::Skill);
    assert_eq!(power_through.rarity, CardRarity::Uncommon);
    assert_eq!(power_through.cost, 1);
    assert_eq!(power_through.base_block, 15);
    assert_eq!(power_through.target, CardTarget::SelfTarget);
    assert_eq!(power_through.upgrade_block, 5);

    let pummel = get_card_definition(CardId::Pummel);
    assert_eq!(pummel.card_type, CardType::Attack);
    assert_eq!(pummel.rarity, CardRarity::Uncommon);
    assert_eq!(pummel.cost, 1);
    assert_eq!(pummel.base_damage, 2);
    assert_eq!(pummel.base_magic, 4);
    assert_eq!(pummel.target, CardTarget::Enemy);
    assert!(pummel.exhaust);
    assert_eq!(pummel.upgrade_magic, 1);

    let rage = get_card_definition(CardId::Rage);
    assert_eq!(rage.card_type, CardType::Skill);
    assert_eq!(rage.rarity, CardRarity::Uncommon);
    assert_eq!(rage.cost, 0);
    assert_eq!(rage.base_magic, 3);
    assert_eq!(rage.target, CardTarget::SelfTarget);
    assert_eq!(rage.upgrade_magic, 2);
}

#[test]
fn ironclad_multi_hit_and_rage_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let mut pommel_plus = CombatCard::for_scenario(CardId::PommelStrike, 300);
    pommel_plus.upgrades = 1;
    let pommel_actions = resolve_card_play(CardId::PommelStrike, &state, &pommel_plus, Some(111));
    assert_eq!(pommel_actions.len(), 2);
    match &pommel_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 111);
            assert_eq!(info.base, 11);
            assert_eq!(info.output, 11);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Pommel Strike+ should damage first, got {other:?}"),
    }
    assert!(matches!(pommel_actions[1].action, Action::DrawCards(2)));

    let mut power_through_plus = CombatCard::for_scenario(CardId::PowerThrough, 301);
    power_through_plus.upgrades = 1;
    let power_through_actions =
        resolve_card_play(CardId::PowerThrough, &state, &power_through_plus, None);
    assert_eq!(power_through_actions.len(), 2);
    match &power_through_actions[0].action {
        Action::MakeConstructedCopyInHand { original, amount } => {
            assert_eq!(original.id, CardId::Wound);
            assert_eq!(*amount, 2);
        }
        other => panic!("Power Through should generate constructed Wounds, got {other:?}"),
    }
    assert!(matches!(
        power_through_actions[1].action,
        Action::GainBlock {
            target: 0,
            amount: 20
        }
    ));

    let mut pummel_plus = CombatCard::for_scenario(CardId::Pummel, 302);
    pummel_plus.upgrades = 1;
    let pummel_actions = resolve_card_play(CardId::Pummel, &state, &pummel_plus, Some(111));
    assert_eq!(pummel_actions.len(), 5);
    for (index, action) in pummel_actions.into_iter().enumerate() {
        match action.action {
            Action::PummelDamage(info) if index < 4 => {
                assert_eq!(info.source, 0);
                assert_eq!(info.target, 111);
                assert_eq!(info.base, 3);
                assert_eq!(info.output, 3);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            Action::Damage(info) if index == 4 => {
                assert_eq!(info.source, 0);
                assert_eq!(info.target, 111);
                assert_eq!(info.base, 3);
                assert_eq!(info.output, 3);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            other => panic!(
                "Java Pummel+ should emit four PummelDamageAction hits and one final DamageAction, got {other:?} at index {index}"
            ),
        }
    }

    let mut rage_plus = CombatCard::for_scenario(CardId::Rage, 303);
    rage_plus.upgrades = 1;
    let rage_actions = resolve_card_play(CardId::Rage, &state, &rage_plus, None);
    assert_eq!(rage_actions.len(), 1);
    assert!(matches!(
        rage_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Rage,
            amount: 5
        }
    ));
}

#[test]
fn rage_power_hooks_match_java_source() {
    let mut state = crate::test_support::blank_test_combat();
    let strike = CombatCard::for_scenario(CardId::Strike, 310);
    let defend = CombatCard::for_scenario(CardId::Defend, 311);
    let rage_block =
        crate::content::powers::resolve_power_on_card_played(PowerId::Rage, &state, 0, &strike, 5);
    assert_eq!(
        rage_block.as_slice(),
        &[Action::GainBlock {
            target: 0,
            amount: 5
        }]
    );
    assert!(crate::content::powers::resolve_power_on_card_played(
        PowerId::Rage,
        &state,
        0,
        &defend,
        5,
    )
    .is_empty());
    let mut turn_start_state = state.clone();
    assert!(crate::content::powers::resolve_power_at_turn_start(
        PowerId::Rage,
        &mut turn_start_state,
        0,
        5
    )
    .is_empty());
    assert_eq!(
        crate::content::powers::resolve_power_at_end_of_turn(
            &Power {
                power_type: PowerId::Rage,
                instance_id: None,
                amount: 5,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            },
            &mut state,
            0,
        )
        .as_slice(),
        &[Action::RemovePower {
            target: 0,
            power_id: PowerId::Rage
        }]
    );
}

#[test]
fn ironclad_rampage_and_rupture_definitions_match_java_sources() {
    let rampage = get_card_definition(CardId::Rampage);
    assert_eq!(rampage.card_type, CardType::Attack);
    assert_eq!(rampage.rarity, CardRarity::Uncommon);
    assert_eq!(rampage.cost, 1);
    assert_eq!(rampage.base_damage, 8);
    assert_eq!(rampage.base_magic, 5);
    assert_eq!(rampage.target, CardTarget::Enemy);
    assert_eq!(rampage.upgrade_magic, 3);

    let reaper = get_card_definition(CardId::Reaper);
    assert_eq!(reaper.card_type, CardType::Attack);
    assert_eq!(reaper.rarity, CardRarity::Rare);
    assert_eq!(reaper.cost, 2);
    assert_eq!(reaper.base_damage, 4);
    assert_eq!(reaper.target, CardTarget::AllEnemy);
    assert!(reaper.is_multi_damage);
    assert!(reaper.exhaust);
    assert!(reaper.tags.contains(&CardTag::Healing));
    assert_eq!(reaper.upgrade_damage, 1);

    let reckless_charge = get_card_definition(CardId::RecklessCharge);
    assert_eq!(reckless_charge.card_type, CardType::Attack);
    assert_eq!(reckless_charge.rarity, CardRarity::Uncommon);
    assert_eq!(reckless_charge.cost, 0);
    assert_eq!(reckless_charge.base_damage, 7);
    assert_eq!(reckless_charge.target, CardTarget::Enemy);
    assert_eq!(reckless_charge.upgrade_damage, 3);

    let rupture = get_card_definition(CardId::Rupture);
    assert_eq!(rupture.card_type, CardType::Power);
    assert_eq!(rupture.rarity, CardRarity::Uncommon);
    assert_eq!(rupture.cost, 1);
    assert_eq!(rupture.base_magic, 1);
    assert_eq!(rupture.target, CardTarget::SelfTarget);
    assert_eq!(rupture.upgrade_magic, 1);
}

#[test]
fn ironclad_rampage_and_rupture_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 31;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 42;
    state.entities.monsters = vec![first, second];

    let mut rampage_plus = CombatCard::for_scenario(CardId::Rampage, 320);
    rampage_plus.upgrades = 1;
    let rampage_actions = resolve_card_play(CardId::Rampage, &state, &rampage_plus, Some(31));
    assert_eq!(rampage_actions.len(), 2);
    match &rampage_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 31);
            assert_eq!(info.base, 10);
            assert_eq!(info.output, 10);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Rampage+ should damage before modifying itself, got {other:?}"),
    }
    assert_eq!(
        rampage_actions[1].action,
        Action::ModifyCardDamage {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(320),
            amount: 8,
        }
    );

    let mut reaper_plus = CombatCard::for_scenario(CardId::Reaper, 321);
    reaper_plus.upgrades = 1;
    let reaper_actions = resolve_card_play(CardId::Reaper, &state, &reaper_plus, None);
    assert_eq!(reaper_actions.len(), 1);
    match &reaper_actions[0].action {
        Action::VampireDamageAllEnemies {
            source,
            damages,
            damage_type,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(damages.as_slice(), &[7, 7]);
            assert_eq!(*damage_type, DamageType::Normal);
        }
        other => panic!("Reaper+ should emit VampireDamageAllEnemiesAction, got {other:?}"),
    }

    let mut reckless_plus = CombatCard::for_scenario(CardId::RecklessCharge, 322);
    reckless_plus.upgrades = 1;
    let reckless_actions =
        resolve_card_play(CardId::RecklessCharge, &state, &reckless_plus, Some(42));
    assert_eq!(reckless_actions.len(), 2);
    match &reckless_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 42);
            assert_eq!(info.base, 12);
            assert_eq!(info.output, 12);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Reckless Charge+ should damage first, got {other:?}"),
    }
    assert!(matches!(
        reckless_actions[1].action,
        Action::MakeTempCardInDrawPile {
            card_id: CardId::Dazed,
            amount: 1,
            random_spot: true,
            to_bottom: false,
            upgraded: false
        }
    ));

    let mut rupture_plus = CombatCard::for_scenario(CardId::Rupture, 323);
    rupture_plus.upgrades = 1;
    let rupture_actions = resolve_card_play(CardId::Rupture, &state, &rupture_plus, None);
    assert_eq!(rupture_actions.len(), 1);
    assert!(matches!(
        rupture_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Rupture,
            amount: 2
        }
    ));
}

#[test]
fn rupture_and_reaper_execution_hooks_match_java_sources() {
    let state = crate::test_support::blank_test_combat();
    let rupture_actions = crate::content::powers::resolve_power_on_hp_lost(
        PowerId::Rupture,
        &state,
        0,
        3,
        2,
        Some(0),
        DamageType::HpLoss,
    );
    assert_eq!(
        rupture_actions.as_slice(),
        &[Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 2
        }]
    );
    assert!(crate::content::powers::resolve_power_on_hp_lost(
        PowerId::Rupture,
        &state,
        0,
        3,
        2,
        None,
        DamageType::HpLoss,
    )
    .is_empty());

    let mut rupture_state = crate::test_support::blank_test_combat();
    rupture_state.entities.player.current_hp = 70;
    rupture_state
        .entities
        .monsters
        .push(crate::test_support::test_monster(EnemyId::JawWorm));
    rupture_state.entities.power_db.insert(
        0,
        vec![Power {
            power_type: PowerId::Rupture,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::damage::handle_lose_hp(0, 5, true, &mut rupture_state);
    assert_eq!(
        rupture_state.pop_next_action(),
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 2
        }),
        "Java RupturePower grants its power amount, not the HP lost amount"
    );

    let mut reaper_state = crate::test_support::blank_test_combat();
    reaper_state.entities.player.current_hp = 50;
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 31;
    first.current_hp = 20;
    first.max_hp = 20;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 42;
    second.current_hp = 20;
    second.max_hp = 20;
    reaper_state.entities.monsters = vec![first, second];

    crate::engine::action_handlers::damage::handle_vampire_damage_all_enemies(
        0,
        smallvec::smallvec![3, 7],
        DamageType::Normal,
        &mut reaper_state,
    );

    assert_eq!(reaper_state.entities.monsters[0].current_hp, 17);
    assert_eq!(reaper_state.entities.monsters[1].current_hp, 13);
    assert_eq!(
        reaper_state.entities.player.current_hp, 50,
        "Java VampireDamageAllEnemiesAction queues HealAction instead of healing inline"
    );
    assert_eq!(
        reaper_state.pop_next_action(),
        Some(Action::Heal {
            target: 0,
            amount: 10
        })
    );
    crate::engine::action_handlers::damage::handle_heal(0, 10, &mut reaper_state);
    assert_eq!(reaper_state.entities.player.current_hp, 60);

    let mut flower_state = reaper_state.clone();
    flower_state.entities.player.current_hp = 50;
    flower_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::MagicFlower,
        ));
    crate::engine::action_handlers::damage::handle_vampire_damage_all_enemies(
        0,
        smallvec::smallvec![2, 2],
        DamageType::Normal,
        &mut flower_state,
    );
    assert_eq!(
        flower_state.pop_next_action(),
        Some(Action::Heal {
            target: 0,
            amount: 4
        })
    );
    crate::engine::action_handlers::damage::handle_heal(0, 4, &mut flower_state);
    assert_eq!(
        flower_state.entities.player.current_hp, 56,
        "Java Reaper queues HealAction, so Magic Flower modifies the vampire heal"
    );

    let mut bloom_state = reaper_state.clone();
    bloom_state.entities.player.current_hp = 50;
    bloom_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::MarkOfTheBloom,
        ));
    crate::engine::action_handlers::damage::handle_vampire_damage_all_enemies(
        0,
        smallvec::smallvec![2, 2],
        DamageType::Normal,
        &mut bloom_state,
    );
    assert_eq!(
        bloom_state.pop_next_action(),
        Some(Action::Heal {
            target: 0,
            amount: 4
        })
    );
    crate::engine::action_handlers::damage::handle_heal(0, 4, &mut bloom_state);
    assert_eq!(
        bloom_state.entities.player.current_hp, 50,
        "Java Mark of the Bloom returns zero from onPlayerHeal, including Reaper heals"
    );

    let mut single_vampire_state = crate::test_support::blank_test_combat();
    single_vampire_state.entities.player.current_hp = 70;
    let mut parasite = crate::test_support::test_monster(EnemyId::ShelledParasite);
    parasite.id = 77;
    parasite.current_hp = 10;
    parasite.max_hp = 30;
    single_vampire_state.entities.monsters = vec![parasite];
    crate::engine::action_handlers::damage::handle_vampire_damage(
        DamageInfo {
            source: 77,
            target: 0,
            base: 6,
            output: 6,
            damage_type: DamageType::Normal,
            is_modified: true,
        },
        &mut single_vampire_state,
    );
    assert_eq!(single_vampire_state.entities.player.current_hp, 64);
    assert_eq!(single_vampire_state.entities.monsters[0].current_hp, 10);
    assert_eq!(
        single_vampire_state.pop_next_action(),
        Some(Action::Heal {
            target: 77,
            amount: 6
        }),
        "Java VampireDamageAction queues HealAction to top instead of healing inline"
    );
    crate::engine::action_handlers::damage::handle_heal(77, 6, &mut single_vampire_state);
    assert_eq!(single_vampire_state.entities.monsters[0].current_hp, 16);
}

#[test]
fn ironclad_upgrade_and_exhaust_utility_definitions_match_java_sources() {
    let searing_blow = get_card_definition(CardId::SearingBlow);
    assert_eq!(searing_blow.card_type, CardType::Attack);
    assert_eq!(searing_blow.rarity, CardRarity::Uncommon);
    assert_eq!(searing_blow.cost, 2);
    assert_eq!(searing_blow.base_damage, 12);
    assert_eq!(searing_blow.target, CardTarget::Enemy);

    let second_wind = get_card_definition(CardId::SecondWind);
    assert_eq!(second_wind.card_type, CardType::Skill);
    assert_eq!(second_wind.rarity, CardRarity::Uncommon);
    assert_eq!(second_wind.cost, 1);
    assert_eq!(second_wind.base_block, 5);
    assert_eq!(second_wind.target, CardTarget::SelfTarget);
    assert_eq!(second_wind.upgrade_block, 2);

    let seeing_red = get_card_definition(CardId::SeeingRed);
    assert_eq!(seeing_red.card_type, CardType::Skill);
    assert_eq!(seeing_red.rarity, CardRarity::Uncommon);
    assert_eq!(seeing_red.cost, 1);
    assert_eq!(seeing_red.target, CardTarget::None);
    assert!(seeing_red.exhaust);
    let mut seeing_red_plus = CombatCard::for_scenario(CardId::SeeingRed, 330);
    seeing_red_plus.upgrades = 1;
    assert_eq!(upgraded_base_cost_override(&seeing_red_plus), Some(0));

    let sentinel = get_card_definition(CardId::Sentinel);
    assert_eq!(sentinel.card_type, CardType::Skill);
    assert_eq!(sentinel.rarity, CardRarity::Uncommon);
    assert_eq!(sentinel.cost, 1);
    assert_eq!(sentinel.base_block, 5);
    assert_eq!(sentinel.base_magic, 0);
    assert_eq!(sentinel.target, CardTarget::SelfTarget);
    assert_eq!(sentinel.upgrade_block, 3);
}

#[test]
fn ironclad_upgrade_and_exhaust_utility_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![
            Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::Dexterity,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            },
        ],
    );

    let mut searing_blow_plus_2 = CombatCard::for_scenario(CardId::SearingBlow, 331);
    searing_blow_plus_2.upgrades = 2;
    let searing_actions =
        resolve_card_play(CardId::SearingBlow, &state, &searing_blow_plus_2, Some(77));
    assert_eq!(searing_actions.len(), 1);
    match &searing_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 77);
            assert_eq!(info.base, 23);
            assert_eq!(info.output, 23);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Searing Blow+2 should emit evaluated DamageAction, got {other:?}"),
    }

    let mut second_wind_plus = CombatCard::for_scenario(CardId::SecondWind, 332);
    second_wind_plus.upgrades = 1;
    let second_wind_actions =
        resolve_card_play(CardId::SecondWind, &state, &second_wind_plus, None);
    assert_eq!(second_wind_actions.len(), 1);
    assert!(matches!(
        second_wind_actions[0].action,
        Action::BlockPerNonAttack { block_per_card: 9 }
    ));

    let seeing_red_actions = resolve_card_play(
        CardId::SeeingRed,
        &state,
        &CombatCard::for_scenario(CardId::SeeingRed, 333),
        None,
    );
    assert_eq!(seeing_red_actions.len(), 1);
    assert!(matches!(
        seeing_red_actions[0].action,
        Action::GainEnergy { amount: 2 }
    ));

    let mut sentinel_plus = CombatCard::for_scenario(CardId::Sentinel, 334);
    sentinel_plus.upgrades = 1;
    let sentinel_actions = resolve_card_play(CardId::Sentinel, &state, &sentinel_plus, None);
    assert_eq!(sentinel_actions.len(), 1);
    assert!(matches!(
        sentinel_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 10
        }
    ));
}

#[test]
fn sentinel_exhaust_trigger_matches_java_add_to_top_energy() {
    let state = crate::test_support::blank_test_combat();
    let sentinel = CombatCard::for_scenario(CardId::Sentinel, 340);
    let sentinel_hooks = resolve_card_on_exhaust(&sentinel, &state);
    assert_eq!(sentinel_hooks.len(), 1);
    assert!(matches!(
        sentinel_hooks[0].action,
        Action::GainEnergy { amount: 2 }
    ));
    assert_eq!(
        sentinel_hooks[0].insertion_mode,
        crate::runtime::action::AddTo::Top
    );

    let mut sentinel_plus = CombatCard::for_scenario(CardId::Sentinel, 341);
    sentinel_plus.upgrades = 1;
    let sentinel_plus_hooks = resolve_card_on_exhaust(&sentinel_plus, &state);
    assert_eq!(sentinel_plus_hooks.len(), 1);
    assert!(matches!(
        sentinel_plus_hooks[0].action,
        Action::GainEnergy { amount: 3 }
    ));
    assert_eq!(
        sentinel_plus_hooks[0].insertion_mode,
        crate::runtime::action::AddTo::Top
    );

    let mut exhaust_state = crate::test_support::blank_test_combat();
    exhaust_state.zones.hand = vec![sentinel_plus];
    crate::content::powers::store::set_powers_for(
        &mut exhaust_state,
        0,
        vec![Power {
            power_type: PowerId::FeelNoPain,
            instance_id: None,
            amount: 4,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    crate::engine::action_handlers::cards::handle_exhaust_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(341),
        crate::state::PileType::Hand,
        &mut exhaust_state,
    );
    assert_eq!(
        exhaust_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 3 })
    );
    assert_eq!(
        exhaust_state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 4
        })
    );

    let mut charon_state = crate::test_support::blank_test_combat();
    charon_state.entities.monsters = vec![crate::test_support::test_monster(EnemyId::JawWorm)];
    charon_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::CharonsAshes,
        ));
    charon_state.zones.hand = vec![CombatCard::for_scenario(CardId::Sentinel, 342)];
    crate::engine::action_handlers::cards::handle_exhaust_card(
        crate::runtime::combat::CardLineageId::constructed_scenario(342),
        crate::state::PileType::Hand,
        &mut charon_state,
    );
    assert_eq!(
        charon_state.pop_next_action(),
        Some(Action::GainEnergy { amount: 2 }),
        "Sentinel triggerOnExhaust is called after relic onExhaust and uses Java addToTop, so it resolves first"
    );
    assert!(
        matches!(
            charon_state.pop_next_action(),
            Some(Action::DamageAllEnemies { .. })
        ),
        "Charon's Ashes addToTop action remains next after the later Sentinel addToTop"
    );
}

#[test]
fn burning_pact_exhausted_sentinel_energy_precedes_followup_draw() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Sentinel, 350)];

    let burning_pact = CombatCard::for_scenario(CardId::BurningPact, 351);
    let actions = resolve_card_play(CardId::BurningPact, &state, &burning_pact, None);
    state.queue_actions(actions);

    let exhaust = state
        .pop_next_action()
        .expect("Burning Pact should queue its exhaust action before drawing");
    crate::engine::action_handlers::execute_action(exhaust, &mut state);

    assert_eq!(
        state.pop_next_action(),
        Some(Action::GainEnergy { amount: 2 }),
        "Sentinel addToTop should interrupt Burning Pact's already-queued DrawCardAction"
    );
    assert_eq!(state.pop_next_action(), Some(Action::DrawCards(2)));
}

#[test]
fn ironclad_exhaust_debuff_and_intent_definitions_match_java_sources() {
    let sever_soul = get_card_definition(CardId::SeverSoul);
    assert_eq!(sever_soul.card_type, CardType::Attack);
    assert_eq!(sever_soul.rarity, CardRarity::Uncommon);
    assert_eq!(sever_soul.cost, 2);
    assert_eq!(sever_soul.base_damage, 16);
    assert_eq!(sever_soul.target, CardTarget::Enemy);
    assert_eq!(sever_soul.upgrade_damage, 6);

    let shockwave = get_card_definition(CardId::Shockwave);
    assert_eq!(shockwave.card_type, CardType::Skill);
    assert_eq!(shockwave.rarity, CardRarity::Uncommon);
    assert_eq!(shockwave.cost, 2);
    assert_eq!(shockwave.base_magic, 3);
    assert_eq!(shockwave.target, CardTarget::AllEnemy);
    assert!(shockwave.exhaust);
    assert_eq!(shockwave.upgrade_magic, 2);

    let shrug = get_card_definition(CardId::ShrugItOff);
    assert_eq!(shrug.card_type, CardType::Skill);
    assert_eq!(shrug.rarity, CardRarity::Common);
    assert_eq!(shrug.cost, 1);
    assert_eq!(shrug.base_block, 8);
    assert_eq!(shrug.base_magic, 0);
    assert_eq!(shrug.target, CardTarget::SelfTarget);
    assert_eq!(shrug.upgrade_block, 3);

    let spot_weakness = get_card_definition(CardId::SpotWeakness);
    assert_eq!(spot_weakness.card_type, CardType::Skill);
    assert_eq!(spot_weakness.rarity, CardRarity::Uncommon);
    assert_eq!(spot_weakness.cost, 1);
    assert_eq!(spot_weakness.base_magic, 3);
    assert_eq!(spot_weakness.target, CardTarget::SelfAndEnemy);
    assert_eq!(spot_weakness.upgrade_magic, 1);
}

#[test]
fn ironclad_exhaust_debuff_and_intent_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![
            Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::Dexterity,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            },
        ],
    );
    let mut attack_monster = crate::test_support::test_monster(EnemyId::JawWorm);
    attack_monster.id = 501;
    attack_monster.set_planned_move_id(1);
    let attack_spec =
        crate::semantics::combat::MonsterMoveSpec::Attack(crate::semantics::combat::AttackSpec {
            base_damage: 11,
            hits: 1,
            damage_kind: crate::semantics::combat::DamageKind::Normal,
        });
    attack_monster.move_state.planned_steps = Some(attack_spec.to_steps());
    attack_monster.move_state.planned_visible_spec = Some(attack_spec);

    let mut defend_monster = crate::test_support::test_monster(EnemyId::Cultist);
    defend_monster.id = 502;
    defend_monster.set_planned_move_id(3);
    let defend_spec =
        crate::semantics::combat::MonsterMoveSpec::Defend(crate::semantics::combat::DefendSpec {
            block: 6,
        });
    defend_monster.move_state.planned_steps = Some(defend_spec.to_steps());
    defend_monster.move_state.planned_visible_spec = Some(defend_spec);
    state.entities.monsters = vec![attack_monster, defend_monster];

    let mut sever_plus = CombatCard::for_scenario(CardId::SeverSoul, 350);
    sever_plus.upgrades = 1;
    let sever_actions = resolve_card_play(CardId::SeverSoul, &state, &sever_plus, Some(501));
    assert_eq!(sever_actions.len(), 2);
    assert!(matches!(
        sever_actions[0].action,
        Action::ExhaustAllNonAttack
    ));
    match &sever_actions[1].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 501);
            assert_eq!(info.base, 24);
            assert_eq!(info.output, 24);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Sever Soul+ should damage after ExhaustAllNonAttack, got {other:?}"),
    }

    let mut shockwave_plus = CombatCard::for_scenario(CardId::Shockwave, 351);
    shockwave_plus.upgrades = 1;
    let shockwave_actions = resolve_card_play(CardId::Shockwave, &state, &shockwave_plus, None);
    assert_eq!(shockwave_actions.len(), 4);
    assert!(matches!(
        shockwave_actions[0].action,
        Action::ApplyPower {
            source: 0,
            target: 501,
            power_id: PowerId::Weak,
            amount: 5
        }
    ));
    assert!(matches!(
        shockwave_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 501,
            power_id: PowerId::Vulnerable,
            amount: 5
        }
    ));
    assert!(matches!(
        shockwave_actions[2].action,
        Action::ApplyPower {
            source: 0,
            target: 502,
            power_id: PowerId::Weak,
            amount: 5
        }
    ));
    assert!(matches!(
        shockwave_actions[3].action,
        Action::ApplyPower {
            source: 0,
            target: 502,
            power_id: PowerId::Vulnerable,
            amount: 5
        }
    ));

    let mut shrug_plus = CombatCard::for_scenario(CardId::ShrugItOff, 352);
    shrug_plus.upgrades = 1;
    let shrug_actions = resolve_card_play(CardId::ShrugItOff, &state, &shrug_plus, None);
    assert_eq!(shrug_actions.len(), 2);
    assert!(matches!(
        shrug_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 13
        }
    ));
    assert!(matches!(shrug_actions[1].action, Action::DrawCards(1)));

    let mut spot_plus = CombatCard::for_scenario(CardId::SpotWeakness, 353);
    spot_plus.upgrades = 1;
    let spot_actions = resolve_card_play(CardId::SpotWeakness, &state, &spot_plus, Some(501));
    assert_eq!(spot_actions.len(), 1);
    assert!(matches!(
        spot_actions[0].action,
        Action::SpotWeakness {
            target: 501,
            amount: 4
        }
    ));

    let mut spot_state = state.clone();
    crate::engine::action_handlers::execute_action(spot_actions[0].action.clone(), &mut spot_state);
    let queued_apply = spot_state.pop_next_action();
    assert_eq!(
        queued_apply,
        Some(Action::ApplyPower {
            source: 0,
            target: 0,
            power_id: PowerId::Strength,
            amount: 4,
        })
    );
    crate::engine::action_handlers::execute_action(queued_apply.unwrap(), &mut spot_state);
    assert_eq!(
        crate::content::powers::store::powers_for(&spot_state, 0)
            .unwrap()
            .iter()
            .find(|power| power.power_type == PowerId::Strength)
            .map(|power| power.amount),
        Some(6)
    );

    let mut changed_state = state.clone();
    changed_state.entities.monsters[0].set_planned_move_id(2);
    crate::engine::action_handlers::execute_action(
        spot_actions[0].action.clone(),
        &mut changed_state,
    );
    assert_eq!(changed_state.pop_next_action(), None);
    assert_eq!(
        crate::content::powers::store::powers_for(&changed_state, 0)
            .unwrap()
            .iter()
            .find(|power| power.power_type == PowerId::Strength)
            .map(|power| power.amount),
        Some(2)
    );
}

#[test]
fn second_wind_block_per_non_attack_matches_java_add_to_top_order() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 350),
        CombatCard::for_scenario(CardId::Defend, 351),
        CombatCard::for_scenario(CardId::ShrugItOff, 352),
    ];

    crate::engine::action_handlers::damage::handle_block_per_non_attack(7, &mut state);

    assert_eq!(
        state.pop_next_action(),
        Some(Action::ExhaustCard {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(352),
            source_pile: crate::state::PileType::Hand
        })
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ExhaustCard {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(351),
            source_pile: crate::state::PileType::Hand
        })
    );
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 7
        })
    ));
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::GainBlock {
            target: 0,
            amount: 7
        })
    ));
}

#[test]
fn sever_soul_exhaust_all_non_attack_queues_exhausts_before_following_damage() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 360),
        CombatCard::for_scenario(CardId::Defend, 361),
        CombatCard::for_scenario(CardId::ShrugItOff, 362),
    ];
    state.queue_action_back(Action::Damage(crate::runtime::action::DamageInfo {
        source: 0,
        target: 501,
        base: 16,
        output: 16,
        damage_type: DamageType::Normal,
        is_modified: false,
    }));

    crate::engine::action_handlers::damage::handle_exhaust_all_non_attack(&mut state);

    assert_eq!(
        state.pop_next_action(),
        Some(Action::ExhaustCard {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(362),
            source_pile: crate::state::PileType::Hand
        })
    );
    assert_eq!(
        state.pop_next_action(),
        Some(Action::ExhaustCard {
            card_lineage: crate::runtime::combat::CardLineageId::constructed_scenario(361),
            source_pile: crate::state::PileType::Hand
        })
    );
    assert!(matches!(
        state.pop_next_action(),
        Some(Action::Damage(crate::runtime::action::DamageInfo {
            target: 501,
            ..
        }))
    ));
}

#[test]
fn ironclad_random_and_exhaust_attack_definitions_match_java_sources() {
    let sword_boomerang = get_card_definition(CardId::SwordBoomerang);
    assert_eq!(sword_boomerang.card_type, CardType::Attack);
    assert_eq!(sword_boomerang.rarity, CardRarity::Common);
    assert_eq!(sword_boomerang.cost, 1);
    assert_eq!(sword_boomerang.base_damage, 3);
    assert_eq!(sword_boomerang.base_magic, 3);
    assert_eq!(sword_boomerang.target, CardTarget::AllEnemy);
    assert_eq!(sword_boomerang.upgrade_magic, 1);

    let thunderclap = get_card_definition(CardId::ThunderClap);
    assert_eq!(thunderclap.name, "Thunderclap");
    assert_eq!(thunderclap.card_type, CardType::Attack);
    assert_eq!(thunderclap.rarity, CardRarity::Common);
    assert_eq!(thunderclap.cost, 1);
    assert_eq!(thunderclap.base_damage, 4);
    assert_eq!(thunderclap.base_magic, 0);
    assert_eq!(thunderclap.target, CardTarget::AllEnemy);
    assert!(thunderclap.is_multi_damage);
    assert_eq!(thunderclap.upgrade_damage, 3);

    let true_grit = get_card_definition(CardId::TrueGrit);
    assert_eq!(true_grit.card_type, CardType::Skill);
    assert_eq!(true_grit.rarity, CardRarity::Common);
    assert_eq!(true_grit.cost, 1);
    assert_eq!(true_grit.base_block, 7);
    assert_eq!(true_grit.base_magic, 0);
    assert_eq!(true_grit.target, CardTarget::SelfTarget);
    assert_eq!(true_grit.upgrade_block, 2);

    let twin_strike = get_card_definition(CardId::TwinStrike);
    assert_eq!(twin_strike.card_type, CardType::Attack);
    assert_eq!(twin_strike.rarity, CardRarity::Common);
    assert_eq!(twin_strike.cost, 1);
    assert_eq!(twin_strike.base_damage, 5);
    assert_eq!(twin_strike.target, CardTarget::Enemy);
    assert!(twin_strike.tags.contains(&CardTag::Strike));
    assert_eq!(twin_strike.upgrade_damage, 2);
}

#[test]
fn ironclad_random_and_exhaust_attack_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![
            Power {
                power_type: PowerId::Strength,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            },
            Power {
                power_type: PowerId::Dexterity,
                instance_id: None,
                amount: 2,
                extra_data: 0,
                payload: crate::runtime::combat::PowerPayload::None,
                just_applied: false,
            },
        ],
    );
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 601;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 602;
    state.entities.monsters = vec![first, second];

    let mut sword_plus = CombatCard::for_scenario(CardId::SwordBoomerang, 370);
    sword_plus.upgrades = 1;
    let sword_actions = resolve_card_play(CardId::SwordBoomerang, &state, &sword_plus, None);
    assert_eq!(sword_actions.len(), 4);
    for action in &sword_actions {
        match &action.action {
            Action::AttackDamageRandomEnemyCard { card } => {
                assert_eq!(card.id, CardId::SwordBoomerang);
                assert_eq!(card.upgrades, 1);
            }
            other => {
                panic!("Sword Boomerang+ should queue AttackDamageRandomEnemyAction, got {other:?}")
            }
        }
    }

    let mut thunder_plus = CombatCard::for_scenario(CardId::ThunderClap, 371);
    thunder_plus.upgrades = 1;
    let thunder_actions = resolve_card_play(CardId::ThunderClap, &state, &thunder_plus, None);
    assert_eq!(thunder_actions.len(), 3);
    match &thunder_actions[0].action {
        Action::DamageAllEnemies {
            source,
            damages,
            damage_type,
            is_modified,
        } => {
            assert_eq!(*source, 0);
            assert_eq!(damages.as_slice(), &[9, 9]);
            assert_eq!(*damage_type, DamageType::Normal);
            assert!(!*is_modified);
        }
        other => panic!("Thunderclap+ should damage all enemies first, got {other:?}"),
    }
    assert!(matches!(
        thunder_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 601,
            power_id: PowerId::Vulnerable,
            amount: 1
        }
    ));
    assert!(matches!(
        thunder_actions[2].action,
        Action::ApplyPower {
            source: 0,
            target: 602,
            power_id: PowerId::Vulnerable,
            amount: 1
        }
    ));

    let mut true_grit_plus = CombatCard::for_scenario(CardId::TrueGrit, 372);
    true_grit_plus.upgrades = 1;
    let mut true_grit_state = state.clone();
    true_grit_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 373),
        CombatCard::for_scenario(CardId::Defend, 374),
    ];
    let true_grit_actions =
        resolve_card_play(CardId::TrueGrit, &true_grit_state, &true_grit_plus, None);
    assert_eq!(true_grit_actions.len(), 2);
    assert!(matches!(
        true_grit_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 11
        }
    ));
    assert!(matches!(
        true_grit_actions[1].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: false,
            any_number: false,
            can_pick_zero: false
        }
    ));

    let mut twin_plus = CombatCard::for_scenario(CardId::TwinStrike, 375);
    twin_plus.upgrades = 1;
    let twin_actions = resolve_card_play(CardId::TwinStrike, &state, &twin_plus, Some(601));
    assert_eq!(twin_actions.len(), 2);
    for action in &twin_actions {
        match &action.action {
            Action::Damage(info) => {
                assert_eq!(info.source, 0);
                assert_eq!(info.target, 601);
                assert_eq!(info.base, 9);
                assert_eq!(info.output, 9);
                assert_eq!(info.damage_type, DamageType::Normal);
            }
            other => panic!("Twin Strike+ should emit two DamageActions, got {other:?}"),
        }
    }
}

#[test]
fn random_enemy_attacks_ignore_half_dead_monsters_like_java_random_monster() {
    let mut state = crate::test_support::blank_test_combat();
    let mut half_dead = crate::test_support::test_monster(EnemyId::Darkling);
    half_dead.id = 610;
    half_dead.current_hp = 20;
    half_dead.half_dead = true;
    state.entities.monsters = vec![half_dead];

    crate::engine::action_handlers::damage::handle_damage_random_enemy(
        0,
        7,
        DamageType::Normal,
        &mut state,
    );

    let target = state
        .entities
        .monsters
        .iter()
        .find(|m| m.id == 610)
        .expect("test monster should remain present");
    assert_eq!(
        target.current_hp, 20,
        "Java MonsterGroup.getRandomMonster(aliveOnly=true) excludes halfDead monsters"
    );
}

#[test]
fn random_enemy_attacks_do_not_filter_zero_hp_before_action_like_java_random_monster() {
    let mut state = crate::test_support::blank_test_combat();
    let mut zero_hp_not_dying = crate::test_support::test_monster(EnemyId::JawWorm);
    zero_hp_not_dying.id = 614;
    zero_hp_not_dying.current_hp = 0;
    zero_hp_not_dying.is_dying = false;
    zero_hp_not_dying.is_escaped = false;
    zero_hp_not_dying.half_dead = false;
    state.entities.monsters = vec![zero_hp_not_dying];

    crate::engine::action_handlers::damage::handle_damage_random_enemy(
        0,
        7,
        DamageType::Normal,
        &mut state,
    );

    match state
        .pop_next_action()
        .expect("Java random target selection can still pick a 0 HP non-dying monster")
    {
        Action::Damage(info) => {
            assert_eq!(info.target, 614);
            assert_eq!(info.output, 7);
        }
        other => panic!("DamageRandomEnemyAction should queue DamageAction, got {other:?}"),
    }
}

#[test]
fn attack_damage_random_enemy_card_recalculates_damage_at_execution_like_java() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 611;
    target.current_hp = 30;
    state.entities.monsters = vec![target];

    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 4,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    crate::engine::action_handlers::execute_action(
        Action::AttackDamageRandomEnemyCard {
            card: Box::new(CombatCard::for_scenario(CardId::SwordBoomerang, 612)),
        },
        &mut state,
    );

    assert_eq!(
        state.entities.monsters[0].current_hp, 30,
        "Java AttackDamageRandomEnemyAction queues a DamageAction instead of damaging inline"
    );
    match state
        .pop_next_action()
        .expect("AttackDamageRandomEnemyAction should queue DamageAction")
    {
        Action::Damage(info) => {
            assert_eq!(info.target, 611);
            assert_eq!(
                info.output, 7,
                "Java recalculates the card at random-target action execution time"
            );
        }
        other => panic!("AttackDamageRandomEnemyAction should queue DamageAction, got {other:?}"),
    }
}

#[test]
fn true_grit_exhaust_action_edges_match_java_exhaust_action() {
    let empty_state = crate::test_support::blank_test_combat();
    let true_grit = CombatCard::for_scenario(CardId::TrueGrit, 380);
    let empty_actions = resolve_card_play(CardId::TrueGrit, &empty_state, &true_grit, None);
    assert_eq!(empty_actions.len(), 2);
    assert!(matches!(
        empty_actions[0].action,
        Action::GainBlock {
            target: 0,
            amount: 7
        }
    ));
    assert_eq!(
        empty_actions[1].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: true,
            any_number: false,
            can_pick_zero: false,
        },
        "Java True Grit queues ExhaustAction even when it will fizzle on an empty hand"
    );

    let mut one_card_state = crate::test_support::blank_test_combat();
    one_card_state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 381)];
    let one_card_actions = resolve_card_play(CardId::TrueGrit, &one_card_state, &true_grit, None);
    assert_eq!(one_card_actions.len(), 2);
    assert!(matches!(
        one_card_actions[1].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: true,
            any_number: false,
            can_pick_zero: false
        }
    ));

    let mut two_card_state = crate::test_support::blank_test_combat();
    two_card_state.zones.hand = vec![
        CombatCard::for_scenario(CardId::Strike, 382),
        CombatCard::for_scenario(CardId::Defend, 383),
    ];
    let two_card_actions = resolve_card_play(CardId::TrueGrit, &two_card_state, &true_grit, None);
    assert_eq!(two_card_actions.len(), 2);
    assert!(matches!(
        two_card_actions[1].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: true,
            any_number: false,
            can_pick_zero: false
        }
    ));

    let mut true_grit_plus = CombatCard::for_scenario(CardId::TrueGrit, 384);
    true_grit_plus.upgrades = 1;
    let one_card_plus_actions =
        resolve_card_play(CardId::TrueGrit, &one_card_state, &true_grit_plus, None);
    assert_eq!(one_card_plus_actions.len(), 2);
    assert!(matches!(
        one_card_plus_actions[1].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: false,
            any_number: false,
            can_pick_zero: false
        }
    ));

    let two_card_plus_actions =
        resolve_card_play(CardId::TrueGrit, &two_card_state, &true_grit_plus, None);
    assert_eq!(two_card_plus_actions.len(), 2);
    assert!(matches!(
        two_card_plus_actions[1].action,
        Action::ExhaustFromHand {
            amount: 1,
            random: false,
            any_number: false,
            can_pick_zero: false
        }
    ));
}

#[test]
fn ironclad_debuff_draw_xcost_and_wound_definitions_match_java_sources() {
    let uppercut = get_card_definition(CardId::Uppercut);
    assert_eq!(uppercut.card_type, CardType::Attack);
    assert_eq!(uppercut.rarity, CardRarity::Uncommon);
    assert_eq!(uppercut.cost, 2);
    assert_eq!(uppercut.base_damage, 13);
    assert_eq!(uppercut.base_magic, 1);
    assert_eq!(uppercut.target, CardTarget::Enemy);
    assert_eq!(uppercut.upgrade_magic, 1);

    let warcry = get_card_definition(CardId::Warcry);
    assert_eq!(warcry.card_type, CardType::Skill);
    assert_eq!(warcry.rarity, CardRarity::Common);
    assert_eq!(warcry.cost, 0);
    assert_eq!(warcry.base_magic, 1);
    assert_eq!(warcry.target, CardTarget::SelfTarget);
    assert!(warcry.exhaust);
    assert_eq!(warcry.upgrade_magic, 1);

    let whirlwind = get_card_definition(CardId::Whirlwind);
    assert_eq!(whirlwind.card_type, CardType::Attack);
    assert_eq!(whirlwind.rarity, CardRarity::Uncommon);
    assert_eq!(whirlwind.cost, -1);
    assert_eq!(whirlwind.base_damage, 5);
    assert_eq!(whirlwind.target, CardTarget::AllEnemy);
    assert!(whirlwind.is_multi_damage);
    assert_eq!(whirlwind.upgrade_damage, 3);

    let wild_strike = get_card_definition(CardId::WildStrike);
    assert_eq!(wild_strike.card_type, CardType::Attack);
    assert_eq!(wild_strike.rarity, CardRarity::Common);
    assert_eq!(wild_strike.cost, 1);
    assert_eq!(wild_strike.base_damage, 12);
    assert_eq!(wild_strike.target, CardTarget::Enemy);
    assert!(wild_strike.tags.contains(&CardTag::Strike));
    assert_eq!(wild_strike.upgrade_damage, 5);
}

#[test]
fn whirlwind_spends_captured_energy_before_nunchaku_gain_resolves() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 3;
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Whirlwind, 39_201)];
    state.entities.monsters = vec![crate::test_support::test_monster(
        crate::content::monsters::EnemyId::JawWorm,
    )];
    let mut nunchaku =
        crate::content::relics::RelicState::new(crate::content::relics::RelicId::Nunchaku);
    nunchaku.counter = 9;
    state.entities.player.add_relic(nunchaku);

    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut state)
        .expect("Whirlwind should be playable");
    while let Some(action) = state.pop_next_action() {
        crate::engine::action_handlers::execute_action(action, &mut state);
    }

    assert_eq!(state.turn.energy, 1);
    let settled = state
        .zones
        .discard_pile
        .iter()
        .next()
        .expect("settled Whirlwind should enter the discard pile");
    assert_eq!(settled.energy_on_use, 0);
    assert_eq!(settled.exhaust_override, None);
}

#[test]
fn ironclad_debuff_draw_xcost_and_wound_runtime_actions_match_java_use_methods() {
    let mut state = crate::test_support::blank_test_combat();
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::Strength,
            instance_id: None,
            amount: 2,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );
    let mut first = crate::test_support::test_monster(EnemyId::JawWorm);
    first.id = 701;
    let mut second = crate::test_support::test_monster(EnemyId::Cultist);
    second.id = 702;
    state.entities.monsters = vec![first, second];

    let mut uppercut_plus = CombatCard::for_scenario(CardId::Uppercut, 390);
    uppercut_plus.upgrades = 1;
    let uppercut_actions = resolve_card_play(CardId::Uppercut, &state, &uppercut_plus, Some(701));
    assert_eq!(uppercut_actions.len(), 3);
    match &uppercut_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 701);
            assert_eq!(info.base, 15);
            assert_eq!(info.output, 15);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Uppercut+ should damage before debuffs, got {other:?}"),
    }
    assert!(matches!(
        uppercut_actions[1].action,
        Action::ApplyPower {
            source: 0,
            target: 701,
            power_id: PowerId::Weak,
            amount: 2
        }
    ));
    assert!(matches!(
        uppercut_actions[2].action,
        Action::ApplyPower {
            source: 0,
            target: 701,
            power_id: PowerId::Vulnerable,
            amount: 2
        }
    ));

    let mut warcry_plus = CombatCard::for_scenario(CardId::Warcry, 391);
    warcry_plus.upgrades = 1;
    let warcry_actions = resolve_card_play(CardId::Warcry, &state, &warcry_plus, None);
    assert_eq!(warcry_actions.len(), 2);
    assert!(matches!(warcry_actions[0].action, Action::DrawCards(2)));
    assert!(matches!(
        warcry_actions[1].action,
        Action::PutOnDeck {
            amount: 1,
            random: false
        }
    ));

    let mut whirlwind_plus = CombatCard::for_scenario(CardId::Whirlwind, 392);
    whirlwind_plus.upgrades = 1;
    whirlwind_plus.energy_on_use = 3;
    let whirlwind_actions = resolve_card_play(CardId::Whirlwind, &state, &whirlwind_plus, None);
    assert_eq!(whirlwind_actions.len(), 1);
    match &whirlwind_actions[0].action {
        Action::Whirlwind {
            damages,
            damage_type,
            free_to_play_once,
            energy_on_use,
        } => {
            assert_eq!(damages.as_slice(), &[10, 10]);
            assert_eq!(*damage_type, DamageType::Normal);
            assert!(!*free_to_play_once);
            assert_eq!(*energy_on_use, 3);
        }
        other => panic!("Whirlwind+ should emit WhirlwindAction equivalent, got {other:?}"),
    }

    let mut chemical_x_state = state.clone();
    chemical_x_state.turn.energy = 3;
    chemical_x_state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ChemicalX,
        ));
    crate::engine::action_handlers::execute_action(
        Action::Whirlwind {
            damages: smallvec::smallvec![10, 10],
            damage_type: DamageType::Normal,
            free_to_play_once: false,
            energy_on_use: 3,
        },
        &mut chemical_x_state,
    );
    assert_eq!(chemical_x_state.turn.energy, 0);
    let mut queued_damage_all = 0;
    while let Some(action) = chemical_x_state.pop_next_action() {
        assert!(matches!(action, Action::DamageAllEnemies { .. }));
        queued_damage_all += 1;
    }
    assert_eq!(queued_damage_all, 5);

    let mut free_x_state = state.clone();
    free_x_state.turn.energy = 3;
    crate::engine::action_handlers::execute_action(
        Action::Whirlwind {
            damages: smallvec::smallvec![10, 10],
            damage_type: DamageType::Normal,
            free_to_play_once: true,
            energy_on_use: 3,
        },
        &mut free_x_state,
    );
    assert_eq!(free_x_state.turn.energy, 3);

    let mut hand_x_state = state.clone();
    hand_x_state.turn.energy = 3;
    hand_x_state.zones.hand = vec![whirlwind_plus.clone()];
    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut hand_x_state)
        .expect("Whirlwind should be playable with current energy captured as X");
    assert_eq!(
        hand_x_state.turn.energy, 3,
        "Java does not spend X-card energy in the generic useCard path"
    );
    let queued_whirlwind = hand_x_state
        .pop_next_action()
        .expect("WhirlwindAction should be queued before UseCardDone");
    match &queued_whirlwind {
        Action::Whirlwind { energy_on_use, .. } => assert_eq!(*energy_on_use, 3),
        other => panic!("WhirlwindAction should be queued before UseCardDone, got {other:?}"),
    }
    crate::engine::action_handlers::execute_action(queued_whirlwind, &mut hand_x_state);
    assert_eq!(hand_x_state.turn.energy, 0);

    let mut autoplay_x_state = state.clone();
    autoplay_x_state.turn.energy = 3;
    let mut queued_whirlwind_card = CombatCard::for_scenario(CardId::Whirlwind, 394);
    queued_whirlwind_card.energy_on_use = 3;
    autoplay_x_state.enqueue_card_play(
        crate::runtime::combat::QueuedCardPlay {
            card: queued_whirlwind_card,
            target: None,
            energy_on_use: 3,
            ignore_energy_total: false,
            autoplay: true,
            random_target: false,
            is_end_turn_autoplay: false,
            purge_on_use: false,
            source: crate::runtime::combat::QueuedCardSource::Normal,
        },
        false,
    );
    let flush_autoplay = autoplay_x_state
        .pop_next_action()
        .expect("enqueueing an autoplay card should schedule a queue flush");
    assert!(matches!(flush_autoplay, Action::FlushNextQueuedCard));
    crate::engine::action_handlers::execute_action(flush_autoplay, &mut autoplay_x_state);
    let queued_direct_play = autoplay_x_state
        .pop_next_action()
        .expect("autoplay queue should emit a direct card play");
    match &queued_direct_play {
        Action::PlayCardDirect { card, .. } => assert!(
            !card.free_to_play_once,
            "Java autoplay sets isInAutoplay/ignoreEnergyOnUse, not freeToPlayOnce"
        ),
        other => panic!("autoplay queue should emit PlayCardDirect, got {other:?}"),
    }
    crate::engine::action_handlers::execute_action(queued_direct_play, &mut autoplay_x_state);
    let autoplay_whirlwind = autoplay_x_state
        .pop_next_action()
        .expect("autoplay Whirlwind should queue WhirlwindAction");
    match &autoplay_whirlwind {
        Action::Whirlwind {
            free_to_play_once,
            energy_on_use,
            ..
        } => {
            assert!(!*free_to_play_once);
            assert_eq!(*energy_on_use, 3);
        }
        other => panic!("autoplay Whirlwind should emit WhirlwindAction, got {other:?}"),
    }
    crate::engine::action_handlers::execute_action(autoplay_whirlwind, &mut autoplay_x_state);
    assert_eq!(
        autoplay_x_state.turn.energy, 0,
        "Java autoplayed X-cost cards still let their card-specific X action spend energy"
    );

    let mut wild_plus = CombatCard::for_scenario(CardId::WildStrike, 393);
    wild_plus.upgrades = 1;
    let wild_actions = resolve_card_play(CardId::WildStrike, &state, &wild_plus, Some(702));
    assert_eq!(wild_actions.len(), 2);
    match &wild_actions[0].action {
        Action::Damage(info) => {
            assert_eq!(info.source, 0);
            assert_eq!(info.target, 702);
            assert_eq!(info.base, 19);
            assert_eq!(info.output, 19);
            assert_eq!(info.damage_type, DamageType::Normal);
        }
        other => panic!("Wild Strike+ should damage before Wound generation, got {other:?}"),
    }
    assert!(matches!(
        wild_actions[1].action,
        Action::MakeTempCardInDrawPile {
            card_id: CardId::Wound,
            amount: 1,
            random_spot: true,
            to_bottom: false,
            upgraded: false
        }
    ));
}

#[test]
fn lethal_damage_filters_post_combat_actions_like_java_action_manager() {
    let mut state = crate::test_support::blank_test_combat();
    let mut target = crate::test_support::test_monster(EnemyId::JawWorm);
    target.id = 720;
    target.current_hp = 5;
    state.entities.monsters = vec![target];

    state.queue_action_back(Action::MakeTempCardInDrawPile {
        card_id: CardId::Wound,
        amount: 1,
        random_spot: true,
        to_bottom: false,
        upgraded: false,
    });
    state.queue_action_back(Action::DrawCards(1));
    state.queue_action_back(Action::GainEnergy { amount: 1 });
    state.queue_action_back(Action::ApplyPower {
        source: 0,
        target: 720,
        power_id: PowerId::Vulnerable,
        amount: 1,
    });
    state.queue_action_back(Action::Whirlwind {
        damages: smallvec::smallvec![3],
        damage_type: DamageType::Normal,
        free_to_play_once: false,
        energy_on_use: 1,
    });
    state.queue_action_back(Action::DamageRandomEnemy {
        source: 0,
        base_damage: 3,
        damage_type: DamageType::Normal,
    });
    state.queue_action_back(Action::AttackDamageRandomEnemyCard {
        card: Box::new(CombatCard::for_scenario(CardId::SwordBoomerang, 721)),
    });
    state.queue_action_back(Action::DropkickDamageAndEffect {
        target: 720,
        damage_info: crate::runtime::action::DamageInfo {
            source: 0,
            target: 720,
            base: 5,
            output: 5,
            damage_type: DamageType::Normal,
            is_modified: false,
        },
    });
    state.queue_action_back(Action::GainBlock {
        target: 0,
        amount: 3,
    });
    state.queue_action_back(Action::Heal {
        target: 0,
        amount: 2,
    });
    state.queue_action_back(Action::UseCardDone {
        should_exhaust: false,
        trigger_after_use_hooks: true,
    });

    crate::engine::action_handlers::execute_action(
        Action::Damage(crate::runtime::action::DamageInfo {
            source: 0,
            target: 720,
            base: 99,
            output: 99,
            damage_type: DamageType::Normal,
            is_modified: false,
        }),
        &mut state,
    );

    let remaining: Vec<_> = std::iter::from_fn(|| state.pop_next_action()).collect();
    assert_eq!(
        remaining,
        vec![
            Action::DamageRandomEnemy {
                source: 0,
                base_damage: 3,
                damage_type: DamageType::Normal,
            },
            Action::GainBlock {
                target: 0,
                amount: 3
            },
            Action::Heal {
                target: 0,
                amount: 2
            },
            Action::UseCardDone {
                should_exhaust: false,
                trigger_after_use_hooks: true,
            }
        ],
        "Java GameActionManager.clearPostCombatActions keeps DamageRandomEnemyAction/Heal/GainBlock/UseCardAction and removes generated cards, draw, energy, powers, and AttackDamageRandomEnemyAction"
    );
}

#[test]
fn transmutation_x_cost_action_matches_java_energy_and_chemical_x_timing() {
    let mut state = crate::test_support::blank_test_combat();
    state.turn.energy = 3;
    let mut transmutation = CombatCard::for_scenario(CardId::Transmutation, 394);
    transmutation.upgrades = 1;
    transmutation.energy_on_use = 1;

    let actions = resolve_card_play(CardId::Transmutation, &state, &transmutation, None);
    assert_eq!(actions.len(), 1);
    match &actions[0].action {
        Action::Transmutation {
            upgraded,
            free_to_play_once,
            energy_on_use,
        } => {
            assert!(*upgraded);
            assert!(!*free_to_play_once);
            assert_eq!(
                *energy_on_use, 3,
                "Java Transmutation.use raises stale energyOnUse to current EnergyPanel.totalCount"
            );
        }
        other => panic!("Transmutation should emit TransmutationAction equivalent, got {other:?}"),
    }

    state
        .entities
        .player
        .add_relic(crate::content::relics::RelicState::new(
            crate::content::relics::RelicId::ChemicalX,
        ));
    crate::engine::action_handlers::execute_action(actions[0].action.clone(), &mut state);
    assert_eq!(state.turn.energy, 0);
    assert_eq!(
        state.rng.card_random_rng.counter, 5,
        "Java TransmutationAction.update samples concrete colorless cards before queuing MakeTempCardInHandAction"
    );
    let mut generated = 0;
    while let Some(action) = state.pop_next_action() {
        match action {
            Action::MakeConstructedCopyInHand { original, amount } => {
                assert_eq!(amount, 1);
                assert!(state.colorless_combat_pool().contains(&original.id));
                assert_eq!(original.upgrades, 1);
                let expected_cost_for_turn = if original.combat_cost_without_turn_override_java() >= 0 {
                    0
                } else {
                    -1
                };
                assert_eq!(
                    original.cost_for_turn_java(),
                    expected_cost_for_turn,
                    "Java setCostForTurn(0) is ignored by X-cost colorless cards"
                );
            }
            other => panic!(
                "TransmutationAction should queue concrete MakeTempCardInHandAction cards, got {other:?}"
            ),
        }
        generated += 1;
    }
    assert_eq!(generated, 5);
}

#[test]
fn magnetism_power_locks_random_colorless_cards_at_turn_start() {
    let monster = crate::test_support::test_monster(EnemyId::JawWorm);
    let mut state = crate::test_support::combat_with_monsters(vec![monster]);

    assert_eq!(state.rng.card_random_rng.counter, 0);
    let actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::MagnetismPower,
        &mut state,
        0,
        2,
    );

    assert_eq!(
        state.rng.card_random_rng.counter, 2,
        "Java MagnetismPower.atStartOfTurn samples random colorless cards before queuing MakeTempCardInHandAction"
    );
    assert_eq!(actions.len(), 2);
    for action in actions {
        match action {
            Action::MakeConstructedCopyInHand { original, amount } => {
                assert_eq!(amount, 1);
                assert!(state.colorless_combat_pool().contains(&original.id));
            }
            other => panic!("Magnetism should queue concrete colorless cards, got {other:?}"),
        }
    }
}

#[test]
fn magnetism_make_temp_constructor_master_reality_persists_until_execution() {
    let monster = crate::test_support::test_monster(EnemyId::JawWorm);
    let mut state = crate::test_support::combat_with_monsters(vec![monster]);
    crate::content::powers::store::set_powers_for(
        &mut state,
        0,
        vec![Power {
            power_type: PowerId::MasterRealityPower,
            instance_id: None,
            amount: -1,
            extra_data: 0,
            payload: crate::runtime::combat::PowerPayload::None,
            just_applied: false,
        }],
    );

    let actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::MagnetismPower,
        &mut state,
        0,
        1,
    );
    let Action::MakeConstructedCopyInHand { original, amount } = actions[0].clone() else {
        panic!("Magnetism should queue a concrete constructed MakeTempCardInHandAction payload");
    };
    assert_eq!(amount, 1);
    assert_eq!(
        original.upgrades, 1,
        "Java MakeTempCardInHandAction constructor applies Master Reality before the action executes"
    );

    crate::content::powers::store::set_powers_for(&mut state, 0, vec![]);
    crate::engine::action_handlers::execute_action(
        Action::MakeConstructedCopyInHand { original, amount },
        &mut state,
    );
    assert_eq!(
        state.zones.hand[0].upgrades, 1,
        "Removing Master Reality before action execution does not erase the constructor-time upgrade"
    );
}

#[test]
fn magnetism_power_skips_when_monsters_are_basically_dead() {
    let mut monster = crate::test_support::test_monster(EnemyId::JawWorm);
    monster.is_dying = true;
    let mut state = crate::test_support::combat_with_monsters(vec![monster]);

    let actions = crate::content::powers::resolve_power_at_turn_start(
        PowerId::MagnetismPower,
        &mut state,
        0,
        2,
    );

    assert!(actions.is_empty());
    assert_eq!(
        state.rng.card_random_rng.counter, 0,
        "Java MagnetismPower checks MonsterGroup.areMonstersBasicallyDead before sampling cards"
    );
}

#[test]
fn jack_of_all_trades_locks_random_colorless_cards_when_used() {
    let mut state = crate::test_support::blank_test_combat();
    let mut jack = CombatCard::for_scenario(CardId::JackOfAllTrades, 3970);
    jack.upgrades = 1;
    state.zones.hand = vec![jack];

    assert_eq!(state.rng.card_random_rng.counter, 0);
    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut state)
        .expect("Jack Of All Trades should be playable");

    assert_eq!(
        state.rng.card_random_rng.counter, 2,
        "Java JackOfAllTrades.use samples each random colorless card before queuing MakeTempCardInHandAction"
    );
    for _ in 0..2 {
        match state.pop_next_action() {
            Some(Action::MakeConstructedCopyInHand { original, amount }) => {
                assert_eq!(amount, 1);
                assert!(state.colorless_combat_pool().contains(&original.id));
            }
            other => {
                panic!("Jack Of All Trades should queue concrete colorless cards, got {other:?}")
            }
        }
    }
}

#[test]
fn metamorphosis_locks_random_attacks_and_combat_cost_when_used() {
    let mut state = crate::test_support::blank_test_combat();
    state.meta.player_class = "Silent".to_string();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Metamorphosis, 3971)];

    assert_eq!(state.rng.card_random_rng.counter, 0);
    crate::engine::action_handlers::cards::handle_play_card_from_hand(0, None, &mut state)
        .expect("Metamorphosis should be playable");

    assert_eq!(
        state.rng.card_random_rng.counter, 3,
        "Java Metamorphosis.use samples concrete random attacks before queuing MakeTempCardInDrawPileAction"
    );
    for _ in 0..3 {
        match state.pop_next_action() {
            Some(Action::MakeCopyInDrawPile {
                original,
                amount,
                random_spot,
                to_bottom,
            }) => {
                assert_eq!(amount, 1);
                assert!(random_spot);
                assert!(!to_bottom);
                let generated_def = get_card_definition(original.id);
                assert_eq!(generated_def.card_type, CardType::Attack);
                if generated_def.cost > 0 {
                    assert_eq!(
                        original.combat_cost_without_turn_override_java(),
                        0,
                        "Java Metamorphosis mutates positive-cost generated cards to combat cost 0"
                    );
                    assert_eq!(original.cost_for_turn_java(), 0);
                } else {
                    assert_eq!(
                        original.combat_cost_without_turn_override_java(),
                        i32::from(generated_def.cost)
                    );
                }
            }
            other => panic!("Metamorphosis should queue concrete draw-pile cards, got {other:?}"),
        }
    }
}

#[test]
fn forethought_resolves_as_execution_time_action_like_java() {
    let state = crate::test_support::blank_test_combat();
    let forethought = CombatCard::for_scenario(CardId::Forethought, 830);
    let actions = resolve_card_play(CardId::Forethought, &state, &forethought, None);
    assert_eq!(actions.len(), 1);
    assert_eq!(
        actions[0].action,
        Action::Forethought { upgraded: false },
        "Java Forethought.use always queues ForethoughtAction; hand size is read when that action executes"
    );

    let mut one_card_state = crate::test_support::blank_test_combat();
    let mut temporarily_free_defend = CombatCard::for_scenario(CardId::Defend, 831);
    temporarily_free_defend.set_cost_for_turn_java(0);
    one_card_state.zones.hand = vec![temporarily_free_defend];

    crate::engine::action_handlers::cards::handle_forethought(false, &mut one_card_state);

    assert!(one_card_state.zones.hand.is_empty());
    assert_eq!(one_card_state.zones.draw_pile.len(), 1);
    assert_eq!(
        one_card_state.zones.draw_pile[0].lineage,
        crate::runtime::combat::CardLineageId::constructed_scenario(831)
    );
    assert!(
        one_card_state.zones.draw_pile[0].free_to_play_once,
        "Java ForethoughtAction auto-move path also checks AbstractCard.cost, not costForTurn"
    );
    assert_eq!(
        one_card_state.pop_next_action(),
        None,
        "Java ForethoughtAction auto-moves the only selectable card without opening the hand-select screen"
    );
}

#[test]
fn upgraded_forethought_opens_any_number_selection_at_execution() {
    let mut state = crate::test_support::blank_test_combat();
    state.zones.hand = vec![CombatCard::for_scenario(CardId::Strike, 832)];

    crate::engine::action_handlers::cards::handle_forethought(true, &mut state);

    assert!(matches!(
        state.pop_next_action(),
        Some(Action::SuspendForHandSelect {
            min: 0,
            max: 99,
            can_cancel: true,
            filter: crate::state::HandSelectFilter::Any,
            reason: crate::state::HandSelectReason::PutToBottomOfDraw,
        })
    ));
}

#[test]
fn thinking_ahead_uses_java_use_time_hand_visibility() {
    let mut direct_empty_hand = crate::test_support::blank_test_combat();
    direct_empty_hand.zones.draw_pile =
        (vec![CombatCard::for_scenario(CardId::Strike, 833)]).into();
    let thinking = CombatCard::for_scenario(CardId::ThinkingAhead, 834);

    let direct_actions = resolve_card_play_with_context(
        CardId::ThinkingAhead,
        &direct_empty_hand,
        &thinking,
        None,
        CardUseContext {
            played_from_hand: false,
        },
    );
    assert_eq!(direct_actions.len(), 1);
    assert!(matches!(direct_actions[0].action, Action::DrawCards(2)));

    let hand_play_actions = resolve_card_play_with_context(
        CardId::ThinkingAhead,
        &direct_empty_hand,
        &thinking,
        None,
        CardUseContext {
            played_from_hand: true,
        },
    );
    assert_eq!(hand_play_actions.len(), 2);
    assert!(matches!(
        hand_play_actions[1].action,
        Action::SuspendForHandSelect {
            reason: crate::state::HandSelectReason::PutOnDrawPile,
            ..
        }
    ));

    let mut direct_with_hand = direct_empty_hand.clone();
    direct_with_hand.zones.hand = vec![CombatCard::for_scenario(CardId::Defend, 835)];
    let direct_with_hand_actions = resolve_card_play_with_context(
        CardId::ThinkingAhead,
        &direct_with_hand,
        &thinking,
        None,
        CardUseContext {
            played_from_hand: false,
        },
    );
    assert_eq!(
        direct_with_hand_actions.len(),
        2,
        "Java direct/autoplay use paths only queue PutOnDeckAction when another card is already in hand"
    );
}

#[test]
fn discard_cards_queue_java_discard_action_instead_of_prechecking_hand() {
    let empty_state = crate::test_support::blank_test_combat();

    let acrobatics = CombatCard::for_scenario(CardId::Acrobatics, 840);
    let acrobatics_actions = resolve_card_play(CardId::Acrobatics, &empty_state, &acrobatics, None);
    assert_eq!(acrobatics_actions.len(), 2);
    assert!(matches!(acrobatics_actions[0].action, Action::DrawCards(3)));
    assert_eq!(
        acrobatics_actions[1].action,
        Action::DiscardFromHand {
            amount: 1,
            random: false,
            end_turn: false,
        },
        "Java Acrobatics.use always queues DiscardAction after DrawCardAction; it does not precheck the hand before drawing"
    );

    let mut acrobatics_plus = CombatCard::for_scenario(CardId::Acrobatics, 8440);
    acrobatics_plus.upgrades = 1;
    let acrobatics_plus_actions =
        resolve_card_play(CardId::Acrobatics, &empty_state, &acrobatics_plus, None);
    assert_eq!(acrobatics_plus_actions[0].action, Action::DrawCards(4));
    assert_eq!(
        acrobatics_plus_actions[1].action,
        Action::DiscardFromHand {
            amount: 1,
            random: false,
            end_turn: false,
        }
    );

    let survivor = CombatCard::for_scenario(CardId::Survivor, 841);
    let survivor_actions = resolve_card_play(CardId::Survivor, &empty_state, &survivor, None);
    assert_eq!(survivor_actions.len(), 2);
    assert_eq!(
        survivor_actions[1].action,
        Action::DiscardFromHand {
            amount: 1,
            random: false,
            end_turn: false,
        }
    );

    let prepared = CombatCard::for_scenario(CardId::Prepared, 8441);
    let prepared_base_actions = resolve_card_play(CardId::Prepared, &empty_state, &prepared, None);
    assert_eq!(prepared_base_actions[0].action, Action::DrawCards(1));
    assert_eq!(
        prepared_base_actions[1].action,
        Action::DiscardFromHand {
            amount: 1,
            random: false,
            end_turn: false,
        }
    );

    let mut prepared_plus = CombatCard::for_scenario(CardId::Prepared, 842);
    prepared_plus.upgrades = 1;
    let prepared_actions = resolve_card_play(CardId::Prepared, &empty_state, &prepared_plus, None);
    assert_eq!(prepared_actions[0].action, Action::DrawCards(2));
    assert_eq!(
        prepared_actions[1].action,
        Action::DiscardFromHand {
            amount: 2,
            random: false,
            end_turn: false,
        }
    );

    let dagger_actions = resolve_card_play(
        CardId::DaggerThrow,
        &empty_state,
        &CombatCard::for_scenario(CardId::DaggerThrow, 843),
        Some(844),
    );
    assert_eq!(
        dagger_actions[2].action,
        Action::DiscardFromHand {
            amount: 1,
            random: false,
            end_turn: false,
        }
    );
}

mod armaments_tests;
mod deck_to_hand_tests;
mod exhume_tests;
mod shuffle_tests;
mod silent_and_colorless_tests;
mod upgrade_instance_tests;
