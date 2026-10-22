//! Current event scene from imported original localization. This is display
//! data only; the simulator's typed event options remain the action source.

use sts_oracle_learning::content::events::face_trader::FaceTraderResult;
use sts_oracle_learning::state::events::EventId;
use sts_presentation::Locale;

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
            .map(clean_scene)
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

fn clean_scene(raw: &str) -> String {
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
        assert_eq!(clean_scene("A statue. NL ~Stop.~"), "A statue. Stop.");
        assert_eq!(clean_scene("A face. NL Trade?"), "A face. Trade?");
        assert_eq!(clean_scene("#ygold for @ME?@"), "gold for ME?");
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
