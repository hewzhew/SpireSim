//! Current event scene from the installed game's localization. This is display
//! data only; the simulator's typed event options remain the action source.

use std::fs::File;
use std::io::Read;
use std::path::{Path, PathBuf};

use sts_oracle_learning::content::events::face_trader::FaceTraderResult;
use sts_oracle_learning::state::events::EventId;
use zip::ZipArchive;

const DEFAULT_GAME_JAR: &str =
    r"C:\Program Files (x86)\Steam\steamapps\common\SlayTheSpire\desktop-1.0.jar";

pub(crate) fn current_scene(
    event: EventId,
    screen: usize,
    face_trader_result: Option<FaceTraderResult>,
) -> Option<String> {
    if event == EventId::FaceTrader && screen == 2 && face_trader_result.is_none() {
        return Some("Unknown (Face Trader result choice unavailable)".to_owned());
    }
    let index = description_index(event, screen, face_trader_result)?;
    let path = std::env::var_os("STS_GAME_JAR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(DEFAULT_GAME_JAR));
    Some(
        read_description(&path, "FaceTrader", index)
            .unwrap_or_else(|_| "Unknown (installed game text unavailable)".to_owned()),
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

fn read_description(path: &Path, event_key: &str, index: usize) -> Result<String, String> {
    let file = File::open(path).map_err(|e| e.to_string())?;
    let mut archive = ZipArchive::new(file).map_err(|e| e.to_string())?;
    let mut source = archive
        .by_name("localization/eng/events.json")
        .map_err(|e| e.to_string())?;
    let mut json = String::new();
    source
        .read_to_string(&mut json)
        .map_err(|e| e.to_string())?;
    parse_description(&json, event_key, index)
}

fn parse_description(json: &str, event_key: &str, index: usize) -> Result<String, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| e.to_string())?;
    let raw = value
        .get(event_key)
        .and_then(|event| event.get("DESCRIPTIONS"))
        .and_then(|descriptions| descriptions.get(index))
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| format!("missing event description {event_key}[{index}]"))?;
    Ok(clean_scene(raw))
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
            current_scene(EventId::FaceTrader, 2, None).as_deref(),
            Some("Unknown (Face Trader result choice unavailable)")
        );
    }

    #[test]
    fn parses_and_cleans_the_selected_description_only() {
        let json = r##"{"FaceTrader":{"DESCRIPTIONS":["A statue. NL ~Stop.~","A face. NL Trade?","#ygold for @ME?@"]}}"##;
        assert_eq!(
            parse_description(json, "FaceTrader", 0).unwrap(),
            "A statue. Stop."
        );
        assert_eq!(
            parse_description(json, "FaceTrader", 1).unwrap(),
            "A face. Trade?"
        );
        assert_eq!(
            parse_description(json, "FaceTrader", 2).unwrap(),
            "gold for ME?"
        );
        assert!(parse_description(json, "FaceTrader", 3).is_err());
    }

    #[test]
    fn missing_jar_is_a_read_error_not_a_game_error() {
        assert!(read_description(Path::new("no-such-game.jar"), "FaceTrader", 0).is_err());
    }
}
