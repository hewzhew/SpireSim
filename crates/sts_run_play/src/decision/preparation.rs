//! Source-bound display blocks for preparation clients. All rules come from
//! existing native renderers; selection uses typed identities, never headings.
use super::*;
use sts_oracle_learning::content::cards::{self, CardId};
use sts_presentation::Locale;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CardRuleReadingV1 {
    pub card: CardId,
    pub upgrades: u8,
    pub available: bool,
    pub view: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CandidateReadingV1 {
    pub command: String,
    pub label: String,
    pub view: String,
    pub card_rules: Vec<CardRuleReadingV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ShopOfferReadingV1 {
    pub action: TaskStrategicActionV1,
    pub command: Option<String>,
    pub label: String,
    pub view: String,
    pub card_rules: Vec<CardRuleReadingV1>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PreparationReadingV1 {
    pub schema: &'static str,
    pub public_snapshot_id: String,
    pub overview: String,
    pub selections: String,
    pub candidates: Vec<CandidateReadingV1>,
    pub inventory: crate::compact::InventoryReadingV1,
    pub stock: Vec<ShopOfferReadingV1>,
    pub routes: String,
}

fn card_rules(action: &TaskStrategicActionV1, locale: Locale) -> Vec<CardRuleReadingV1> {
    crate::agent_protocol::acquired_card_mechanics_for_action_v1(action)
        .iter()
        .map(|facts| {
            let (card, upgrades) = (facts.current.card, facts.current.upgrades);
            let mut view = String::new();
            crate::mechanics_info::write_printed_card_rule(&mut view, card, upgrades, locale);
            CardRuleReadingV1 {
                card,
                upgrades,
                available: cards::mechanics::explanation(card).is_some(),
                view,
            }
        })
        .collect()
}

pub(crate) fn reading(
    decision: &TaskPublicDecisionV1,
    locale: Locale,
) -> Result<Option<PreparationReadingV1>, String> {
    let TaskPublicObservationV1::Strategic { observation } = &decision.observation else {
        return Ok(None);
    };
    let rendered = super::read_public_decision_v1(
        decision,
        &[DecisionDetailV1::Options],
        None,
        None,
        None,
        locale,
    )?;
    let mut overview = rendered.summary.clone();
    crate::mechanics_info::write_preparation_window(&mut overview, observation);
    crate::mechanics_info::write_key_status(
        &mut overview,
        observation.run.keys,
        observation.run_goal == sts_oracle_learning::ai::planner_core::PlannerRunGoal::HeartVictory,
    );
    let mut selections = String::new();
    super::render_selections(
        &mut selections,
        decision,
        &rendered.selections,
        false,
        locale,
    )?;
    let upgrades = super::smith::comparisons(observation, &rendered.options)?;
    let mut candidates = Vec::new();
    for (ordinal, option) in rendered.options.iter().enumerate() {
        let TaskPublicCandidateV1::Strategic { action } = &option.candidate else {
            continue;
        };
        let command = crate::selector::semantic_action_for_choice_v1(decision, &option.choice)?;
        let mut label = option.label.clone();
        if let Some(upgrade) = upgrades
            .iter()
            .find(|group| group.ordinals.contains(&ordinal))
        {
            write!(label, " | {}", upgrade.changes).unwrap();
        }
        let mut view = format!("  Effect label: {label}\n");
        super::write_choice_mechanics(&mut view, action, false);
        super::write_event_resource_modifiers(&mut view, action, observation);
        candidates.push(CandidateReadingV1 {
            command,
            label,
            view,
            card_rules: card_rules(action, locale),
        });
    }
    let mut stock = Vec::new();
    if let PlannerDecisionContext::Shop { inventory, .. } = &observation.context {
        for (action, name, price, status) in super::shop_offers(inventory) {
            let legal = rendered.options.iter().filter(|option|
                matches!(&option.candidate, TaskPublicCandidateV1::Strategic { action: supplied } if *supplied == action)
            ).collect::<Vec<_>>();
            let command = match (status, legal.as_slice()) {
                (PlannerShopPurchaseStatusV1::Purchasable, [option]) => Some(
                    crate::selector::semantic_action_for_choice_v1(decision, &option.choice)?,
                ),
                (PlannerShopPurchaseStatusV1::Blocked { .. }, []) => None,
                _ => return Err("Shop presentation and supplied purchase choices disagree".into()),
            };
            let label = format!("{name} {price}g{}", super::purchase_suffix(status));
            let mut view = format!("  {label}\n");
            if let TaskStrategicActionV1::BuyCard { card, upgrades, .. } = &action {
                crate::mechanics_info::write_printed_card_rule(&mut view, *card, *upgrades, locale);
            } else {
                super::write_choice_mechanics(&mut view, &action, false);
            }
            stock.push(ShopOfferReadingV1 {
                command,
                label,
                view,
                card_rules: card_rules(&action, locale),
                action,
            });
        }
    }
    let routes = if observation.run.pending_act_entry.is_none() {
        crate::public_map_reading::preparation_routes(&observation.public_map)?.unwrap_or_default()
    } else {
        String::new()
    };
    Ok(Some(PreparationReadingV1 {
        schema: "sts-preparation-reading-v1",
        public_snapshot_id: decision.information.snapshot_id.clone(),
        overview,
        selections,
        candidates,
        inventory: crate::compact::inventory_reading_decision_v1(
            decision.clone(),
            true,
            None,
            locale,
        )?,
        stock,
        routes,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::agent_protocol::{AgentCapabilitiesV1, AgentPermissionEnvelopeV1};
    use crate::decision_tests::{controller_with_session, fixture, TestDirectory};

    #[test]
    fn preparation_locale_changes_inventory_text_while_preserving_public_actions_and_source() {
        let dir = TestDirectory::new();
        let game = controller_with_session(&dir.0, "preparation-locale", &fixture("shop_twins"));
        let before = game.head().clone();
        let capabilities =
            AgentCapabilitiesV1::new(AgentPermissionEnvelopeV1::public_manual_v1(None));
        let en = crate::agent_protocol::project_agent_observation_v1(
            &before,
            capabilities.clone(),
            None,
            Locale::En,
        )
        .unwrap();
        let zh = crate::agent_protocol::project_agent_observation_v1(
            &before,
            capabilities,
            None,
            Locale::ZhCn,
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&en.position).unwrap(),
            serde_json::to_value(&zh.position).unwrap()
        );
        let en = en.preparation_reading.unwrap();
        let zh = zh.preparation_reading.unwrap();
        assert_eq!(en.public_snapshot_id, zh.public_snapshot_id);
        assert!(en.inventory.view.starts_with("Deck ("));
        assert!(zh.inventory.view.starts_with("永久卡组（"));
        assert_eq!(
            en.candidates
                .iter()
                .map(|row| &row.command)
                .collect::<Vec<_>>(),
            zh.candidates
                .iter()
                .map(|row| &row.command)
                .collect::<Vec<_>>()
        );
        assert_eq!(
            en.stock
                .iter()
                .map(|row| (&row.action, &row.command))
                .collect::<Vec<_>>(),
            zh.stock
                .iter()
                .map(|row| (&row.action, &row.command))
                .collect::<Vec<_>>()
        );
        assert_eq!(*game.head(), before);
    }

    #[test]
    fn preparation_blocks_preserve_their_native_inventory_and_legal_bindings() {
        for kind in [
            "reward",
            "shop_twins",
            "shop_poor",
            "full_reward",
            "empty_cage",
            "route",
            "campfire",
        ] {
            let dir = TestDirectory::new();
            let mut session = fixture(if kind == "campfire" { "reward" } else { kind });
            if kind == "campfire" {
                session.engine_state = sts_oracle_learning::state::core::EngineState::Campfire;
                session.run_state.master_deck.clear();
                session.run_state.add_card_to_deck(CardId::Corruption);
            }
            let game = controller_with_session(&dir.0, kind, &session);
            let head = game.head().clone();
            let public = head
                .current_decision
                .as_ref()
                .unwrap()
                .public_decision_v1()
                .unwrap();
            let capabilities = AgentCapabilitiesV1::new(
                AgentPermissionEnvelopeV1::a20_first_unrestricted_v1(None),
            );
            let observation = crate::agent_protocol::project_agent_observation_v1(
                &head,
                capabilities,
                None,
                Locale::En,
            )
            .unwrap();
            let projection = observation.preparation_reading.as_ref().unwrap();
            assert_eq!(
                projection.public_snapshot_id,
                head.current_decision.as_ref().unwrap().public_snapshot_id
            );
            assert_eq!(
                projection.inventory.view,
                crate::compact::render_inventory_decision_v1(
                    public.clone(),
                    true,
                    None,
                    Locale::En
                )
                .unwrap()
            );
            assert!(projection
                .inventory
                .view
                .contains(&projection.inventory.resources));
            for block in &projection.inventory.cards {
                assert!(projection.inventory.view.contains(&block.view));
            }
            for block in &projection.inventory.relics {
                assert!(projection.inventory.resources.contains(&block.view));
            }
            for candidate in &projection.candidates {
                let selector = crate::selector::parse_action_selector_v1(
                    &candidate.command,
                    "preparation test",
                )
                .unwrap();
                let choice =
                    crate::selector::resolve_action_selector_v1(&public, &selector).unwrap();
                public.validate_choice(&choice).unwrap();
            }
            for offer in &projection.stock {
                let matching = projection
                    .candidates
                    .iter()
                    .filter(|candidate| Some(&candidate.command) == offer.command.as_ref())
                    .count();
                assert_eq!(matching, usize::from(offer.command.is_some()));
            }
            if kind == "shop_poor" {
                assert!(!projection.stock.is_empty());
                assert!(projection.stock.iter().all(|offer| offer.command.is_none()));
            }
            if kind == "campfire" {
                assert!(projection
                    .candidates
                    .iter()
                    .any(|candidate| candidate.view.contains("cost 3c -> 2c")));
            }
            assert_eq!(*game.head(), head, "projection must not execute or search");
            if let Ok(output) = std::env::var("STS_PREPARATION_FIXTURE_OUTPUT") {
                let file = std::path::Path::new(&output).join(format!("{kind}.json"));
                use std::io::Write;
                let mut saved = std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(file)
                    .unwrap();
                saved
                    .write_all(&serde_json::to_vec_pretty(&observation).unwrap())
                    .unwrap();
            }
        }
    }

    #[test]
    fn combat_has_no_strategic_preparation_projection() {
        let dir = TestDirectory::new();
        let game = controller_with_session(&dir.0, "combat", &fixture("combat_multiturn"));
        let public = game
            .head()
            .current_decision
            .as_ref()
            .unwrap()
            .public_decision_v1()
            .unwrap();
        assert!(reading(&public, Locale::En).unwrap().is_none());
    }
}
