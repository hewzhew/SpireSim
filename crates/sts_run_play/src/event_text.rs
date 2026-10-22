//! Event scenes and buttons from imported original localization. This is display
//! data only; the simulator's typed event options remain the action source.

use std::fmt::Write;
use sts_oracle_learning::ai::planner_core::{
    PlannerEventOptionPublicSemanticsV1, PlannerEventPublicEffectV1, PlannerEventRelicPoolV1,
};
use sts_oracle_learning::content::events::face_trader::FaceTraderResult;
use sts_oracle_learning::state::events::EventId;
use sts_presentation::Locale;

#[derive(Clone, Copy)]
enum Amount {
    Heal,
    GainMaxHp,
    GainGold,
    LoseGold,
    LoseHp,
}

impl Amount {
    fn read(self, effects: &[PlannerEventPublicEffectV1]) -> Option<i32> {
        use PlannerEventPublicEffectV1 as Effect;
        let mut values = effects.iter().filter_map(|effect| match (self, effect) {
            (Self::Heal, Effect::Heal { amount })
            | (Self::GainMaxHp, Effect::GainMaxHp { amount })
            | (Self::GainGold, Effect::GainGold { amount })
            | (Self::LoseGold, Effect::LoseGold { amount })
            | (Self::LoseHp, Effect::LoseHp { amount }) => Some(*amount),
            _ => None,
        });
        let value = values.next()?;
        values.next().is_none().then_some(value)
    }
}

#[derive(Clone, Copy)]
enum Part {
    Original(usize),
    Amount(Amount),
}

/// Explicit bindings to the event owner's screen and button indices. Neither
/// the raw source label nor a translated string determines which option this is.
/// Unbound phases and incomplete public effects retain their source label.
pub(crate) fn option_label(
    event: EventId,
    screen: usize,
    option: usize,
    public: &PlannerEventOptionPublicSemanticsV1,
    locale: Locale,
) -> Result<Option<String>, String> {
    use Part::{Amount as Value, Original as Text};
    let Some(semantics) = &public.structured_semantics else {
        return Ok(None);
    };
    let recipe: &[Part] = match (event, screen, option) {
        (EventId::FaceTrader, 0, 0) => &[Text(4)],
        (EventId::FaceTrader, 1, 0) => &[
            Text(0),
            Value(Amount::LoseHp),
            Text(5),
            Value(Amount::GainGold),
            Text(1),
        ],
        (EventId::FaceTrader, 1, 1)
            if semantics.effects.iter().any(|effect| {
                matches!(
                    effect,
                    PlannerEventPublicEffectV1::ObtainRelic {
                        count: 1,
                        pool: PlannerEventRelicPoolV1::RandomFace
                    }
                )
            }) =>
        {
            &[Text(2)]
        }
        (EventId::FaceTrader, 1, 2) | (EventId::FaceTrader, 2, 0) => &[Text(3)],
        (EventId::BigFish, 0, 0) => &[Text(0), Value(Amount::Heal), Text(1)],
        (EventId::BigFish, 0, 1) => &[Text(2), Value(Amount::GainMaxHp), Text(3)],
        (EventId::BigFish, 0, 2) => &[Text(4)],
        (EventId::BigFish, 1, 0) => &[Text(5)],
        (EventId::Lab, 0, 0) => &[Text(0)],
        (EventId::LivingWall, 0, 0) => &[Text(0)],
        (EventId::LivingWall, 0, 1) => &[Text(1)],
        (EventId::LivingWall, 0, 2) => &[Text(2)],
        (EventId::LivingWall, 1, 0) => &[Text(6)],
        (EventId::ShiningLight, 0, 0) => &[Text(0), Value(Amount::LoseHp), Text(1)],
        (EventId::ShiningLight, 0, 1) | (EventId::ShiningLight, 1, 0) => &[Text(2)],
        (EventId::WorldOfGoop, 0, 0) => &[
            Text(0),
            Value(Amount::GainGold),
            Text(1),
            Value(Amount::LoseHp),
            Text(2),
        ],
        (EventId::WorldOfGoop, 0, 1) => &[Text(3), Value(Amount::LoseGold), Text(4)],
        (EventId::WorldOfGoop, 1, 0) => &[Text(5)],
        (EventId::Ssssserpent, 0, 0) => &[Text(0), Value(Amount::GainGold), Text(1)],
        (EventId::Ssssserpent, 0, 1) => &[Text(2)],
        (EventId::Ssssserpent, 1, 0) => &[Text(3)],
        (EventId::Ssssserpent, 2, 0) => &[Text(4)],
        (EventId::GoldenShrine, 0, 0) => &[Text(0), Value(Amount::GainGold), Text(1)],
        (EventId::GoldenShrine, 0, 1) => &[Text(2)],
        (EventId::GoldenShrine, 0, 2) | (EventId::GoldenShrine, 1, 0) => &[Text(3)],
        (EventId::AccursedBlacksmith, 0, 0) => &[Text(0)],
        (EventId::AccursedBlacksmith, 0, 1) => &[Text(1)],
        (EventId::AccursedBlacksmith, 0, 2) | (EventId::AccursedBlacksmith, 1, 0) => &[Text(2)],
        (EventId::Duplicator | EventId::Transmorgrifier | EventId::UpgradeShrine, 0, 0) => {
            &[Text(0)]
        }
        (EventId::Duplicator | EventId::Transmorgrifier | EventId::UpgradeShrine, 0, 1)
        | (EventId::Duplicator | EventId::Transmorgrifier | EventId::UpgradeShrine, 1, 0) => {
            &[Text(1)]
        }
        (EventId::BackTotheBasics, 0, 0) => &[Text(0)],
        (EventId::BackTotheBasics, 0, 1) => &[Text(1)],
        (EventId::BackTotheBasics, 1, 0) => &[Text(3)],
        (EventId::Mushrooms, 0, 0) => &[Text(0)],
        (EventId::Mushrooms, 0, 1) => &[Text(1), Value(Amount::Heal), Text(2)],
        (EventId::Mushrooms, 1, 0) => &[Text(4)],
        (EventId::Mushrooms, 2, 0) => &[Text(3)],
        _ => return Ok(None),
    };
    let mut output = String::new();
    for part in recipe {
        match part {
            Part::Original(index) => output.push_str(
                locale
                    .original_event_option(event.stable_id(), *index)
                    .ok_or_else(|| {
                        format!("missing original event option: {locale}/{event:?}/{index}")
                    })?,
            ),
            Part::Amount(kind) => {
                let Some(amount) = kind.read(&semantics.effects) else {
                    return Ok(None);
                };
                write!(output, "{amount}").unwrap();
            }
        }
    }
    Ok(Some(clean_text(&output)))
}

pub(crate) fn current_scene(
    event: EventId,
    screen: usize,
    face_trader_result: Option<FaceTraderResult>,
    locale: Locale,
) -> Option<String> {
    if event == EventId::FaceTrader && screen == 2 && face_trader_result.is_none() {
        return Some(
            locale
                .text("decision", "event.unknownFaceTraderResult", &[])
                .expect("event result message"),
        );
    }
    let index = description_index(event, screen, face_trader_result)?;
    Some(
        locale
            .original_event_description("FaceTrader", index)
            .map(clean_text)
            .unwrap_or_else(|| {
                locale
                    .text("decision", "event.sceneUnavailable", &[])
                    .expect("missing scene message")
            }),
    )
}

fn description_index(
    event: EventId,
    screen: usize,
    face_trader_result: Option<FaceTraderResult>,
) -> Option<usize> {
    match (event, screen, face_trader_result) {
        (EventId::FaceTrader, 0 | 1, _) => Some(screen),
        (EventId::FaceTrader, 2, Some(FaceTraderResult::Touch)) => Some(2),
        (EventId::FaceTrader, 2, Some(FaceTraderResult::Trade)) => Some(3),
        (EventId::FaceTrader, 2, Some(FaceTraderResult::Leave)) => Some(4),
        _ => None,
    }
}

fn clean_text(raw: &str) -> String {
    raw.split_whitespace()
        .filter(|token| *token != "NL")
        .map(|token| {
            token
                .replace('~', "")
                .replace('@', "")
                .replace("#y", "")
                .replace("#r", "")
                .replace("#g", "")
                .replace("#b", "")
                .replace("#p", "")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn public(effects: Vec<PlannerEventPublicEffectV1>) -> PlannerEventOptionPublicSemanticsV1 {
        PlannerEventOptionPublicSemanticsV1 {
            public_label: Some("source label deliberately unrelated to the button".into()),
            structured_semantics: Some(sts_oracle_learning::ai::planner_core::PlannerEventOptionStructuredSemanticsV1 {
                effects,
                transition: sts_oracle_learning::ai::planner_core::PlannerEventTransitionV1::AdvanceScreen,
            }),
            ..Default::default()
        }
    }

    #[test]
    fn original_buttons_use_typed_values_and_never_resolve_from_source_prose() {
        use PlannerEventPublicEffectV1 as Effect;
        let facts = public(vec![
            Effect::LoseHp { amount: 11 },
            Effect::GainGold { amount: 123 },
        ]);
        let en = option_label(EventId::FaceTrader, 1, 0, &facts, Locale::En)
            .unwrap()
            .unwrap();
        let zh = option_label(EventId::FaceTrader, 1, 0, &facts, Locale::ZhCn)
            .unwrap()
            .unwrap();
        assert_eq!(en, "[Touch] Lose 11 HP, gain 123 Gold.");
        assert!(
            zh.contains("[触碰]") && zh.contains("失去11") && zh.contains("123 金币"),
            "{zh}"
        );
        let facts = public(vec![Effect::Heal { amount: 17 }]);
        let en = option_label(EventId::BigFish, 0, 0, &facts, Locale::En)
            .unwrap()
            .unwrap();
        assert_eq!(en, "[Banana] Heal 17 HP.");
        let facts = public(vec![Effect::GainMaxHp { amount: 9 }]);
        let en = option_label(EventId::BigFish, 0, 1, &facts, Locale::En)
            .unwrap()
            .unwrap();
        assert_eq!(en, "[Donut] Max HP +9.");
    }

    #[test]
    fn unbound_phases_or_missing_and_ambiguous_values_keep_the_source_label() {
        use PlannerEventPublicEffectV1 as Effect;
        for facts in [
            PlannerEventOptionPublicSemanticsV1::default(),
            public(vec![]),
            public(vec![Effect::Heal { amount: 8 }, Effect::Heal { amount: 9 }]),
        ] {
            assert_eq!(
                option_label(EventId::BigFish, 0, 0, &facts, Locale::ZhCn).unwrap(),
                None
            );
        }
        let facts = public(vec![Effect::Heal { amount: 9 }]);
        for (event, screen, index) in [
            (EventId::BigFish, 99, 0),
            (EventId::BigFish, 0, 99),
            (EventId::Neow, 1, 0),
            (EventId::Lab, 1, 0),
        ] {
            assert_eq!(
                option_label(event, screen, index, &facts, Locale::ZhCn).unwrap(),
                None
            );
        }
    }

    #[test]
    fn original_face_probability_label_is_not_used_for_a_replacement_reward() {
        use PlannerEventPublicEffectV1 as Effect;
        let replacement = public(vec![Effect::ObtainRelic {
            count: 1,
            pool: PlannerEventRelicPoolV1::Specific {
                relic: sts_oracle_learning::content::relics::RelicId::Circlet,
            },
        }]);
        assert_eq!(
            option_label(EventId::FaceTrader, 1, 1, &replacement, Locale::ZhCn).unwrap(),
            None
        );
        let normal = public(vec![Effect::ObtainRelic {
            count: 1,
            pool: PlannerEventRelicPoolV1::RandomFace,
        }]);
        assert!(
            option_label(EventId::FaceTrader, 1, 1, &normal, Locale::ZhCn)
                .unwrap()
                .unwrap()
                .contains("[交易]")
        );
    }

    #[test]
    fn face_trader_shows_only_the_current_unambiguous_scene() {
        assert_eq!(description_index(EventId::FaceTrader, 0, None), Some(0));
        assert_eq!(description_index(EventId::FaceTrader, 1, None), Some(1));
        assert_eq!(description_index(EventId::FaceTrader, 2, None), None);
        assert_eq!(
            description_index(EventId::FaceTrader, 2, Some(FaceTraderResult::Touch)),
            Some(2)
        );
        assert_eq!(
            description_index(EventId::FaceTrader, 2, Some(FaceTraderResult::Trade)),
            Some(3)
        );
        assert_eq!(
            description_index(EventId::FaceTrader, 2, Some(FaceTraderResult::Leave)),
            Some(4)
        );
        assert_eq!(description_index(EventId::Nest, 0, None), None);
        assert_eq!(
            current_scene(EventId::FaceTrader, 2, None, Locale::En).as_deref(),
            Some("Unknown (Face Trader result choice unavailable)")
        );
    }

    #[test]
    fn parses_and_cleans_the_selected_description_only() {
        assert_eq!(clean_text("A statue. NL ~Stop.~"), "A statue. Stop.");
        assert_eq!(clean_text("A face. NL Trade?"), "A face. Trade?");
        assert_eq!(clean_text("#ygold for @ME?@"), "gold for ME?");
    }

    #[test]
    fn scenes_use_connection_language_and_only_the_publicly_selected_branch() {
        let en = current_scene(EventId::FaceTrader, 0, None, Locale::En).unwrap();
        let zh = current_scene(EventId::FaceTrader, 0, None, Locale::ZhCn).unwrap();
        assert_ne!(en, zh);
        assert!(en.contains("statue"));
        assert!(!en.contains("Compensation"));
        assert!(!zh.contains("NL"));
        assert!(!zh.contains("#y"));
        assert!(current_scene(EventId::Nest, 0, None, Locale::ZhCn).is_none());
        let unknown = current_scene(EventId::FaceTrader, 2, None, Locale::ZhCn).unwrap();
        assert!(unknown.contains("换脸商结果选项未公开"));
    }
}
