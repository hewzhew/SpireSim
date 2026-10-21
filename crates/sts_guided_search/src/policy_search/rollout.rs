//! Evaluate queried leaves with the same receding-search execution owner.
use super::*;

#[derive(Clone, Serialize)]
pub(super) struct LeafSource {
    pub slot: usize,
    pub decision: usize,
    pub node: usize,
    pub specification: ConstructedCombat,
    pub parent_inputs: Vec<ClientInput>,
    pub root_selection: Option<(usize, Vec<usize>)>,
    pub search_branches: Vec<crate::CombatBranch>,
    pub parent_source: Option<Box<LeafSource>>,
}

impl PolicySearchBatch {
    /// Clone pending queries. Child searches have their own budget/revision;
    /// no observation or search work is installed in the parent.
    pub fn leaf_rollouts(
        &self,
        revision: u64,
        slots: &[usize],
        simulations: usize,
        max_decisions: usize,
    ) -> Result<Self, String> {
        if revision != self.revision
            || slots.is_empty()
            || !(1..=4096).contains(&simulations)
            || !(1..=1024).contains(&max_decisions)
            || slots
                .iter()
                .enumerate()
                .any(|(i, slot)| slots[..i].contains(slot))
        {
            return Err("invalid leaf continuation request".into());
        }
        let mut episodes = Vec::with_capacity(slots.len());
        let mut sources = Vec::with_capacity(slots.len());
        for &slot in slots {
            let ep = self
                .episodes
                .get(slot)
                .ok_or("leaf continuation slot absent")?;
            let (index, path) = ep.tree.pending.as_ref().ok_or("slot has no queried leaf")?;
            let root = &ep.tree.nodes[0].state;
            sources.push(LeafSource {
                slot,
                decision: ep.decisions,
                node: *index,
                specification: self.specifications[slot].clone(),
                parent_inputs: ep.inputs.clone(),
                root_selection: root
                    .selection_family()
                    .map(|family| (family, root.selected_members().unwrap().to_vec())),
                search_branches: path
                    .iter()
                    .map(|(node, edge)| ep.tree.nodes[*node].choices.branches()[*edge].clone())
                    .collect(),
                parent_source: self.leaf_sources.get(slot).cloned().map(Box::new),
            });
            episodes.push(Episode {
                tree: Tree::new(ep.tree.space.clone(), ep.tree.nodes[*index].state.clone())?,
                inputs: vec![],
                decisions: 0,
                transitions: 0,
            });
        }
        Ok(Self {
            specifications: sources.iter().map(|s| s.specification.clone()).collect(),
            episodes,
            simulations,
            max_decisions,
            revision: NEXT_REVISION.fetch_add(1, Ordering::Relaxed),
            source_revision: Some(revision),
            leaf_sources: sources,
        })
    }
}
