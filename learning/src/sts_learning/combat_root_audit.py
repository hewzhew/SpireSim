"""Compact typed curriculum facts for opaque exact combat roots."""

from __future__ import annotations

import operator
from collections import Counter
from collections.abc import Sequence
from dataclasses import dataclass


class CombatRootAuditError(RuntimeError):
    """A bridge root audit omitted or malformed curriculum identity."""


@dataclass(frozen=True)
class CombatRootDeckCard:
    card_id: str
    upgrades: int
    count: int

    def as_mapping(self) -> dict[str, object]:
        return {
            "card_id": self.card_id,
            "upgrades": self.upgrades,
            "count": self.count,
        }


@dataclass(frozen=True)
class CombatRootAudit:
    seed: int
    act: int
    floor: int
    ascension_level: int
    hp: int
    max_hp: int
    potion_ids: tuple[str | None, ...]
    encounter_id: str
    monster_ids: tuple[str, ...]
    is_elite_fight: bool
    is_boss_fight: bool
    deck: tuple[CombatRootDeckCard, ...]
    relic_ids: tuple[str, ...]

    @property
    def deck_card_count(self) -> int:
        return sum(card.count for card in self.deck)

    def as_mapping(self) -> dict[str, object]:
        return {
            "seed": self.seed,
            "act": self.act,
            "floor": self.floor,
            "ascension_level": self.ascension_level,
            "hp": self.hp,
            "max_hp": self.max_hp,
            "potion_ids": self.potion_ids,
            "encounter_id": self.encounter_id,
            "monster_ids": self.monster_ids,
            "is_elite_fight": self.is_elite_fight,
            "is_boss_fight": self.is_boss_fight,
            "deck": tuple(card.as_mapping() for card in self.deck),
            "relic_ids": self.relic_ids,
        }


def read_combat_root_audit(source: object, slot_index: int) -> CombatRootAudit:
    """Read one bounded public audit without exposing the root checkpoint."""

    audit_source = getattr(source, "combat_root_audit", None)
    if not callable(audit_source):
        raise CombatRootAuditError("combat root source lacks combat_root_audit()")
    try:
        audit = audit_source(_nonnegative(slot_index, "combat root audit slot"))
    except Exception as error:
        raise CombatRootAuditError(f"combat root audit failed: {error}") from error
    ascension_level = _attribute_integer(
        audit,
        "ascension_level",
        "combat root audit ascension",
    )
    if ascension_level > 20:
        raise CombatRootAuditError("combat root audit ascension exceeds 20")
    return CombatRootAudit(
        seed=_attribute_integer(audit, "seed", "combat root audit seed"),
        act=_attribute_integer(audit, "act", "combat root audit act"),
        floor=_attribute_integer(audit, "floor", "combat root audit floor"),
        ascension_level=ascension_level,
        hp=_attribute_integer(audit, "hp", "combat root audit hp"),
        max_hp=_positive_attribute_integer(
            audit,
            "max_hp",
            "combat root audit max hp",
        ),
        potion_ids=_optional_identities(audit, "potion_ids"),
        encounter_id=_text_attribute(audit, "encounter_id"),
        monster_ids=_identities(audit, "monster_ids"),
        is_elite_fight=_boolean_attribute(audit, "is_elite_fight"),
        is_boss_fight=_boolean_attribute(audit, "is_boss_fight"),
        deck=_deck(audit),
        relic_ids=_identities(audit, "relic_ids"),
    )


def read_combat_root_audits(
    source: object,
    slot_indices: Sequence[int],
) -> tuple[CombatRootAudit, ...]:
    return tuple(read_combat_root_audit(source, slot) for slot in slot_indices)


def _deck(audit: object) -> tuple[CombatRootDeckCard, ...]:
    raw_cards = _attribute(audit, "master_deck_cards")
    if not isinstance(raw_cards, Sequence) or isinstance(raw_cards, (str, bytes)):
        raise CombatRootAuditError("combat root audit deck must be a sequence")
    counts: Counter[tuple[str, int]] = Counter()
    for raw in raw_cards:
        if (
            not isinstance(raw, Sequence)
            or isinstance(raw, (str, bytes))
            or len(raw) != 2
        ):
            raise CombatRootAuditError(
                "combat root audit card must contain identity and upgrades"
            )
        card_id = raw[0]
        if not isinstance(card_id, str) or not card_id:
            raise CombatRootAuditError(
                "combat root audit card identity must be non-empty text"
            )
        upgrades = _nonnegative(raw[1], "combat root audit card upgrades")
        counts[(card_id, upgrades)] += 1
    return tuple(
        CombatRootDeckCard(card_id, upgrades, count)
        for (card_id, upgrades), count in sorted(counts.items())
    )


def _identities(audit: object, name: str) -> tuple[str, ...]:
    raw = _attribute(audit, name)
    if not isinstance(raw, Sequence) or isinstance(raw, (str, bytes)):
        raise CombatRootAuditError(f"combat root audit {name} must be a sequence")
    identities = tuple(raw)
    if any(not isinstance(identity, str) or not identity for identity in identities):
        raise CombatRootAuditError(
            f"combat root audit {name} must contain non-empty text"
        )
    return identities


def _optional_identities(audit: object, name: str) -> tuple[str | None, ...]:
    raw = _attribute(audit, name)
    if not isinstance(raw, Sequence) or isinstance(raw, (str, bytes)):
        raise CombatRootAuditError(f"combat root audit {name} must be a sequence")
    identities = tuple(raw)
    if any(
        identity is not None
        and (not isinstance(identity, str) or not identity)
        for identity in identities
    ):
        raise CombatRootAuditError(
            f"combat root audit {name} must contain text or None"
        )
    return identities


def _text_attribute(source: object, name: str) -> str:
    value = _attribute(source, name)
    if not isinstance(value, str) or not value:
        raise CombatRootAuditError(f"combat root audit {name} must be non-empty text")
    return value


def _boolean_attribute(source: object, name: str) -> bool:
    value = _attribute(source, name)
    if not isinstance(value, bool):
        raise CombatRootAuditError(f"combat root audit {name} must be boolean")
    return value


def _attribute_integer(source: object, name: str, label: str) -> int:
    return _nonnegative(_attribute(source, name), label)


def _positive_attribute_integer(source: object, name: str, label: str) -> int:
    value = _attribute_integer(source, name, label)
    if value == 0:
        raise CombatRootAuditError(f"{label} must be positive")
    return value


def _attribute(source: object, name: str) -> object:
    try:
        return getattr(source, name)
    except AttributeError as error:
        raise CombatRootAuditError(f"combat root audit lacks {name}") from error


def _nonnegative(value: object, name: str) -> int:
    if isinstance(value, bool):
        raise CombatRootAuditError(f"{name} must be an integer, not bool")
    try:
        normalized = operator.index(value)
    except TypeError as error:
        raise CombatRootAuditError(f"{name} must be an integer") from error
    if normalized < 0:
        raise CombatRootAuditError(f"{name} must be non-negative")
    return normalized
