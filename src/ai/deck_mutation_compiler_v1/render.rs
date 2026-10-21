use super::types::{CompiledDeckMutationDecisionV1, DeckMutationPlanCandidateV1};

const MAX_GROUP_ITEMS: usize = 8;

pub fn render_compiled_deck_mutation_decision_v1(
    decision: &CompiledDeckMutationDecisionV1,
) -> String {
    let mut lines = Vec::new();
    lines.push("Deck mutation evidence:".to_string());
    lines.push(format!(
        "facts: reason={:?} min={} max={} candidates={} label_role={}",
        decision.reason,
        decision.min_choices,
        decision.max_choices,
        decision.candidate_plans.len(),
        decision.label_role
    ));
    lines.push(format!(
        "execution: head={}",
        decision
            .selected_plan
            .as_ref()
            .map(render_plan_line)
            .unwrap_or_else(|| "-".to_string())
    ));
    lines.push(format!(
        "candidate_pool: total={} branch_active={} inspect_only={} blocked={}",
        decision.candidate_plans.len(),
        decision.branch_active_plans.len(),
        decision.inspect_only_plans.len(),
        decision.blocked_plans.len(),
    ));
    push_plan_group(
        &mut lines,
        "scheduler: branch_active",
        &decision.branch_active_plans,
    );
    push_plan_group(
        &mut lines,
        "candidate_pool: inspect_only",
        &decision.inspect_only_plans,
    );
    push_plan_group(
        &mut lines,
        "candidate_pool: blocked",
        &decision.blocked_plans,
    );
    lines.join("\n")
}

fn push_plan_group(lines: &mut Vec<String>, label: &str, plans: &[DeckMutationPlanCandidateV1]) {
    if plans.is_empty() {
        lines.push(format!("{label}: -"));
        return;
    }

    lines.push(format!("{label}: {}", plans.len()));
    for (idx, plan) in plans.iter().take(MAX_GROUP_ITEMS).enumerate() {
        lines.push(format!("  {idx}. {}", render_plan_line(plan)));
    }
    let hidden = plans.len().saturating_sub(MAX_GROUP_ITEMS);
    if hidden > 0 {
        lines.push(format!("  ... {hidden} more"));
    }
}

fn render_plan_line(plan: &DeckMutationPlanCandidateV1) -> String {
    format!(
        "{} | command={} | role={:?} | scheduler=[{}] | diagnostics=[confidence={:.2} reps={} suppressed={}] | reasons=[{}] | risks=[{}]",
        plan.step.effect_label,
        plan.step.command,
        plan.role,
        render_allowed(plan),
        plan.confidence,
        plan.representative_count,
        plan.suppressed_count,
        render_short_list(&plan.reasons),
        render_short_list(&plan.risks)
    )
}

fn render_allowed(plan: &DeckMutationPlanCandidateV1) -> String {
    let allowed = &plan.allowed_consumers;
    let mut labels = Vec::new();
    if allowed.execute_autopilot {
        labels.push("execute");
    }
    if allowed.branch_active {
        labels.push("branch_active");
    }
    if allowed.branch_frozen {
        labels.push("branch_frozen");
    }
    if allowed.inspect {
        labels.push("inspect");
    }
    if allowed.replay {
        labels.push("replay");
    }
    if allowed.human_prompt {
        labels.push("human_prompt");
    }
    if labels.is_empty() {
        "-".to_string()
    } else {
        labels.join(",")
    }
}

fn render_short_list(items: &[String]) -> String {
    if items.is_empty() {
        return "-".to_string();
    }
    let mut text = items.iter().take(3).cloned().collect::<Vec<_>>();
    let hidden = items.len().saturating_sub(text.len());
    if hidden > 0 {
        text.push(format!("... {hidden} more"));
    }
    text.join("; ")
}
