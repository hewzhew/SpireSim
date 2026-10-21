from __future__ import annotations

import json
from pathlib import Path
import tempfile
import unittest

from tools.wiki.mediawiki_direct import acquire_export
from tools.wiki.mediawiki_records import iter_pages, verify_bundle


class FakeTransport:
    def __init__(self) -> None:
        self.calls: list[dict] = []

    def request(self, parameters: dict) -> dict:
        self.calls.append(parameters.copy())
        if parameters.get("meta") == "siteinfo":
            return {
                "query": {
                    "general": {"sitename": "Fixture", "lang": "en"},
                    "statistics": {"pages": 2, "articles": 2},
                    "rightsinfo": {},
                    "namespaces": {"0": {"id": 0, "name": ""}},
                    "namespacealiases": [],
                }
            }
        if "revids" in parameters:
            revision_id = int(parameters["revids"])
            page_id = 1 if revision_id == 101 else 2
            return {
                "batchcomplete": True,
                "query": {
                    "pages": [
                        {
                            "pageid": page_id,
                            "ns": 0,
                            "title": "Alpha" if page_id == 1 else "Beta",
                            "revisions": [{"revid": revision_id, "slots": {"main": {"content": "text"}}}],
                        }
                    ]
                },
            }
        if "gapcontinue" not in parameters:
            self.assert_request_contract(parameters)
            return {
                "continue": {"continue": "gapcontinue||", "gapcontinue": "Beta"},
                "query": {
                    "pages": [
                        {"pageid": 1, "ns": 0, "title": "Alpha", "lastrevid": 101}
                    ]
                },
            }
        return {
            "batchcomplete": True,
            "query": {
                "pages": [{"pageid": 2, "ns": 0, "title": "Beta", "lastrevid": 202}]
            },
        }

    @staticmethod
    def assert_request_contract(parameters: dict) -> None:
        if "rvlimit" in parameters:
            raise AssertionError("multi-page generator requests may not set rvlimit")
        if parameters.get("prop") != "info":
            raise AssertionError("allpages must enumerate info before explicit revid lookup")


class MediaWikiDirectTests(unittest.TestCase):
    def test_direct_transport_writes_a_complete_transport_neutral_bundle(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            transport = FakeTransport()
            bundle = acquire_export(
                transport=transport,
                source_id="fixture",
                api_url="https://example.invalid/api.php",
                source_root=Path(temporary) / "fixture",
                capture_name="capture",
            )
            verified = verify_bundle(bundle)
            self.assertEqual(verified.page_count, 2)
            self.assertEqual([page["title"] for page in iter_pages(bundle)], ["Alpha", "Beta"])
            manifest = json.loads((bundle.parent / "manifest.json").read_text(encoding="utf-8"))
            self.assertEqual(manifest["bundle"]["sha256"], verified.sha256)
            calls_after_completion = len(transport.calls)
            resumed = acquire_export(
                transport=transport,
                source_id="fixture",
                api_url="https://example.invalid/api.php",
                source_root=Path(temporary) / "fixture",
                capture_name="capture",
                resume=True,
            )
            self.assertEqual(resumed, bundle)
            self.assertEqual(len(transport.calls), calls_after_completion)


if __name__ == "__main__":
    unittest.main()
