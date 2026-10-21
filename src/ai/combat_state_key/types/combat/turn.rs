use crate::content::cards::CardId;
use crate::runtime::combat::OrbId;

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatTurnKey {
    pub(crate) turn_count: u32,
    pub(crate) phase: CombatPhaseKey,
    pub(crate) energy: u8,
    pub(crate) turn_start_draw_modifier: i32,
    pub(crate) counters: CombatTurnCountersKey,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) enum CombatPhaseKey {
    PlayerTurn,
    MonsterTurn,
    TurnTransition,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, serde::Serialize)]
pub(crate) struct CombatTurnCountersKey {
    pub(crate) cards_played_this_turn: u8,
    pub(crate) attacks_played_this_turn: u8,
    pub(crate) cards_discarded_this_turn: u16,
    pub(crate) card_ids_played_this_turn: Vec<CardId>,
    pub(crate) card_ids_played_this_combat: Vec<CardId>,
    pub(crate) orbs_channeled_this_turn: Vec<OrbId>,
    pub(crate) orbs_channeled_this_combat: Vec<OrbId>,
    pub(crate) mantra_gained_this_combat: i32,
    pub(crate) times_damaged_this_combat: u8,
    pub(crate) victory_triggered: bool,
    pub(crate) discovery_cost_for_turn: Option<u8>,
    pub(crate) early_end_turn_pending: bool,
    pub(crate) skip_monster_turn_pending: bool,
    pub(crate) end_turn_hand_discarded: bool,
    pub(crate) player_escaping: bool,
    pub(crate) escape_pending_reward: bool,
}
