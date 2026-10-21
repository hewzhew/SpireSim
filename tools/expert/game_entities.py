"""Complete simulator identities with optional accepted Wiki-title aliases."""

from __future__ import annotations

from dataclasses import dataclass
from pathlib import Path
import re
from typing import Any

from tools.wiki.entity_query import simulator_entity_identity_aliases
from tools.wiki.simulator_catalog import (
    RELIC_IDENTITY_CONTRACT_SOURCE,
    build_simulator_catalog,
    simulator_entity_identity_manifest,
)


@dataclass(frozen=True)
class GameEntityCatalog:
    entities: tuple[dict[str, Any], ...]
    status: str
    tracked_paths: tuple[Path, ...]
    alias_count: int


def _humanize(stable_id: str) -> str:
    value = re.sub(r"(?<=[a-z0-9])(?=[A-Z])", " ", stable_id)
    return value.replace("_", " ")


def load_game_entity_catalog(repository: Path) -> GameEntityCatalog:
    repository = repository.resolve(strict=True)
    marker = repository / RELIC_IDENTITY_CONTRACT_SOURCE
    if not marker.is_file():
        return GameEntityCatalog((), "reference_only", (), 0)

    simulator_entities, source_hashes = build_simulator_catalog(repository)
    alias_database = (
        repository / ".oracle-lab" / "external-research" / "entities.sqlite3"
    )
    identity_aliases: dict[str, tuple[str, ...]] = {}
    tracked_paths = [repository / relative for relative in source_hashes]
    status = "simulator_complete"
    if alias_database.is_file():
        identity_aliases = simulator_entity_identity_aliases(
            database=alias_database,
            expected_entity_identities=simulator_entity_identity_manifest(
                simulator_entities
            ),
        )
        tracked_paths.append(alias_database)
        status = "simulator_complete_with_wiki_aliases"

    rows = []
    for entity in simulator_entities:
        aliases = list(
            dict.fromkeys(
                value
                for value in (
                    entity.stable_id,
                    _humanize(entity.stable_id),
                    entity.display_name,
                    entity.external_id,
                    *entity.external_aliases,
                    *identity_aliases.get(entity.entity_key, ()),
                )
                if isinstance(value, str) and value
            )
        )
        rows.append(
            {
                "schema": "reference-entity-v1",
                "id": entity.entity_key,
                "kind": entity.kind,
                "canonical_name": _humanize(entity.stable_id),
                "aliases": aliases,
                "related": [],
                "identity_source": "simulator",
            }
        )
    return GameEntityCatalog(
        entities=tuple(rows),
        status=status,
        tracked_paths=tuple(tracked_paths),
        alias_count=sum(len(row["aliases"]) for row in rows),
    )
