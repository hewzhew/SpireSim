"""Frozen candidate-membership trees over named public semantic facts.

Readouts keep observation and candidate ownership separate. The optional owned
card upgrade readouts preserve direct same-token identity. The local-choice
readout omits the shared offered-card catalog; none uses graph joins or game
value rules.
"""
from __future__ import annotations

import json
import math
from collections import defaultdict
from dataclasses import dataclass

import numpy as np

from .policy import BatchPolicyChoice, BehaviorManifestId
from .semantic_batch import select_semantic_decision_rows


FIELD_READOUT = "owned-field-statistics-v1"
CARD_UPGRADE_READOUT = "owned-card-upgrade-statistics-v1"
LOCAL_CARD_READOUT = "owned-card-local-choice-statistics-v1"
CARD_CLASS_READOUT = "owned-card-class-local-choice-statistics-v1"
FIELD_READOUTS = (FIELD_READOUT, CARD_UPGRADE_READOUT, LOCAL_CARD_READOUT, CARD_CLASS_READOUT)
FIELD_CHOICE_TARGET_RELEVANCE = "choice-target-v1"
FIELD_GRADED_RELEVANCE = "preferred-acceptable-v1"
FIELD_RELEVANCES = (FIELD_CHOICE_TARGET_RELEVANCE, FIELD_GRADED_RELEVANCE)
_UPGRADE_READOUTS = (CARD_UPGRADE_READOUT, LOCAL_CARD_READOUT, CARD_CLASS_READOUT)
_LOCAL_READOUTS = (LOCAL_CARD_READOUT, CARD_CLASS_READOUT)


def field_readout_limits(readout):
    base = ("Counts and scalar count/sum/min/max by token kind and ownership; "
            "no graph relations. Missing differs from zero. Frozen training vocabulary ignores unseen columns.")
    return (base + (" Direct owned CardId-to-CardUpgrades/CanUpgrade association only; no effects-by-identity or cross-token joins."
                    if readout in _UPGRADE_READOUTS else " No entity field associations.")
            + (" Shared scope0 OfferedCard catalog omitted; own candidate payload and current inventory retained."
               if readout in _LOCAL_READOUTS else "")
            + (" Direct owned CardType/Basic-rarity groups preserve counts and scalar summaries; no value or turn-output estimates."
               if readout == CARD_CLASS_READOUT else ""))


def validate_field_relevance(relevance, objective):
    if not isinstance(relevance, str) or relevance not in FIELD_RELEVANCES:
        raise ValueError("field relevance differs")
    if relevance == FIELD_GRADED_RELEVANCE and objective != "lambdarank":
        raise ValueError("graded field relevance requires lambdarank")


def _enum_names(schema, key):
    values = schema.get(key)
    if (not isinstance(values, dict) or not values
            or any(not isinstance(name, str) or not name or "." in name
                   or type(number) is not int or number < 0
                   for name, number in values.items())
            or len(set(values.values())) != len(values)):
        raise ValueError(f"field classifier requires distinct {key} schema ids")
    return {number: name for name, number in values.items()}


class StrategicFieldEncoder:
    """One frozen, training-only column vocabulary; addresses are not features."""

    def __init__(self, schema, feature_names=None, *, readout=FIELD_READOUT):
        if not isinstance(readout, str) or readout not in FIELD_READOUTS:
            raise ValueError("field classifier readout differs")
        self.readout = readout
        self.schema = json.loads(json.dumps(schema))
        self.kinds = _enum_names(schema, "token_kind")
        self.categories = _enum_names(schema, "categorical_field")
        self.scalars = _enum_names(schema, "scalar_field")
        self.relations = _enum_names(schema, "relation_kind")
        sizes = schema.get("categorical_vocabulary_size", {})
        self.category_sizes = {}
        for field, size in sizes.items():
            number = int(field)
            if (number in self.category_sizes or number not in self.categories
                    or type(size) is not int or size <= 0):
                raise ValueError("field classifier categorical vocabulary differs")
            self.category_sizes[number] = size
        if set(self.category_sizes) != set(self.categories):
            raise ValueError("field classifier categorical vocabulary differs")
        if readout in _UPGRADE_READOUTS and (
                "Card" not in self.kinds.values() or "CardId" not in self.categories.values()
                or "CardCanUpgrade" not in self.categories.values() or "CardUpgrades" not in self.scalars.values()):
            raise ValueError("owned card upgrade readout requires typed identity and upgrade fields")
        if readout in _LOCAL_READOUTS and "OfferedCard" not in self.kinds.values():
            raise ValueError("local card choice readout requires typed offered card tokens")
        if readout == CARD_CLASS_READOUT:
            if not {"CardType", "CardRarity"} <= set(self.categories.values()):
                raise ValueError("owned card class readout requires typed type and rarity fields")
            self.card_types = _enum_names(schema, "card_type")
            self.card_rarities = _enum_names(schema, "card_rarity")
            basic = [value for value, name in self.card_rarities.items() if name == "Basic"]
            if not basic:
                raise ValueError("owned card class readout requires Basic rarity")
            self.basic_rarity = basic[0]
        if feature_names is not None:
            if (not isinstance(feature_names, (list, tuple)) or not feature_names
                    or any(not isinstance(name, str) for name in feature_names)
                    or list(feature_names) != sorted(set(feature_names))):
                raise ValueError("field classifier vocabulary must be sorted distinct names")
            for name in feature_names:
                self._validate_name(name)
        self.feature_names = None if feature_names is None else tuple(feature_names)
        self._columns = (None if feature_names is None else
                         {name: i for i, name in enumerate(feature_names)})

    def _validate_name(self, name):
        parts = name.split(".")
        if self.readout in _LOCAL_READOUTS and (
                parts == ["observation", "token_count", "OfferedCard"]
                or len(parts) > 2 and parts[0] == "observation" and parts[2] == "OfferedCard"):
            raise ValueError("local card choice vocabulary contains shared offered card catalog")
        if len(parts) >= 2 and parts[1] == "card_class":
            if (self.readout != CARD_CLASS_READOUT or len(parts) not in {6, 8}
                    or parts[:3] != ["observation", "card_class", "Card"]
                    or not parts[3].isdigit() or int(parts[3]) not in self.card_types
                    or parts[4] not in {"0", "1"}):
                raise ValueError("field classifier vocabulary contains an unknown card class feature")
            if len(parts) == 6 and parts[5] == "token_count":
                return
            if (len(parts) == 8 and parts[5] == "scalar" and parts[6] in self.scalars.values()
                    and parts[7] in {"count", "sum", "min", "max"}):
                return
            raise ValueError("field classifier vocabulary contains an unknown card class feature")
        if len(parts) >= 2 and parts[1] == "card_identity":
            if (self.readout not in _UPGRADE_READOUTS or len(parts) not in {5, 7}
                    or parts[:3] != ["observation", "card_identity", "Card"]):
                raise ValueError("field classifier vocabulary contains an unknown feature")
            identity_field = next(i for i, value in self.categories.items() if value == "CardId")
            if not parts[3].isdigit() or not 0 <= int(parts[3]) < self.category_sizes[identity_field]:
                raise ValueError("field classifier owned card identity outside schema")
            if len(parts) == 5 and parts[4] == "token_count":
                return
            if len(parts) == 7 and parts[4:6] == ["scalar", "CardUpgrades"] and parts[6] in {"count", "sum", "min", "max"}:
                return
            upgrade_field = next(i for i, value in self.categories.items() if value == "CardCanUpgrade")
            if (len(parts) == 7 and parts[4:6] == ["category", "CardCanUpgrade"]
                    and parts[6].isdigit() and 0 <= int(parts[6]) < self.category_sizes[upgrade_field]):
                return
            raise ValueError("field classifier vocabulary contains an unknown feature")
        if len(parts) < 3 or parts[0] not in {"observation", "candidate"}:
            raise ValueError("field classifier vocabulary contains an unknown feature")
        if parts[1] == "token_count" and len(parts) == 3 and parts[2] in self.kinds.values():
            return
        if len(parts) == 5 and parts[2] in self.kinds.values():
            if (parts[1] == "scalar" and parts[3] in self.scalars.values()
                    and parts[4] in {"count", "sum", "min", "max"}):
                return
            if parts[1] == "category" and parts[3] in self.categories.values():
                field = next(i for i, value in self.categories.items() if value == parts[3])
                if parts[4].isdigit() and 0 <= int(parts[4]) < self.category_sizes[field]:
                    return
        raise ValueError("field classifier vocabulary contains an unknown feature")

    def candidate_features(self, batch):
        # Reuse the structural owner before interpreting enum/value domains.
        batch = select_semantic_decision_rows(batch, range(len(batch["slot_indices"])))
        sem = batch["semantic"]
        if sem["schema_version"] != self.schema["version"]:
            raise ValueError("field classifier semantic schema differs")
        kinds, scopes = sem["token"]["kind"], sem["token"]["scope"]
        for values, allowed, name in ((kinds, self.kinds, "token kind"),
                                     (sem["categorical"]["field"], self.categories, "categorical field"),
                                     (sem["scalar"]["field"], self.scalars, "scalar field"),
                                     (sem["relation"]["relation"], self.relations, "relation kind")):
            if not set(map(int, values)) <= set(allowed):
                raise ValueError(f"field classifier unknown {name}")
        for field, value in zip(sem["categorical"]["field"], sem["categorical"]["value"], strict=True):
            if not 0 <= value < self.category_sizes[int(field)]:
                raise ValueError("field classifier categorical value outside schema")
        if not np.isfinite(sem["scalar"]["value"]).all():
            raise ValueError("field classifier scalar values must be finite")
        tokens = sem["token"]["row_splits"]
        candidates = batch["candidate_row_splits"]
        token_rows = np.repeat(np.arange(len(batch["slot_indices"])), np.diff(tokens).astype(np.int64))
        included = ((scopes != 0) | (kinds != next(i for i, name in self.kinds.items() if name == "OfferedCard"))
                    if self.readout in _LOCAL_READOUTS else np.ones(len(kinds), dtype=bool))
        parts = defaultdict(dict)
        groups = defaultdict(list)
        for token, kind in enumerate(kinds):
            if not included[token]:
                continue
            part = parts[int(token_rows[token]), int(scopes[token])]
            key = f"token_count.{self.kinds[int(kind)]}"
            part[key] = part.get(key, 0) + 1
        table = sem["categorical"]
        for token, field, value in zip(table["token_indices"], table["field"], table["value"], strict=True):
            if not included[token]:
                continue
            key = f"category.{self.kinds[int(kinds[token])]}.{self.categories[int(field)]}.{int(value)}"
            part = parts[int(token_rows[token]), int(scopes[token])]
            part[key] = part.get(key, 0) + 1
        table = sem["scalar"]
        for token, field, value in zip(table["token_indices"], table["field"], table["value"], strict=True):
            if not included[token]:
                continue
            groups[int(token_rows[token]), int(scopes[token]), int(kinds[token]), int(field)].append(float(value))
        for (row, owner, kind, field), values in groups.items():
            stem = f"scalar.{self.kinds[kind]}.{self.scalars[field]}"
            # Presence count distinguishes missing from a supplied zero.
            total = math.fsum(sorted(values))
            if not math.isfinite(total):
                raise ValueError("field classifier scalar summary must be finite")
            parts[row, owner].update({stem+".count": len(values), stem+".sum": total,
                                      stem+".min": min(values), stem+".max": max(values)})
        features = []
        keyed = (self._owned_upgrade_features(sem, token_rows)
                 if self.readout in _UPGRADE_READOUTS else {})
        classes = (self._owned_class_features(sem, token_rows)
                   if self.readout == CARD_CLASS_READOUT else {})
        for row in range(len(batch["slot_indices"])):
            observation = {"observation."+key: value for key, value in parts[row, 0].items()}
            observation.update(keyed.get(row, {}))
            observation.update(classes.get(row, {}))
            for index in range(int(candidates[row]), int(candidates[row+1])):
                token = int(sem["candidate_token_indices"][index])
                features.append(observation | {"candidate."+key: value
                                for key, value in parts[row, int(scopes[token])].items()})
        return features, candidates.copy()

    def _owned_upgrade_features(self, sem, token_rows):
        tokens = sem["token"]
        owned = {i for i, (kind, scope) in enumerate(zip(tokens["kind"], tokens["scope"], strict=True))
                 if self.kinds[int(kind)] == "Card" and scope == 0}
        identities = {}
        table = sem["categorical"]
        for token, field, value in zip(table["token_indices"], table["field"], table["value"], strict=True):
            token = int(token)
            if token in owned and self.categories[int(field)] == "CardId":
                if token in identities:
                    raise ValueError("owned card requires a single explicit identity")
                identities[token] = int(value)
        if set(identities) != owned:
            raise ValueError("owned card requires a single explicit identity")
        result, groups = defaultdict(dict), defaultdict(list)
        def stem(token):
            return f"observation.card_identity.Card.{identities[token]}"
        for token in identities:
            part = result[int(token_rows[token])]
            name = stem(token)+".token_count"
            part[name] = part.get(name, 0)+1
        for token, field, value in zip(table["token_indices"], table["field"], table["value"], strict=True):
            token = int(token)
            if token in identities and self.categories[int(field)] == "CardCanUpgrade":
                part = result[int(token_rows[token])]
                name = stem(token)+f".category.CardCanUpgrade.{int(value)}"
                part[name] = part.get(name, 0)+1
        table = sem["scalar"]
        for token, field, value in zip(table["token_indices"], table["field"], table["value"], strict=True):
            token = int(token)
            if token in identities and self.scalars[int(field)] == "CardUpgrades":
                groups[int(token_rows[token]), stem(token)+".scalar.CardUpgrades"].append(float(value))
        for (row, name), values in groups.items():
            total = math.fsum(sorted(values))
            if not math.isfinite(total):
                raise ValueError("owned card upgrade summary must be finite")
            result[row].update({name+".count": len(values), name+".sum": total,
                                name+".min": min(values), name+".max": max(values)})
        return result

    def _owned_class_features(self, sem, token_rows):
        tokens = sem["token"]
        owned = {i for i, (kind, scope) in enumerate(zip(tokens["kind"], tokens["scope"], strict=True))
                 if self.kinds[int(kind)] == "Card" and scope == 0}
        categories = defaultdict(dict)
        table = sem["categorical"]
        for token, field, value in zip(table["token_indices"], table["field"], table["value"], strict=True):
            token, name = int(token), self.categories[int(field)]
            if token in owned and name in {"CardType", "CardRarity"}:
                if name in categories[token]:
                    raise ValueError("owned card class requires unique type and rarity")
                categories[token][name] = int(value)
        if any(set(categories[token]) != {"CardType", "CardRarity"} for token in owned):
            raise ValueError("owned card class requires explicit type and rarity")
        if any(categories[token]["CardType"] not in self.card_types
               or categories[token]["CardRarity"] not in self.card_rarities for token in owned):
            raise ValueError("owned card class enum outside schema")
        stems = {token: f"observation.card_class.Card.{categories[token]['CardType']}."
                 f"{int(categories[token]['CardRarity'] == self.basic_rarity)}" for token in owned}
        result, groups = defaultdict(dict), defaultdict(list)
        for token, stem in stems.items():
            part = result[int(token_rows[token])]
            name = stem+".token_count"
            part[name] = part.get(name, 0)+1
        table = sem["scalar"]
        for token, field, value in zip(table["token_indices"], table["field"], table["value"], strict=True):
            token = int(token)
            if token in owned:
                groups[int(token_rows[token]), stems[token]+".scalar."+self.scalars[int(field)]].append(float(value))
        for (row, stem), values in groups.items():
            total = math.fsum(sorted(values))
            if not math.isfinite(total):
                raise ValueError("owned card class scalar summary must be finite")
            result[row].update({stem+".count": len(values), stem+".sum": total,
                                stem+".min": min(values), stem+".max": max(values)})
        return result

    def transform(self, features):
        if self._columns is None:
            raise ValueError("field classifier requires a frozen vocabulary")
        from scipy.sparse import csr_matrix
        values, columns, pointers = [], [], [0]
        for row in features:
            for name, value in sorted(row.items()):
                column = self._columns.get(name)
                # Frozen columns cannot expand when seeing new public facts.
                if column is not None:
                    values.append(value)
                    columns.append(column)
            pointers.append(len(values))
        return csr_matrix((np.asarray(values, dtype=np.float64),
                           np.asarray(columns, dtype=np.int32),
                           np.asarray(pointers, dtype=np.int32)),
                          shape=(len(features), len(self.feature_names)))

    def evaluation_support(self, training_features, training_splits, evaluation_features,
                           evaluation_splits, *, training_rows):
        """Exact frozen-input overlap, independent of labels, scores and fitting.

        Sparse absent columns and explicit zero have the same numeric value.
        Full means the observation features retained by this readout before
        vocabulary projection, not the native graph. Comparing them prevents
        ignored facts being called previously observed when only the projection
        matches.
        """
        if self._columns is None:
            raise ValueError("field support requires a frozen vocabulary")

        def decisions(features, splits):
            splits = np.asarray(splits)
            if (splits.ndim != 1 or not np.issubdtype(splits.dtype, np.integer)
                    or len(splits) < 1 or splits[0] != 0 or splits[-1] != len(features)
                    or np.any(np.diff(splits) <= 0)):
                raise ValueError("field support requires aligned nonempty decisions")
            result = []
            for start, end in zip(splits[:-1], splits[1:], strict=True):
                group = features[int(start):int(end)]
                full = tuple(sorted((key, value) for key, value in group[0].items()
                                    if key.startswith("observation.")))
                if any(tuple(sorted((key, value) for key, value in row.items()
                                    if key.startswith("observation."))) != full for row in group):
                    raise ValueError("field support requires shared observation facts")
                read = tuple((key, value) for key, value in full if key in self._columns and value != 0)
                # Share one observation signature across its candidates instead
                # of copying hundreds of observation pairs for every option.
                candidates = tuple((read, tuple(sorted((key, value) for key, value in row.items()
                    if not key.startswith("observation.") and key in self._columns and value != 0)))
                    for row in group)
                result.append((full, read, candidates))
            return result

        training = decisions(training_features, training_splits)
        evaluation = decisions(evaluation_features, evaluation_splits)
        indices = tuple(training_rows)
        if (any(type(index) is not int or not 0 <= index < len(training) for index in indices)
                or len(set(indices)) != len(indices)):
            raise ValueError("field support requires distinct supervised training rows")
        selected = [training[index] for index in indices]
        full_known = {full for full, _, _ in selected}
        read_known = {read for _, read, _ in selected}
        candidates_known = {candidate for _, _, candidates in selected for candidate in candidates}
        buckets = defaultdict(list)
        row_cohorts = []
        for full, read, candidates in evaluation:
            cohort = ("new_observation" if read not in read_known else
                      "known_observation_new_candidates" if any(candidate not in candidates_known for candidate in candidates)
                      else "all_candidates_seen")
            row_cohorts.append(cohort)
            buckets[cohort].append((full, read))
        report = {
            "schema": "sts-field-evaluation-support-v1",
            "basis": "Exact frozen numeric input overlap; full means unprojected readout features, not the native graph. "
                     "Not learned competence or independent validation.",
            "supervised_training_rows": len(selected),
            "training_distinct_full_observations": len(full_known),
            "training_distinct_read_observations": len(read_known),
            "evaluation_rows": len(evaluation),
            "evaluation_distinct_full_observations": len({full for full, _, _ in evaluation}),
            "evaluation_distinct_read_observations": len({read for _, read, _ in evaluation}),
            "cohorts": {
                name: {"rows": len(rows), "distinct_read_observations": len({read for _, read in rows}),
                       "full_observation_seen_rows": sum(full in full_known for full, _ in rows)}
                for name, rows in sorted(buckets.items())},
        }
        return report, tuple(row_cohorts)


@dataclass(frozen=True)
class FieldCandidateScores:
    values: np.ndarray
    row_splits: np.ndarray

    def greedy_ordinals(self):
        return tuple(int(self.values[int(start):int(end)].argmax())
                     for start, end in zip(self.row_splits[:-1], self.row_splits[1:], strict=True))


def validate_field_booster(model, encoder, *, objective="binary"):
    dump = model.dump_model()
    if not isinstance(objective, str) or objective not in {"binary", "lambdarank"}:
        raise ValueError("saved field classifier objective differs")
    expected = "binary sigmoid:1" if objective == "binary" else "lambdarank"
    if (dump.get("objective") != expected or dump.get("num_tree_per_iteration") != 1
            or dump.get("feature_names") != [f"f{i}" for i in range(len(encoder.feature_names))]
            or not dump.get("tree_info")):
        raise ValueError("saved field classifier weights are incompatible")
    def finite(value):
        if isinstance(value, dict):
            return all(finite(item) for item in value.values())
        if isinstance(value, list):
            return all(finite(item) for item in value)
        return not isinstance(value, float) or math.isfinite(value)
    if not finite(dump):
        raise ValueError("saved field classifier weights must be finite")


class GreedyStrategicFieldPolicy:
    def __init__(self, model, encoder, behavior_manifest_id, *, objective="binary"):
        if not isinstance(behavior_manifest_id, BehaviorManifestId):
            raise ValueError("field classifier requires typed behavior identity")
        validate_field_booster(model, encoder, objective=objective)
        self.model = model
        self.encoder = encoder
        self.objective = objective
        self.behavior_manifest_id = behavior_manifest_id

    def scores(self, batch):
        features, splits = self.encoder.candidate_features(batch)
        values = np.asarray(self.model.predict(self.encoder.transform(features),
                            raw_score=True, num_threads=2), dtype=np.float64)
        if values.shape != (int(splits[-1]),) or not np.isfinite(values).all():
            raise ValueError("field classifier scores must be finite and aligned")
        return FieldCandidateScores(values, splits)

    def choose(self, batch):
        return BatchPolicyChoice.deterministic(self.scores(batch).greedy_ordinals(), self.behavior_manifest_id)
