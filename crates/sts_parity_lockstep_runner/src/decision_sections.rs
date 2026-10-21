//! Read-only views of the same source-bound actions. Never rebuild action handles.
use crate::{DecisionSectionV1, DecisionStateV1};
use sts_parity_lockstep::{DecisionSurfaceV1, SemanticActionSpaceV1, SemanticActionV1};

impl DecisionSectionV1 {
    pub fn is_action_section(self) -> bool {
        matches!(self, Self::Potions | Self::Shop | Self::Removal)
    }

    pub fn choices(self, source: &SemanticActionSpaceV1) -> SemanticActionSpaceV1 {
        let actions = source
            .actions
            .iter()
            .filter(|entry| {
                let action = &entry.action;
                let shop_choice = match action {
                    SemanticActionV1::Select {
                        surface: DecisionSurfaceV1::Shop,
                        choice,
                    } => Some(choice),
                    _ => None,
                };
                match self {
                    Self::Potions => matches!(
                        action,
                        SemanticActionV1::UsePotion { .. } | SemanticActionV1::DiscardPotion { .. }
                    ),
                    Self::Shop => shop_choice.is_some_and(|choice| {
                        matches!(
                            choice.kind.as_str(),
                            "shop_card" | "shop_relic" | "shop_potion"
                        )
                    }),
                    Self::Removal => shop_choice.is_some_and(|choice| choice.kind == "deck_card"),
                    Self::Position | Self::Map | Self::Board => {
                        !matches!(action, SemanticActionV1::DiscardPotion { .. })
                            && !shop_choice.is_some_and(|choice| {
                                matches!(
                                    choice.kind.as_str(),
                                    "shop_card" | "shop_relic" | "shop_potion" | "deck_card"
                                )
                            })
                    }
                }
            })
            .cloned()
            .collect();
        SemanticActionSpaceV1 { actions }
    }

    pub fn render(self, state: &DecisionStateV1) -> Result<Option<String>, String> {
        Ok(match self {
            Self::Position => None,
            Self::Map => Some(state.render_map()),
            Self::Board => Some(
                crate::decision_match::render(&state.room, None)
                    .unwrap_or_else(|| "No active match board".into()),
            ),
            Self::Potions => Some(state.render_potions()?),
            Self::Shop => Some("Shop purchases".into()),
            Self::Removal => Some(
                state
                    .shop
                    .as_ref()
                    .and_then(|shop| shop.purge.as_ref())
                    .map(|purge| format!("Card removal: {} gold", purge.price))
                    .unwrap_or_else(|| "Card removal".into()),
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sts_parity_lockstep::SemanticChoiceV1;

    #[test]
    fn folded_groups_preserve_exact_source_handles_and_game_actions() {
        let mut actions = vec![
            SemanticActionV1::DiscardPotion {
                slot: 0,
                potion_id: "blockpotion".into(),
            },
            SemanticActionV1::UsePotion {
                slot: 0,
                potion_id: "blockpotion".into(),
                target: None,
            },
            SemanticActionV1::Cancel {
                surface: DecisionSurfaceV1::Shop,
            },
        ];
        for (kind, id) in [
            ("shop_card", "armaments"),
            ("shop_relic", "bagofpreparation"),
            ("shop_potion", "strengthpotion"),
            ("deck_card", "starter/1"),
            ("deck_card", "starter/2"),
        ] {
            actions.push(SemanticActionV1::Select {
                surface: DecisionSurfaceV1::Shop,
                choice: SemanticChoiceV1::new(kind, id, 0),
            });
        }
        let mut source = SemanticActionSpaceV1::new(actions).unwrap();
        // Deliberately opaque references: presentation may not replace or shorten them.
        for entry in &mut source.actions {
            entry.handle = format!("source-view:{}", entry.handle);
        }
        let before = source.clone();
        let ordinary = DecisionSectionV1::Position.choices(&source);
        assert_eq!(ordinary.actions.len(), 2); // Back and the useful potion-use action.
        let potions = DecisionSectionV1::Potions.choices(&source);
        assert_eq!(potions.actions.len(), 2);
        let shop = DecisionSectionV1::Shop.choices(&source);
        assert_eq!(shop.actions.len(), 3);
        let removal = DecisionSectionV1::Removal.choices(&source);
        assert_eq!(removal.actions.len(), 2);
        for group in [&ordinary, &potions, &shop, &removal] {
            for entry in &group.actions {
                assert!(source.actions.contains(entry));
            }
        }
        assert_ne!(removal.actions[0].handle, removal.actions[1].handle);
        assert_eq!(source, before);
        assert!(DecisionSectionV1::Shop.choices(&potions).actions.is_empty());
    }
}
