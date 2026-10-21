use crate::JavaRuntimeObservationV1;
use sts_parity_lockstep::{NaturalMatchCellV1, NaturalMatchGameV1};

pub(crate) fn board(
    observation: &JavaRuntimeObservationV1,
    screen: i64,
) -> Result<NaturalMatchGameV1, String> {
    let raw = &observation.frame["parity_internal"]["match_game"];
    if raw["schema"] != "sts-match-game-state-v1"
        || raw["stable"] != true
        || !matches!(
            (screen, raw["screen"].as_str()),
            (2, Some("PLAY")) | (3, Some("COMPLETE"))
        )
    {
        return Err("Java Match and Keep lacks its stable board observation".into());
    }
    let mut cards: Vec<NaturalMatchCellV1> =
        serde_json::from_value(raw["cards"].clone()).map_err(|e| e.to_string())?;
    for card in &mut cards {
        card.card_id = sts_java_parity_protocol::canonical_game_id(&card.card_id);
    }
    let board = NaturalMatchGameV1 {
        attempts_remaining: serde_json::from_value(raw["attempts_remaining"].clone())
            .map_err(|e| e.to_string())?,
        first: serde_json::from_value(raw["first"].clone()).map_err(|e| e.to_string())?,
        last_pair: serde_json::from_value(raw["last_pair"].clone()).map_err(|e| e.to_string())?,
        cards,
    };
    board.validate(screen)?;
    Ok(board)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn match_board_requires_complete_stable_observations() {
        let mut observation = JavaRuntimeObservationV1 {
            producer: sts_parity_lockstep::ProducerIdentityV1 {
                implementation: "fixture".into(),
                version: "1".into(),
                session_epoch: "test".into(),
            },
            frame_id: 1,
            actions: vec![],
            frame: json!({}),
        };
        assert!(board(&observation, 2).is_err());
        observation.frame = json!({"parity_internal":{"match_game":{
            "schema":"sts-match-game-state-v1","screen":"PLAY","stable":true,
            "attempts_remaining":5,"first":null,"last_pair":null,
            "cards":(0..12).map(|position| json!({"position":position,"card_id":"Bash",
                "upgrades":0,"matched":false,"revealed":false})).collect::<Vec<_>>()}}});
        assert_eq!(board(&observation, 2).unwrap().cards[0].card_id, "bash");
        observation.frame["parity_internal"]["match_game"]["first"] = json!(0);
        assert!(board(&observation, 2).is_err());
        observation.frame["parity_internal"]["match_game"]["cards"][0]["revealed"] = json!(true);
        assert!(!board(&observation, 2).unwrap().can_flip(0));
        observation.frame["parity_internal"]["match_game"]["stable"] = json!(false);
        assert!(board(&observation, 2).is_err());
    }
}
