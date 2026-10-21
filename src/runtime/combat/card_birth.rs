use super::*;

/// Typed cause owned by one observed combat transition.
///
/// The enum is deliberately closed. A caller that cannot name the actual
/// producer must leave birth observation disabled instead of labeling every
/// generated card with the top-level action by guesswork.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "kind")]
pub enum CombatCardBirthCause {
    /// Births between a typed room-entry action and the first combat input.
    CombatEntryResolution,
    CardPlay {
        producer_lineage: CardLineageId,
    },
    /// All card births enclosed by one exact EndTurn-to-stable transition.
    ///
    /// This names the observed transition scope, not a guessed lower-level
    /// enemy, relic, or power producer.
    EndTurnResolution,
    /// Births enclosed by resolving a Discovery selection or its skip action.
    DiscoveryResolution,
    /// Fresh cards created while the queue resumes after a combat hand/grid/Codex
    /// selection. The adjacent typed choice owns the scope, not the selected card.
    CombatSelectionResolution,
    PotionUse {
        potion_id: crate::content::potions::PotionId,
        slot: usize,
    },
    RelicTrigger {
        relic_id: crate::content::relics::RelicId,
    },
    PowerTrigger {
        power_id: PowerId,
        owner: crate::EntityId,
    },
    EncounterRule {
        encounter_id: crate::content::monsters::factory::EncounterId,
    },
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CombatCardBirthZone {
    Draw,
    Hand,
    Discard,
    Exhaust,
    Limbo,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCardBirthProduct {
    pub generation_index: u32,
    pub card: crate::state::DomainCardSnapshot,
    pub first_zone: Option<CombatCardBirthZone>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autoplay_copy: Option<CombatCardBirthAutoplayCopy>,
}

/// An actual Omniscience stat-equivalent copy, observed when it is created for
/// the pending auto-play action. It may never enter a CardGroup.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCardBirthAutoplayCopy {
    pub producer_lineage: CardLineageId,
    pub purged: bool,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CombatCardBirthReceipt {
    pub cause: CombatCardBirthCause,
    pub allocated_count: u32,
    pub complete: bool,
    pub products: Vec<CombatCardBirthProduct>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unsupported_reason: Option<String>,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub(crate) struct CombatCardBirthObservation {
    cause: Option<CombatCardBirthCause>,
    first_ordinal: u32,
    products: Vec<CombatCardBirthProduct>,
}

impl CombatState {
    pub fn begin_card_birth_observation(&mut self, cause: CombatCardBirthCause) {
        self.runtime.card_birth_observation = CombatCardBirthObservation {
            cause: Some(cause),
            first_ordinal: self.zones.combat_lineage_allocator.next_creation_ordinal(),
            products: Vec::new(),
        };
    }

    pub(crate) fn observe_card_entering_zone(
        &mut self,
        card: &CombatCard,
        first_zone: CombatCardBirthZone,
    ) {
        let observation = &mut self.runtime.card_birth_observation;
        if observation.cause.is_none()
            || card.lineage.origin() != CardLineageOrigin::CombatGenerated
            || card.lineage.ordinal() < observation.first_ordinal
            || observation
                .products
                .iter()
                .any(|product| product.card.lineage == card.lineage)
        {
            return;
        }
        observation.products.push(CombatCardBirthProduct {
            generation_index: card.lineage.ordinal() - observation.first_ordinal,
            card: crate::state::DomainCardSnapshot {
                id: card.id,
                upgrades: card.upgrades,
                lineage: card.lineage,
            },
            first_zone: Some(first_zone),
            autoplay_copy: None,
        });
    }

    pub(crate) fn observe_omniscience_copy(&mut self, producer: &CombatCard, copy: &CombatCard) {
        let observation = &mut self.runtime.card_birth_observation;
        if observation.cause.is_none() {
            return;
        }
        observation.products.push(CombatCardBirthProduct {
            generation_index: copy.lineage.ordinal() - observation.first_ordinal,
            card: crate::state::DomainCardSnapshot {
                id: copy.id,
                upgrades: copy.upgrades,
                lineage: copy.lineage,
            },
            first_zone: None,
            autoplay_copy: Some(CombatCardBirthAutoplayCopy {
                producer_lineage: producer.lineage,
                purged: false,
            }),
        });
    }

    pub(crate) fn observe_autoplay_copy_purged(&mut self, card: &CombatCard) {
        for product in &mut self.runtime.card_birth_observation.products {
            if product.card.lineage == card.lineage {
                if let Some(copy) = &mut product.autoplay_copy {
                    copy.purged = true;
                }
            }
        }
    }

    fn retains_autoplay_copy(&self, lineage: CardLineageId) -> bool {
        use crate::runtime::action::Action;
        self.zones
            .queued_cards
            .iter()
            .any(|item| item.card.lineage == lineage)
            || self.engine.action_queue.iter().any(|action| match action {
                Action::EnqueueCardPlay { item, .. } => item.card.lineage == lineage,
                Action::PlayCardDirect { card, .. } | Action::UseCardAfterUseHooks { card } => {
                    card.lineage == lineage
                }
                _ => false,
            })
    }

    fn copy_in_card_zone(&self, lineage: CardLineageId) -> bool {
        self.zones
            .hand
            .iter()
            .chain(self.zones.draw_pile.iter())
            .chain(self.zones.discard_pile.iter())
            .chain(self.zones.exhaust_pile.iter())
            .chain(self.zones.limbo.iter())
            .any(|card| card.lineage == lineage)
    }

    pub fn take_card_birth_observation(&mut self) -> Option<CombatCardBirthReceipt> {
        let mut observation = std::mem::take(&mut self.runtime.card_birth_observation);
        let cause = observation.cause.take()?;
        observation
            .products
            .sort_unstable_by_key(|product| product.generation_index);
        let allocated_count = self
            .zones
            .combat_lineage_allocator
            .next_creation_ordinal()
            .saturating_sub(observation.first_ordinal);
        let complete = usize::try_from(allocated_count)
            .is_ok_and(|count| count == observation.products.len())
            && observation
                .products
                .iter()
                .enumerate()
                .all(|(index, product)| {
                    u32::try_from(index) == Ok(product.generation_index)
                        && match (&product.first_zone, &product.autoplay_copy) {
                            (Some(_), None) => true,
                            (None, Some(copy)) => {
                                copy.producer_lineage != product.card.lineage
                                    && !self.copy_in_card_zone(product.card.lineage)
                                    && (copy.purged
                                        != self.retains_autoplay_copy(product.card.lineage))
                            }
                            _ => false,
                        }
                });
        Some(CombatCardBirthReceipt {
            cause,
            allocated_count,
            complete,
            products: observation.products,
            unsupported_reason: (!complete).then(|| {
                "one or more allocated combat-generated lineages lack a zone entry or a witnessed auto-play copy lifecycle"
                    .to_owned()
            }),
        })
    }

    pub fn clear_card_birth_observation(&mut self) {
        self.runtime.card_birth_observation = CombatCardBirthObservation::default();
    }
}
