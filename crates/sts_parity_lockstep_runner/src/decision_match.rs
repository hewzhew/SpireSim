use serde_json::Value;
use sts_parity_lockstep::match_cell_label;

pub(crate) fn redact(room: &mut Value) {
    if let Some(cards) = room["match_game"]["cards"].as_array_mut() {
        for card in cards {
            if card["revealed"] != true {
                card["card_id"] = Value::Null;
                card["upgrades"] = Value::Null;
            }
        }
    }
}

fn card_name(card: &Value) -> String {
    card["card_id"]
        .as_str()
        .map(|id| crate::manual::format_card(id, card["upgrades"].as_u64().unwrap_or(0), None))
        .unwrap_or_else(|| "?".into())
}

pub(crate) fn render(room: &Value, before: Option<&Value>) -> Option<String> {
    let board = &room["match_game"];
    let cards = board["cards"].as_array()?;
    let remaining = match board["attempts_remaining"].as_u64()? {
        0 => "complete".into(),
        1 => "1 attempt left".into(),
        count => format!("{count} attempts left"),
    };
    // A continued view (including branch comparison) needs the current result,
    // not another copy of the grid. Explicit board inspection has no before.
    if before.is_some_and(|room| room["match_game"]["cards"].is_array()) {
        if let Some(first) = board["first"].as_u64() {
            return Some(format!(
                "{}: {}; choose second cell ({remaining})",
                match_cell_label(first as u8),
                card_name(cards.get(first as usize)?)
            ));
        }
        if let Some(pair) = board["last_pair"].as_array() {
            let a = pair.first()?.as_u64()? as usize;
            let b = pair.get(1)?.as_u64()? as usize;
            let first = cards.get(a)?;
            let second = cards.get(b)?;
            let result = if first["matched"] == true && second["matched"] == true {
                format!(
                    "Matched {} ({}, {})",
                    card_name(first),
                    match_cell_label(a as u8),
                    match_cell_label(b as u8)
                )
            } else {
                format!(
                    "{} {} / {} {}: mismatch",
                    match_cell_label(a as u8),
                    card_name(first),
                    match_cell_label(b as u8),
                    card_name(second)
                )
            };
            return Some(format!("{result}; {remaining}"));
        }
        return Some(format!("Match and Keep: {remaining}"));
    }
    let mut lines = vec![format!(
        "Match and Keep: {remaining}{}",
        if room["layout_source"] == "rust_preview" {
            " (Rust layout preview)"
        } else {
            ""
        }
    )];
    for row in cards.chunks(4) {
        lines.push(
            row.iter()
                .map(|card| {
                    let position = card["position"].as_u64().unwrap() as u8;
                    format!(
                        "{} {}{}",
                        match_cell_label(position),
                        card_name(card),
                        if card["matched"] == true {
                            " [matched]"
                        } else if board["first"] == card["position"] {
                            " [up]"
                        } else {
                            ""
                        }
                    )
                })
                .collect::<Vec<_>>()
                .join(" | "),
        );
    }
    if let Some(pair) = board["last_pair"].as_array() {
        lines.push(format!(
            "Last pair: {}, {}",
            match_cell_label(pair[0].as_u64()? as u8),
            match_cell_label(pair[1].as_u64()? as u8)
        ));
    }
    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn decision_board_keeps_hidden_faces_private_and_continuations_compact() {
        let mut room = json!({"match_game":{
            "attempts_remaining":5, "first":null, "last_pair":null,
            "cards":(0..12).map(|position| json!({
                "position":position,
                "card_id":if position < 2 {"seeingred"} else {"parasite"},
                "upgrades":0, "matched":false, "revealed":false
            })).collect::<Vec<_>>()
        }});
        let preview = room.clone();
        redact(&mut room);
        let full = render(&room, None).unwrap();
        assert!(!full.contains("Seeing Red") && !full.contains("Parasite"));
        assert!(full.contains("R1C1") && full.contains("R3C4"));

        let before = room.clone();
        room["match_game"]["first"] = json!(0);
        room["match_game"]["cards"][0] = preview["match_game"]["cards"][0].clone();
        room["match_game"]["cards"][0]["revealed"] = json!(true);
        let flip = render(&room, Some(&before)).unwrap();
        assert!(flip.contains("R1C1") && flip.contains("Seeing Red"));
        assert!(!flip.contains("Parasite") && !flip.contains("R3C4"));

        let before = room.clone();
        room["match_game"]["first"] = Value::Null;
        room["match_game"]["attempts_remaining"] = json!(4);
        room["match_game"]["last_pair"] = json!([0, 1]);
        room["match_game"]["cards"][1] = preview["match_game"]["cards"][1].clone();
        for index in [0, 1] {
            room["match_game"]["cards"][index]["revealed"] = json!(true);
            room["match_game"]["cards"][index]["matched"] = json!(true);
        }
        let matched = render(&room, Some(&before)).unwrap();
        assert_eq!(matched.matches("Seeing Red").count(), 1);
        assert!(matched.contains("R1C1") && matched.contains("R1C2"));
        assert!(!matched.contains("R3C4"));
        let mut preview = preview;
        preview["layout_source"] = json!("rust_preview");
        let full = render(&preview, None).unwrap();
        assert!(full.contains("Rust layout preview") && full.contains("Parasite"));
    }
}
