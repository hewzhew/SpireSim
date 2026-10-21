use crate::{
    DomainObservationV1, ParityDomainV1, SemanticActionV1, SemanticChoiceV1, SemanticScalarV1,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

pub const ACTION_SPACE_SCHEMA_V1: &str = "sts-lockstep-action-space-v1";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticActionHandleV1 {
    pub handle: String,
    pub action: SemanticActionV1,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticActionSpaceV1 {
    pub actions: Vec<SemanticActionHandleV1>,
}

impl SemanticActionSpaceV1 {
    pub fn from_snapshot(snapshot: &crate::ParitySnapshotV1) -> Result<Self, String> {
        let observation = snapshot
            .domains
            .get(&ParityDomainV1::ActionSpace)
            .ok_or_else(|| "current snapshot omitted its action space".to_owned())?;
        let crate::DomainEvidenceV1::Observed { schema, value } = &observation.evidence else {
            return Err("current action space is unobserved".to_owned());
        };
        if schema != ACTION_SPACE_SCHEMA_V1 {
            return Err(format!(
                "unsupported current action-space schema {schema:?}"
            ));
        }
        let actions = serde_json::from_value::<Vec<SemanticActionV1>>(value.clone())
            .map_err(|error| format!("current action space is malformed: {error}"))?;
        Self::new(actions)
    }

    /// Builds one canonical action space. Runtime UI order is deliberately
    /// ignored; repeated semantic choices must carry explicit occurrences.
    pub fn new(actions: impl IntoIterator<Item = SemanticActionV1>) -> Result<Self, String> {
        let mut keyed = actions
            .into_iter()
            .map(|action| {
                action.validate()?;
                let key = serde_json::to_string(&action)
                    .map_err(|error| format!("failed to encode semantic action: {error}"))?;
                Ok((key, action))
            })
            .collect::<Result<Vec<_>, String>>()?;
        keyed.sort_by(|left, right| left.0.cmp(&right.0));
        if keyed.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err("semantic action space contains a duplicate action".to_owned());
        }
        let actions = keyed
            .into_iter()
            .enumerate()
            .map(|(index, (_, action))| SemanticActionHandleV1 {
                handle: format!("A{index}"),
                action,
            })
            .collect();
        Ok(Self { actions })
    }

    pub fn resolve(&self, handle: &str) -> Option<&SemanticActionV1> {
        self.actions
            .iter()
            .find(|candidate| candidate.handle == handle)
            .map(|candidate| &candidate.action)
    }

    pub fn domain_observation(&self) -> Result<(ParityDomainV1, DomainObservationV1), String> {
        let value = serde_json::to_value(
            self.actions
                .iter()
                .map(|handle| &handle.action)
                .collect::<Vec<_>>(),
        )
        .map_err(|error| format!("failed to project semantic action space: {error}"))?;
        Ok((
            ParityDomainV1::ActionSpace,
            DomainObservationV1::observed(ACTION_SPACE_SCHEMA_V1, value),
        ))
    }

    pub fn compact_lines(&self) -> Vec<String> {
        self.actions
            .iter()
            .map(|handle| format!("{} {}", handle.handle, action_summary(&handle.action)))
            .collect()
    }
}

fn action_summary(action: &SemanticActionV1) -> String {
    match action {
        SemanticActionV1::Select { surface, choice } => {
            format!("select {surface:?} {}", choice_summary(choice)).to_lowercase()
        }
        SemanticActionV1::SubmitSelection { surface, choices } => format!(
            "submit {:?} [{}]",
            surface,
            choices
                .iter()
                .map(choice_summary)
                .collect::<Vec<_>>()
                .join(", ")
        )
        .to_lowercase(),
        SemanticActionV1::PlayCard {
            card_id, target, ..
        } => target.as_ref().map_or_else(
            || format!("play {card_id}"),
            |target| format!("play {card_id} -> {}", choice_summary(target)),
        ),
        SemanticActionV1::UsePotion {
            slot,
            potion_id,
            target,
        } => target.as_ref().map_or_else(
            || format!("use potion[{slot}] {potion_id}"),
            |target| {
                format!(
                    "use potion[{slot}] {potion_id} -> {}",
                    choice_summary(target)
                )
            },
        ),
        SemanticActionV1::DiscardPotion { slot, potion_id } => {
            format!("discard potion[{slot}] {potion_id}")
        }
        SemanticActionV1::EndTurn => "end turn".to_owned(),
        SemanticActionV1::Proceed { surface } => format!("proceed {surface:?}").to_lowercase(),
        SemanticActionV1::Cancel { surface } => format!("cancel {surface:?}").to_lowercase(),
    }
}

fn choice_summary(choice: &SemanticChoiceV1) -> String {
    let mut detail = format!("{}:{}", choice.kind, choice.id);
    if choice.occurrence > 0 {
        detail.push_str(&format!("#{}", choice.occurrence));
    }
    let attributes = choice
        .attributes
        .iter()
        .map(|(key, value)| format!("{key}={}", scalar_summary(value)))
        .collect::<Vec<_>>();
    if !attributes.is_empty() {
        detail.push_str(&format!(" ({})", attributes.join(", ")));
    }
    detail
}

fn scalar_summary(value: &SemanticScalarV1) -> String {
    match value {
        SemanticScalarV1::Text(value) => value.clone(),
        SemanticScalarV1::Integer(value) => value.to_string(),
        SemanticScalarV1::Boolean(value) => value.to_string(),
    }
}

pub fn choice_with_attributes_v1(
    kind: impl Into<String>,
    id: impl Into<String>,
    occurrence: u32,
    attributes: impl IntoIterator<Item = (String, SemanticScalarV1)>,
) -> SemanticChoiceV1 {
    SemanticChoiceV1 {
        kind: kind.into(),
        id: id.into(),
        occurrence,
        attributes: attributes.into_iter().collect::<BTreeMap<_, _>>(),
    }
}

pub fn action_space_value_v1(space: &SemanticActionSpaceV1) -> Result<Value, String> {
    space
        .domain_observation()
        .and_then(|(_, observation)| match observation.evidence {
            crate::DomainEvidenceV1::Observed { value, .. } => Ok(value),
            crate::DomainEvidenceV1::Unobserved { .. } => {
                Err("action space projection was unexpectedly unobserved".to_owned())
            }
        })
}
