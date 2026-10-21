const PREFIX_PREVIEW_LIMIT: usize = 160;
const PREFIX_TOKEN_LIMIT: usize = 12;

#[derive(Clone, Debug, Default)]
pub(in crate::ai::combat_search_v2) struct TurnPrefixState {
    pub(super) prefix_length: u16,
    pub(super) cards_played: u16,
    pub(super) potions_used: u16,
    pub(super) potions_discarded: u16,
    pub(super) other_actions: u16,
    pub(super) origin_key: Option<String>,
    pub(super) tokens: Vec<String>,
    pub(super) tokens_truncated: bool,
    pub(super) signature_preview: String,
    pub(super) signature_truncated: bool,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct TurnPrefixSummary {
    pub(super) prefix: TurnPrefixState,
    pub(super) legal_actions: usize,
}

#[derive(Clone, Debug)]
pub(in crate::ai::combat_search_v2) struct TurnPrefixObservation {
    pub(super) observed_at_state_query: u64,
    pub(super) prefix: TurnPrefixState,
    pub(super) legal_actions: usize,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(in crate::ai::combat_search_v2) enum TurnPrefixKind {
    Empty,
    CardOnly,
    PotionOnly,
    CardAndPotion,
    WithDiscardPotion,
    WithOther,
}

impl TurnPrefixState {
    pub(super) fn kind(&self) -> TurnPrefixKind {
        if self.prefix_length == 0 {
            TurnPrefixKind::Empty
        } else if self.other_actions > 0 {
            TurnPrefixKind::WithOther
        } else if self.potions_discarded > 0 {
            TurnPrefixKind::WithDiscardPotion
        } else if self.cards_played > 0 && self.potions_used > 0 {
            TurnPrefixKind::CardAndPotion
        } else if self.cards_played > 0 {
            TurnPrefixKind::CardOnly
        } else if self.potions_used > 0 {
            TurnPrefixKind::PotionOnly
        } else {
            TurnPrefixKind::WithOther
        }
    }

    pub(in crate::ai::combat_search_v2) fn push_token(&mut self, token: &str) {
        if self.tokens.len() < PREFIX_TOKEN_LIMIT {
            self.tokens.push(token.to_string());
        } else {
            self.tokens_truncated = true;
        }

        if self.signature_truncated {
            return;
        }
        let separator_len = usize::from(!self.signature_preview.is_empty());
        let required_len = self.signature_preview.len() + separator_len + token.len();
        if required_len > PREFIX_PREVIEW_LIMIT {
            if self.signature_preview.len() + 4 <= PREFIX_PREVIEW_LIMIT {
                self.signature_preview.push_str(">...");
            }
            self.signature_truncated = true;
            return;
        }
        if !self.signature_preview.is_empty() {
            self.signature_preview.push('>');
        }
        self.signature_preview.push_str(token);
    }

    pub(in crate::ai::combat_search_v2) fn prefix_length(&self) -> usize {
        self.prefix_length as usize
    }

    pub(in crate::ai::combat_search_v2) fn origin_key(&self) -> Option<&str> {
        self.origin_key.as_deref()
    }

    pub(in crate::ai::combat_search_v2) fn ordered_sequence_key(&self) -> Option<String> {
        if self.prefix_length == 0 {
            return None;
        }
        Some(token_sequence_key(&self.tokens, self.tokens_truncated))
    }

    pub(in crate::ai::combat_search_v2) fn unordered_sequence_key(&self) -> Option<String> {
        if self.prefix_length == 0 {
            return None;
        }
        let mut tokens = self.tokens.clone();
        tokens.sort();
        Some(token_sequence_key(&tokens, self.tokens_truncated))
    }
}

impl TurnPrefixKind {
    pub(super) fn label(self) -> &'static str {
        match self {
            TurnPrefixKind::Empty => "empty",
            TurnPrefixKind::CardOnly => "card_only",
            TurnPrefixKind::PotionOnly => "potion_only",
            TurnPrefixKind::CardAndPotion => "card_and_potion",
            TurnPrefixKind::WithDiscardPotion => "with_discard_potion",
            TurnPrefixKind::WithOther => "with_other",
        }
    }
}

fn token_sequence_key(tokens: &[String], truncated: bool) -> String {
    let mut key = tokens.join(">");
    if truncated {
        if !key.is_empty() {
            key.push('>');
        }
        key.push_str("...");
    }
    key
}
