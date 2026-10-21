from __future__ import annotations

import argparse
import json
from pathlib import Path
import sys

from .reader import list_subjects, lookup


KIND_LABELS = {
    "immediate_value": "当前收益",
    "long_term_cost": "长期代价",
    "decision_conditions": "取舍条件",
}


def render(result: dict) -> str:
    subject = result["subject"]
    lines = [subject["name"]]
    if subject["mechanics"]:
        lines.append(f"机制：{subject['mechanics']}")
    if result["notes"]:
        lines.extend(["", "整理"])
    for note in result["notes"]:
        lines.append(f"{KIND_LABELS.get(note['kind'], note['kind'])}：{note['text']}")
    if result.get("source_lines"):
        lines.extend(["", f"来源摘记 · {result['source_page']}", *result["source_lines"]])
    if result["passages"]:
        lines.extend(["", "来源段落"])
    for number, passage in enumerate(result["passages"], 1):
        lines.append(
            f"{number}. {passage['author']} · {passage['source_title']} · "
            f"{passage['title']} · {passage['locator']}"
        )
        lines.append(f"   {passage['path']}#{passage['block_key']}")
        lines.append(f"   {passage['text']}")
    return "\n".join(lines)


def render_many(results: list[dict]) -> str:
    lines = ["候选"]
    for index, result in enumerate(results, 1):
        subject = result["subject"]
        lines.append(f"{index}. {subject['name']}")
        if subject["mechanics"]:
            lines.append(f"   机制：{subject['mechanics']}")

    kinds = list(KIND_LABELS)
    kinds.extend(sorted({note["kind"] for result in results for note in result["notes"]} - set(kinds)))
    for kind in kinds:
        entries = [
            (index, result, note)
            for index, result in enumerate(results, 1)
            for note in result["notes"]
            if note["kind"] == kind
        ]
        if not entries:
            continue
        lines.extend(["", KIND_LABELS.get(kind, kind)])
        for index, result, note in entries:
            lines.append(f"{index}. {result['subject']['name']}：{note['text']}")

    lines.extend(["", "来源与摘记"])
    for index, result in enumerate(results, 1):
        lines.append(f"{index}. {result['subject']['name']}")
        if result.get("source_lines"):
            lines.append(f"   {result['source_page']}")
            lines.extend(f"   {line}" for line in result["source_lines"])
        for number, passage in enumerate(result["passages"], 1):
            lines.append(
                f"{index}.{number} {passage['author']} · {passage['source_title']} · "
                f"{passage['title']} · {passage['locator']}"
            )
            lines.append(f"   {passage['path']}#{passage['block_key']}")
            lines.append(f"   {passage['text']}")
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="Read strategy by an exact subject or registered alias")
    parser.add_argument("--repository", type=Path, default=Path.cwd())
    parser.add_argument("subject", nargs="*", help="subject names or registered aliases")
    parser.add_argument("--list", action="store_true", help="list available subjects")
    parser.add_argument("--json", action="store_true")
    args = parser.parse_args(argv)
    try:
        if args.list:
            if args.subject:
                parser.error("a subject and --list cannot be combined")
            result = list_subjects(args.repository)
        else:
            if not args.subject:
                parser.error("provide a subject or --list")
            results = [lookup(args.repository, subject) for subject in args.subject]
            result = results[0] if len(results) == 1 else results
    except (OSError, ValueError, KeyError) as error:
        print(error, file=sys.stderr)
        return 1
    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    elif args.list:
        for item in result:
            print(f"{item['name']}\t{item['passage_count']}")
    else:
        print(render(results[0]) if len(results) == 1 else render_many(results))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
