//! Original out-of-combat card faces. This selects the fields actually used
//! by each original upgrade method, not every field named UPGRADE_DESCRIPTION.
use super::{card_blueprint_persistent_base_numbers_v1, get_card_definition, CardId};
use crate::runtime::combat::CardBlueprint;
use sts_presentation::{CardTextField, CardTextValues, Locale};

pub fn original_printed_card_text(
    card: CardId,
    upgrades: u8,
    locale: Locale,
) -> Result<Option<String>, String> {
    let mut blueprint = CardBlueprint::fresh(card);
    blueprint.upgrades = upgrades;
    let numbers = card_blueprint_persistent_base_numbers_v1(&blueprint);
    let definition = get_card_definition(card);
    let field = if upgrades > 0 && uses_original_upgrade_description(card) {
        CardTextField::UpgradeDescription
    } else {
        CardTextField::Description
    };
    locale.original_card_text(
        card.stable_id(),
        field,
        CardTextValues {
            damage: numbers.damage,
            block: numbers.block,
            magic: definition.base_magic + i32::from(upgrades > 0) * definition.upgrade_magic,
        },
    )
}

fn uses_original_upgrade_description(card: CardId) -> bool {
    matches!(
        card,
        CardId::Amplify
            | CardId::Buffer
            | CardId::Capacitor
            | CardId::Chaos
            | CardId::Chill
            | CardId::Coolheaded
            | CardId::Darkness
            | CardId::EchoForm
            | CardId::Fission
            | CardId::Glacier
            | CardId::Heatsinks
            | CardId::HelloWorld
            | CardId::Hologram
            | CardId::Loop
            | CardId::MachineLearning
            | CardId::MultiCast
            | CardId::Rainbow
            | CardId::Seek
            | CardId::Stack
            | CardId::StaticDischarge
            | CardId::Storm
            | CardId::Tempest
            | CardId::Turbo
            | CardId::Apparition
            | CardId::Blind
            | CardId::DeepBreath
            | CardId::Discovery
            | CardId::Enlightenment
            | CardId::Forethought
            | CardId::JackOfAllTrades
            | CardId::SecretTechnique
            | CardId::SecretWeapon
            | CardId::ThinkingAhead
            | CardId::Transmutation
            | CardId::Trip
            | CardId::Adrenaline
            | CardId::AfterImage
            | CardId::Burst
            | CardId::CalculatedGamble
            | CardId::Catalyst
            | CardId::CloakAndDagger
            | CardId::Doppelganger
            | CardId::InfiniteBlades
            | CardId::Malaise
            | CardId::Outmaneuver
            | CardId::Prepared
            | CardId::Reflex
            | CardId::StormOfSteel
            | CardId::Tactician
            | CardId::WellLaidPlans
            | CardId::Alpha
            | CardId::BattleHymn
            | CardId::Blasphemy
            | CardId::Collect
            | CardId::ConjureBlade
            | CardId::DevaForm
            | CardId::EmptyMind
            | CardId::Establishment
            | CardId::ForeignInfluence
            | CardId::Meditate
            | CardId::Worship
            | CardId::Armaments
            | CardId::Bloodletting
            | CardId::Brutality
            | CardId::DoubleTap
            | CardId::DualWield
            | CardId::Entrench
            | CardId::Evolve
            | CardId::HeavyBlade
            | CardId::LimitBreak
            | CardId::Offering
            | CardId::PerfectedStrike
            | CardId::PommelStrike
            | CardId::Pummel
            | CardId::Sentinel
            | CardId::TrueGrit
            | CardId::Warcry
            | CardId::Burn
            | CardId::Miracle
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_printed_faces_select_real_upgrade_text_and_current_printed_numbers() {
        let face = |card, upgrades, locale| {
            original_printed_card_text(card, upgrades, locale)
                .unwrap()
                .unwrap()
        };
        assert_eq!(
            face(CardId::Bash, 1, Locale::En),
            "Deal 10 damage.\nApply 3 Vulnerable."
        );
        assert_eq!(
            face(CardId::ShrugItOff, 1, Locale::ZhCn),
            "获得 11 点格挡。\n抽1张牌。"
        );
        assert!(face(CardId::Armaments, 0, Locale::En).contains("Upgrade a card"));
        assert!(face(CardId::Armaments, 1, Locale::En).contains("Upgrade all cards"));
        assert!(face(CardId::Catalyst, 1, Locale::En).contains("Triple"));
        assert!(!face(CardId::LimitBreak, 1, Locale::En).contains("Exhaust"));
        assert!(face(CardId::BodySlam, 1, Locale::En).contains("equal to your Block"));
        assert!(!face(CardId::BodySlam, 1, Locale::En).contains("Deals 0"));
        assert!(face(CardId::GrandFinale, 1, Locale::En).contains("Deal 60 damage"));
        assert!(face(CardId::CorpseExplosion, 1, Locale::En).contains("Poison"));
        assert!(face(CardId::SearingBlow, 3, Locale::En).contains("Deal 27 damage"));
        assert!(face(CardId::Electrodynamics, 1, Locale::En).contains("Channel 3 Lightning"));
        assert_eq!(
            face(CardId::Adrenaline, 1, Locale::En)
                .matches("[Energy]")
                .count(),
            2
        );
        assert!(original_printed_card_text(CardId::Impulse, 0, Locale::En)
            .unwrap()
            .is_none());
    }

    #[test]
    fn original_printed_faces_cover_each_sourced_identity_in_both_languages_without_state() {
        for &id in CardId::STABLE_IDS {
            let card = CardId::from_stable_id(id).unwrap();
            for locale in [Locale::En, Locale::ZhCn] {
                for upgrades in [0, 1] {
                    let text = original_printed_card_text(card, upgrades, locale).unwrap();
                    if card == CardId::Impulse {
                        assert!(text.is_none());
                        continue;
                    }
                    let text = text.unwrap();
                    assert!(!text.is_empty(), "{id}/{upgrades}/{locale}");
                    for marker in ["!D!", "!B!", "!M!", "DEPRECATED", " NL "] {
                        assert!(!text.contains(marker), "{id}/{upgrades}/{locale}: {text}");
                    }
                }
            }
        }
    }
}
