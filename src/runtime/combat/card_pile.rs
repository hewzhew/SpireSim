use super::{CardLineageId, CombatCard, DrawCardKind, DrawPileChange};
use serde::ser::SerializeSeq;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt;
use std::iter::FromIterator;
use std::ops::{Deref, DerefMut, Index, IndexMut};
use std::slice;
use std::sync::Arc;

/// Draw-pile storage specialized for exact-search snapshots.
///
/// The active pile is the range `start..end`, in draw order (top first).
/// Drawing only clones the one returned card and advances `start`; cloned
/// combat states continue sharing the immutable backing order. Operations
/// that really change order materialize the active range first.
pub struct DrawPile {
    cards: Arc<Vec<CombatCard>>,
    start: usize,
    end: usize,
    // Ephemeral, opt-in mailbox; absent from clone, equality and serialization.
    observation: Option<Vec<DrawPileChange>>,
}

impl DrawPile {
    pub(crate) fn begin_observation(&mut self) {
        self.observation = Some(Vec::new());
    }

    pub(crate) fn take_observation(&mut self) -> Option<Vec<DrawPileChange>> {
        self.observation.take()
    }

    fn record(&mut self, change: DrawPileChange) {
        if let Some(events) = &mut self.observation {
            events.push(change);
        }
    }

    fn forget_observed_order(&mut self) {
        self.record(DrawPileChange::OrderUnknown { count: self.len() });
    }

    /// Prefix is already shuffled and in Rust top-first order. Keeping the
    /// mailbox across replacement preserves earlier draws in this transition.
    pub(crate) fn prepend_shuffled(&mut self, mut prefix: Vec<CombatCard>) {
        let count = prefix.len();
        let observation = self.observation.take();
        prefix.extend(std::mem::take(self));
        *self = prefix.into();
        self.observation = observation;
        self.record(DrawPileChange::PrependShuffled { count });
    }

    fn active(&self) -> &[CombatCard] {
        &self.cards[self.start..self.end]
    }

    fn materialize(&mut self) -> &mut Vec<CombatCard> {
        if self.start != 0 || self.end != self.cards.len() {
            self.cards = Arc::new(self.active().to_vec());
            self.start = 0;
            self.end = self.cards.len();
        }
        Arc::make_mut(&mut self.cards)
    }

    pub(crate) fn as_mut_slice(&mut self) -> &mut [CombatCard] {
        self.forget_observed_order();
        self.materialize().as_mut_slice()
    }

    pub fn draw_top(&mut self) -> Option<CombatCard> {
        let card = self.active().first()?.clone();
        self.start += 1;
        self.record(DrawPileChange::TakeTop {
            card: (&card).into(),
        });
        Some(card)
    }

    pub fn push_top(&mut self, card: CombatCard) {
        self.record(DrawPileChange::PutTop {
            card: (&card).into(),
        });
        if self.start > 0 {
            if let Some(cards) = Arc::get_mut(&mut self.cards) {
                self.start -= 1;
                cards[self.start] = card;
                return;
            }
        }
        self.materialize().insert(0, card);
        self.end += 1;
    }

    pub fn push(&mut self, card: CombatCard) {
        self.record(DrawPileChange::PutBottom {
            card: (&card).into(),
        });
        self.materialize().push(card);
        self.end += 1;
    }

    pub fn insert(&mut self, index: usize, card: CombatCard) {
        self.record(DrawPileChange::InsertUnpositioned {
            card: (&card).into(),
        });
        self.materialize().insert(index, card);
        self.end += 1;
    }

    pub fn remove(&mut self, index: usize) -> CombatCard {
        let card = self.materialize().remove(index);
        self.end -= 1;
        self.record(DrawPileChange::RemoveUnpositioned {
            card: (&card).into(),
        });
        card
    }

    pub fn remove_by_lineage(&mut self, lineage: CardLineageId) -> Option<CombatCard> {
        let index = self.iter().position(|card| card.lineage == lineage)?;
        Some(self.remove(index))
    }

    pub fn pop(&mut self) -> Option<CombatCard> {
        let card = self.active().last()?.clone();
        self.end -= 1;
        self.record(DrawPileChange::RemoveUnpositioned {
            card: (&card).into(),
        });
        Some(card)
    }

    pub fn clear(&mut self) {
        self.cards = Arc::new(Vec::new());
        self.start = 0;
        self.end = 0;
        self.forget_observed_order();
    }

    pub fn append(&mut self, other: &mut Vec<CombatCard>) {
        let added = other.len();
        self.materialize().append(other);
        self.end += added;
        self.forget_observed_order();
    }

    pub fn reverse(&mut self) {
        self.forget_observed_order();
        self.materialize().reverse();
    }

    pub fn swap(&mut self, left: usize, right: usize) {
        self.forget_observed_order();
        self.materialize().swap(left, right);
    }

    pub fn iter_mut(&mut self) -> slice::IterMut<'_, CombatCard> {
        self.forget_observed_order();
        self.materialize().iter_mut()
    }

    /// Mutate card attributes without discarding known order when every card
    /// remains the same observable kind in the same position. Unlike `iter_mut`,
    /// this checks the result and falls back to an unknown-order receipt if a
    /// caller changes a card ID or upgrade level.
    pub(crate) fn mutate_cards_with_kind_tracking(
        &mut self,
        mut mutate: impl FnMut(&mut CombatCard),
    ) {
        let mut changed_kind = false;
        for card in self.materialize().iter_mut() {
            let before = DrawCardKind::from(&*card);
            mutate(card);
            changed_kind |= DrawCardKind::from(&*card) != before;
        }
        if changed_kind {
            self.forget_observed_order();
        }
    }

    pub fn into_vec(self) -> Vec<CombatCard> {
        let mut cards = Arc::unwrap_or_clone(self.cards);
        if self.end < cards.len() {
            cards.truncate(self.end);
        }
        if self.start > 0 {
            cards.drain(..self.start);
        }
        cards
    }
}

impl Clone for DrawPile {
    fn clone(&self) -> Self {
        Self {
            cards: self.cards.clone(),
            start: self.start,
            end: self.end,
            observation: None,
        }
    }
}

impl Default for DrawPile {
    fn default() -> Self {
        Vec::new().into()
    }
}

impl fmt::Debug for DrawPile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for DrawPile {
    fn eq(&self, other: &Self) -> bool {
        self.active() == other.active()
    }
}

impl Eq for DrawPile {}

impl Serialize for DrawPile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.active().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for DrawPile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<CombatCard>::deserialize(deserializer).map(Into::into)
    }
}

impl Deref for DrawPile {
    type Target = [CombatCard];

    fn deref(&self) -> &Self::Target {
        self.active()
    }
}

impl AsRef<[CombatCard]> for DrawPile {
    fn as_ref(&self) -> &[CombatCard] {
        self.active()
    }
}

impl From<Vec<CombatCard>> for DrawPile {
    fn from(cards: Vec<CombatCard>) -> Self {
        let end = cards.len();
        Self {
            cards: Arc::new(cards),
            start: 0,
            end,
            observation: None,
        }
    }
}

impl FromIterator<CombatCard> for DrawPile {
    fn from_iter<T: IntoIterator<Item = CombatCard>>(iter: T) -> Self {
        Vec::from_iter(iter).into()
    }
}

impl IntoIterator for DrawPile {
    type Item = CombatCard;
    type IntoIter = std::vec::IntoIter<CombatCard>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a DrawPile {
    type Item = &'a CombatCard;
    type IntoIter = slice::Iter<'a, CombatCard>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut DrawPile {
    type Item = &'a mut CombatCard;
    type IntoIter = slice::IterMut<'a, CombatCard>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl PartialEq<Vec<CombatCard>> for DrawPile {
    fn eq(&self, other: &Vec<CombatCard>) -> bool {
        self.active() == other.as_slice()
    }
}

/// Discard-pile storage specialized for branch-heavy exact search.
///
/// Java observes the discard pile in insertion order (bottom first, top
/// last), and state identity reads that order much more often than a rule
/// mutates an old discard. Snapshots therefore share the existing prefix and
/// a short append tail independently. Appending only detaches that tail;
/// identity and guide paths still read two contiguous slices without walking
/// a node chain. Selection, old-card mutation, and shuffle are explicit
/// flattening boundaries.
#[derive(Clone)]
pub struct DiscardPile {
    base: Arc<Vec<CombatCard>>,
    appended: Arc<Vec<CombatCard>>,
}

impl DiscardPile {
    pub fn len(&self) -> usize {
        self.base.len() + self.appended.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> DiscardIter<'_> {
        DiscardIter {
            base: self.base.iter(),
            appended: self.appended.iter(),
        }
    }

    pub fn iter_mut(&mut self) -> slice::IterMut<'_, CombatCard> {
        self.materialize().iter_mut()
    }

    pub fn push(&mut self, card: CombatCard) {
        if self.appended.is_empty() {
            if let Some(base) = Arc::get_mut(&mut self.base) {
                base.push(card);
                return;
            }
        }
        Arc::make_mut(&mut self.appended).push(card);
    }

    pub fn remove(&mut self, index: usize) -> CombatCard {
        assert!(index < self.len(), "discard pile index out of bounds");
        if index >= self.base.len() {
            return Arc::make_mut(&mut self.appended).remove(index - self.base.len());
        }
        self.materialize().remove(index)
    }

    pub fn remove_by_lineage(&mut self, lineage: CardLineageId) -> Option<CombatCard> {
        let index = self.iter().position(|card| card.lineage == lineage)?;
        Some(self.remove(index))
    }

    pub fn clear(&mut self) {
        *self = Self::default();
    }

    pub fn into_vec(self) -> Vec<CombatCard> {
        if self.appended.is_empty() {
            return Arc::unwrap_or_clone(self.base);
        }
        self.iter().cloned().collect()
    }

    fn materialize(&mut self) -> &mut Vec<CombatCard> {
        if !self.appended.is_empty() {
            self.base = Arc::new(self.iter().cloned().collect());
            self.appended = Arc::new(Vec::new());
        }
        Arc::make_mut(&mut self.base)
    }
}

impl Default for DiscardPile {
    fn default() -> Self {
        Self {
            base: Arc::new(Vec::new()),
            appended: Arc::new(Vec::new()),
        }
    }
}

impl fmt::Debug for DiscardPile {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.debug_list().entries(self.iter()).finish()
    }
}

impl PartialEq for DiscardPile {
    fn eq(&self, other: &Self) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

impl Eq for DiscardPile {}

impl Serialize for DiscardPile {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut sequence = serializer.serialize_seq(Some(self.len()))?;
        for card in self {
            sequence.serialize_element(card)?;
        }
        sequence.end()
    }
}

impl<'de> Deserialize<'de> for DiscardPile {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Vec::<CombatCard>::deserialize(deserializer).map(Into::into)
    }
}

impl Index<usize> for DiscardPile {
    type Output = CombatCard;

    fn index(&self, index: usize) -> &Self::Output {
        if index < self.base.len() {
            &self.base[index]
        } else {
            &self.appended[index - self.base.len()]
        }
    }
}

impl IndexMut<usize> for DiscardPile {
    fn index_mut(&mut self, index: usize) -> &mut Self::Output {
        &mut self.materialize()[index]
    }
}

impl From<Vec<CombatCard>> for DiscardPile {
    fn from(cards: Vec<CombatCard>) -> Self {
        Self {
            base: Arc::new(cards),
            appended: Arc::new(Vec::new()),
        }
    }
}

impl From<DiscardPile> for Vec<CombatCard> {
    fn from(pile: DiscardPile) -> Self {
        pile.into_vec()
    }
}

impl FromIterator<CombatCard> for DiscardPile {
    fn from_iter<T: IntoIterator<Item = CombatCard>>(iter: T) -> Self {
        Vec::from_iter(iter).into()
    }
}

impl IntoIterator for DiscardPile {
    type Item = CombatCard;
    type IntoIter = std::vec::IntoIter<CombatCard>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a DiscardPile {
    type Item = &'a CombatCard;
    type IntoIter = DiscardIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl PartialEq<Vec<CombatCard>> for DiscardPile {
    fn eq(&self, other: &Vec<CombatCard>) -> bool {
        self.len() == other.len() && self.iter().eq(other.iter())
    }
}

#[derive(Clone)]
pub struct DiscardIter<'a> {
    base: slice::Iter<'a, CombatCard>,
    appended: slice::Iter<'a, CombatCard>,
}

impl<'a> Iterator for DiscardIter<'a> {
    type Item = &'a CombatCard;

    fn next(&mut self) -> Option<Self::Item> {
        self.base.next().or_else(|| self.appended.next())
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        let remaining = self.base.len() + self.appended.len();
        (remaining, Some(remaining))
    }
}

impl ExactSizeIterator for DiscardIter<'_> {}

/// Allocation-free read view used where game rules can inspect several pile
/// representations through one interface.
#[derive(Clone, Copy)]
pub enum CardPileView<'a> {
    Contiguous(&'a [CombatCard]),
    Discard(&'a DiscardPile),
}

impl<'a> CardPileView<'a> {
    pub fn len(self) -> usize {
        match self {
            Self::Contiguous(cards) => cards.len(),
            Self::Discard(cards) => cards.len(),
        }
    }

    pub fn is_empty(self) -> bool {
        self.len() == 0
    }

    pub fn iter(self) -> CardPileViewIter<'a> {
        match self {
            Self::Contiguous(cards) => CardPileViewIter::Contiguous(cards.iter()),
            Self::Discard(cards) => CardPileViewIter::Discard(cards.iter()),
        }
    }
}

impl Index<usize> for CardPileView<'_> {
    type Output = CombatCard;

    fn index(&self, index: usize) -> &Self::Output {
        match self {
            Self::Contiguous(cards) => &cards[index],
            Self::Discard(cards) => &cards[index],
        }
    }
}

pub enum CardPileViewIter<'a> {
    Contiguous(slice::Iter<'a, CombatCard>),
    Discard(DiscardIter<'a>),
}

impl<'a> Iterator for CardPileViewIter<'a> {
    type Item = &'a CombatCard;

    fn next(&mut self) -> Option<Self::Item> {
        match self {
            Self::Contiguous(cards) => cards.next(),
            Self::Discard(cards) => cards.next(),
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        match self {
            Self::Contiguous(cards) => cards.size_hint(),
            Self::Discard(cards) => cards.size_hint(),
        }
    }
}

impl ExactSizeIterator for CardPileViewIter<'_> {}

/// A combat card pile with cheap snapshots and ordinary `Vec` ergonomics.
///
/// Exact search clones combat positions far more often than it mutates every
/// card zone.  Sharing the immutable backing allocation keeps those snapshots
/// cheap; the first mutable operation detaches through `Arc::make_mut`.
/// `serde(transparent)` deliberately preserves the existing JSON array wire
/// format so continuations, fingerprints, and external tools do not learn
/// about this ownership optimization.
#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(transparent)]
pub struct SharedCardPile(Arc<Vec<CombatCard>>);

impl SharedCardPile {
    pub fn into_vec(self) -> Vec<CombatCard> {
        Arc::unwrap_or_clone(self.0)
    }
}

impl Deref for SharedCardPile {
    type Target = Vec<CombatCard>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for SharedCardPile {
    fn deref_mut(&mut self) -> &mut Self::Target {
        Arc::make_mut(&mut self.0)
    }
}

impl From<Vec<CombatCard>> for SharedCardPile {
    fn from(cards: Vec<CombatCard>) -> Self {
        Self(Arc::new(cards))
    }
}

impl From<SharedCardPile> for Vec<CombatCard> {
    fn from(pile: SharedCardPile) -> Self {
        pile.into_vec()
    }
}

impl FromIterator<CombatCard> for SharedCardPile {
    fn from_iter<T: IntoIterator<Item = CombatCard>>(iter: T) -> Self {
        Vec::from_iter(iter).into()
    }
}

impl Extend<CombatCard> for SharedCardPile {
    fn extend<T: IntoIterator<Item = CombatCard>>(&mut self, iter: T) {
        Arc::make_mut(&mut self.0).extend(iter);
    }
}

impl IntoIterator for SharedCardPile {
    type Item = CombatCard;
    type IntoIter = std::vec::IntoIter<CombatCard>;

    fn into_iter(self) -> Self::IntoIter {
        self.into_vec().into_iter()
    }
}

impl<'a> IntoIterator for &'a SharedCardPile {
    type Item = &'a CombatCard;
    type IntoIter = slice::Iter<'a, CombatCard>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl<'a> IntoIterator for &'a mut SharedCardPile {
    type Item = &'a mut CombatCard;
    type IntoIter = slice::IterMut<'a, CombatCard>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter_mut()
    }
}

impl PartialEq<Vec<CombatCard>> for SharedCardPile {
    fn eq(&self, other: &Vec<CombatCard>) -> bool {
        self.as_slice() == other.as_slice()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::content::cards::CardId;

    #[test]
    fn clone_shares_until_first_mutation() {
        let original: SharedCardPile = vec![CombatCard::for_scenario(CardId::Strike, 1)].into();
        let mut changed = original.clone();

        assert!(Arc::ptr_eq(&original.0, &changed.0));
        changed.push(CombatCard::for_scenario(CardId::Defend, 2));

        assert!(!Arc::ptr_eq(&original.0, &changed.0));
        assert_eq!(original.len(), 1);
        assert_eq!(changed.len(), 2);
    }

    #[test]
    fn serde_wire_format_remains_a_plain_array() {
        let cards = vec![CombatCard::for_scenario(CardId::Bash, 7)];
        let pile: SharedCardPile = cards.clone().into();

        assert_eq!(
            serde_json::to_value(&pile).expect("serialize card pile"),
            serde_json::to_value(&cards).expect("serialize card vec")
        );
        assert_eq!(
            serde_json::from_value::<SharedCardPile>(
                serde_json::to_value(&cards).expect("serialize source vec")
            )
            .expect("deserialize card pile"),
            cards
        );
    }

    #[test]
    fn draw_cursor_keeps_sibling_snapshot_and_serializes_only_active_cards() {
        let first = CombatCard::for_scenario(CardId::Strike, 1);
        let second = CombatCard::for_scenario(CardId::Defend, 2);
        let mut advanced: DrawPile = vec![first.clone(), second.clone()].into();
        let sibling = advanced.clone();

        assert_eq!(advanced.draw_top(), Some(first));
        assert!(Arc::ptr_eq(&advanced.cards, &sibling.cards));
        assert_eq!(advanced.as_ref(), std::slice::from_ref(&second));
        assert_eq!(
            sibling.as_ref(),
            [CombatCard::for_scenario(CardId::Strike, 1), second.clone()]
        );
        assert_eq!(
            serde_json::to_value(&advanced).expect("serialize active draw pile"),
            serde_json::to_value(vec![second]).expect("serialize expected active cards")
        );
    }

    #[test]
    fn order_mutation_materializes_only_the_mutated_branch() {
        let mut branch: DrawPile = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ]
        .into();
        let sibling = branch.clone();

        assert_eq!(
            branch.draw_top().map(|card| card.lineage),
            Some(crate::test_support::scenario_lineage(1))
        );
        branch.push_top(CombatCard::for_scenario(CardId::Bash, 3));

        assert!(!Arc::ptr_eq(&branch.cards, &sibling.cards));
        assert_eq!(
            branch.iter().map(|card| card.lineage).collect::<Vec<_>>(),
            [3, 2].map(crate::test_support::scenario_lineage).to_vec()
        );
        assert_eq!(
            sibling.iter().map(|card| card.lineage).collect::<Vec<_>>(),
            [1, 2].map(crate::test_support::scenario_lineage).to_vec()
        );
    }

    #[test]
    fn discard_append_after_branch_reuses_base_and_detaches_tail() {
        let original: DiscardPile = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ]
        .into();
        let mut branch = original.clone();

        assert!(Arc::ptr_eq(&original.base, &branch.base));
        assert!(Arc::ptr_eq(&original.appended, &branch.appended));
        branch.push(CombatCard::for_scenario(CardId::Bash, 3));

        assert!(Arc::ptr_eq(&original.base, &branch.base));
        assert!(!Arc::ptr_eq(&original.appended, &branch.appended));
        assert_eq!(
            branch.iter().map(|card| card.lineage).collect::<Vec<_>>(),
            [1, 2, 3]
                .map(crate::test_support::scenario_lineage)
                .to_vec()
        );
        assert_eq!(
            original.iter().map(|card| card.lineage).collect::<Vec<_>>(),
            [1, 2].map(crate::test_support::scenario_lineage).to_vec()
        );
    }

    #[test]
    fn discard_arbitrary_removal_materializes_only_the_mutated_branch() {
        let base: DiscardPile = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
        ]
        .into();
        let mut branch = base.clone();
        branch.push(CombatCard::for_scenario(CardId::Bash, 3));
        let sibling = branch.clone();

        assert_eq!(
            branch.remove(0).lineage,
            crate::runtime::combat::CardLineageId::constructed_scenario(1)
        );

        assert!(!Arc::ptr_eq(&branch.base, &sibling.base));
        assert!(branch.appended.is_empty());
        assert!(!sibling.appended.is_empty());
        assert_eq!(
            branch.iter().map(|card| card.lineage).collect::<Vec<_>>(),
            [2, 3].map(crate::test_support::scenario_lineage).to_vec()
        );
        assert_eq!(
            sibling.iter().map(|card| card.lineage).collect::<Vec<_>>(),
            [1, 2, 3]
                .map(crate::test_support::scenario_lineage)
                .to_vec()
        );
    }

    #[test]
    fn discard_top_removal_preserves_the_sibling_snapshot() {
        let base: DiscardPile = vec![CombatCard::for_scenario(CardId::Strike, 1)].into();
        let mut branch = base.clone();
        branch.push(CombatCard::for_scenario(CardId::Defend, 2));

        assert_eq!(
            branch.remove(1).lineage,
            crate::runtime::combat::CardLineageId::constructed_scenario(2)
        );
        assert_eq!(
            base.iter().map(|card| card.lineage).collect::<Vec<_>>(),
            vec![crate::test_support::scenario_lineage(1)]
        );
        assert_eq!(branch.into_vec(), base.into_vec());
    }

    #[test]
    fn discard_serde_and_flattening_preserve_java_order() {
        let base: DiscardPile = vec![CombatCard::for_scenario(CardId::Strike, 1)].into();
        let mut branch = base.clone();
        branch.push(CombatCard::for_scenario(CardId::Defend, 2));
        branch.push(CombatCard::for_scenario(CardId::Bash, 3));
        let expected = vec![
            CombatCard::for_scenario(CardId::Strike, 1),
            CombatCard::for_scenario(CardId::Defend, 2),
            CombatCard::for_scenario(CardId::Bash, 3),
        ];

        let serialized = serde_json::to_value(&branch).expect("serialize discard pile");
        assert_eq!(
            serialized,
            serde_json::to_value(&expected).expect("serialize expected cards")
        );
        assert_eq!(
            serde_json::from_value::<DiscardPile>(serialized)
                .expect("deserialize discard pile")
                .into_vec(),
            expected
        );
    }
}
