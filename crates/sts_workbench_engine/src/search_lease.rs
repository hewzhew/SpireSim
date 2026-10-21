use crate::domain_manifest::manifest_scalar_v1;
use crate::lease_source::{LeaseScopeV1, LeaseSourceV1};
use crate::search::{SearchExecutionV1, SearchStatusV1, SearchTranslationStatusV1};
use std::collections::BTreeMap;
use std::fmt::Write;
use std::path::Path;

const SEARCH_MANIFEST_VERSION_V1: &str = "STS-SEARCH 1";

#[derive(Clone, Debug, Eq, PartialEq)]
struct WitnessLeaseEntryV1 {
    handle: String,
    preview_available: bool,
    plan_step_count: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WitnessLeaseV1 {
    scope: LeaseScopeV1,
    entries: BTreeMap<String, WitnessLeaseEntryV1>,
    detail_handle: String,
    text: String,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct ResolvedWitnessAliasV1 {
    pub alias: String,
    pub handle: String,
    pub plan_step_count: u64,
    pub scope: LeaseScopeV1,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct WitnessLeaseErrorV1 {
    pub code: &'static str,
    pub detail: String,
}

impl WitnessLeaseV1 {
    pub fn new(
        repository: &Path,
        source: LeaseSourceV1,
        searched: &SearchExecutionV1,
    ) -> Result<Self, WitnessLeaseErrorV1> {
        let scope = LeaseScopeV1::new(
            searched.root_id.clone(),
            searched.action_space_id.clone(),
            source,
        );
        scope
            .validate(repository)
            .map_err(|error| witness_error("WitnessExpired", error.detail))?;
        if !matches!(
            searched.status,
            SearchStatusV1::Witness | SearchStatusV1::BudgetUnknown
        ) || searched.lane_count == 0
            || searched.status == SearchStatusV1::Witness && searched.witnesses.is_empty()
            || searched.status == SearchStatusV1::BudgetUnknown && !searched.witnesses.is_empty()
            || !artifact_handle_has_shape(&searched.detail_handle)
        {
            return Err(witness_error(
                "SearchInvalid",
                "search result cannot issue a scoped witness lease",
            ));
        }

        let authority = match &scope.source {
            LeaseSourceV1::ExplicitRoot => "observe_only",
            LeaseSourceV1::SelectedHead(_) => "lab_head",
        };
        let mut text = format!(
            "{SEARCH_MANIFEST_VERSION_V1}\nauthority={authority}\n\n[search]\nstatus={} exit={} lanes={} frontier={} inspect=D0\n\n[witnesses]\n",
            searched.status.token(),
            manifest_scalar_v1(&searched.summary_exit)
                .map_err(|detail| witness_error("SearchInvalid", detail))?,
            searched.lane_count,
            searched.bounded_frontier_count,
        );
        let mut entries = BTreeMap::new();
        if searched.witnesses.is_empty() {
            writeln!(text, "none").unwrap();
        }
        for (ordinal, witness) in searched.witnesses.iter().enumerate() {
            if witness.plan_step_count == 0 || !artifact_handle_has_shape(&witness.witness_handle) {
                return Err(witness_error(
                    "SearchInvalid",
                    "search witness cannot issue a short alias",
                ));
            }
            let alias = format!("W{ordinal}");
            let preview_available = witness.translation == SearchTranslationStatusV1::Complete;
            writeln!(
                text,
                "{alias} steps={} final_hp={} potions={} quality={} translation={} preview={}",
                witness.plan_step_count,
                witness.final_hp,
                witness.total_potions_consumed,
                if witness.satisfies_requested_quality {
                    "satisfied"
                } else {
                    "not_satisfied"
                },
                witness.translation.token(),
                if preview_available {
                    "available"
                } else {
                    "unavailable"
                },
            )
            .unwrap();
            if entries
                .insert(
                    alias,
                    WitnessLeaseEntryV1 {
                        handle: witness.witness_handle.clone(),
                        preview_available,
                        plan_step_count: witness.plan_step_count,
                    },
                )
                .is_some()
            {
                return Err(witness_error(
                    "SearchInvalid",
                    "search generated duplicate witness aliases",
                ));
            }
        }
        Ok(Self {
            scope,
            entries,
            detail_handle: searched.detail_handle.clone(),
            text,
        })
    }

    pub fn text(&self) -> &str {
        &self.text
    }

    pub fn resolve(
        &self,
        repository: &Path,
        alias: &str,
    ) -> Result<ResolvedWitnessAliasV1, WitnessLeaseErrorV1> {
        if !is_alias(alias, 'W') {
            return Err(witness_error(
                "WitnessUnknown",
                "preview requires one W alias from the current search",
            ));
        }
        self.scope
            .validate(repository)
            .map_err(|error| witness_error("WitnessExpired", error.detail))?;
        let entry = self.entries.get(alias).ok_or_else(|| {
            witness_error(
                "WitnessUnknown",
                format!("{alias} is absent from the current search"),
            )
        })?;
        if !entry.preview_available {
            return Err(witness_error(
                "WitnessUnavailable",
                format!("{alias} has an incomplete mechanics translation"),
            ));
        }
        Ok(ResolvedWitnessAliasV1 {
            alias: alias.to_owned(),
            handle: entry.handle.clone(),
            plan_step_count: entry.plan_step_count,
            scope: self.scope.clone(),
        })
    }

    pub fn detail_handle(&self) -> &str {
        &self.detail_handle
    }

    pub fn scope(&self) -> &LeaseScopeV1 {
        &self.scope
    }
}

pub(super) fn search_required() -> WitnessLeaseErrorV1 {
    witness_error(
        "SearchRequired",
        "no current witness lease; call search before using a W alias",
    )
}

fn witness_error(code: &'static str, detail: impl Into<String>) -> WitnessLeaseErrorV1 {
    WitnessLeaseErrorV1 {
        code,
        detail: detail.into(),
    }
}

fn is_alias(value: &str, prefix: char) -> bool {
    value
        .strip_prefix(prefix)
        .and_then(|ordinal| ordinal.parse::<usize>().ok())
        .is_some_and(|ordinal| value == format!("{prefix}{ordinal}"))
}

fn artifact_handle_has_shape(value: &str) -> bool {
    value
        .strip_prefix("workbench-artifact-v1-")
        .is_some_and(|digest| {
            digest.len() == 64 && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::SearchWitnessSummaryV1;
    use crate::test_support::paired_store_fixture;

    fn artifact(byte: char) -> String {
        format!(
            "workbench-artifact-v1-{}",
            std::iter::repeat_n(byte, 64).collect::<String>()
        )
    }

    fn searched(fixture: &crate::test_support::PairedStoreFixture) -> SearchExecutionV1 {
        SearchExecutionV1 {
            root_id: fixture.source.root_id.clone(),
            action_space_id: fixture.source.root.preflight.action_space_id.clone(),
            status: SearchStatusV1::Witness,
            summary_exit: "satisfied".to_owned(),
            lane_count: 2,
            bounded_frontier_count: 1,
            witnesses: vec![
                SearchWitnessSummaryV1 {
                    witness_handle: artifact('a'),
                    plan_step_count: 8,
                    translation: SearchTranslationStatusV1::Complete,
                    final_hp: 73,
                    total_potions_consumed: 0,
                    satisfies_requested_quality: true,
                },
                SearchWitnessSummaryV1 {
                    witness_handle: artifact('b'),
                    plan_step_count: 6,
                    translation: SearchTranslationStatusV1::MechanicsGap,
                    final_hp: 61,
                    total_potions_consumed: 1,
                    satisfies_requested_quality: false,
                },
            ],
            detail_handle: artifact('c'),
        }
    }

    #[test]
    fn witness_manifest_is_short_and_hides_artifact_identities() {
        let fixture = paired_store_fixture();
        let searched = searched(&fixture);
        let lease =
            WitnessLeaseV1::new(&fixture.repository, LeaseSourceV1::ExplicitRoot, &searched)
                .unwrap();
        assert!(lease.text().contains(
            "W0 steps=8 final_hp=73 potions=0 quality=satisfied translation=complete preview=available"
        ));
        assert!(lease.text().contains("W1 steps=6"));
        assert!(!lease.text().contains("workbench-artifact"));
        assert_eq!(
            lease.resolve(&fixture.repository, "W0").unwrap().handle,
            searched.witnesses[0].witness_handle
        );
        assert_eq!(
            lease.resolve(&fixture.repository, "W1").unwrap_err().code,
            "WitnessUnavailable"
        );
        assert_eq!(lease.detail_handle(), searched.detail_handle);
    }

    #[test]
    fn selected_witness_alias_expires_when_head_moves() {
        let fixture = paired_store_fixture();
        let searched = searched(&fixture);
        let selected = fixture.store.resolve_head_selector("official").unwrap();
        let lease = WitnessLeaseV1::new(
            &fixture.repository,
            LeaseSourceV1::SelectedHead(Box::new(selected)),
            &searched,
        )
        .unwrap();
        fixture
            .store
            .commit_selected_receipt_idempotent("official", &fixture.first.receipt_id)
            .unwrap();
        assert_eq!(
            lease.resolve(&fixture.repository, "W0").unwrap_err().code,
            "WitnessExpired"
        );
    }
}
