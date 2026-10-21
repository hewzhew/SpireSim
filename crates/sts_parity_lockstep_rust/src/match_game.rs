use sts_oracle_run_control::{content::events::match_and_keep, state::events::EventState};
use sts_parity_lockstep::{NaturalMatchCellV1, NaturalMatchGameV1};

pub(crate) fn board(event: &EventState) -> Result<NaturalMatchGameV1, String> {
    let ed = &event.extra_data;
    if ed.len() != 18 || event.card_payload.len() != 6 {
        return Err("Rust Match and Keep omitted its dealt board".into());
    }
    let cell = |index: i32| -> Result<u8, String> {
        if !(0..12).contains(&index) {
            return Err("invalid dealt card position".into());
        }
        Ok(match_and_keep::cell_position(index as usize) as u8)
    };
    let mut cards = (0..12)
        .map(|i| {
            let card = event
                .card_payload
                .get(usize::try_from(ed[i]).map_err(|_| "negative card type")?)
                .ok_or("invalid Match and Keep card type")?;
            Ok(NaturalMatchCellV1 {
                position: cell(i as i32)?,
                card_id: sts_java_parity_protocol::canonical_game_id(card.card_id.stable_id()),
                upgrades: card.upgrades,
                matched: ed[12] & (1 << i) != 0,
                revealed: ed[17] & (1 << i) != 0,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;
    cards.sort_by_key(|c| c.position);
    let board = NaturalMatchGameV1 {
        attempts_remaining: ed[13].try_into().map_err(|_| "invalid attempt count")?,
        first: (ed[14] != -1).then(|| cell(ed[14])).transpose()?,
        last_pair: if ed[15] == -1 && ed[16] == -1 {
            None
        } else {
            Some([cell(ed[15])?, cell(ed[16])?])
        },
        cards,
    };
    board.validate(event.current_screen as i64)?;
    Ok(board)
}
