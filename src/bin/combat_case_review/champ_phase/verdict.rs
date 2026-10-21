use super::types::{
    ChampPhaseAuditFlag, ChampPhaseAuditVerdict, ChampPhaseSnapshot, ChampResourceTiming,
    ChampSplitTrigger,
};

pub(super) fn champ_phase_flags(
    split_trigger: Option<&ChampSplitTrigger>,
    post_split: Option<&ChampPhaseSnapshot>,
    resources: &ChampResourceTiming,
    replay_truncated: bool,
) -> Vec<ChampPhaseAuditFlag> {
    let mut flags = Vec::new();
    if split_trigger.is_some() {
        flags.push(ChampPhaseAuditFlag::SplitObserved);
    } else {
        flags.push(ChampPhaseAuditFlag::NoSplitReached);
    }
    if resources.disarm_used_before_split {
        flags.push(ChampPhaseAuditFlag::DisarmSpentBeforeSplit);
    }
    if resources.fear_potion_used_before_split {
        flags.push(ChampPhaseAuditFlag::FearPotionSpentBeforeSplit);
    }
    if resources.strength_potion_used_before_split || resources.steroid_potion_used_before_split {
        flags.push(ChampPhaseAuditFlag::BurstPotionSpentBeforeSplit);
    }
    if let Some(snapshot) = post_split {
        if snapshot.player_hp * 4 <= snapshot.player_max_hp.max(1) {
            flags.push(ChampPhaseAuditFlag::SplitWithLowHp);
        }
        if snapshot.champ_hp * 5 > snapshot.champ_max_hp * 2 {
            flags.push(ChampPhaseAuditFlag::SplitWithChampHpStillHigh);
        }
    }
    if replay_truncated {
        flags.push(ChampPhaseAuditFlag::ReplayTruncated);
    }
    flags
}

pub(super) fn champ_phase_verdict(
    flags: &[ChampPhaseAuditFlag],
    split_observed: bool,
    replay_engine_limited: bool,
) -> ChampPhaseAuditVerdict {
    if replay_engine_limited {
        return ChampPhaseAuditVerdict::Unclear;
    }
    if !split_observed {
        return ChampPhaseAuditVerdict::NoSplitReached;
    }
    if flags
        .iter()
        .any(|flag| matches!(flag, ChampPhaseAuditFlag::SplitWithLowHp))
    {
        return ChampPhaseAuditVerdict::SplitWithLowHp;
    }
    if flags.iter().any(|flag| {
        matches!(
            flag,
            ChampPhaseAuditFlag::DisarmSpentBeforeSplit
                | ChampPhaseAuditFlag::FearPotionSpentBeforeSplit
                | ChampPhaseAuditFlag::BurstPotionSpentBeforeSplit
        )
    }) {
        return ChampPhaseAuditVerdict::ResourceSpentBeforeSplit;
    }
    ChampPhaseAuditVerdict::SplitObserved
}
