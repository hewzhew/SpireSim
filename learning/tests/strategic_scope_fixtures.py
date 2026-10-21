"""Broad declarations for interface tests, never strategic quality evidence."""
from sts_learning.strategic_model_scope import StrategicModelApplicability


def interface_scope_contract(schema, sites, *, acts=(0, 1, 2, 3, 4)):
    fields = ("CardId", "RelicId", "PotionId", "CardMechanicCoverage",
              "CardMechanicRole", "PotionMechanicRole", "ActionKind", "RunGoal",
              "PlayerClass", "BossEncounterId")
    domains = {}
    for name in fields:
        if name in schema["categorical_field"]:
            field = schema["categorical_field"][name]
            size = schema["categorical_vocabulary_size"].get(
                field, schema["categorical_vocabulary_size"].get(str(field)))
            domains[name] = list(range(size))
    return {"schema": "sts-strategic-applicability-v1", "sites": {
        getattr(site, "value", site): {"acts": list(acts), "floor_range": [0, 100],
            "ascension_range": [0, 20], "categorical_domains": domains} for site in sites}}


def interface_scopes(schema, sites):
    scope = StrategicModelApplicability.from_contract(interface_scope_contract(schema, sites),
        schema=schema, model_sites=[site.value for site in sites])
    return {site: scope for site in sites}


def add_fixture_run_axes(schema):
    schema["token_kind"]["Run"] = max(schema["token_kind"].values()) + 1
    schema["completeness"] = {"Complete": 1}
    for name in ("Act", "Floor", "AscensionLevel"):
        schema["scalar_field"][name] = max(schema["scalar_field"].values()) + 1
    return schema


def fitted_summary(payload_sha):
    return {"model_sha256": payload_sha, "validation": {"model_round_trip": True},
            "stop_reason": "training_fit", "final_training": {
                "supervised_rows": 1, "choices": {"adoptable": 1, "outside_set": 0, "unknown": 0},
                "minimum_margin": 0.5}}
