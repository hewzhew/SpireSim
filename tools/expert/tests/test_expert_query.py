from __future__ import annotations

import hashlib
import json
from pathlib import Path
import tempfile
import unittest

from tools.expert.query import ExpertKnowledgeOwner


def _write(path: Path, text: str) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(text, encoding="utf-8")


def _jsonl(path: Path, rows: list[dict]) -> None:
    _write(
        path,
        "".join(json.dumps(row, ensure_ascii=False) + "\n" for row in rows),
    )


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _fixture(repository: Path) -> None:
    expert = repository / "reference" / "expert"
    source = (
        repository
        / ".oracle-lab"
        / "external-research"
        / "sources"
        / "fixture-guide"
    )
    text_path = source / "corpus" / "text.md"
    records_path = source / "corpus" / "records.json"
    asset_path = source / "assets" / "floor-1-tier-list.png"
    _write(
        text_path,
        "# Guide\nBrutality is useful only with a real draw need.\n"
        "Body Slam needs sufficient block quality.\n",
    )
    _write(
        records_path,
        json.dumps(
            {
                "posts": [
                    {
                        "id": "post-1",
                        "floor": "floor 3",
                        "text": "Brutality needs current draw support and costs HP.",
                    },
                    {
                        "id": "post-2",
                        "floor": "floor 4",
                        "text": "Body Slam needs sufficient block quality.",
                    }
                ]
            }
        ),
    )
    asset_path.parent.mkdir(parents=True, exist_ok=True)
    asset_path.write_bytes(b"fixture-tier-list-image")
    manifest = {
        "schema": "external-research-source-v1",
        "id": "fixture-guide",
        "provider": "fixture",
        "status": "complete",
        "source": {"title": "Fixture Guide"},
        "canonical_corpus": {
            "text": {"path": "corpus/text.md", "sha256": _sha256(text_path)},
            "records": {
                "path": "corpus/records.json",
                "sha256": _sha256(records_path),
            },
        },
        "assets": [
            {
                "path": "assets/floor-1-tier-list.png",
                "sha256": _sha256(asset_path),
                "media_type": "image/png",
                "role": "inline_source_asset",
            }
        ],
    }
    _write(source / "source.json", json.dumps(manifest))
    _write(
        repository
        / ".oracle-lab"
        / "external-research"
        / "sources"
        / "fixture-wiki"
        / "source.json",
        json.dumps(
            {"schema": "external-research-mediawiki-source-v1", "id": "fixture-wiki"}
        ),
    )
    _jsonl(
        expert / "sources.jsonl",
        [
            {
                "id": "fixture-guide",
                "status": "complete",
                "kind": "community_expert_guide",
                "language": "en",
                "author": "Fixture Author",
                "title": "Fixture Guide",
                "published_at": "2025-01-01",
                "scope_note": "A20 with save/load allowed",
                "credibility": "expert testimony, not a policy label",
                "raw_cache": ".oracle-lab/external-research/sources/fixture-guide/",
                "corpus_text_sha256": manifest["canonical_corpus"]["text"]["sha256"],
                "records_sha256": manifest["canonical_corpus"]["records"]["sha256"],
                "reviewed_indices": [
                    {
                        "path": "reference/expert/indices/fixture-cards.jsonl",
                        "expected_rows": 1,
                    }
                ],
                "reviewed_outlines": [
                    {
                        "path": "reference/expert/outlines/fixture-guide.jsonl",
                        "expected_rows": 1,
                    }
                ],
                "media_annotations": [
                    {
                        "path": "reference/expert/media/fixture-guide.jsonl",
                        "expected_rows": 1,
                    }
                ],
            }
        ],
    )
    _jsonl(
        expert / "topics.jsonl",
        [
            {
                "schema": "expert-topic-v1",
                "id": "draw-access",
                "canonical_name": "draw and access",
                "aliases": ["过牌", "draw need"],
                "tags": ["draw", "access"],
                "description": "fixture draw topic",
            }
        ],
    )
    entities = [
        {
            "schema": "reference-entity-v1",
            "id": "card:Brutality",
            "kind": "card",
            "canonical_name": "Brutality",
            "aliases": ["残暴"],
            "related": [{"entity_id": "mechanic:draw", "relation": "provides"}],
        },
        {
            "schema": "reference-entity-v1",
            "id": "mechanic:draw",
            "kind": "mechanic",
            "canonical_name": "draw",
            "aliases": ["过牌"],
            "related": [],
        },
        {
            "schema": "reference-entity-v1",
            "id": "card:Panacea",
            "kind": "card",
            "canonical_name": "Panacea",
            "aliases": ["万能药"],
            "related": [],
        },
    ]
    _jsonl(repository / "reference" / "entities.jsonl", entities)
    _jsonl(
        expert / "passages.jsonl",
        [
            {
                "schema": "expert-passage-v1",
                "id": "fixture-brutality",
                "source_id": "fixture-guide",
                "record": {"id": "post-1", "locator": "floor 3"},
                "entity_ids": ["card:Brutality"],
                "anchors": ["costs HP"],
            }
        ],
    )
    _jsonl(
        expert / "indices" / "fixture-cards.jsonl",
        [
            {
                "schema": "expert-source-entity-record-v1",
                "id": "fixture-body-slam",
                "source_id": "fixture-guide",
                "record": {"id": "post-2", "locator": "floor 4"},
                "entity": {
                    "schema": "reference-entity-v1",
                    "id": "card:BodySlam",
                    "kind": "card",
                    "canonical_name": "Body Slam",
                    "aliases": ["BodySlam", "全身撞击", "肚皮"],
                    "related": [],
                },
                "anchors": ["Body Slam", "block quality"],
                "tags": ["ironclad", "card-review"],
                "review_state": "located",
            }
        ],
    )
    _jsonl(
        expert / "outlines" / "fixture-guide.jsonl",
        [
            {
                "schema": "expert-source-outline-record-v1",
                "id": "fixture-guide-outline-post-1",
                "source_id": "fixture-guide",
                "record": {"id": "post-1", "locator": "floor 3"},
                "kind": "card-context",
                "title": "Brutality draw requirement",
                "summary": "Brutality depends on a current draw need.",
                "anchors": ["draw support"],
                "topic_ids": ["draw-access"],
                "entity_ids": ["card:Brutality"],
                "phases": ["whole-run"],
                "decisions": ["card reward"],
            }
        ],
    )
    _jsonl(
        expert / "media" / "fixture-guide.jsonl",
        [
            {
                "schema": "expert-source-media-annotation-v1",
                "id": "fixture-guide-floor1-tier-body-slam",
                "source_id": "fixture-guide",
                "asset": {
                    "path": "assets/floor-1-tier-list.png",
                    "sha256": _sha256(asset_path),
                },
                "annotation_kind": "tier_membership",
                "entity_ids": ["card:BodySlam"],
                "observation": {
                    "source_label": "Body Slam",
                    "tier_label": "Good",
                },
                "selector": {"type": "tier_row_label", "value": "Good"},
                "transcription": {
                    "method": "model_visual_inspection",
                    "created_by": "Fixture Codex",
                    "created_at": "2026-08-25",
                    "review_state": "model_transcribed_unverified",
                },
                "scope": {
                    "coverage": "partial_targeted",
                    "image_complete": False,
                },
            }
        ],
    )
    _jsonl(
        expert / "claims.jsonl",
        [
            {
                "id": "fixture-claim",
                "source_ids": ["fixture-guide"],
                "entity_ids": ["card:Brutality"],
                "statement": "Brutality is conditional draw.",
                "caveats": ["not a fixed score"],
                "tags": ["draw"],
            }
        ],
    )
    _write(
        expert / "notes" / "fixture-guide-index.md",
        "Brutality appears in the draw section.\n",
    )
    _write(
        expert / "notes" / "research-backlog.md",
        "fixture-guide should later cross-check Brutality.\n",
    )


class ExpertKnowledgeOwnerTests(unittest.TestCase):
    def test_literal_search_uses_catalog_admission_and_bounded_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            owner = ExpertKnowledgeOwner(repository)

            self.assertEqual(list(owner.sources), ["fixture-guide"])
            result = owner.literal_search("Brutality", max_results=2)
            self.assertGreater(result["total_match_count"], 2)
            self.assertEqual(result["returned_match_count"], 2)
            self.assertTrue(result["truncated"])
            self.assertTrue(
                all(
                    match["evidence_kind"]
                    != "external-research-mediawiki-source-v1"
                    for match in result["matches"]
                )
            )
            self.assertTrue(all("path" in match for match in result["matches"]))
            expanded = owner.literal_search_any(
                ["Brutality", "conditional"], max_results=20
            )
            self.assertEqual(expanded["queries"], ["Brutality", "conditional"])
            self.assertGreaterEqual(
                expanded["total_match_count"], result["total_match_count"]
            )
            scoped = owner.literal_search(
                "Brutality", source_id="fixture-guide", max_results=20
            )
            self.assertTrue(
                all(
                    match["source_id"] in {None, "fixture-guide"}
                    for match in scoped["matches"]
                )
            )
            self.assertFalse(
                any(
                    match["path"].endswith("research-backlog.md")
                    for match in scoped["matches"]
                )
            )

    def test_entity_evidence_and_verification_are_structured(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            owner = ExpertKnowledgeOwner(repository)

            evidence = owner.entity_evidence("残暴")
            self.assertEqual(evidence["entity"]["id"], "card:Brutality")
            self.assertEqual(len(evidence["direct_reviewed_passages"]), 1)
            self.assertEqual(
                evidence["direct_reviewed_passages"][0]["evidence_kind"],
                "reviewed_expert_passage",
            )
            self.assertEqual(
                evidence["direct_reviewed_passages"][0]["path"],
                "reference/expert/passages.jsonl",
            )
            self.assertEqual(
                evidence["direct_reviewed_passages"][0]["line_number"], 1
            )
            self.assertIn(
                "costs HP", evidence["direct_reviewed_passages"][0]["excerpt"]
            )
            self.assertEqual(
                evidence["source_contexts"],
                [
                    {
                        "source_id": "fixture-guide",
                        "title": "Fixture Guide",
                        "author": "Fixture Author",
                        "published_at": "2025-01-01",
                        "language": "en",
                        "kind": "community_expert_guide",
                        "status": "complete",
                        "scope_note": "A20 with save/load allowed",
                        "credibility": "expert testimony, not a policy label",
                    }
                ],
            )
            self.assertEqual(len(evidence["direct_reviewed_claims"]), 1)
            self.assertIn("mechanic:draw", evidence["adjacent_entity_ids"])

    def test_loaded_owner_refreshes_after_note_change(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            owner = ExpertKnowledgeOwner(repository)
            note = repository / "reference" / "expert" / "notes" / "new-note.md"
            note.write_text("new knowledge", encoding="utf-8")

            result = owner.literal_search("new knowledge")
            self.assertTrue(any("new knowledge" in row["text"] for row in result["matches"]))

    def test_corpus_edit_is_read_without_updating_old_digest_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            owner = ExpertKnowledgeOwner(repository)
            text_path = (
                repository / ".oracle-lab/external-research/sources/fixture-guide/corpus/text.md"
            )
            text_path.write_text(
                text_path.read_text(encoding="utf-8") + "\nnewly revised source line\n",
                encoding="utf-8",
            )
            result = owner.literal_search("newly revised source line")
            self.assertTrue(any("newly revised source line" in row["text"] for row in result["matches"]))

    def test_missing_source_image_does_not_block_text_evidence(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            owner = ExpertKnowledgeOwner(repository)
            asset_path = (
                repository
                / ".oracle-lab/external-research/sources/fixture-guide/assets/floor-1-tier-list.png"
            )
            asset_path.unlink()
            evidence = owner.entity_evidence("肚皮")
            self.assertEqual(len(evidence["direct_reviewed_passages"]), 1)
            self.assertEqual(evidence["direct_source_media_transcriptions"], [])

    def test_structured_source_coverage_remains_available(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            coverage = ExpertKnowledgeOwner(repository).coverage()
            source = coverage["sources"][0]
            self.assertEqual(source["coverage_status"], "complete_structured")
            self.assertEqual(source["covered_record_count"], 2)
            cards = next(
                item for item in coverage["entity_evidence_coverage"]
                if item["kind"] == "card"
            )
            self.assertEqual(cards["entity_count"], 3)
            self.assertEqual(cards["direct_evidence_entity_count"], 2)

    def test_outline_with_unknown_entity_fails_during_owner_load(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            path = (
                repository
                / "reference"
                / "expert"
                / "outlines"
                / "fixture-guide.jsonl"
            )
            rows = [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines()]
            rows[0]["entity_ids"] = ["card:Missing"]
            _jsonl(path, rows)

            with self.assertRaisesRegex(ValueError, "references unknown entities"):
                ExpertKnowledgeOwner(repository)

    def test_reviewed_index_supplies_entity_and_attributed_passage(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            owner = ExpertKnowledgeOwner(repository)

            evidence = owner.entity_evidence("肚皮")

            self.assertEqual(evidence["entity"]["id"], "card:BodySlam")
            self.assertEqual(len(evidence["direct_reviewed_passages"]), 1)
            passage = evidence["direct_reviewed_passages"][0]
            self.assertEqual(passage["passage_id"], "fixture-body-slam")
            self.assertEqual(
                passage["path"], "reference/expert/indices/fixture-cards.jsonl"
            )
            self.assertEqual(passage["line_number"], 1)
            self.assertIn("block quality", passage["excerpt"])

    def test_media_transcription_is_direct_but_not_rendered_as_authored_text(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            owner = ExpertKnowledgeOwner(repository)

            media = owner.entity_evidence("Body Slam")[
                "direct_source_media_transcriptions"
            ]

            self.assertEqual(len(media), 1)
            self.assertEqual(media[0]["observation"]["tier_label"], "Good")
            self.assertEqual(
                media[0]["transcription"]["review_state"],
                "model_transcribed_unverified",
            )
            self.assertEqual(media[0]["scope"]["coverage"], "partial_targeted")
            self.assertFalse(media[0]["scope"]["image_complete"])
            self.assertTrue(media[0]["asset"]["path"].endswith("floor-1-tier-list.png"))
            self.assertEqual(media[0]["id"], "fixture-guide-floor1-tier-body-slam")

    def test_reviewed_index_can_change_without_updating_a_count_pin(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            catalog_path = repository / "reference" / "expert" / "sources.jsonl"
            catalog = json.loads(catalog_path.read_text(encoding="utf-8"))
            catalog["reviewed_indices"][0]["expected_rows"] = 2
            _jsonl(catalog_path, [catalog])

            owner = ExpertKnowledgeOwner(repository)
            self.assertEqual(owner.entity_evidence("肚皮")["entity"]["id"], "card:BodySlam")

    def test_cataloged_source_with_foreign_schema_fails_closed(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture(repository)
            manifest_path = (
                repository
                / ".oracle-lab"
                / "external-research"
                / "sources"
                / "fixture-guide"
                / "source.json"
            )
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            manifest["schema"] = "external-research-mediawiki-source-v1"
            manifest_path.write_text(json.dumps(manifest), encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "unexpected schema"):
                ExpertKnowledgeOwner(repository)


if __name__ == "__main__":
    unittest.main()
