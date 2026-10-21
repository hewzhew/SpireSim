"""Direct, resumable MediaWiki API acquisition.

The resulting bundle is transport-neutral and can be consumed by the same
normalizer as a browser/userscript export.
"""

from __future__ import annotations

from collections import Counter
from dataclasses import dataclass
from datetime import datetime, timezone
import gzip
import json
import os
from pathlib import Path
import shutil
import time
from typing import Any, Protocol
import urllib.error
import urllib.parse
import urllib.request

from .mediawiki_records import (
    BUNDLE_SCHEMA,
    BundleError,
    canonical_json,
    file_sha256,
    iter_records,
    page_ids_sha256,
    utc_now,
    verify_bundle,
)


SITEINFO_PROPERTIES = (
    "general|namespaces|namespacealiases|statistics|rightsinfo"
)


class MediaWikiTransport(Protocol):
    def request(self, parameters: dict[str, Any]) -> dict[str, Any]: ...


class MediaWikiApiError(RuntimeError):
    pass


@dataclass
class DirectMediaWikiClient:
    api_url: str
    user_agent: str
    delay_seconds: float = 0.5
    timeout_seconds: float = 45.0
    max_retries: int = 6
    _last_request_at: float = 0.0

    def request(self, parameters: dict[str, Any]) -> dict[str, Any]:
        query = {
            "format": "json",
            "formatversion": 2,
            "utf8": 1,
            "maxlag": 5,
            **parameters,
        }
        url = f"{self.api_url}?{urllib.parse.urlencode(query)}"
        last_error: Exception | None = None

        for attempt in range(self.max_retries + 1):
            wait = self.delay_seconds - (time.monotonic() - self._last_request_at)
            if wait > 0:
                time.sleep(wait)
            request = urllib.request.Request(
                url,
                headers={
                    "User-Agent": self.user_agent,
                    "Accept": "application/json",
                    "Accept-Encoding": "gzip",
                },
            )
            try:
                self._last_request_at = time.monotonic()
                with urllib.request.urlopen(request, timeout=self.timeout_seconds) as response:
                    payload = response.read()
                    if response.headers.get("Content-Encoding", "").casefold() == "gzip":
                        payload = gzip.decompress(payload)
                decoded = json.loads(payload.decode("utf-8"))
                if not isinstance(decoded, dict):
                    raise MediaWikiApiError("MediaWiki response was not a JSON object")
                error = decoded.get("error")
                if isinstance(error, dict):
                    code = error.get("code")
                    if code == "maxlag" and attempt < self.max_retries:
                        time.sleep(min(30.0, 2.0**attempt))
                        continue
                    raise MediaWikiApiError(
                        f"MediaWiki API error {code!r}: {error.get('info', error)}"
                    )
                return decoded
            except (
                urllib.error.HTTPError,
                urllib.error.URLError,
                TimeoutError,
                json.JSONDecodeError,
                UnicodeDecodeError,
                OSError,
            ) as error:
                last_error = error
                status = error.code if isinstance(error, urllib.error.HTTPError) else None
                retryable = status in {408, 425, 429, 500, 502, 503, 504} or status is None
                if not retryable or attempt >= self.max_retries:
                    break
                retry_after = None
                if isinstance(error, urllib.error.HTTPError):
                    retry_after = error.headers.get("Retry-After")
                try:
                    retry_seconds = float(retry_after) if retry_after else 2.0**attempt
                except ValueError:
                    retry_seconds = 2.0**attempt
                time.sleep(min(60.0, max(1.0, retry_seconds)))

        raise MediaWikiApiError(f"request failed after retries: {url}: {last_error}")


def fetch_siteinfo(transport: MediaWikiTransport) -> dict[str, Any]:
    response = transport.request(
        {
            "action": "query",
            "meta": "siteinfo",
            "siprop": SITEINFO_PROPERTIES,
        }
    )
    query = response.get("query")
    if not isinstance(query, dict):
        raise MediaWikiApiError("siteinfo response lacks query object")
    return query


def public_namespace_ids(siteinfo: dict[str, Any]) -> list[int]:
    namespaces = siteinfo.get("namespaces")
    if isinstance(namespaces, dict):
        values = namespaces.values()
    elif isinstance(namespaces, list):
        values = namespaces
    else:
        raise MediaWikiApiError("siteinfo lacks namespaces")
    ids = {
        entry["id"]
        for entry in values
        if isinstance(entry, dict)
        and isinstance(entry.get("id"), int)
        and entry["id"] >= 0
    }
    return sorted(ids)


def _write_json_atomic(path: Path, value: Any) -> None:
    temporary = path.with_suffix(path.suffix + ".tmp")
    with temporary.open("w", encoding="utf-8", newline="\n") as output:
        output.write(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True))
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())
    os.replace(temporary, path)


def _append_record(path: Path, record: dict[str, Any]) -> None:
    with path.open("a", encoding="utf-8", newline="\n") as output:
        output.write(canonical_json(record))
        output.write("\n")
        output.flush()
        os.fsync(output.fileno())


def _read_partial(path: Path) -> tuple[dict[str, Any], set[int], Counter[str]]:
    header: dict[str, Any] | None = None
    page_ids: set[int] = set()
    namespace_counts: Counter[str] = Counter()
    for line_number, record in iter_records(path):
        if line_number == 1:
            if record.get("kind") != "header" or record.get("schema") != BUNDLE_SCHEMA:
                raise BundleError(f"{path}: partial export has invalid header")
            header = record
            continue
        if record.get("kind") != "page":
            raise BundleError(f"{path}:{line_number}: partial export contains non-page record")
        page = record.get("page", {})
        page_id = page.get("pageid")
        namespace_id = page.get("ns")
        if not isinstance(page_id, int) or not isinstance(namespace_id, int):
            raise BundleError(f"{path}:{line_number}: partial page identity is invalid")
        if page_id in page_ids:
            raise BundleError(f"{path}:{line_number}: partial export contains duplicate pageid")
        page_ids.add(page_id)
        namespace_counts[str(namespace_id)] += 1
    if header is None:
        raise BundleError(f"{path}: partial export is empty")
    return header, page_ids, namespace_counts


def _default_capture_name() -> str:
    return datetime.now(timezone.utc).strftime("%Y-%m-%dT%H%M%SZ-mediawiki-api")


def _attach_latest_revisions(
    transport: MediaWikiTransport, pages: list[dict[str, Any]]
) -> list[dict[str, Any]]:
    revision_ids = [
        page["lastrevid"]
        for page in pages
        if isinstance(page.get("lastrevid"), int) and page["lastrevid"] > 0
    ]
    revisions_by_page: dict[int, list[dict[str, Any]]] = {}
    for offset in range(0, len(revision_ids), 50):
        chunk = revision_ids[offset : offset + 50]
        response = transport.request(
            {
                "action": "query",
                "revids": "|".join(str(revision_id) for revision_id in chunk),
                "prop": "revisions",
                "rvslots": "main",
                "rvprop": "ids|timestamp|sha1|size|content|contentmodel",
            }
        )
        if "continue" in response:
            raise MediaWikiApiError(
                "explicit revid lookup unexpectedly returned continuation"
            )
        query = response.get("query", {})
        revision_pages = query.get("pages", []) if isinstance(query, dict) else []
        if not isinstance(revision_pages, list):
            raise MediaWikiApiError("explicit revid response query.pages was not an array")
        for revision_page in revision_pages:
            page_id = revision_page.get("pageid")
            revisions = revision_page.get("revisions")
            if not isinstance(page_id, int) or not isinstance(revisions, list):
                raise MediaWikiApiError("explicit revid response contained an invalid page")
            if page_id in revisions_by_page:
                raise MediaWikiApiError(
                    f"explicit revid response repeated pageid {page_id}"
                )
            revisions_by_page[page_id] = revisions

    combined = []
    for page in pages:
        page_id = page["pageid"]
        last_revision = page.get("lastrevid")
        revisions = revisions_by_page.get(page_id, [])
        if isinstance(last_revision, int) and last_revision > 0:
            if len(revisions) != 1 or revisions[0].get("revid") != last_revision:
                raise MediaWikiApiError(
                    f"explicit latest revision mismatch for pageid {page_id}: expected {last_revision}"
                )
        combined.append({**page, "revisions": revisions})
    return combined


def acquire_export(
    *,
    transport: MediaWikiTransport,
    source_id: str,
    api_url: str,
    source_root: Path,
    namespace_ids: list[int] | None = None,
    capture_name: str | None = None,
    resume: bool = False,
    max_pages: int | None = None,
) -> Path:
    capture_name = capture_name or _default_capture_name()
    capture_dir = source_root / "captures" / capture_name
    partial_path = capture_dir / "export.jsonl.partial"
    state_path = capture_dir / "state.json"
    output_path = capture_dir / "export.jsonl.gz"

    if resume:
        # A process may finish network acquisition and gzip creation, then be
        # stopped by a newer local verifier.  Re-verify that immutable bundle
        # before consulting the partial continuation state; never redownload
        # thousands of pages merely to finish the local manifest.
        if output_path.is_file():
            verified = verify_bundle(output_path)
            if (
                verified.header.get("source_id") != source_id
                or verified.header.get("api_url") != api_url
            ):
                raise BundleError("completed resume artifact has a different source identity")
            manifest = {
                "schema": "mediawiki-capture-v1",
                "source_id": source_id,
                "api_url": api_url,
                "transport": "direct-mediawiki-api",
                "bundle": {
                    "path": output_path.name,
                    "sha256": verified.sha256,
                    "page_count": verified.page_count,
                    "namespace_counts": verified.namespace_counts,
                },
                "completed_at": verified.trailer.get("completed_at"),
            }
            _write_json_atomic(capture_dir / "manifest.json", manifest)
            partial_path.unlink(missing_ok=True)
            state_path.unlink(missing_ok=True)
            return output_path
        if not partial_path.is_file() or not state_path.is_file():
            raise BundleError(f"cannot resume without {partial_path} and {state_path}")
        header, page_ids, namespace_counts = _read_partial(partial_path)
        state = json.loads(state_path.read_text(encoding="utf-8"))
        siteinfo = header["siteinfo"]
        requested_namespaces = header["namespace_ids"]
        if source_id != header.get("source_id") or api_url != header.get("api_url"):
            raise BundleError("resume source identity does not match partial export")
        if namespace_ids is not None and namespace_ids != requested_namespaces:
            raise BundleError("resume namespace_ids do not match partial export")
    else:
        if capture_dir.exists():
            raise FileExistsError(f"fresh capture path already exists: {capture_dir}")
        capture_dir.mkdir(parents=True)
        siteinfo = fetch_siteinfo(transport)
        available_namespaces = set(public_namespace_ids(siteinfo))
        requested_namespaces = (
            public_namespace_ids(siteinfo)
            if namespace_ids is None
            else sorted(set(namespace_ids))
        )
        unavailable = set(requested_namespaces) - available_namespaces
        if unavailable:
            raise BundleError(f"requested namespaces are unavailable: {sorted(unavailable)}")
        header = {
            "schema": BUNDLE_SCHEMA,
            "kind": "header",
            "source_id": source_id,
            "api_url": api_url,
            "retrieved_at": utc_now(),
            "namespace_ids": requested_namespaces,
            "siteinfo": siteinfo,
            "request_contract": {
                "generator": "allpages",
                "includes": ["info", "explicit-lastrevid-content"],
                "includes_redirect_pages": True,
                "includes_file_binaries": False,
            },
        }
        partial_path.write_text(canonical_json(header) + "\n", encoding="utf-8", newline="\n")
        page_ids = set()
        namespace_counts = Counter()
        state = {"namespace_position": 0, "continuation": {}}
        _write_json_atomic(state_path, state)

    while state["namespace_position"] < len(requested_namespaces):
        namespace_id = requested_namespaces[state["namespace_position"]]
        parameters: dict[str, Any] = {
            "action": "query",
            "generator": "allpages",
            "gapnamespace": namespace_id,
            "gaplimit": 50,
            "gapfilterredir": "all",
            "prop": "info",
            "inprop": "url",
        }
        parameters.update(state.get("continuation", {}))
        response = transport.request(parameters)
        query = response.get("query", {})
        pages = query.get("pages", []) if isinstance(query, dict) else []
        if not isinstance(pages, list):
            raise MediaWikiApiError("allpages response query.pages was not an array")

        pages = _attach_latest_revisions(transport, pages)

        for page in sorted(pages, key=lambda value: value.get("pageid", -1)):
            if not isinstance(page, dict) or not isinstance(page.get("pageid"), int):
                raise MediaWikiApiError("allpages response contained an invalid page")
            page_id = page["pageid"]
            if page_id in page_ids:
                continue
            if page.get("ns") != namespace_id:
                raise MediaWikiApiError(
                    f"allpages returned namespace {page.get('ns')} while reading {namespace_id}"
                )
            _append_record(partial_path, {"kind": "page", "page": page})
            page_ids.add(page_id)
            namespace_counts[str(namespace_id)] += 1
            if max_pages is not None and len(page_ids) >= max_pages:
                raise MediaWikiApiError(
                    f"intentional max-pages stop after {len(page_ids)} pages; resume this capture"
                )

        continuation = response.get("continue")
        if isinstance(continuation, dict):
            state["continuation"] = continuation
        else:
            state["namespace_position"] += 1
            state["continuation"] = {}
        _write_json_atomic(state_path, state)

    trailer = {
        "schema": BUNDLE_SCHEMA,
        "kind": "complete",
        "source_id": source_id,
        "page_count": len(page_ids),
        "namespace_counts": dict(namespace_counts),
        "page_ids_sha256": page_ids_sha256(page_ids),
        "completed_at": utc_now(),
    }
    _append_record(partial_path, trailer)
    with partial_path.open("rb") as source, gzip.open(output_path, "xb", compresslevel=6) as target:
        shutil.copyfileobj(source, target, length=1024 * 1024)
    verified = verify_bundle(output_path)
    manifest = {
        "schema": "mediawiki-capture-v1",
        "source_id": source_id,
        "api_url": api_url,
        "transport": "direct-mediawiki-api",
        "bundle": {
            "path": output_path.name,
            "sha256": verified.sha256,
            "page_count": verified.page_count,
            "namespace_counts": verified.namespace_counts,
        },
        "completed_at": trailer["completed_at"],
    }
    _write_json_atomic(capture_dir / "manifest.json", manifest)
    partial_path.unlink()
    state_path.unlink()
    return output_path


def probe_summary(transport: MediaWikiTransport, api_url: str) -> dict[str, Any]:
    siteinfo = fetch_siteinfo(transport)
    general = siteinfo.get("general", {})
    statistics = siteinfo.get("statistics", {})
    rights = siteinfo.get("rightsinfo", {})
    return {
        "api_url": api_url,
        "site_name": general.get("sitename"),
        "generator": general.get("generator"),
        "language": general.get("lang"),
        "statistics": statistics,
        "rights": rights,
        "namespace_ids": public_namespace_ids(siteinfo),
    }
