//! Invert a resolved input through the same symbolic-selection decoder.
use super::*;

impl LearningSelectionDraftV1 {
    /// Returns append/submit ordinals for this unchanged, empty draft.
    /// Typed private identities select domains; the returned path contains no
    /// identities and still has to pass ordinary choose/step validation.
    pub fn ordinal_path_for_input_v1(
        &self,
        input: &ClientInput,
    ) -> Result<Option<(Vec<usize>, LearningActionV1)>, LearningModelInputError> {
        if !self.selected_domain_indices.is_empty() {
            return Ok(None);
        }
        let selected: Option<Vec<usize>> = match (&self.family, input) {
            (LearningSelectionFamilyStateV1::Run(family), ClientInput::SubmitSelection(resolution))
                if resolution.scope == family.scope => resolution
                .selected_card_lineages()
                .iter()
                .map(|lineage| family.selectable_card_lineages.iter().position(|id| id == lineage))
                .collect(),
            (LearningSelectionFamilyStateV1::Combat { public, private_resolution },
                ClientInput::SubmitSelection(resolution))
                if matches!((public.input_encoding, resolution.scope),
                    (CombatSelectionInputEncodingV2::SubmitSelectionHandCardLineages, SelectionScope::Hand)
                    | (CombatSelectionInputEncodingV2::SubmitSelectionGridCardLineages, SelectionScope::Grid)) => resolution
                .selected_card_lineages()
                .iter()
                .map(|selected| private_resolution.domain.iter().position(|domain|
                    matches!(domain, CombatSelectionDomainResolutionV1::Card { lineage } if lineage == selected)))
                .collect(),
            (LearningSelectionFamilyStateV1::Combat { public, private_resolution }, ClientInput::SubmitScryDiscard(indices))
                if public.input_encoding == CombatSelectionInputEncodingV2::SubmitScryDiscardIndices => indices
                .iter()
                .map(|selected| private_resolution.domain.iter().position(|domain|
                    matches!(domain, CombatSelectionDomainResolutionV1::Scry { index, .. } if *index == *selected as u64)))
                .collect(),
            _ => None,
        };
        let Some(selected) = selected else {
            return Ok(None);
        };
        let mut draft = self.clone();
        let mut path = Vec::with_capacity(selected.len() + 1);
        for selected in selected {
            let decision = draft.decision();
            let mut ordinals = decision.candidates.iter().enumerate().filter_map(|(ordinal, candidate)|
                matches!(candidate.semantics, LearningSelectionCandidateSemanticsV1::Append { domain_index } if domain_index == selected).then_some(ordinal));
            let Some(ordinal) = ordinals.next() else {
                return Ok(None);
            };
            if ordinals.next().is_some() {
                return Ok(None);
            }
            if !matches!(draft.choose(ordinal)?, LearningSelectionStepV1::Continue) {
                return Ok(None);
            }
            path.push(ordinal);
        }
        let decision = draft.decision();
        let Some(submit) = decision.candidates.iter().position(|candidate| {
            matches!(
                candidate.semantics,
                LearningSelectionCandidateSemanticsV1::Submit
            )
        }) else {
            return Ok(None);
        };
        let LearningSelectionStepV1::Apply(action) = draft.choose(submit)? else {
            return Ok(None);
        };
        let matches = match (&action, input) {
            (LearningActionV1::CombatInput { input: actual }, expected) => actual == expected,
            (
                LearningActionV1::RunSelection {
                    resolution: actual, ..
                },
                ClientInput::SubmitSelection(expected),
            ) => actual == expected,
            _ => false,
        };
        if !matches {
            return Ok(None);
        }
        path.push(submit);
        Ok(Some((path, action)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime::combat::CardLineageId;

    #[test]
    fn inverse_round_trips_ordered_targets_and_empty_selection_without_mutating_the_draft() {
        let a = CardLineageId::master_deck(1);
        let b = CardLineageId::master_deck(2);
        let c = CardLineageId::master_deck(3);
        let draft = LearningSelectionDraftV1::from_run(
            "choice".into(),
            SelectionScope::Deck,
            SelectionReason::Transform,
            0,
            2,
            vec![a, b, c],
            vec![0, 1, 2],
        );
        for selected in [vec![c, a], vec![]] {
            let input = ClientInput::SubmitSelection(SelectionResolution::card_lineages(
                SelectionScope::Deck,
                selected,
            ));
            let (path, expected) = draft.ordinal_path_for_input_v1(&input).unwrap().unwrap();
            let mut actual = draft.clone();
            for ordinal in &path[..path.len() - 1] {
                assert!(matches!(
                    actual.choose(*ordinal).unwrap(),
                    LearningSelectionStepV1::Continue
                ));
            }
            let LearningSelectionStepV1::Apply(action) =
                actual.choose(*path.last().unwrap()).unwrap()
            else {
                panic!("must submit");
            };
            assert_eq!(action, expected);
        }
        assert!(draft.selected_domain_indices().is_empty());
        for selected in [
            vec![a, a],
            vec![CardLineageId::master_deck(99)],
            vec![a, b, c],
        ] {
            assert!(draft
                .ordinal_path_for_input_v1(&ClientInput::SubmitSelection(
                    SelectionResolution::card_lineages(SelectionScope::Deck, selected)
                ))
                .unwrap()
                .is_none());
        }
        assert!(draft
            .ordinal_path_for_input_v1(&ClientInput::SubmitSelection(
                SelectionResolution::card_lineages(SelectionScope::Hand, vec![a])
            ))
            .unwrap()
            .is_none());
    }
}
