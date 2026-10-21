from __future__ import annotations

import gzip
import json
from pathlib import Path
import tempfile
import unittest

from tools.wiki.mediawiki_records import (
    BUNDLE_SCHEMA,
    BundleError,
    normalize_page,
    page_ids_sha256,
    plain_text_from_wikitext,
    extract_section_outline,
    verify_bundle,
)


class MediaWikiRecordsTests(unittest.TestCase):
    def write_bundle(self, records: list[dict], *, gzip_output: bool = False) -> Path:
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        path = Path(temporary.name) / ("bundle.jsonl.gz" if gzip_output else "bundle.jsonl")
        opener = gzip.open if gzip_output else path.open
        if gzip_output:
            stream = opener(path, "wt", encoding="utf-8", newline="\n")
        else:
            stream = opener("w", encoding="utf-8", newline="\n")
        with stream:
            for record in records:
                stream.write(json.dumps(record, ensure_ascii=False) + "\n")
        return path

    def complete_records(self) -> list[dict]:
        page_ids = [7, 11]
        return [
            {
                "schema": BUNDLE_SCHEMA,
                "kind": "header",
                "source_id": "fixture",
                "api_url": "https://example.invalid/api.php",
                "retrieved_at": "2026-08-20T00:00:00Z",
                "namespace_ids": [0, 10],
                "siteinfo": {"namespaces": {"0": {"id": 0}, "10": {"id": 10}}},
            },
            {"kind": "page", "page": {"pageid": 7, "ns": 0, "title": "Alpha"}},
            {"kind": "page", "page": {"pageid": 11, "ns": 10, "title": "Beta"}},
            {
                "schema": BUNDLE_SCHEMA,
                "kind": "complete",
                "source_id": "fixture",
                "page_count": 2,
                "namespace_counts": {"0": 1, "10": 1},
                "page_ids_sha256": page_ids_sha256(page_ids),
                "completed_at": "2026-08-20T00:01:00Z",
            },
        ]

    def test_complete_bundle_is_verified_streamingly(self) -> None:
        bundle = verify_bundle(self.write_bundle(self.complete_records(), gzip_output=True))
        self.assertEqual(bundle.page_count, 2)
        self.assertEqual(bundle.namespace_counts, {"0": 1, "10": 1})

    def test_missing_trailer_stays_incomplete(self) -> None:
        records = self.complete_records()[:-1]
        with self.assertRaisesRegex(BundleError, "no complete trailer"):
            verify_bundle(self.write_bundle(records))

    def test_acquisition_issue_pages_are_counted_and_checked(self) -> None:
        records = self.complete_records()
        records[1]["page"]["acquisition_warnings"] = [
            {"stage": "page_info", "code": "fixture", "message": "fallback"}
        ]
        records[-1]["acquisition_warning_pages"] = 1
        records[-1]["acquisition_error_pages"] = 0
        verified = verify_bundle(self.write_bundle(records))
        self.assertEqual(verified.acquisition_warning_pages, 1)
        self.assertEqual(verified.acquisition_error_pages, 0)

        records[-1]["acquisition_warning_pages"] = 0
        with self.assertRaisesRegex(BundleError, "acquisition_warning_pages"):
            verify_bundle(self.write_bundle(records))

    def test_duplicate_page_id_is_rejected(self) -> None:
        records = self.complete_records()
        records.insert(3, {"kind": "page", "page": {"pageid": 7, "ns": 0, "title": "Other"}})
        with self.assertRaisesRegex(BundleError, "duplicate pageid"):
            verify_bundle(self.write_bundle(records))

    def test_mediawiki_titles_are_not_globally_case_insensitive(self) -> None:
        records = self.complete_records()
        records[1]["page"]["title"] = "Colorless Cards"
        records[2]["page"].update({"ns": 0, "title": "Colorless cards"})
        records[-1]["namespace_counts"] = {"0": 2}
        verified = verify_bundle(self.write_bundle(records))
        self.assertEqual(verified.page_count, 2)

    def test_exact_duplicate_namespace_title_is_rejected(self) -> None:
        records = self.complete_records()
        records[1]["page"]["title"] = "Same"
        records[2]["page"].update({"ns": 0, "title": "Same"})
        records[-1]["namespace_counts"] = {"0": 2}
        with self.assertRaisesRegex(BundleError, "duplicate namespace/title"):
            verify_bundle(self.write_bundle(records))

    def test_normalization_preserves_provenance_and_relations(self) -> None:
        page = {
            "pageid": 42,
            "ns": 0,
            "title": "残暴",
            "fullurl": "https://example.invalid/wiki/残暴",
            "acquisition_warnings": [
                {
                    "stage": "page_info",
                    "code": "fixture_warning",
                    "message": "fixture metadata fallback",
                }
            ],
            "revisions": [
                {
                    "revid": 99,
                    "timestamp": "2026-08-20T00:00:00Z",
                    "sha1": "abc",
                    "slots": {
                        "main": {
                            "contentmodel": "wikitext",
                            "content": "{{卡牌|英文=Brutality}}\n[[分类:战士卡牌]]\n参见[[撕裂|撕裂牌]]。",
                        }
                    },
                }
            ],
        }
        normalized = normalize_page(
            page,
            source_id="huiji",
            siteinfo={
                "namespaces": {
                    "10": {"id": 10, "name": "模板", "canonical": "Template"},
                    "14": {"id": 14, "name": "分类", "canonical": "Category"},
                }
            },
        )
        self.assertEqual(normalized["revision_id"], 99)
        self.assertEqual(normalized["categories"], ["战士卡牌"])
        self.assertIn("卡牌", normalized["templates"])
        self.assertIn("撕裂", normalized["internal_links"])
        self.assertIn("参见撕裂牌", normalized["plain_text"])
        self.assertEqual(normalized["acquisition_warnings"][0]["code"], "fixture_warning")
        self.assertEqual(normalized["acquisition_errors"], [])

    def test_display_link_templates_keep_visible_entity_names(self) -> None:
        projected = plain_text_from_wikitext(
            "{{Card Infobox|Brutality}}{{C|Brutality}} triggers "
            "{{C|Rupture}} and {{R|Runic Cube}}; "
            "{{QueryLink|Cards|color:Red|Ironclad}}."
        )
        self.assertEqual(
            projected,
            "Brutality triggers Rupture and Runic Cube; Ironclad.",
        )

    def test_huiji_link_templates_keep_strategy_subjects(self) -> None:
        projected = plain_text_from_wikitext(
            "由于{{Power_link|人工制品}}的存在，单单依靠"
            "{{Card_link|萎靡}}、{{Card_link|缴械}}并不理想；"
            "可用{{Relic_link|红面具}}破除。"
        )
        self.assertEqual(
            projected,
            "由于人工制品的存在，单单依靠萎靡、缴械并不理想；可用红面具破除。",
        )

    def test_section_outline_preserves_explicit_heading_hierarchy(self) -> None:
        wikitext = """Lead text.
== Fat Gremlin ==
Fat summary.
=== Pattern ===
Pattern text.
== Mad Gremlin ==
Mad summary.
"""
        outline = extract_section_outline(wikitext)
        self.assertEqual(outline["extraction"], "explicit_wikitext_headings_only")
        self.assertEqual(outline["section_count"], 3)
        fat, pattern, mad = outline["sections"]
        self.assertEqual(fat["path"], ["Fat Gremlin"])
        self.assertEqual(pattern["path"], ["Fat Gremlin", "Pattern"])
        self.assertEqual(pattern["parent_document_order"], 1)
        self.assertEqual(fat["subtree_end"], mad["heading_start"])
        self.assertLess(fat["own_body_end"], fat["subtree_end"])
        self.assertEqual(mad["parent_document_order"], None)
        self.assertNotIn("Pattern text", json.dumps(outline))


if __name__ == "__main__":
    unittest.main()
