from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from tools.wiki.mediawiki_direct import acquire_export
from tools.wiki.wiki_corpus import (
    index_inventory,
    list_pages,
    normalize_export,
    read_data_table,
    read_page,
    read_page_sections,
    rebuild_index,
    search_index,
    search_section_references,
    structured_data_inventory,
)


class OnePageTransport:
    def request(self, parameters: dict) -> dict:
        if parameters.get("meta") == "siteinfo":
            return {
                "query": {
                    "general": {
                        "sitename": "Fixture Wiki",
                        "lang": "zh-hans",
                        "generator": "MediaWiki fixture",
                    },
                    "statistics": {"pages": 1, "articles": 1},
                    "rightsinfo": {},
                    "namespaces": {
                        "0": {"id": 0, "name": "", "canonical": "", "content": True},
                        "10": {"id": 10, "name": "模板", "canonical": "Template"},
                        "14": {"id": 14, "name": "分类", "canonical": "Category"},
                    },
                    "namespacealiases": [],
                }
            }
        if "revids" not in parameters:
            return {
                "batchcomplete": True,
                "query": {
                    "pages": [
                        {
                            "pageid": 17,
                            "ns": 0,
                            "title": "残暴",
                            "fullurl": "https://example.invalid/wiki/残暴",
                            "lastrevid": 31,
                            "acquisition_warnings": [
                                {
                                    "stage": "page_info",
                                    "code": "fixture_warning",
                                    "message": "fixture metadata fallback",
                                }
                            ],
                        }
                    ]
                },
            }
        return {
            "batchcomplete": True,
            "query": {
                "pages": [
                    {
                        "pageid": 17,
                        "ns": 0,
                        "title": "残暴",
                        "revisions": [
                            {
                                "revid": 31,
                                "timestamp": "2026-08-20T00:00:00Z",
                                "sha1": "fixture",
                                "slots": {
                                    "main": {
                                        "contentmodel": "wikitext",
                                        "content": "'''残暴'''是战士的能力牌。[[分类:战士卡牌]]\n==机制==\n每回合抽牌。",
                                    }
                                },
                            }
                        ],
                    }
                ]
            },
        }


class WikiCorpusTests(unittest.TestCase):
    def test_verified_bundle_remains_queryable_with_exact_provenance(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            library = Path(temporary) / "sources"
            source_root = library / "fixture-wiki"
            bundle = acquire_export(
                transport=OnePageTransport(),
                source_id="fixture-wiki",
                api_url="https://example.invalid/api.php",
                source_root=source_root,
                capture_name="capture",
                namespace_ids=[0],
            )
            normalize_export(bundle=bundle, source_root=source_root)
            database = Path(temporary) / "wiki.sqlite3"
            summary = rebuild_index(library_root=library, database=database)

            self.assertEqual(summary["source_count"], 1)
            self.assertEqual(summary["page_count"], 1)
            inventory = index_inventory(database=database)[0]
            self.assertEqual(inventory["namespace_counts"], {"0": 1})
            self.assertEqual(inventory["acquisition_warning_pages"], 1)
            self.assertEqual(inventory["acquisition_error_pages"], 0)
            self.assertEqual(list_pages(database=database, source_id="fixture-wiki")[0]["title"], "残暴")
            results = search_index(database=database, query="残暴", limit=5)
            self.assertEqual(results[0]["source_id"], "fixture-wiki")
            self.assertEqual(results[0]["revision_id"], 31)
            self.assertEqual(results[0]["namespace_is_content"], 1)
            self.assertEqual(
                search_index(database=database, query="残暴", content_only=True)[0]["page_id"],
                17,
            )
            page = read_page(database=database, source_id="fixture-wiki", title="残暴")
            self.assertEqual(page["categories"], ["战士卡牌"])
            self.assertIn("战士的能力牌", page["plain_text"])
            self.assertEqual(page["acquisition_warnings"][0]["code"], "fixture_warning")
            self.assertEqual(page["acquisition_errors"], [])
            metadata = read_page(
                database=database,
                source_id="fixture-wiki",
                title="残暴",
                metadata_only=True,
            )
            self.assertNotIn("plain_text", metadata)
            self.assertNotIn("wikitext", metadata)
            self.assertNotIn("internal_links", metadata)
            self.assertGreater(metadata["plain_text_length"], 0)
            self.assertGreater(metadata["wikitext_length"], 0)
            self.assertEqual(metadata["categories"], ["战士卡牌"])
            self.assertEqual(metadata["internal_link_count"], 1)
            outline = read_page_sections(
                database=database,
                source_id="fixture-wiki",
                title="残暴",
            )
            self.assertEqual(outline["schema"], "mediawiki-page-section-outline-v1")
            self.assertEqual(outline["revision_sha1"], "fixture")
            self.assertEqual(outline["section_count"], 1)
            self.assertNotIn("wikitext", outline)
            section = outline["sections"][0]
            outline_with_text = read_page_sections(
                database=database,
                source_id="fixture-wiki",
                title="残暴",
                expected_revision_id=31,
                expected_revision_sha1="fixture",
                include_plain_text=True,
            )
            self.assertIn(
                "每回合抽牌",
                outline_with_text["sections"][0]["subtree_plain_text"],
            )
            with self.assertRaisesRegex(ValueError, "revision changed"):
                read_page_sections(
                    database=database,
                    source_id="fixture-wiki",
                    title="残暴",
                    expected_revision_id=32,
                )
            section_matches = search_section_references(
                database=database,
                query="每回合抽牌",
                section_references=[
                    {
                        "source_id": outline["source_id"],
                        "page_id": outline["page_id"],
                        "page_title": outline["title"],
                        "revision_id": outline["revision_id"],
                        "revision_sha1": outline["revision_sha1"],
                        "section_document_order": section["document_order"],
                        "section_title": section["title"],
                        "section_path": section["path"],
                        "heading_start": section["heading_start"],
                        "subtree_end": section["subtree_end"],
                    }
                ],
            )
            self.assertEqual(len(section_matches), 1)
            self.assertEqual(section_matches[0]["section_path"], ["机制"])
            self.assertIn("[每回合抽牌]", section_matches[0]["snippet"])
            self.assertNotIn("wikitext", section_matches[0])
            source_manifest = json.loads(
                (source_root / "source.json").read_text(encoding="utf-8")
            )
            self.assertEqual(
                source_manifest["status"], "captured_complete_text_with_metadata_warnings"
            )
            self.assertEqual(source_manifest["acquisition"]["warning_pages"], 1)

    def test_structured_data_inventory_reports_schema_without_bodies(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            database = Path(temporary) / "wiki.sqlite3"
            import sqlite3

            connection = sqlite3.connect(database)
            connection.execute(
                """
                CREATE TABLE pages(
                    page_id INTEGER, source_id TEXT, namespace_id INTEGER, title TEXT,
                    revision_id INTEGER, revision_timestamp TEXT, revision_sha1 TEXT,
                    content_model TEXT, wikitext TEXT
                )
                """
            )
            connection.executemany(
                "INSERT INTO pages VALUES (?,?,?,?,?,?,?,?,?)",
                [
                    (
                        1,
                        "fixture",
                        3500,
                        "Data:Card/Brutality.json",
                        7,
                        "2026-01-01T00:00:00Z",
                        "one",
                        "BSON",
                        json.dumps({"id": "Brutality", "NAME": "残暴"}),
                    ),
                    (
                        2,
                        "fixture",
                        3500,
                        "Data:Card data.tabx",
                        8,
                        "2026-01-02T00:00:00Z",
                        "two",
                        "Tabular",
                        json.dumps(
                            {
                                "schema": {"fields": [{"name": "card_id"}]},
                                "data": [["Brutality"]],
                            }
                        ),
                    ),
                    (
                        3,
                        "fixture",
                        3500,
                        "Data:Spine/example/atlas",
                        9,
                        "2026-01-03T00:00:00Z",
                        "three",
                        "wikitext",
                        "atlas body",
                    ),
                ],
            )
            connection.commit()
            connection.close()

            inventory = structured_data_inventory(database=database, source_id="fixture")
            self.assertEqual(inventory["page_count"], 3)
            self.assertEqual(inventory["parse_issue_count"], 0)
            card = inventory["families"][0]
            self.assertEqual(card["family"], "Card")
            self.assertEqual(card["page_count"], 2)
            self.assertEqual(card["tabular_fields"], ["card_id"])
            self.assertEqual(card["tabular_row_count"], 1)
            self.assertNotIn("atlas body", json.dumps(inventory))
            spine = inventory["families"][1]
            self.assertEqual(spine["family"], "Spine")
            self.assertEqual(spine["non_json_page_count"], 1)

            table = read_data_table(
                database=database,
                source_id="fixture",
                title="Data:Card data.tabx",
                columns=["card_id"],
                limit=1,
            )
            self.assertEqual(table["row_count"], 1)
            self.assertEqual(table["rows"], [{"card_id": "Brutality"}])
            self.assertNotIn("schema", table)


if __name__ == "__main__":
    unittest.main()
