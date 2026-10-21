"""Teacher-authored conditional preferences over an unchanged public menu.

This is executable strategy knowledge, not fitted labels or game mechanics.
The first guide covers only an Ironclad starter inventory in Act 1. Native
semantic facts supply card numbers; the paired native audit supplies identities.
"""
from __future__ import annotations

import dataclasses
import hashlib
import json
import math
from collections import Counter, defaultdict
from pathlib import Path

from .decision_rows import PreparedDecisionRows
from .fixed_combat_environment import canonical_json
from .policy import BehaviorManifestId
from .semantic_batch import select_semantic_decision_rows


FIRST_ATTACK_GUIDE = "act1-first-attack-v1"
_ATTACKS = frozenset({"Anger", "Carnage", "Cleave", "Hemokinesis", "PommelStrike", "TwinStrike"})
_STARTER = frozenset({"Strike", "Defend", "Bash", "AscendersBane"})
_COVERED = _ATTACKS | {"GhostlyArmor", "RecklessCharge", "Clash", "WildStrike", "IronWave", "BodySlam"}
# Stable gameplay ordinals from the core, reviewed with semantic schema 32.
# These bind the paired audit identities, rather than deriving them from damage.
_CARD_IDS = {"Strike": 0, "Defend": 1, "Bash": 2, "Cleave": 3, "IronWave": 4,
             "TwinStrike": 6, "PommelStrike": 20, "Hemokinesis": 36, "GhostlyArmor": 46,
             "WildStrike": 52, "RecklessCharge": 53, "Anger": 59, "Carnage": 71,
             "Clash": 72, "BodySlam": 73, "AscendersBane": 82}
_RELIC_IDS = {"BurningBlood": 20, "NeowsLament": 96}
_RELATIONS = {
    "healthy_starter_first_attack": "At least half HP; reviewed starter inventory and relics; efficient attack over ordinary block or conditional small output.",
    "healthy_starter_hemo_over_wound": "At least half HP; Hemokinesis requests at least Wild Strike's damage for the same payment; Burning Blood and no owned status payoff.",
    "early_starter_preserve_access": "Floors 1..4; at least three-quarter HP; four unupgraded Defends; preserve access over ordinary Ghostly Armor, Iron Wave, Body Slam or Clash.",
    "critical_hp_immediate_block": "At most quarter HP; reviewed starter inventory and relics; no remaining Neow's Lament activations; Ghostly Armor over Hemokinesis, Reckless Charge or skip.",
}


@dataclasses.dataclass(frozen=True)
class GuidePreference:
    better: int
    worse: int
    reason: str


@dataclasses.dataclass(frozen=True)
class GuideRuleEvaluation:
    rule: str
    blocked_by: tuple[str, ...]
    preference_count: int


@dataclasses.dataclass(frozen=True)
class GuideDecision:
    status: str
    ordinal: int | None
    contenders: tuple[int, ...]
    preferences: tuple[GuidePreference, ...]
    issues: tuple[str, ...]
    rules: tuple[GuideRuleEvaluation, ...] = ()

    def record(self):
        return json.loads(canonical_json(dataclasses.asdict(self)))


def resolve_preferences(candidate_count, preferences, *, issues=(), rules=()):
    """Resolve a partial order; an isolated new option remains a contender."""
    if type(candidate_count) is not int or candidate_count <= 0:
        raise ValueError("guide requires a nonempty complete menu")
    preferences = tuple(preferences)
    successors = [set() for _ in range(candidate_count)]
    indegree = [0] * candidate_count
    for preference in preferences:
        if (not isinstance(preference, GuidePreference)
                or any(type(i) is not int or not 0 <= i < candidate_count
                       for i in (preference.better, preference.worse))
                or preference.better == preference.worse or not preference.reason):
            raise ValueError("invalid guide preference")
        if preference.worse not in successors[preference.better]:
            successors[preference.better].add(preference.worse)
            indegree[preference.worse] += 1
    contenders = tuple(i for i, degree in enumerate(indegree) if degree == 0)
    pending = list(contenders)
    visited = 0
    while pending:
        source = pending.pop()
        visited += 1
        for target in successors[source]:
            indegree[target] -= 1
            if indegree[target] == 0:
                pending.append(target)
    if visited != candidate_count:
        return GuideDecision("unknown", None, tuple(range(candidate_count)), preferences,
                             (*issues, "conflicting_preferences"), tuple(rules))
    if len(contenders) > 1 and not issues:
        issues = ("residual_comparison",)
    resolved = len(contenders) == 1 and not issues
    return GuideDecision("resolved" if resolved else "unknown",
                         contenders[0] if resolved else None, contenders, preferences, tuple(issues), tuple(rules))


class StrategicGuideUnresolved(RuntimeError):
    """The full public input is preserved; no cohort choice was submitted."""
    def __init__(self, batch, decisions):
        self.prepared = PreparedDecisionRows.capture(batch)
        self.decisions = tuple(decisions)
        super().__init__("compiled strategic guide leaves unresolved choices")


class CompiledStrategicGuide:
    """Fixed Codex strategy relations; no teacher lookup during inference."""
    def __init__(self, schema, *, name=FIRST_ATTACK_GUIDE):
        if name != FIRST_ATTACK_GUIDE or schema.get("version") != 32:
            raise ValueError("guide requires its declared name and semantic schema 32")
        self._kinds = {value: key for key, value in schema["token_kind"].items()}
        self._categories = {value: key for key, value in schema["categorical_field"].items()}
        self._scalars = {value: key for key, value in schema["scalar_field"].items()}
        self._actions = dict(schema["action_kind"])
        self._complete = schema["completeness"]["Complete"]
        self._card_reward = schema["context_kind"]["CardReward"]
        self._reward_kinds = dict(schema["reward_kind"])
        self.schema_sha256 = hashlib.sha256(canonical_json(schema).encode()).hexdigest()
        self._manifest_json = canonical_json({
            "guide": name, "teacher": "Codex", "semantic_schema_sha256": self.schema_sha256,
            "implementation_sha256": hashlib.sha256(Path(__file__).read_bytes()).hexdigest(),
            "conditions": {"act": 1, "inventory": "starter cards only; persistent numbers unchanged",
                           "relics": "BurningBlood; optional NeowsLament with checked lifecycle", "healthy_hp_fraction": 0.5,
                           "critical_hp_fraction": 0.25},
            "attack_candidates": sorted(_ATTACKS),
            "relations": _RELATIONS,
            "reviewed_identities": {"cards": _CARD_IDS, "relics": _RELIC_IDS},
            "residual": "complete menu partial order; unresolved stops before choice",
        })
        self.behavior_manifest_id = BehaviorManifestId(hashlib.sha256(self._manifest_json.encode()).digest())

    def manifest(self):
        return json.loads(self._manifest_json)

    def evaluate(self, batch, audits):
        # Use the native row algebra for ownership and shape validation, without
        # converting the observation into model vocabulary/statistical columns.
        batch = select_semantic_decision_rows(batch, range(len(batch["slot_indices"])))
        if len(audits) != len(batch["slot_indices"]):
            raise ValueError("guide audit rows differ")
        if batch["semantic"]["schema_version"] != 32:
            raise ValueError("guide semantic version differs")
        return tuple(self._evaluate_row(select_semantic_decision_rows(batch, [row]), audit)
                     for row, audit in enumerate(audits))

    def _evaluate_row(self, batch, audit):
        count = int(batch["candidate_counts"][0])
        def unknown(*issues):
            return resolve_preferences(count, (), issues=issues)
        if (not isinstance(audit, dict)
                or audit.get("schema") != "sts-learning-strategic-decision-audit-v2"
                or audit.get("decision_site") != "card_reward"
                or len(audit.get("candidates", ())) != count):
            raise ValueError("guide requires a paired native card reward audit")
        sem = batch["semantic"]
        if int(sem["completeness"][0]) != self._complete:
            return unknown("incomplete_public_facts")
        categories, scalars = defaultdict(lambda: defaultdict(list)), defaultdict(dict)
        for token, field, value in zip(*[sem["categorical"][key]
                                         for key in ("token_indices", "field", "value")], strict=True):
            categories[int(token)][self._categories[int(field)]].append(int(value))
        for token, field, value in zip(*[sem["scalar"][key]
                                         for key in ("token_indices", "field", "value")], strict=True):
            if not math.isfinite(float(value)):
                return unknown("nonfinite_public_number")
            name = self._scalars[int(field)]
            if name in scalars[int(token)]:
                raise ValueError("guide requires unique scalar facts on each token")
            scalars[int(token)][name] = float(value)
        observed = defaultdict(list)
        for token, (kind, scope) in enumerate(zip(sem["token"]["kind"], sem["token"]["scope"], strict=True)):
            if scope == 0:
                observed[self._kinds[int(kind)]].append(token)
        if len(observed["Run"]) != 1 or len(observed["Context"]) != 1:
            return unknown("missing_run_or_context")
        run, context = observed["Run"][0], observed["Context"][0]
        if categories[context]["ContextKind"] != [self._card_reward]:
            return unknown("not_a_card_reward_context")
        facts = scalars[run]
        if not {"Act", "CurrentHp", "MaxHp"} <= facts.keys():
            return unknown("missing_act_or_hp")
        hp, maximum = facts["CurrentHp"], facts["MaxHp"]
        if not (0 < hp <= maximum) or any(not value.is_integer() for value in (hp, maximum)):
            return unknown("invalid_hp")
        # PlayerClass=0 is the core's schema-31 Ironclad identity, not inferred
        # from the names of offered cards.
        if facts["Act"] != 1 or categories[run]["PlayerClass"] != [0]:
            return unknown("outside_act1_ironclad")
        deck, relics = audit.get("public_deck"), audit.get("public_relics")
        if (not isinstance(deck, list) or len(deck) != len(observed["Card"])
                or not isinstance(relics, list) or len(relics) != len(observed["Relic"])):
            return unknown("missing_or_misaligned_inventory")
        relic_issues, lament_active = self._reviewed_relics(relics)
        for card in deck:
            if not {"card", "upgrades", "cost_modifier", "misc_value",
                    "base_damage_override", "base_block_override"} <= card.keys():
                return unknown("missing_inventory_numbers")
            if (card.get("card") not in _STARTER or type(card.get("upgrades")) is not int
                    or card["upgrades"] not in (0, 1) or card.get("cost_modifier") != 0
                    or card.get("misc_value") != 0 or card.get("base_damage_override") is not None
                    or card.get("base_block_override") is not None):
                return unknown("outside_starter_inventory")
        for card, token in zip(deck, observed["Card"], strict=True):
            if categories[token]["CardId"] != [_CARD_IDS[card["card"]]]:
                raise ValueError("guide audit inventory differs from semantic cards")
        if not relic_issues:
            if any(type(relic.get("counter")) is not int or type(relic.get("amount")) is not int
                   for relic in relics):
                return unknown("missing_relic_lifecycle")
            expected = sorted((_RELIC_IDS[relic["relic"]], relic["counter"], int(relic["used_up"]), relic["amount"])
                              for relic in relics)
            actual = []
            for token in observed["Relic"]:
                identity, used = categories[token]["RelicId"], categories[token]["RelicUsedUp"]
                if (len(identity) != 1 or len(used) != 1
                        or not {"RelicCounter", "RelicAmount"} <= scalars[token].keys()):
                    return unknown("missing_relic_lifecycle")
                actual.append((identity[0], scalars[token]["RelicCounter"], used[0], scalars[token]["RelicAmount"]))
            if sorted(actual) != expected:
                raise ValueError("guide audit relics differ from semantic inventory")
        amounts = Counter(card["card"] for card in deck)
        if not (3 <= amounts["Strike"] <= 5 and 2 <= amounts["Defend"] <= 4
                and amounts["Bash"] == 1 and amounts["AscendersBane"] <= 1):
            return unknown("outside_starter_inventory")
        reward_kinds = [categories[token]["RewardKind"] for token in observed["PendingReward"]]
        if (reward_kinds.count([self._reward_kinds["CardReward"]]) > 1
                or [self._reward_kinds["Relic"]] in reward_kinds):
            return unknown("joint_reward_preparation_unresolved")
        healthy, critical = 2 * hp >= maximum, 4 * hp <= maximum
        if not healthy and not critical:
            return unknown("intermediate_hp_tradeoff")
        roles, issues, candidate_facts = [], list(relic_issues), []
        for ordinal, (candidate, token) in enumerate(zip(audit["candidates"], sem["candidate_token_indices"], strict=True)):
            token = int(token)
            kind = candidate.get("kind")
            if kind == "skip_card_reward":
                expected, role = self._actions["SkipCardReward"], "skip"
            elif kind == "take_card":
                expected, role = self._actions["TakeCard"], candidate.get("card")
                if (type(candidate.get("upgrades")) is not int or candidate["upgrades"] not in (0, 1)
                        or scalars[token].get("ActionUpgrades") != candidate["upgrades"]):
                    return unknown(f"missing_candidate_upgrade:{ordinal}")
                if role in _COVERED:
                    if categories[token]["ActionCardId"] != [_CARD_IDS[role]]:
                        raise ValueError("guide audit card differs from semantic candidate")
                    if not self._reviewed_numbers(role, scalars[token], categories[token], candidate["upgrades"]):
                        issues.append(f"missing_or_uncovered_candidate_numbers:{ordinal}")
                        role = "uncovered"
                else:
                    role = "uncovered"
            else:
                expected, role = None, "uncovered"
            if expected is not None and categories[token]["ActionKind"] != [expected]:
                raise ValueError("guide audit action differs from semantic candidate")
            roles.append(role)
            candidate_facts.append(scalars[token])
        preferences, rules = [], []

        def relation(name, checks, compare):
            blocked = tuple(reason for reason, passed in checks if not passed)
            edges = [] if blocked else [GuidePreference(better, worse, name)
                for better in range(count) for worse in range(count)
                if better != worse and compare(better, worse)]
            rules.append(GuideRuleEvaluation(name, blocked, len(edges)))
            preferences.extend(edges)

        reviewed_relics = not relic_issues
        relation("healthy_starter_first_attack", (
            ("hp_below_half", healthy), ("unreviewed_relic_effects", reviewed_relics),
        ), lambda a, b: roles[a] in _ATTACKS and roles[b] in {
            "GhostlyArmor", "RecklessCharge", "Clash", "IronWave", "BodySlam", "skip"})
        relation("healthy_starter_hemo_over_wound", (
            ("hp_below_half", healthy), ("unreviewed_relic_effects", reviewed_relics),
        ), lambda a, b: roles[a] == "Hemokinesis" and roles[b] == "WildStrike"
            and self._damage(candidate_facts[a], audit["candidates"][a]["upgrades"])
                >= self._damage(candidate_facts[b], audit["candidates"][b]["upgrades"]))
        floor = facts.get("Floor")
        early = floor is not None and floor.is_integer() and 1 <= floor <= 4
        ordinary_defends = sum(card["card"] == "Defend" and card["upgrades"] == 0 for card in deck)
        relation("early_starter_preserve_access", (
            ("outside_early_preparation", early), ("hp_below_three_quarters", 4 * hp >= 3 * maximum),
            ("basic_block_inventory_changed", ordinary_defends == 4),
            ("unreviewed_relic_effects", reviewed_relics),
        ), lambda a, b: roles[a] == "skip" and roles[b] in {"GhostlyArmor", "Clash", "IronWave", "BodySlam"}
            and audit["candidates"][b]["upgrades"] == 0)
        relation("critical_hp_immediate_block", (
            ("hp_above_quarter", critical), ("unreviewed_relic_effects", reviewed_relics),
            ("neows_lament_buffer_changes_urgency", not lament_active),
        ), lambda a, b: roles[a] == "GhostlyArmor" and roles[b] in {"Hemokinesis", "RecklessCharge", "skip"})
        if critical and lament_active and reviewed_relics:
            issues.append("neows_lament_buffer_changes_urgency")
        issues.extend(f"uncovered_candidate:{i}" for i, role in enumerate(roles) if role == "uncovered")
        return resolve_preferences(count, preferences, issues=issues, rules=rules)

    @staticmethod
    def _reviewed_relics(relics):
        # Lament does not change these long-term output/access defaults, but
        # its remaining easy fights change the critical-HP urgency premise.
        by_name = {relic.get("relic"): relic for relic in relics}
        if (len(by_name) != len(relics) or not set(by_name) <= {"BurningBlood", "NeowsLament"}
                or "BurningBlood" not in by_name or by_name["BurningBlood"].get("used_up") is not False):
            return ("uncovered_relic_configuration",), False
        lament = by_name.get("NeowsLament")
        if lament is None:
            return (), False
        counter, used = lament.get("counter"), lament.get("used_up")
        if type(counter) is not int or type(used) is not bool:
            return ("missing_neows_lament_lifecycle",), False
        if not ((used and counter == -2) or (not used and 1 <= counter <= 3)):
            return ("uncovered_neows_lament_lifecycle",), False
        return (), not used

    @staticmethod
    def _damage(facts, upgrades):
        return facts["CardDefinitionDamage"] + upgrades * facts["CardUpgradeDamage"]

    @staticmethod
    def _reviewed_numbers(card, facts, categories, upgrades):
        required = {"CardBaseCost", "CardDefinitionDamage", "CardUpgradeDamage",
                    "CardDefinitionBlock", "CardUpgradeBlock"}
        if not required <= facts.keys():
            return False
        cost = facts["CardBaseCost"]
        damage = CompiledStrategicGuide._damage(facts, upgrades)
        block = facts["CardDefinitionBlock"] + upgrades * facts["CardUpgradeBlock"]
        if card == "GhostlyArmor":
            return cost == 1 and block >= 10
        if card == "RecklessCharge":
            return (cost == 0 and damage >= 7 and facts.get("CardStatusCreationRequest") == 1)
        # These enum values are the explicitly bound schema-31 mechanism
        # encoding: Attack delivery, fixed hits, base/Block amount, Hand-only
        # Attack condition, and Status inserted in Draw after the attack.
        if (categories["CardDamageDeliveryKind"] != [0] or categories["CardAttackHitsKind"] != [0]
                or categories["CardAttackAmountKind"] != [3 if card == "BodySlam" else 0]
                or categories["CardAttackPlayCondition"] != [1 if card == "Clash" else 0]):
            return False
        hits = facts.get("CardAttackFixedHits")
        if hits is None or not hits.is_integer() or hits <= 0:
            return False
        if card == "Clash":
            return cost == 0 and damage >= 14 and hits == 1
        if card == "WildStrike":
            return (cost == 1 and damage >= 12 and hits == 1 and facts.get("CardStatusCreationRequest") == 1
                    and categories["CardStatusCreationDestination"] == [1]
                    and categories["CardStatusCreationTiming"] == [0]
                    and categories["CardStatusCreationCardId"] == [78])
        if card == "IronWave":
            return upgrades == 0 and cost == 1 and damage == 5 and block == 5 and hits == 1
        if card == "BodySlam":
            return upgrades == 0 and cost == 1 and hits == 1
        if card == "Hemokinesis" and facts.get("CardSelfHpLossOnPlay") != 2:
            return False
        minimum = 6 if cost == 0 else 8 if card == "Cleave" else 9 if cost == 1 else 20
        return cost in (0, 1, 2) and damage * hits >= minimum
