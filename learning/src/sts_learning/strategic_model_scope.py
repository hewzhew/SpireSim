"""Explicit usage boundaries, separate from labels and strategic competence.

Domains use the bridge's typed facts. They neither replicate card effects nor
infer learned capability from the presence of a feature in a training sample.
"""
from __future__ import annotations

import hashlib
import json
import math
from collections.abc import Mapping
from dataclasses import dataclass

import numpy as np

from .semantic_batch import select_semantic_decision_rows


_SITES = frozenset(("card_reward", "campfire"))
_DOMAINS = ("CardId", "RelicId", "PotionId", "CardMechanicCoverage",
            "CardMechanicRole", "PotionMechanicRole", "ActionKind", "RunGoal",
            "PlayerClass", "BossEncounterId")
_AXES = ("Act", "Floor", "AscensionLevel")
DECISION_CHOICE_TARGET = "preferred-if-declared-else-acceptable-v1"


@dataclass(frozen=True)
class StrategicModelApplicability:
    """Immutable declaration; passing it means eligible for a diagnostic only."""

    contract_json: str
    schema_json: str

    @classmethod
    def from_contract(cls, value, *, schema, model_sites):
        if (not isinstance(value, Mapping) or set(value) != {"schema", "sites"}
                or value["schema"] != "sts-strategic-applicability-v1"
                or not isinstance(value["sites"], Mapping) or not value["sites"]
                or not set(value["sites"]) <= set(model_sites) <= _SITES):
            raise ValueError("model applicability requires declared trained sites")
        categories, scalars = schema["categorical_field"], schema["scalar_field"]
        if not set(_AXES) <= set(scalars) or "Run" not in schema["token_kind"]:
            raise ValueError("model applicability requires bridge run axes")
        required = {name for name in _DOMAINS if name in categories}
        for site, scope in value["sites"].items():
            keys = {"acts", "floor_range", "ascension_range", "categorical_domains"}
            if (not isinstance(scope, Mapping) or
                    not keys <= set(scope) <= keys | {"observation_categorical_domains"}):
                raise ValueError(f"model applicability has malformed {site} scope")
            _integers(scope["acts"], 5, "acts", nonempty=True)
            _range(scope["floor_range"], "floor_range")
            _range(scope["ascension_range"], "ascension_range", maximum=20)
            domains = scope["categorical_domains"]
            if (not isinstance(domains, Mapping) or not required <= set(domains)
                    or not set(domains) <= set(categories)):
                raise ValueError("model applicability must declare typed mechanic and identity domains")
            for name, values in domains.items():
                field = categories[name]
                size = schema["categorical_vocabulary_size"].get(
                    field, schema["categorical_vocabulary_size"].get(str(field)))
                _integers(values, size, name)
            observation = scope.get("observation_categorical_domains", {})
            if (not isinstance(observation, Mapping)
                    or not set(observation) <= set(schema["token_kind"])):
                raise ValueError("model applicability has unknown observation token domains")
            for kind, fields in observation.items():
                if (not isinstance(fields, Mapping) or not fields
                        or not set(fields) <= set(categories)):
                    raise ValueError("model applicability has malformed observation categorical domains")
                for name, values in fields.items():
                    field = categories[name]
                    size = schema["categorical_vocabulary_size"].get(
                        field, schema["categorical_vocabulary_size"].get(str(field)))
                    _integers(values, size, f"{kind}.{name}")
        # JSON copies also prevent a caller's mutable declaration changing use.
        return cls(json.dumps(value, sort_keys=True, allow_nan=False),
                   json.dumps(schema, sort_keys=True, allow_nan=False))

    def contract(self):
        return json.loads(self.contract_json)

    @property
    def schema_sha256(self):
        schema = json.loads(self.schema_json)
        # These native tables have numeric field keys; JSON stringifies them.
        # Restore their type before comparing the executor's canonical digest.
        for key in ("categorical_vocabulary_size", "identity_residual_categorical_fields"):
            schema[key] = {int(field): number for field, number in schema[key].items()}
        return hashlib.sha256(json.dumps(schema, sort_keys=True, separators=(",", ":"),
                                          allow_nan=False).encode()).hexdigest()

    def issues(self, site, batch):
        """One issue list per row, over the complete public candidate surface."""
        value, schema = self.contract(), json.loads(self.schema_json)
        if site not in value["sites"]:
            raise ValueError("model applicability does not declare the requested site")
        # Reuse the real row validator, including scope ownership checks.
        batch = select_semantic_decision_rows(batch, range(len(batch["slot_indices"])))
        semantic = batch["semantic"]
        if semantic["schema_version"] != schema["version"]:
            raise ValueError("model applicability semantic schema differs")
        scope = value["sites"][site]
        token = semantic["token"]
        splits, kinds, owners = token["row_splits"], token["kind"], token["scope"]
        categorical, scalar = semantic["categorical"], semantic["scalar"]
        result = [[] for _ in batch["slot_indices"]]
        categories = schema["categorical_field"]
        # Candidate and generated-card aliases share the core's typed domain.
        # Payload placement does not bypass declared content coverage.
        domain_fields = {}
        for name, allowed in scope["categorical_domains"].items():
            fields = [categories[name]]
            if name in ("CardId", "RelicId", "PotionId") and "Action"+name in categories:
                fields.append(categories["Action"+name])
            if name == "CardId" and "CardStatusCreationCardId" in categories:
                fields.append(categories["CardStatusCreationCardId"])
            for field in fields:
                permitted = set(allowed)
                if field in domain_fields:
                    permitted.intersection_update(domain_fields[field][1])
                domain_fields[field] = (name, permitted)
        axes = {schema["scalar_field"][name]: name for name in _AXES}
        observation_fields = {
            (schema["token_kind"][kind], categories[name]): (kind, name, set(allowed))
            for kind, fields in scope.get("observation_categorical_domains", {}).items()
            for name, allowed in fields.items()
        }
        seen_axes = [dict() for _ in result]
        observed_domains = [dict() for _ in result]
        outside_observation = [dict() for _ in result]
        for index, field, number in zip(categorical["token_indices"], categorical["field"],
                                        categorical["value"], strict=True):
            row = int(np.searchsorted(splits, index, side="right") - 1)
            if int(field) in domain_fields:
                name, allowed = domain_fields[int(field)]
                if int(number) not in allowed:
                    observed_domains[row].setdefault(name, set()).add(int(number))
            restricted = observation_fields.get((int(kinds[index]), int(field)))
            if owners[index] == 0 and restricted is not None:
                kind, name, allowed = restricted
                if int(number) not in allowed:
                    outside_observation[row].setdefault((kind, name), set()).add(int(number))
        for index, field, number in zip(scalar["token_indices"], scalar["field"],
                                        scalar["value"], strict=True):
            if int(field) in axes and kinds[index] == schema["token_kind"]["Run"] and owners[index] == 0:
                row = int(np.searchsorted(splits, index, side="right") - 1)
                seen_axes[row].setdefault(axes[int(field)], []).append(float(number))
        for row, issues in enumerate(result):
            if int(semantic["completeness"][row]) != schema["completeness"]["Complete"]:
                issues.append({"code": "incomplete_semantic_observation"})
            for name, numbers in seen_axes[row].items():
                if (len(numbers) != 1 or not math.isfinite(numbers[0])
                        or not numbers[0].is_integer()):
                    issues.append({"code": "unobservable_run_axis", "field": name})
                    continue
                number = int(numbers[0])
                allowed = scope["acts"] if name == "Act" else scope[
                    "floor_range" if name == "Floor" else "ascension_range"]
                accepted = number in allowed if name == "Act" else allowed[0] <= number <= allowed[1]
                if not accepted:
                    issues.append({"code": "run_axis_outside_scope", "field": name, "observed": number})
            for name in _AXES:
                if name not in seen_axes[row]:
                    issues.append({"code": "unobservable_run_axis", "field": name})
            for name, numbers in sorted(observed_domains[row].items()):
                issues.append({"code": "categorical_domain_outside_scope", "field": name,
                               "observed": sorted(numbers)})
            for (kind, name), numbers in sorted(outside_observation[row].items()):
                issues.append({"code": "observation_categorical_domain_outside_scope", "token_kind": kind,
                               "field": name, "observed": sorted(numbers)})
        return tuple(result)


def _integers(value, bound, name, *, nonempty=False):
    if (not isinstance(value, (list, tuple)) or nonempty and not value
            or not isinstance(bound, int) or
            any(type(item) is not int or not 0 <= item < bound for item in value)
            or len(set(value)) != len(value)):
        raise ValueError(f"model applicability {name} requires distinct typed domain values")


def _range(value, name, *, maximum=None):
    if (not isinstance(value, (list, tuple)) or len(value) != 2
            or any(type(item) is not int or item < 0 for item in value)
            or value[0] > value[1] or maximum is not None and value[1] > maximum):
        raise ValueError(f"model applicability {name} requires an ordered integer range")


def require_training_fit(contract, summary):
    """Deployment cannot silently adopt weights from an exhausted fit budget."""
    metrics = summary.get("final_training")
    training = contract.get("training")
    margin = training.get("fit_margin") if isinstance(training, dict) else None
    choices = metrics.get("choices") if isinstance(metrics, dict) else None
    if (summary.get("stop_reason") != "training_fit" or not isinstance(metrics, dict)
            or type(metrics.get("supervised_rows")) is not int or metrics["supervised_rows"] <= 0
            or not isinstance(choices, dict)
            or any(type(choices.get(key)) is not int or choices[key] < 0
                   for key in ("adoptable", "outside_set", "unknown"))
            or choices["outside_set"] != 0 or choices["adoptable"] != metrics["supervised_rows"]
            or isinstance(margin, bool) or not isinstance(margin, (int, float))
            or not math.isfinite(margin) or margin < 0):
        raise ValueError("saved model has not passed complete-decision training fit")
    gap = metrics.get("minimum_margin")
    if gap is not None and (isinstance(gap, bool) or not isinstance(gap, (int, float))
                            or not math.isfinite(gap) or gap < margin):
        raise ValueError("saved model has not passed complete-decision training fit")
    target = contract.get("choice_target")
    if target is None:
        if ("choice_target" in contract or "preferred_training_rows" in contract
                or "preferred_rows" in metrics or "recommendations" in metrics
                or "minimum_recommendation_margin" in metrics):
            raise ValueError("saved model choice-target training fit contract is missing")
        return  # Legacy complete-acceptable-set contract, unchanged.
    preferred = contract.get("preferred_training_rows")
    recommendations = metrics.get("recommendations")
    if (target != DECISION_CHOICE_TARGET
            or type(preferred) is not int or not 0 <= preferred <= metrics["supervised_rows"]
            or type(metrics.get("preferred_rows")) is not int or metrics["preferred_rows"] != preferred
            or not isinstance(recommendations, dict)
            or any(type(recommendations.get(key)) is not int or recommendations[key] < 0
                   for key in ("recommended", "secondary_acceptable", "outside_set", "unknown"))
            or recommendations["recommended"] != metrics["supervised_rows"]
            or recommendations["secondary_acceptable"] != 0
            or recommendations["outside_set"] != choices["outside_set"]
            or recommendations["unknown"] != choices["unknown"]
            or "minimum_recommendation_margin" not in metrics):
        raise ValueError("saved model has not passed recommended-choice training fit")
    gap = metrics["minimum_recommendation_margin"]
    if gap is not None and (isinstance(gap, bool) or not isinstance(gap, (int, float))
                            or not math.isfinite(gap) or gap < margin):
        raise ValueError("saved model has not passed recommended-choice training fit")
