"""Schema and canonical keys for revision-bound entity identity reviews."""

from __future__ import annotations

from datetime import date
import hashlib
import json
from pathlib import Path
from typing import Any


IDENTITY_REVIEW_SCHEMA = "sts-wiki-entity-identity-reviews-v1"
IDENTITY_REVIEW_REPORT_SCHEMA = "sts-wiki-entity-identity-review-report-v1"
RECORD_FIELDS = (
    "record_source_id",
    "record_page_id",
    "record_page_title",
    "record_revision_id",
    "record_locator",
)
REVIEW_DECISIONS = {"accept", "reject", "defer"}


def _canonical_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def structured_record(evidence: dict[str, Any]) -> dict[str, Any]:
    """Return the revision-bound record descriptor carried by structured evidence."""

    missing = [field for field in RECORD_FIELDS if field not in evidence]
    identity = evidence.get("record_identity")
    if missing or not isinstance(identity, dict):
        invalid = [*missing]
        if not isinstance(identity, dict):
            invalid.append("record_identity")
        raise ValueError(
            "structured candidate evidence is missing "
            f"{', '.join(invalid)}; rebuild the entity graph"
        )
    return {field: evidence[field] for field in RECORD_FIELDS} | {
        "record_identity": identity,
    }


def record_key(record: dict[str, Any]) -> tuple[Any, ...]:
    """Create one stable key without weakening any record identity field."""

    return (
        record["record_source_id"],
        record["record_page_id"],
        record["record_page_title"],
        record["record_revision_id"],
        _canonical_json(record["record_locator"]),
        _canonical_json(record["record_identity"]),
    )


def _require_nonempty_string(value: Any, location: str) -> str:
    if not isinstance(value, str) or not value.strip():
        raise ValueError(f"{location} must be a non-empty string")
    return value


def _validate_record(value: Any, location: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise ValueError(f"{location} must be an object")
    expected = {*RECORD_FIELDS, "record_identity"}
    unexpected = sorted(set(value) - expected)
    missing = sorted(expected - set(value))
    if missing or unexpected:
        raise ValueError(
            f"{location} fields do not match the schema; "
            f"missing={missing}, unexpected={unexpected}"
        )
    _require_nonempty_string(value["record_source_id"], f"{location}.record_source_id")
    _require_nonempty_string(value["record_page_title"], f"{location}.record_page_title")
    for field in ("record_page_id", "record_revision_id"):
        if not isinstance(value[field], int) or isinstance(value[field], bool) or value[field] <= 0:
            raise ValueError(f"{location}.{field} must be a positive integer")
    for field in ("record_locator", "record_identity"):
        if not isinstance(value[field], dict) or not value[field]:
            raise ValueError(f"{location}.{field} must be a non-empty object")
    return {field: value[field] for field in RECORD_FIELDS} | {
        "record_identity": value["record_identity"],
    }


def load_identity_reviews(path: Path) -> dict[str, Any]:
    """Load and strictly validate a tracked review ledger."""

    raw = path.read_bytes()
    try:
        value = json.loads(raw.decode("utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"entity identity review ledger is invalid JSON: {path}: {error}") from error
    if not isinstance(value, dict):
        raise ValueError("entity identity review ledger root must be an object")
    expected_root = {"schema", "project", "reviews"}
    if set(value) != expected_root:
        raise ValueError(
            "entity identity review ledger fields do not match the schema; "
            f"missing={sorted(expected_root - set(value))}, "
            f"unexpected={sorted(set(value) - expected_root)}"
        )
    if value["schema"] != IDENTITY_REVIEW_SCHEMA:
        raise ValueError(
            f"unsupported entity identity review schema: {value['schema']!r}; "
            f"expected {IDENTITY_REVIEW_SCHEMA!r}"
        )
    project = _require_nonempty_string(value["project"], "review ledger project")
    if not isinstance(value["reviews"], list):
        raise ValueError("review ledger reviews must be an array")

    reviews: list[dict[str, Any]] = []
    seen: set[tuple[Any, ...]] = set()
    expected_review = {
        "decision",
        "entity_key",
        "record",
        "rationale",
        "reviewer",
        "reviewed_on",
    }
    for index, item in enumerate(value["reviews"]):
        location = f"reviews[{index}]"
        if not isinstance(item, dict):
            raise ValueError(f"{location} must be an object")
        if set(item) != expected_review:
            raise ValueError(
                f"{location} fields do not match the schema; "
                f"missing={sorted(expected_review - set(item))}, "
                f"unexpected={sorted(set(item) - expected_review)}"
            )
        decision = item["decision"]
        if decision not in REVIEW_DECISIONS:
            raise ValueError(
                f"{location}.decision must be one of {sorted(REVIEW_DECISIONS)}"
            )
        entity_key = _require_nonempty_string(item["entity_key"], f"{location}.entity_key")
        rationale = _require_nonempty_string(item["rationale"], f"{location}.rationale")
        reviewer = _require_nonempty_string(item["reviewer"], f"{location}.reviewer")
        reviewed_on = _require_nonempty_string(item["reviewed_on"], f"{location}.reviewed_on")
        try:
            date.fromisoformat(reviewed_on)
        except ValueError as error:
            raise ValueError(f"{location}.reviewed_on must be an ISO date") from error
        record = _validate_record(item["record"], f"{location}.record")
        key = (entity_key, *record_key(record))
        if key in seen:
            raise ValueError(f"{location} duplicates an earlier review for the same entity record")
        seen.add(key)
        reviews.append(
            {
                "decision": decision,
                "entity_key": entity_key,
                "record": record,
                "rationale": rationale,
                "reviewer": reviewer,
                "reviewed_on": reviewed_on,
            }
        )

    return {
        "schema": IDENTITY_REVIEW_SCHEMA,
        "project": project,
        "reviews": reviews,
        "path": str(path.resolve()),
        "sha256": hashlib.sha256(raw).hexdigest(),
    }
