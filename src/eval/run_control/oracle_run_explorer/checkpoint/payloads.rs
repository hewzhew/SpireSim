use serde::{Deserialize, Serialize};

use crate::content::{potions::Potion, relics::RelicState};
use crate::eval::fingerprint::{hash_serializable, FINGERPRINT_ALGORITHM_JSON};
use crate::runtime::combat::CombatCard;
use crate::state::{
    map::{node::Map, state::MapState},
    run::RunStateScheduleCheckpointV1,
    selection::DomainEvent,
};

use super::super::OracleRunReplayStepV1;
use crate::eval::run_control::RunControlSessionCheckpointV1;

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunCheckpointPayloadRecordV1<T> {
    pub payload_id: String,
    pub value: T,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunCheckpointMapRecordV1 {
    pub payload_id: String,
    pub graph_id: String,
    pub map: MapState,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OracleRunCheckpointChainNodeV1<T> {
    pub parent: Option<usize>,
    pub value: T,
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OracleRunSessionPayloadRefsV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub map_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub master_deck_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub relics_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub potions_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub schedule_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emitted_events_tip: Option<usize>,
}

impl OracleRunSessionPayloadRefsV1 {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }
}

#[derive(Clone, Debug, Default, Deserialize, PartialEq, Serialize)]
#[serde(default, deny_unknown_fields)]
pub struct OracleRunCheckpointPayloadsV1 {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub fingerprint_algorithm: Option<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub map_graphs: Vec<OracleRunCheckpointPayloadRecordV1<Map>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub maps: Vec<OracleRunCheckpointMapRecordV1>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub master_decks: Vec<OracleRunCheckpointPayloadRecordV1<Vec<CombatCard>>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub relics: Vec<OracleRunCheckpointPayloadRecordV1<Vec<RelicState>>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub potions: Vec<OracleRunCheckpointPayloadRecordV1<Vec<Option<Potion>>>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub schedules: Vec<OracleRunCheckpointPayloadRecordV1<RunStateScheduleCheckpointV1>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub emitted_event_nodes: Vec<OracleRunCheckpointChainNodeV1<DomainEvent>>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub replay_nodes: Vec<OracleRunCheckpointChainNodeV1<OracleRunReplayStepV1>>,
}

impl OracleRunCheckpointPayloadsV1 {
    pub fn is_empty(&self) -> bool {
        self == &Self::default()
    }

    pub(crate) fn externalize_session(
        &mut self,
        checkpoint: &mut RunControlSessionCheckpointV1,
    ) -> Result<OracleRunSessionPayloadRefsV1, String> {
        self.ensure_write_fingerprint_algorithm()?;
        let map_id = self.intern_map(checkpoint.take_run_state_map_for_external_ref())?;
        let master_deck_id = intern_payload(
            checkpoint.take_run_state_master_deck_for_external_ref(),
            &mut self.master_decks,
            "master deck",
        )?;
        let relics_id = intern_payload(
            checkpoint.take_run_state_relics_for_external_ref(),
            &mut self.relics,
            "relics",
        )?;
        let potions_id = intern_payload(
            checkpoint.take_run_state_potions_for_external_ref(),
            &mut self.potions,
            "potions",
        )?;
        let schedule_id = intern_payload(
            checkpoint.take_run_state_schedule_for_external_ref(),
            &mut self.schedules,
            "run schedule",
        )?;
        let emitted_events = checkpoint.take_run_state_emitted_events_for_external_ref();
        let emitted_events_tip = append_chain(&mut self.emitted_event_nodes, &emitted_events);
        Ok(OracleRunSessionPayloadRefsV1 {
            map_id: Some(map_id),
            master_deck_id: Some(master_deck_id),
            relics_id: Some(relics_id),
            potions_id: Some(potions_id),
            schedule_id: Some(schedule_id),
            emitted_events_tip,
        })
    }

    pub(crate) fn hydrate_session(
        &self,
        checkpoint: &mut RunControlSessionCheckpointV1,
        refs: &OracleRunSessionPayloadRefsV1,
    ) -> Result<(), String> {
        self.validate_fingerprint_algorithm()?;
        refs.validate_complete_v1()?;
        if let Some(payload_id) = refs.map_id.as_deref() {
            checkpoint.restore_run_state_map_from_external_ref(self.resolve_map(payload_id)?);
        }
        if let Some(payload_id) = refs.master_deck_id.as_deref() {
            checkpoint.restore_run_state_master_deck_from_external_ref(resolve_payload(
                &self.master_decks,
                payload_id,
                "master deck",
            )?);
        }
        if let Some(payload_id) = refs.relics_id.as_deref() {
            checkpoint.restore_run_state_relics_from_external_ref(resolve_payload(
                &self.relics,
                payload_id,
                "relics",
            )?);
        }
        if let Some(payload_id) = refs.potions_id.as_deref() {
            checkpoint.restore_run_state_potions_from_external_ref(resolve_payload(
                &self.potions,
                payload_id,
                "potions",
            )?);
        }
        if let Some(payload_id) = refs.schedule_id.as_deref() {
            checkpoint.restore_run_state_schedule_from_external_ref(resolve_payload(
                &self.schedules,
                payload_id,
                "run schedule",
            )?);
        }
        if refs.emitted_events_tip.is_some() {
            checkpoint.restore_run_state_emitted_events_from_external_ref(restore_chain(
                &self.emitted_event_nodes,
                refs.emitted_events_tip,
                "emitted events",
            )?);
        }
        Ok(())
    }

    pub(crate) fn intern_replay(&mut self, replay: &[OracleRunReplayStepV1]) -> Option<usize> {
        append_chain(&mut self.replay_nodes, replay)
    }

    pub(crate) fn restore_replay(
        &self,
        tip: Option<usize>,
    ) -> Result<Vec<OracleRunReplayStepV1>, String> {
        if tip.is_some() {
            self.validate_fingerprint_algorithm()?;
        }
        restore_chain(&self.replay_nodes, tip, "replay")
    }

    fn ensure_write_fingerprint_algorithm(&mut self) -> Result<(), String> {
        match self.fingerprint_algorithm.as_deref() {
            Some(FINGERPRINT_ALGORITHM_JSON) => Ok(()),
            None => {
                self.fingerprint_algorithm = Some(FINGERPRINT_ALGORITHM_JSON.to_string());
                Ok(())
            }
            Some(algorithm) => Err(format!(
                "unsupported oracle checkpoint payload fingerprint algorithm '{algorithm}'"
            )),
        }
    }

    fn validate_fingerprint_algorithm(&self) -> Result<(), String> {
        match self.fingerprint_algorithm.as_deref() {
            Some(FINGERPRINT_ALGORITHM_JSON) => Ok(()),
            None => Err("oracle checkpoint payload fingerprint algorithm is missing".to_string()),
            Some(algorithm) => Err(format!(
                "unsupported oracle checkpoint payload fingerprint algorithm '{algorithm}'"
            )),
        }
    }

    fn intern_map(&mut self, mut map: MapState) -> Result<String, String> {
        let payload_id = hash_serializable(&map);
        let graph = std::mem::take(&mut map.graph);
        let graph_id = intern_payload(graph, &mut self.map_graphs, "map graph")?;
        if let Some(existing) = self
            .maps
            .iter()
            .find(|record| record.payload_id == payload_id)
        {
            if existing.graph_id != graph_id || existing.map != map {
                return Err("oracle checkpoint map payload fingerprint collision".to_string());
            }
            return Ok(payload_id);
        }
        self.maps.push(OracleRunCheckpointMapRecordV1 {
            payload_id: payload_id.clone(),
            graph_id,
            map,
        });
        Ok(payload_id)
    }

    fn resolve_map(&self, payload_id: &str) -> Result<MapState, String> {
        let record = unique_record(&self.maps, payload_id, |record| &record.payload_id, "map")?;
        let mut map = record.map.clone();
        map.graph = resolve_payload(&self.map_graphs, &record.graph_id, "map graph")?;
        if hash_serializable(&map) != payload_id {
            return Err(format!(
                "oracle checkpoint map payload '{payload_id}' failed fingerprint validation"
            ));
        }
        Ok(map)
    }
}

impl OracleRunSessionPayloadRefsV1 {
    fn validate_complete_v1(&self) -> Result<(), String> {
        for (name, value) in [
            ("map_id", self.map_id.as_deref()),
            ("master_deck_id", self.master_deck_id.as_deref()),
            ("relics_id", self.relics_id.as_deref()),
            ("potions_id", self.potions_id.as_deref()),
            ("schedule_id", self.schedule_id.as_deref()),
        ] {
            if value.is_none() {
                return Err(format!(
                    "oracle checkpoint session payload reference '{name}' is missing"
                ));
            }
        }
        Ok(())
    }
}

fn intern_payload<T>(
    value: T,
    records: &mut Vec<OracleRunCheckpointPayloadRecordV1<T>>,
    label: &str,
) -> Result<String, String>
where
    T: PartialEq + Serialize,
{
    let payload_id = hash_serializable(&value);
    if let Some(existing) = records
        .iter()
        .find(|record| record.payload_id == payload_id)
    {
        if existing.value != value {
            return Err(format!(
                "oracle checkpoint {label} payload fingerprint collision"
            ));
        }
        return Ok(payload_id);
    }
    records.push(OracleRunCheckpointPayloadRecordV1 {
        payload_id: payload_id.clone(),
        value,
    });
    Ok(payload_id)
}

fn resolve_payload<T>(
    records: &[OracleRunCheckpointPayloadRecordV1<T>],
    payload_id: &str,
    label: &str,
) -> Result<T, String>
where
    T: Clone + Serialize,
{
    let record = unique_record(records, payload_id, |record| &record.payload_id, label)?;
    if hash_serializable(&record.value) != payload_id {
        return Err(format!(
            "oracle checkpoint {label} payload '{payload_id}' failed fingerprint validation"
        ));
    }
    Ok(record.value.clone())
}

fn unique_record<'a, T>(
    records: &'a [T],
    payload_id: &str,
    id: impl Fn(&'a T) -> &'a str,
    label: &str,
) -> Result<&'a T, String> {
    let mut matches = records.iter().filter(|record| id(record) == payload_id);
    let record = matches
        .next()
        .ok_or_else(|| format!("missing oracle checkpoint {label} payload '{payload_id}'"))?;
    if matches.next().is_some() {
        return Err(format!(
            "oracle checkpoint duplicated {label} payload '{payload_id}'"
        ));
    }
    Ok(record)
}

fn append_chain<T>(
    nodes: &mut Vec<OracleRunCheckpointChainNodeV1<T>>,
    values: &[T],
) -> Option<usize>
where
    T: Clone + PartialEq,
{
    let mut tip = None;
    for value in values {
        let node_id = nodes
            .iter()
            .position(|node| node.parent == tip && node.value == *value)
            .unwrap_or_else(|| {
                let node_id = nodes.len();
                nodes.push(OracleRunCheckpointChainNodeV1 {
                    parent: tip,
                    value: value.clone(),
                });
                node_id
            });
        tip = Some(node_id);
    }
    tip
}

fn restore_chain<T>(
    nodes: &[OracleRunCheckpointChainNodeV1<T>],
    mut tip: Option<usize>,
    label: &str,
) -> Result<Vec<T>, String>
where
    T: Clone,
{
    let mut seen = std::collections::BTreeSet::new();
    let mut values = Vec::new();
    while let Some(node_id) = tip {
        if !seen.insert(node_id) {
            return Err(format!("oracle checkpoint {label} chain contains a cycle"));
        }
        let node = nodes
            .get(node_id)
            .ok_or_else(|| format!("oracle checkpoint {label} node {node_id} is missing"))?;
        values.push(node.value.clone());
        tip = node.parent;
    }
    values.reverse();
    Ok(values)
}
