use std::collections::BTreeSet;

use super::{
    ManagedWorkspaceManifestV1, WorkspaceGcPlanV1, MANAGED_WORKSPACE_MANIFEST_SCHEMA_V1,
    WORKSPACE_GC_PLAN_SCHEMA_V2,
};

pub(super) fn validate_manifest_v1(
    manifest: &ManagedWorkspaceManifestV1,
    workspace_name: &str,
) -> Result<(), String> {
    if manifest.schema != MANAGED_WORKSPACE_MANIFEST_SCHEMA_V1
        || manifest.kind != "disposable_fork"
        || manifest.workspace_name != workspace_name
    {
        return Err("managed workspace manifest identity is unsupported".to_string());
    }
    validate_short_text_v1("target session id", &manifest.target_session_id)?;
    validate_short_text_v1("source session path", &manifest.source.session_path)?;
    validate_short_text_v1("source session id", &manifest.source.session_id)?;
    validate_short_text_v1("source archive node id", &manifest.source.archive_node_id)?;
    validate_short_text_v1("expected cleanup phase", &manifest.expected_cleanup_phase)
}

pub(super) fn validate_plan_v1(plan: &WorkspaceGcPlanV1) -> Result<(), String> {
    if plan.schema != WORKSPACE_GC_PLAN_SCHEMA_V2 {
        return Err("workspace GC plan schema is unsupported".to_string());
    }
    let mut names = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut total_bytes = 0u64;
    for target in &plan.targets {
        validate_manifest_v1(&target.manifest, &target.workspace_name)?;
        if !names.insert(target.workspace_name.clone()) || !paths.insert(target.path.clone()) {
            return Err("workspace GC plan contains duplicate targets".to_string());
        }
        total_bytes = total_bytes.saturating_add(target.size_bytes);
    }
    if total_bytes != plan.total_bytes {
        return Err("workspace GC plan byte total is inconsistent".to_string());
    }
    Ok(())
}

pub(super) fn validate_short_text_v1(description: &str, value: &str) -> Result<(), String> {
    if value.trim().is_empty() || value.len() > 512 {
        return Err(format!(
            "managed workspace {description} is empty or too long"
        ));
    }
    Ok(())
}
