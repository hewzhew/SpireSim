use crate::{
    action::{astrolabe_grid_selection_v1, JavaActionExecutionV1},
    projection::JavaStableCardBindingV1,
    transport::JavaRuntimeObservationV1,
};
use std::collections::{BTreeMap, BTreeSet};
use sts_parity_lockstep::{
    natural_run_card_binding_v1, DecisionSurfaceV1, NaturalRunCardSourceV1, SemanticActionV1,
};

pub(crate) struct MasterDeckTransformContext {
    owner: &'static str,
    prior_uuids: BTreeSet<String>,
    parents: BTreeMap<String, JavaStableCardBindingV1>,
}

impl MasterDeckTransformContext {
    pub(crate) fn prepare(
        observation: &JavaRuntimeObservationV1,
        semantic: &SemanticActionV1,
        execution: &JavaActionExecutionV1,
        runtime_bindings: &BTreeMap<String, String>,
        rebindings: &BTreeMap<String, JavaStableCardBindingV1>,
    ) -> Result<Option<Self>, String> {
        let (owner, selected) = match semantic {
            SemanticActionV1::Select {
                surface: DecisionSurfaceV1::GridSelect,
                choice,
            } => {
                let Some(mut selected) = astrolabe_grid_selection_v1(observation)? else {
                    return Ok(None);
                };
                if selected.len() != 2 {
                    return Ok(None);
                }
                let JavaActionExecutionV1::SubmitGridSelect {
                    selected_card_uuids,
                } = execution
                else {
                    return Err("Astrolabe omitted its next original grid click".into());
                };
                let [last] = selected_card_uuids.as_slice() else {
                    return Err("Astrolabe requires one next grid click".into());
                };
                if runtime_bindings.get(last) != Some(&choice.id) || selected.contains(last) {
                    return Err("Astrolabe final click disagrees with its source-bound card".into());
                }
                selected.push(last.clone());
                ("astrolabe", selected)
            }
            SemanticActionV1::SubmitSelection {
                surface: DecisionSurfaceV1::GridSelect,
                choices,
            } if observation.frame["public_base"]["decision"]["event_id"].as_str()
                == Some("DrugDealer")
                && observation.frame["public_base"]["decision"]["grid_select"]
                    ["selection_reason"]
                    .as_str()
                    == Some("DeckTransform") =>
            {
                let identity = crate::action::java_grid_select_boundary_identity_v1(observation)?;
                let JavaActionExecutionV1::SubmitGridSelect {
                    selected_card_uuids,
                } = execution
                else {
                    return Err("DrugDealer omitted its original grid submission".into());
                };
                if identity.event.screen != 1
                    || choices.len() != 2
                    || selected_card_uuids.len() != 2
                    || selected_card_uuids
                        .iter()
                        .zip(choices)
                        .any(|(uuid, choice)| runtime_bindings.get(uuid) != Some(&choice.id))
                {
                    return Err(
                        "DrugDealer requires two source-bound cards at its transform boundary"
                            .into(),
                    );
                }
                ("drugdealer", selected_card_uuids.clone())
            }
            _ => return Ok(None),
        };
        let count = selected.len();
        let cards = observation.frame["parity_internal"]["master_deck"]
            .as_array()
            .ok_or("Master-deck transform predecessor deck missing")?;
        let mut prior_uuids = BTreeSet::new();
        let mut parents = BTreeMap::new();
        for card in cards {
            let uuid = card["card_uuid"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Master-deck transform predecessor UUID missing")?;
            if !prior_uuids.insert(uuid.to_owned()) {
                return Err("Master-deck transform predecessor repeats a UUID".into());
            }
            if !selected.iter().any(|s| s == uuid) {
                continue;
            }
            let binding = match rebindings.get(uuid) {
                Some(existing) => existing.clone(),
                None => {
                    let source: NaturalRunCardSourceV1 =
                        serde_json::from_value(card["source"].clone()).map_err(|_| {
                            "Master-deck transform selected card lacks typed provenance"
                        })?;
                    JavaStableCardBindingV1 {
                        binding: natural_run_card_binding_v1(&source)?,
                        raw_source: Some(source.clone()),
                        source,
                    }
                }
            };
            if runtime_bindings.get(uuid) != Some(&binding.binding) {
                return Err(
                    "Master-deck transform selected provenance disagrees with its active binding"
                        .into(),
                );
            }
            parents.insert(uuid.to_owned(), binding);
        }
        if parents.len() != count
            || parents
                .values()
                .map(|b| &b.binding)
                .collect::<BTreeSet<_>>()
                .len()
                != count
        {
            return Err("Transform requires distinct source-bound predecessors".into());
        }
        Ok(Some(Self {
            owner,
            prior_uuids,
            parents,
        }))
    }

    /// Validate the complete replacement before installing any binding. Product
    /// order is deliberately irrelevant: the original transform call owns identity.
    pub(crate) fn bind(
        self,
        observation: &JavaRuntimeObservationV1,
        rebindings: &mut BTreeMap<String, JavaStableCardBindingV1>,
        combat_bindings: &BTreeMap<String, JavaStableCardBindingV1>,
    ) -> Result<(), String> {
        let cards = observation.frame["parity_internal"]["master_deck"]
            .as_array()
            .ok_or("Master-deck transform successor deck missing")?;
        let mut successor = BTreeSet::new();
        let mut witnessed_parents = BTreeSet::new();
        let mut products = BTreeMap::new();
        for card in cards {
            let uuid = card["card_uuid"]
                .as_str()
                .filter(|s| !s.is_empty())
                .ok_or("Master-deck transform successor UUID missing")?;
            if !successor.insert(uuid.to_owned()) {
                return Err("Master-deck transform successor repeats a UUID".into());
            }
            if self.prior_uuids.contains(uuid) {
                continue;
            }
            let origin = &card["transform_origin"];
            let parent = origin["parent_uuid"]
                .as_str()
                .ok_or("Master-deck transform product lacks its original transform witness")?;
            if origin["schema"].as_str() != Some("master_deck_transform_origin_v1")
                || origin["owner"].as_str() != Some(self.owner)
                || origin["product_uuid"].as_str() != Some(uuid)
                || !witnessed_parents.insert(parent.to_owned())
            {
                return Err(
                    "Master-deck transform product has a malformed or repeated transform witness"
                        .into(),
                );
            }
            let binding = self
                .parents
                .get(parent)
                .ok_or("Master-deck transform witness names an unselected predecessor")?;
            if rebindings.contains_key(uuid)
                || combat_bindings.contains_key(uuid)
                || rebindings
                    .iter()
                    .any(|(id, b)| !self.parents.contains_key(id) && b.binding == binding.binding)
            {
                return Err("Master-deck transform product collides with an active binding".into());
            }
            products.insert(uuid.to_owned(), binding.clone());
        }
        let expected = self.parents.keys().cloned().collect::<BTreeSet<_>>();
        if self
            .prior_uuids
            .difference(&successor)
            .cloned()
            .collect::<BTreeSet<_>>()
            != expected
            || witnessed_parents != expected
            || products.len() != self.parents.len()
        {
            return Err(
                "Master-deck transform witnesses do not cover the exact selected-card replacement"
                    .into(),
            );
        }
        for parent in self.parents.keys() {
            rebindings.remove(parent);
        }
        rebindings.extend(products);
        Ok(())
    }
}
