use super::context::DeckContext;
use crate::content::cards::{get_card_definition, CardId, CardType};
use crate::content::relics::RelicId;
use crate::runtime::combat::{CombatCard, MasterDeckLineageAllocator};

pub enum DeckAction {
    PreventObtain,
    GainGold(i32),
    GainMaxHp(i32),
    LoseMaxHp(i32),
    UpdateRelicCounter(RelicId, i32),
    ReaddCardToMasterDeck(CardId),
}

pub struct ObtainResult {
    pub final_cards: Vec<CombatCard>,
    pub actions: Vec<DeckAction>,
}

pub struct RemoveResult {
    pub actions: Vec<DeckAction>,
}

pub struct DeckManager;

impl DeckManager {
    pub fn preview_obtain_upgrades(ctx: &DeckContext, card_id: CardId, current_upgrades: u8) -> u8 {
        if current_upgrades > 0 {
            return current_upgrades;
        }

        let def = get_card_definition(card_id);
        let should_upgrade = match def.card_type {
            CardType::Attack => ctx.has_molten_egg,
            CardType::Skill => ctx.has_toxic_egg,
            CardType::Power => ctx.has_frozen_egg,
            _ => false,
        };

        if should_upgrade {
            1
        } else {
            current_upgrades
        }
    }

    /// Simulates Soul.java/AbstractCard obtain hooks
    pub fn obtain_card(
        ctx: &DeckContext,
        card_id: CardId,
        lineage_allocator: &mut MasterDeckLineageAllocator,
        pre_upgrades: u8,
    ) -> ObtainResult {
        Self::obtain_card_impl(ctx, card_id, lineage_allocator, pre_upgrades, true)
    }

    /// Simulates Java paths that manually call relic `onObtainCard` and then
    /// add the card to the master deck, without the Soul/obtain interception
    /// layer. Notably, Omamori does not block these cards in Java.
    pub fn obtain_card_without_interception(
        ctx: &DeckContext,
        card_id: CardId,
        lineage_allocator: &mut MasterDeckLineageAllocator,
        pre_upgrades: u8,
    ) -> ObtainResult {
        Self::obtain_card_impl(ctx, card_id, lineage_allocator, pre_upgrades, false)
    }

    fn obtain_card_impl(
        ctx: &DeckContext,
        card_id: CardId,
        lineage_allocator: &mut MasterDeckLineageAllocator,
        pre_upgrades: u8,
        allow_interception: bool,
    ) -> ObtainResult {
        let def = get_card_definition(card_id);
        let mut actions = Vec::new();

        let is_curse = def.card_type == CardType::Curse;

        // --- 1. Interception Phase (Omamori) ---
        if allow_interception && is_curse && ctx.has_omamori && ctx.omamori_charges > 0 {
            actions.push(DeckAction::PreventObtain);
            actions.push(DeckAction::UpdateRelicCounter(
                RelicId::Omamori,
                ctx.omamori_charges - 1,
            ));
            return ObtainResult {
                final_cards: Vec::new(),
                actions,
            };
        }

        // --- 2. Side Effect Checks ---
        if is_curse && ctx.has_darkstone_periapt {
            actions.push(DeckAction::GainMaxHp(6));
        }

        if ctx.has_ceramic_fish {
            actions.push(DeckAction::GainGold(9));
        }

        // --- 3. Modification Phase (Eggs) ---
        let preview_upgrades = Self::preview_obtain_upgrades(ctx, card_id, pre_upgrades);
        let copy_count = if ctx.has_hoarder_mod { 3 } else { 1 };
        let mut final_cards = Vec::new();

        for _ in 0..copy_count {
            let mut card = CombatCard::with_lineage(card_id, lineage_allocator.allocate());

            card.upgrades = preview_upgrades;

            final_cards.push(card);
        }

        ObtainResult {
            final_cards,
            actions,
        }
    }

    /// Simulates CardGroup.java removeCard / AbstractCard onRemoveFromMasterDeck hooks
    pub fn remove_card(target_id: CardId) -> RemoveResult {
        let mut actions = Vec::new();

        // 1. Parasite loses max hp
        if target_id == CardId::Parasite {
            actions.push(DeckAction::LoseMaxHp(
                crate::content::cards::curses::parasite::REMOVAL_MAX_HP_LOSS,
            ));
        }

        // 2. Necronomicurse replicates itself
        if target_id == CardId::Necronomicurse {
            // Java Necronomicurse.onRemoveFromMasterDeck starts a
            // NecronomicurseEffect, which directly inserts a fresh curse into
            // masterDeck. It is not an ordinary Soul/ShowCardAndObtainEffect
            // obtain and must not run Omamori, Darkstone, Ceramic Fish, or Egg
            // obtain hooks.
            actions.push(DeckAction::ReaddCardToMasterDeck(CardId::Necronomicurse));
        }

        RemoveResult { actions }
    }
}
