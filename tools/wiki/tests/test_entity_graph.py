from __future__ import annotations

import json
from pathlib import Path
import sqlite3
import tempfile
import unittest

from tools.wiki.entity_graph import _normalized_title_candidates, rebuild_entity_graph
from tools.wiki.entity_query import (
    AttributedMechanicsOwner,
    candidate_audit,
    compact_entity_results,
    entity_graph_inventory,
    mechanics_scope_inventory,
    query_entities,
    query_entity_sections,
    query_identity_reviews,
    search_entity_pages,
    search_entity_mechanics,
    search_bound_entity_sections,
    simulator_entity_identity_aliases,
)
from tools.wiki.simulator_catalog import (
    build_simulator_catalog,
    simulator_entity_identity_manifest,
)


def _write_owner(repository: Path, relative: str, body: str) -> None:
    path = repository / relative
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(body, encoding="utf-8")


def _fixture_repository(repository: Path) -> None:
    _write_owner(
        repository,
        "src/content/relics/identity_v1.json",
        json.dumps(
            {
                "schema_name": "RelicIdentityContract",
                "schema_version": 1,
                "entries": [
                    {
                        "stable_id": "RunicCube",
                        "runtime_symbol": "RunicCube",
                        "external_id": "RunicCube",
                        "external_aliases": [],
                    }
                ],
            }
        ),
    )
    _write_owner(
        repository,
        "src/content/cards/mod.rs",
        """
pub enum CardId {
    Brutality,
}
pub fn java_id(id: CardId) -> &'static str {
    match id { CardId::Brutality => "Brutality", }
}
""",
    )
    _write_owner(
        repository,
        "src/content/potions/mod.rs",
        """
pub enum PotionId {
    FirePotion,
}
pub fn get_potion_definition(id: PotionId) -> PotionDefinition {
    match id { PotionId::FirePotion => PotionDefinition { name: "Fire Potion", }, }
}
""",
    )
    _write_owner(
        repository,
        "src/content/relics/mod.rs",
        """
pub enum RelicId {
    RunicCube,
}
""",
    )
    _write_owner(
        repository,
        "src/content/monsters/mod.rs",
        """
pub enum EnemyId {
    BronzeAutomaton,
}
pub fn get_name(&self) -> &'static str {
    match self { EnemyId::BronzeAutomaton => "Bronze Automaton", }
}
""",
    )
    _write_owner(
        repository,
        "src/content/monsters/factory.rs",
        """
pub enum EncounterId {
    Automaton,
}
pub fn build_encounter(encounter: EncounterId) {
    match encounter {
        EncounterId::Automaton => {
            spawn_monster(EnemyId::BronzeAutomaton);
        }
    }
}
""",
    )
    _write_owner(
        repository,
        "src/state/events/mod.rs",
        """
pub enum EventId {
    Nest,
}
pub const RETIRED_EVENT_IDS: &[EventId] = &[];
""",
    )
    _write_owner(
        repository,
        "src/state/events/identity.rs",
        """
pub const LEGACY_EVENT_NAME_ALIASES: &[(&str, EventId)] = &[
    ("Nest", EventId::Nest),
];
pub const fn event_display_name_v1(event_id: EventId) -> Option<&'static str> {
    Some(match event_id {
        EventId::Nest => "Nest",
    })
}
""",
    )


def _tabular(fields: list[str], rows: list[list[object]]) -> str:
    return json.dumps(
        {"schema": {"fields": [{"name": field} for field in fields]}, "data": rows},
        ensure_ascii=False,
    )


def _write_reviews(
    path: Path,
    *,
    decisions: list[str],
    revision_id: int = 115,
    internal_name: str = "N_est",
) -> None:
    path.write_text(
        json.dumps(
            {
                "schema": "sts-wiki-entity-identity-reviews-v1",
                "project": "fixture-project",
                "reviews": [
                    {
                        "decision": decision,
                        "entity_key": "event:Nest",
                        "record": {
                            "record_source_id": "slaythespire-wikigg-en",
                            "record_page_id": 15,
                            "record_page_title": "The Nest",
                            "record_revision_id": revision_id,
                            "record_locator": {"claim_line": 1},
                            "record_identity": {
                                "internal_name": internal_name,
                                "english_title": "The Nest",
                            },
                        },
                        "rationale": "Fixture review with exact revision-bound evidence.",
                        "reviewer": "fixture-reviewer",
                        "reviewed_on": "2026-08-20",
                    }
                    for decision in decisions
                ],
            },
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )


def _fixture_wiki(database: Path) -> None:
    connection = sqlite3.connect(database)
    connection.executescript(
        """
        CREATE TABLE metadata(key TEXT PRIMARY KEY,value TEXT NOT NULL);
        CREATE TABLE sources(
            source_id TEXT PRIMARY KEY,site_name TEXT,language TEXT,api_url TEXT,
            page_count INTEGER,corpus_sha256 TEXT,status TEXT,
            acquisition_warning_pages INTEGER,acquisition_error_pages INTEGER
        );
        CREATE TABLE pages(
            row_id INTEGER PRIMARY KEY,source_id TEXT,page_id INTEGER,namespace_id INTEGER,
            title TEXT,url TEXT,revision_id INTEGER,revision_timestamp TEXT,revision_sha1 TEXT,
            content_model TEXT,redirect_target TEXT,plain_text TEXT,wikitext TEXT,
            acquisition_warnings_json TEXT,acquisition_errors_json TEXT
        );
        CREATE TABLE templates(source_id TEXT,page_id INTEGER,template TEXT);
        CREATE TABLE categories(source_id TEXT,page_id INTEGER,category TEXT);
        CREATE TABLE namespaces(
            source_id TEXT,namespace_id INTEGER,name TEXT,canonical TEXT,is_content INTEGER,
            case_rule TEXT
        );
        CREATE VIRTUAL TABLE pages_fts USING fts5(
            source_id UNINDEXED,page_id UNINDEXED,title,plain_text,wikitext,
            tokenize='unicode61 remove_diacritics 2'
        );
        """
    )
    connection.execute("INSERT INTO metadata VALUES ('schema','fixture-wiki-v1')")
    connection.executemany(
        "INSERT INTO sources VALUES (?,?,?,?,?,?,?,?,?)",
        [
            ("slay-the-spire-huiji-zh", "Huiji", "zh-hans", "https://zh.invalid", 8, "zh", "complete", 0, 0),
            ("slaythespire-wikigg-en", "wiki.gg", "en", "https://en.invalid", 5, "en", "complete", 0, 0),
            ("slay-the-spire-fandom-en", "Fandom", "en", "https://fandom.invalid", 1, "fan", "complete", 0, 0),
        ],
    )
    pages = []

    def page(
        source: str,
        page_id: int,
        title: str,
        *,
        namespace: int = 0,
        model: str = "wikitext",
        redirect: str | None = None,
        body: str = "",
    ) -> None:
        pages.append(
            (
                source,
                page_id,
                namespace,
                title,
                f"https://example.invalid/{page_id}",
                page_id + 100,
                "2026-08-20T00:00:00Z",
                f"sha-{page_id}",
                model,
                redirect,
                body,
                body,
                "[]",
                "[]",
            )
        )

    zh = "slay-the-spire-huiji-zh"
    en = "slaythespire-wikigg-en"
    fandom = "slay-the-spire-fandom-en"
    page(zh, 1, "残暴")
    page(zh, 2, "火焰药水")
    page(zh, 3, "铜制自动机", redirect="铜自动机")
    page(zh, 4, "铜自动机", body="铜自动机会召唤铜球。")
    page(zh, 5, "符文立方体")
    page(zh, 6, "巢穴", body="{{事件\n|英文名 = The Nest\n}}")
    page(en, 11, "Brutality")
    page(en, 12, "Fire Potion")
    page(
        en,
        13,
        "Bronze Automaton",
        body="== Bronze Automaton ==\nBronze Automaton summons Bronze Orbs.",
    )
    page(en, 14, "Runic Cube", body="Runic Cube draws one card when HP is lost.")
    page(en, 15, "The Nest", body="* Internally called <code>Nest</code>.")
    page(fandom, 21, "Runic Cube")
    page(fandom, 22, "The Nest")
    page(
        zh,
        101,
        "Data:Card data.tabx",
        namespace=3500,
        model="Tabular",
        body=_tabular(["page", "card_id", "name_en"], [["残暴", "Brutality", "Brutality"]]),
    )
    page(
        zh,
        102,
        "Data:Relics.tabx",
        namespace=3500,
        model="Tabular",
        body=_tabular(["id", "name_en", "name_zhs"], [["Runic Cube", "Runic Cube", "符文立方体"]]),
    )
    page(
        zh,
        103,
        "Data:Potions.tabx",
        namespace=3500,
        model="Tabular",
        body=_tabular(["id", "name"], [["FirePotion", "火焰药水"]]),
    )
    page(
        zh,
        104,
        "Data:Monsters.tabx",
        namespace=3500,
        model="Tabular",
        body=_tabular(
            ["id", "name_en", "name_zhs"],
            [["BronzeAutomaton", "Bronze Automaton", "铜制自动机"]],
        ),
    )
    page(
        zh,
        105,
        "Data:Card/Brutality.json",
        namespace=3500,
        model="BSON",
        body=json.dumps({"id": "Brutality", "NAME": "残暴"}, ensure_ascii=False),
    )
    connection.executemany(
        """
        INSERT INTO pages(
            source_id,page_id,namespace_id,title,url,revision_id,revision_timestamp,
            revision_sha1,content_model,redirect_target,plain_text,wikitext,
            acquisition_warnings_json,acquisition_errors_json
        ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
        """,
        pages,
    )
    connection.executemany(
        "INSERT INTO namespaces VALUES (?,?,?,?,?,?)",
        [(zh, 0, "", "", 1, None), (en, 0, "", "", 1, None), (fandom, 0, "", "", 1, None)],
    )
    connection.execute(
        """
        INSERT INTO pages_fts(rowid,source_id,page_id,title,plain_text,wikitext)
        SELECT row_id,source_id,page_id,title,plain_text,wikitext FROM pages
        """
    )
    connection.executemany(
        "INSERT INTO templates VALUES (?,?,?)",
        [
            (zh, 1, "Infobox_Card"),
            (zh, 2, "Infobox Potion"),
            (zh, 4, "Infobox_Monster"),
            (zh, 5, "Infobox_Relic"),
            (en, 11, "CardAutobox"),
            (en, 12, "Potion Infobox"),
            (en, 13, "Monster Infobox"),
            (en, 14, "Relic Infobox"),
            (en, 15, "Enemy Infobox"),
            (fandom, 21, "Relic Infobox"),
            (zh, 6, "事件"),
        ],
    )
    connection.executemany(
        "INSERT INTO categories VALUES (?,?,?)",
        [(en, 15, "Events"), (fandom, 22, "Event")],
    )
    connection.commit()
    connection.close()


class EntityGraphTests(unittest.TestCase):
    def test_kind_suffix_title_candidate_matches_only_its_own_kind(self) -> None:
        self.assertIn(
            "goldenidol",
            _normalized_title_candidates("Golden Idol (Event)", "event"),
        )
        self.assertNotIn(
            "goldenidol", _normalized_title_candidates("Golden Idol (Relic)", "event")
        )

    def test_owner_adapter_requires_exhaustive_authoritative_mappings(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture_repository(repository)
            entities, _ = build_simulator_catalog(repository)
            self.assertEqual(len(entities), 6)
            cards_path = repository / "src/content/cards/mod.rs"
            cards_path.write_text(
                cards_path.read_text(encoding="utf-8").replace(
                    "    Brutality,", "    Brutality,\n    MissingMapping,"
                ),
                encoding="utf-8",
            )
            with self.assertRaisesRegex(ValueError, "not exhaustive"):
                build_simulator_catalog(repository)

    def test_relic_identity_contract_must_be_exhaustive(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture_repository(repository)
            contract = repository / "src/content/relics/identity_v1.json"
            value = json.loads(contract.read_text(encoding="utf-8"))
            value["entries"][0]["runtime_symbol"] = "WrongRelic"
            contract.write_text(json.dumps(value), encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "RelicId stable ID.*not exhaustive"):
                build_simulator_catalog(repository)

    def test_relic_identity_contract_rejects_normalized_external_collisions(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture_repository(repository)
            relics = repository / "src/content/relics/mod.rs"
            relics.write_text(
                relics.read_text(encoding="utf-8").replace(
                    "    RunicCube,", "    RunicCube,\n    OtherRelic,"
                ),
                encoding="utf-8",
            )
            contract = repository / "src/content/relics/identity_v1.json"
            value = json.loads(contract.read_text(encoding="utf-8"))
            value["entries"].append(
                {
                    "stable_id": "OtherRelic",
                    "runtime_symbol": "OtherRelic",
                    "external_id": "Runic Cube!",
                    "external_aliases": [],
                }
            )
            contract.write_text(json.dumps(value), encoding="utf-8")

            with self.assertRaisesRegex(ValueError, "normalized external relic ID"):
                build_simulator_catalog(repository)

    def test_event_alias_owner_is_exhaustive_and_retired_ids_are_not_entities(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture_repository(repository)
            events = repository / "src/state/events/mod.rs"
            events.write_text(
                events.read_text(encoding="utf-8")
                .replace("    Nest,", "    Nest,\n    LegacyNest,")
                .replace(
                    "pub const RETIRED_EVENT_IDS: &[EventId] = &[];",
                    "pub const RETIRED_EVENT_IDS: &[EventId] = &[EventId::LegacyNest];",
                ),
                encoding="utf-8",
            )
            identity = repository / "src/state/events/identity.rs"
            identity.write_text(
                identity.read_text(encoding="utf-8").replace(
                    '        EventId::Nest => "Nest",',
                    '        EventId::Nest => "Nest",\n'
                    "        EventId::LegacyNest => return None,",
                ),
                encoding="utf-8",
            )

            entities, _ = build_simulator_catalog(repository)

            event_entities = [entity for entity in entities if entity.kind == "event"]
            self.assertEqual([entity.stable_id for entity in event_entities], ["Nest"])
            self.assertEqual(event_entities[0].external_aliases, ("Nest",))

    def test_retired_event_stable_ids_remain_loadable_but_are_not_live_entities(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            repository = Path(temporary)
            _fixture_repository(repository)
            events = repository / "src/state/events/mod.rs"
            events.write_text(
                """
pub enum EventId {
    Nest => ("TheNest", 0),
    LegacyNest => ("LegacyNest", 1),
}
pub const RETIRED_EVENT_IDS: &[EventId] = &[EventId::LegacyNest];
""",
                encoding="utf-8",
            )
            identity = repository / "src/state/events/identity.rs"
            identity.write_text(
                identity.read_text(encoding="utf-8").replace(
                    '        EventId::Nest => "Nest",',
                    '        EventId::Nest => "Nest",\n'
                    "        EventId::LegacyNest => return None,",
                ),
                encoding="utf-8",
            )

            entities, _ = build_simulator_catalog(repository)

            event_entities = [entity for entity in entities if entity.kind == "event"]
            self.assertEqual([entity.stable_id for entity in event_entities], ["TheNest"])
            self.assertEqual(event_entities[0].external_aliases, ("Nest",))

    def test_event_alias_owner_rejects_missing_and_repeated_aliases(self) -> None:
        for alias_line, message in (
            ("", "not exhaustive"),
            ('    ("Nest", EventId::SecondNest),\n', "is repeated"),
        ):
            with self.subTest(message=message), tempfile.TemporaryDirectory() as temporary:
                repository = Path(temporary)
                _fixture_repository(repository)
                events = repository / "src/state/events/mod.rs"
                events.write_text(
                    events.read_text(encoding="utf-8").replace(
                        "    Nest,", "    Nest,\n    SecondNest,"
                    ),
                    encoding="utf-8",
                )
                handler = repository / "src/state/events/identity.rs"
                handler.write_text(
                    handler.read_text(encoding="utf-8")
                    .replace("];", f"{alias_line}];")
                    .replace(
                        '        EventId::Nest => "Nest",',
                        '        EventId::Nest => "Nest",\n'
                        '        EventId::SecondNest => "Second Nest",',
                    ),
                    encoding="utf-8",
                )

                with self.assertRaisesRegex(ValueError, message):
                    build_simulator_catalog(repository)

    def test_exact_event_runtime_alias_accepts_typed_english_pages(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            handler = repository / "src/state/events/identity.rs"
            handler.write_text(
                handler.read_text(encoding="utf-8").replace(
                    '("Nest", EventId::Nest)', '("The Nest", EventId::Nest)'
                ),
                encoding="utf-8",
            )
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)
            connection = sqlite3.connect(wiki)
            connection.execute(
                """
                UPDATE pages SET wikitext='',plain_text=''
                WHERE source_id='slaythespire-wikigg-en' AND page_id=15
                """
            )
            connection.commit()
            connection.close()

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            self.assertEqual(summary["kinds"]["event"]["accepted"], 1)
            event = query_entities(
                database=graph, kind="event", stable_id="Nest", query=None
            )[0]
            exact_alias_edges = [
                edge
                for edge in event["edges"]
                if edge["evidence_type"] == "simulator_external_alias_title_exact"
            ]
            self.assertEqual(
                {edge["source_id"] for edge in exact_alias_edges},
                {"slaythespire-wikigg-en", "slay-the-spire-fandom-en"},
            )

    def test_kind_category_owns_page_with_embedded_foreign_infobox(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            self.assertEqual(summary["kinds"]["event"]["accepted"], 1)
            event = query_entities(
                database=graph, kind="event", stable_id="Nest", query=None
            )[0]
            self.assertEqual(
                {
                    edge["title"]
                    for edge in event["edges"]
                    if edge["status"] == "accepted"
                },
                {"The Nest", "巢穴"},
            )

    def test_typed_event_page_with_multiple_active_owners_is_a_conflict(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            events = repository / "src/state/events/mod.rs"
            events.write_text(
                events.read_text(encoding="utf-8").replace(
                    "    Nest,", "    Nest,\n    AnotherNest,"
                ),
                encoding="utf-8",
            )
            handler = repository / "src/state/events/identity.rs"
            handler.write_text(
                handler.read_text(encoding="utf-8")
                .replace('("Nest", EventId::Nest)', '("Fixture Nest", EventId::Nest)')
                .replace(
                    "];", '    ("The Nest", EventId::AnotherNest),\n];'
                )
                .replace(
                    '        EventId::Nest => "Nest",',
                    '        EventId::Nest => "Nest",\n'
                    '        EventId::AnotherNest => "Another Nest",',
                ),
                encoding="utf-8",
            )
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            self.assertEqual(summary["conflict_count"], 2)
            connection = sqlite3.connect(graph)
            try:
                conflicts = connection.execute(
                    "SELECT conflict_type,details_json FROM conflicts ORDER BY conflict_id"
                ).fetchall()
            finally:
                connection.close()
            self.assertTrue(
                all(
                    conflict_type
                    == "typed_event_page_has_multiple_active_owners"
                    for conflict_type, _ in conflicts
                )
            )
            details = [json.loads(details_json) for _, details_json in conflicts]
            self.assertEqual(
                {detail["source_id"] for detail in details},
                {"slaythespire-wikigg-en", "slay-the-spire-fandom-en"},
            )
            self.assertTrue(
                all(
                    detail["entity_keys"]
                    == ["event:AnotherNest", "event:Nest"]
                    for detail in details
                )
            )

    def test_encounter_relation_preserves_direct_coverage_and_traverses_enemy_pages(
        self,
    ) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )
            inventory = entity_graph_inventory(graph)
            encounter = query_entities(
                database=graph,
                kind="encounter",
                stable_id="Automaton",
                query=None,
            )[0]

            self.assertEqual(summary["schema"], "sts-wiki-entity-graph-v5")
            self.assertEqual(summary["simulator_relation_count"], 1)
            encounter_inventory = next(
                item for item in inventory["kinds"] if item["kind"] == "encounter"
            )
            self.assertEqual(encounter_inventory["accepted"], 0)
            self.assertEqual(encounter_inventory["unmatched"], 1)
            self.assertEqual(
                inventory["simulator_relations"][0]["relation_count"], 1
            )
            self.assertEqual(encounter["edges"], [])
            self.assertEqual(len(encounter["relations"]), 1)
            target = encounter["relations"][0]["target"]
            self.assertEqual(target["entity_key"], "enemy:BronzeAutomaton")
            self.assertEqual(
                {page["title"] for page in target["accepted_pages"]},
                {"Bronze Automaton", "铜自动机"},
            )
            self.assertEqual(
                compact_entity_results([encounter])[0]["relations"],
                encounter["relations"],
            )

    def test_entity_search_uses_only_accepted_direct_or_explicit_related_pages(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)
            rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            direct = search_entity_pages(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="BronzeAutomaton",
                query="summons Bronze Orbs",
            )
            self.assertEqual(direct["schema"], "sts-wiki-entity-search-v1")
            self.assertEqual(direct["match_count"], 1)
            self.assertEqual(
                direct["matches"][0]["scope_bindings"][0]["scope"], "direct"
            )
            self.assertEqual(direct["matches"][0]["revision_sha1"], "sha-13")

            wrong_entity = search_entity_pages(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="BronzeAutomaton",
                query="draws one card",
            )
            self.assertEqual(wrong_entity["matches"], [])

            encounter = search_entity_pages(
                entity_database=graph,
                wiki_database=wiki,
                kind="encounter",
                stable_id="Automaton",
                query="summons Bronze Orbs",
            )
            self.assertEqual(encounter["eligible_page_count"], 0)
            self.assertEqual(encounter["matches"], [])

            related = search_entity_pages(
                entity_database=graph,
                wiki_database=wiki,
                kind="encounter",
                stable_id="Automaton",
                query="summons Bronze Orbs",
                include_related=True,
            )
            self.assertEqual(related["match_count"], 1)
            self.assertEqual(
                related["matches"][0]["scope_bindings"][0]["scope"], "related"
            )
            self.assertEqual(
                related["matches"][0]["scope_bindings"][0]["relation_type"],
                "encounter_initial_possible_member",
            )

            connection = sqlite3.connect(graph)
            connection.execute(
                """
                INSERT INTO simulator_entities VALUES
                ('enemy:FixtureTwin','enemy','FixtureTwin','Fixture Twin',NULL,'[]','fixture.rs',1)
                """
            )
            connection.execute(
                """
                INSERT INTO edges(entity_key,source_id,page_id,status,evidence_type,evidence_json)
                VALUES ('enemy:FixtureTwin','slaythespire-wikigg-en',13,'accepted','fixture','{}')
                """
            )
            connection.execute(
                """
                INSERT INTO simulator_relations(
                    relation_type,source_entity_key,target_entity_key,source_path,source_line
                ) VALUES (
                    'encounter_initial_possible_member','encounter:Automaton',
                    'enemy:FixtureTwin','fixture.rs',1
                )
                """
            )
            connection.commit()
            connection.close()
            shared_page = search_entity_pages(
                entity_database=graph,
                wiki_database=wiki,
                kind="encounter",
                stable_id="Automaton",
                query="summons Bronze Orbs",
                include_related=True,
            )["matches"][0]
            self.assertEqual(
                {
                    binding["entity_key"]
                    for binding in shared_page["scope_bindings"]
                },
                {"enemy:BronzeAutomaton", "enemy:FixtureTwin"},
            )
            self.assertEqual(
                set(shared_page["page_accepted_entity_keys"]),
                {"enemy:BronzeAutomaton", "enemy:FixtureTwin"},
            )
            self.assertTrue(shared_page["shared_entity_page"])
            twin_search = search_bound_entity_sections(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="FixtureTwin",
                query="summons Bronze Orbs",
            )
            self.assertEqual(twin_search["eligible_section_count"], 0)
            self.assertEqual(twin_search["match_count"], 0)
            self.assertEqual(twin_search["excluded_pages"][0]["binding_status"], "none")
            twin_mechanics = search_entity_mechanics(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="FixtureTwin",
                query="summons Bronze Orbs",
            )
            self.assertEqual(twin_mechanics["eligible_scope_count"], 0)
            self.assertEqual(twin_mechanics["match_count"], 0)
            self.assertEqual(
                twin_mechanics["excluded_pages"][0]["reason"],
                "shared_page_without_unique_section",
            )
            inventory = mechanics_scope_inventory(
                entity_database=graph,
                wiki_database=wiki,
                example_limit=10,
            )
            self.assertEqual(
                inventory["schema"], "sts-wiki-mechanics-scope-inventory-v1"
            )
            self.assertEqual(inventory["totals"]["entity_count"], 7)
            self.assertGreaterEqual(
                inventory["totals"]["entities_without_accepted_pages"], 2
            )
            self.assertEqual(
                inventory["totals"]["entities_with_accepted_but_no_safe_scope"],
                1,
            )
            self.assertEqual(
                inventory["examples"]["entities_with_accepted_but_no_safe_scope"][
                    0
                ]["entity_key"],
                "enemy:FixtureTwin",
            )
            self.assertEqual(
                inventory["encounters"][
                    "encounters_with_partial_member_coverage"
                ],
                1,
            )
            self.assertEqual(
                inventory["examples"]["encounters_with_uncovered_members"][0][
                    "uncovered_member_entity_keys"
                ],
                ["enemy:FixtureTwin"],
            )
            encounter_inventory = mechanics_scope_inventory(
                entity_database=graph,
                wiki_database=wiki,
                kind="encounter",
                example_limit=10,
            )
            self.assertEqual(
                encounter_inventory["totals"]["accepted_entity_pages"], 0
            )
            self.assertEqual(encounter_inventory["sources"], [])
            self.assertEqual(
                encounter_inventory["encounters"][
                    "encounters_with_partial_member_coverage"
                ],
                1,
            )

            automaton_mechanics = search_entity_mechanics(
                entity_database=graph,
                wiki_database=wiki,
                kind="encounter",
                stable_id="Automaton",
                query="summons Bronze Orbs",
                include_related=True,
            )
            self.assertEqual(automaton_mechanics["match_count"], 1)
            self.assertEqual(
                automaton_mechanics["matches"][0]["scope_type"], "section"
            )
            self.assertEqual(
                automaton_mechanics["matches"][0]["scope_bindings"][0]["scope"],
                "related",
            )
            mechanics_owner = AttributedMechanicsOwner(
                entity_database=graph,
                wiki_database=wiki,
            )
            owner_first = mechanics_owner.search(
                kind="encounter",
                stable_id="Automaton",
                query="summons Bronze Orbs",
                include_related=True,
            )
            owner_second = mechanics_owner.search(
                kind="enemy",
                stable_id="BronzeAutomaton",
                query="Block",
            )
            self.assertEqual(owner_first["match_count"], 1)
            self.assertEqual(
                owner_second["entity"]["entity_key"], "enemy:BronzeAutomaton"
            )

            candidate_only = search_entity_pages(
                entity_database=graph,
                wiki_database=wiki,
                kind="relic",
                stable_id="RunicCube",
                query="draws one card",
            )
            self.assertEqual(candidate_only["eligible_page_count"], 0)
            self.assertEqual(candidate_only["matches"], [])

            sections = query_entity_sections(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="BronzeAutomaton",
            )
            english = next(
                page
                for page in sections["pages"]
                if page["source_id"] == "slaythespire-wikigg-en"
            )
            self.assertEqual(english["binding_status"], "unique")
            self.assertEqual(
                english["matched_sections"][0]["matched_aliases"][0]["value"],
                "Bronze Automaton",
            )
            self.assertEqual(sections["match_rule"], "nfkc_casefold_whitespace_exact")
            self.assertTrue(
                any(page["binding_status"] == "none" for page in sections["pages"])
            )
            candidate_sections = query_entity_sections(
                entity_database=graph,
                wiki_database=wiki,
                kind="relic",
                stable_id="RunicCube",
            )
            self.assertEqual(candidate_sections["accepted_page_count"], 0)
            self.assertEqual(candidate_sections["pages"], [])
            section_search = search_bound_entity_sections(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="BronzeAutomaton",
                query="summons Bronze Orbs",
            )
            self.assertEqual(section_search["eligible_section_count"], 1)
            self.assertEqual(section_search["match_count"], 1)
            self.assertEqual(
                section_search["matches"][0]["section_path"],
                ["Bronze Automaton"],
            )

            with self.assertRaisesRegex(ValueError, "only valid for encounter"):
                search_entity_pages(
                    entity_database=graph,
                    wiki_database=wiki,
                    kind="enemy",
                    stable_id="BronzeAutomaton",
                    query="summons",
                    include_related=True,
                )

            connection = sqlite3.connect(wiki)
            connection.execute(
                "UPDATE pages SET plain_text='changed' WHERE page_id=13"
            )
            connection.commit()
            connection.close()
            with self.assertRaisesRegex(ValueError, "different Wiki database"):
                search_entity_pages(
                    entity_database=graph,
                    wiki_database=wiki,
                    kind="enemy",
                    stable_id="BronzeAutomaton",
                    query="summons",
                )
            with self.assertRaisesRegex(ValueError, "databases changed"):
                mechanics_owner.search(
                    kind="enemy",
                    stable_id="BronzeAutomaton",
                    query="summons",
                )

    def test_entity_section_binding_keeps_repeated_exact_headings_ambiguous(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)
            connection = sqlite3.connect(wiki)
            connection.execute(
                """
                UPDATE pages SET
                    plain_text='Bronze Automaton first Bronze Automaton second',
                    wikitext='== Bronze Automaton ==\nfirst\n== Bronze Automaton ==\nsecond'
                WHERE source_id='slaythespire-wikigg-en' AND page_id=13
                """
            )
            connection.commit()
            connection.close()
            rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            result = query_entity_sections(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="BronzeAutomaton",
            )
            english = next(
                page
                for page in result["pages"]
                if page["source_id"] == "slaythespire-wikigg-en"
            )
            self.assertEqual(english["binding_status"], "ambiguous")
            self.assertEqual(len(english["matched_sections"]), 2)
            search = search_bound_entity_sections(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="BronzeAutomaton",
                query="first",
            )
            self.assertEqual(search["eligible_section_count"], 0)
            self.assertEqual(search["matches"], [])
            self.assertTrue(
                any(
                    page["binding_status"] == "ambiguous"
                    for page in search["excluded_pages"]
                )
            )
            mechanics = search_entity_mechanics(
                entity_database=graph,
                wiki_database=wiki,
                kind="enemy",
                stable_id="BronzeAutomaton",
                query="first",
            )
            self.assertEqual(mechanics["match_count"], 1)
            self.assertEqual(mechanics["matches"][0]["scope_type"], "page")

    def test_graph_keeps_exact_redirect_and_normalized_candidate_distinct(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            self.assertEqual(summary["conflict_count"], 0)
            self.assertEqual(summary["issue_count"], 0)
            self.assertEqual(summary["kinds"]["card"]["accepted"], 1)
            self.assertEqual(summary["kinds"]["enemy"]["accepted"], 1)
            self.assertEqual(summary["kinds"]["event"]["accepted"], 1)
            self.assertEqual(summary["kinds"]["relic"]["accepted"], 0)
            self.assertEqual(summary["kinds"]["relic"]["unmatched"], 1)

            card = query_entities(database=graph, kind=None, stable_id=None, query="残暴")[0]
            self.assertEqual(card["entity_key"], "card:Brutality")
            self.assertTrue(any(edge["status"] == "accepted" for edge in card["edges"]))
            compact = compact_entity_results([card])[0]
            self.assertNotIn("edges", compact)
            self.assertEqual({page["title"] for page in compact["pages"]}, {"残暴", "Brutality"})
            enemy = query_entities(
                database=graph, kind="enemy", stable_id="BronzeAutomaton", query=None
            )[0]
            redirected = [
                edge
                for edge in enemy["edges"]
                if edge["source_id"] == "slay-the-spire-huiji-zh"
            ][0]
            self.assertEqual(redirected["title"], "铜自动机")
            self.assertEqual(redirected["evidence"]["redirect_chain"], ["铜制自动机", "铜自动机"])
            relic = query_entities(
                database=graph, kind="relic", stable_id="RunicCube", query=None
            )[0]
            self.assertTrue(relic["edges"])
            self.assertTrue(all(edge["status"] == "candidate" for edge in relic["edges"]))

            event = query_entities(
                database=graph, kind="event", stable_id="Nest", query=None
            )[0]
            self.assertEqual(
                {edge["title"] for edge in event["edges"]},
                {"The Nest", "巢穴"},
            )
            self.assertTrue(
                all(
                    edge["evidence_type"] == "wikigg_event_internal_name_exact"
                    for edge in event["edges"]
                )
            )

    def test_exact_relic_external_id_is_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            contract = repository / "src/content/relics/identity_v1.json"
            value = json.loads(contract.read_text(encoding="utf-8"))
            value["entries"][0]["external_id"] = "Runic Cube"
            contract.write_text(json.dumps(value), encoding="utf-8")
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            self.assertEqual(summary["kinds"]["relic"]["accepted"], 1)
            relic = query_entities(
                database=graph, kind="relic", stable_id="RunicCube", query=None
            )[0]
            self.assertEqual(relic["external_id"], "Runic Cube")
            self.assertEqual(relic["external_aliases"], [])
            self.assertTrue(
                any(
                    edge["status"] == "accepted"
                    and edge["evidence_type"] == "huiji_structured_exact"
                    for edge in relic["edges"]
                )
            )
            self.assertTrue(
                any(
                    edge["status"] == "accepted"
                    and edge["evidence_type"] == "simulator_external_id_title_exact"
                    for edge in relic["edges"]
                )
            )

            entities, source_hashes = build_simulator_catalog(repository)
            identity_manifest = simulator_entity_identity_manifest(entities)
            aliases = simulator_entity_identity_aliases(
                database=graph,
                expected_entity_identities=identity_manifest,
            )
            self.assertIn("符文立方体", aliases["relic:RunicCube"])

            event_owner = repository / "src/state/events/mod.rs"
            event_owner.write_text(
                event_owner.read_text(encoding="utf-8") + "\n// unrelated semantics edit\n",
                encoding="utf-8",
            )
            unchanged_entities, changed_source_hashes = build_simulator_catalog(repository)
            self.assertNotEqual(source_hashes, changed_source_hashes)
            self.assertEqual(
                identity_manifest,
                simulator_entity_identity_manifest(unchanged_entities),
            )
            aliases = simulator_entity_identity_aliases(
                database=graph,
                expected_entity_identities=simulator_entity_identity_manifest(
                    unchanged_entities
                ),
            )
            self.assertIn("符文立方体", aliases["relic:RunicCube"])

            changed_manifest = dict(identity_manifest)
            changed_identity = list(changed_manifest["relic:RunicCube"])
            changed_identity[2] = "Changed Display Name"
            changed_manifest["relic:RunicCube"] = tuple(changed_identity)
            with self.assertRaisesRegex(ValueError, "identities differ"):
                simulator_entity_identity_aliases(
                    database=graph,
                    expected_entity_identities=changed_manifest,
                )

    def test_candidate_audit_deduplicates_pages_and_reports_collisions(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)
            rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            initial = candidate_audit(
                database=graph, kind="relic", details=True, limit=10
            )["kinds"][0]
            self.assertEqual(initial["with_structured_candidates"], 1)
            self.assertEqual(initial["unique_one_entity_one_record_candidates"], 1)
            self.assertEqual(
                len(initial["details"]["unique_proposals"][0]["record"]["pages"]),
                3,
            )

            connection = sqlite3.connect(graph)
            connection.execute(
                """
                INSERT INTO simulator_entities VALUES
                ('relic:Runic_Cube','relic','Runic_Cube',NULL,NULL,'[]','fixture.rs',1)
                """
            )
            connection.execute(
                "INSERT INTO unmatched_simulator_entities VALUES (?,?)",
                ("relic:Runic_Cube", "fixture collision"),
            )
            connection.execute(
                """
                INSERT INTO edges(entity_key,source_id,page_id,status,evidence_type,evidence_json)
                SELECT 'relic:Runic_Cube',source_id,page_id,status,evidence_type,evidence_json
                FROM edges WHERE entity_key='relic:RunicCube'
                  AND evidence_type='normalized_structured_candidate'
                """
            )
            first_evidence = json.loads(
                connection.execute(
                    """
                    SELECT evidence_json FROM edges WHERE entity_key='relic:RunicCube'
                      AND evidence_type='normalized_structured_candidate' LIMIT 1
                    """
                ).fetchone()[0]
            )
            first_evidence["record_locator"] = {"row_index": 1}
            first_evidence["matched_value"] = "Runic-Cube"
            first_evidence["record_identity"]["id"] = "Runic-Cube"
            connection.execute(
                """
                INSERT INTO wiki_pages VALUES
                ('slay-the-spire-fandom-en',23,'relic','Runic-Cube',NULL,123,NULL,NULL,NULL,'[]')
                """
            )
            connection.execute(
                """
                INSERT INTO edges(entity_key,source_id,page_id,status,evidence_type,evidence_json)
                VALUES (?,?,?,?,?,?)
                """,
                (
                    "relic:RunicCube",
                    "slay-the-spire-fandom-en",
                    23,
                    "candidate",
                    "normalized_structured_candidate",
                    json.dumps(first_evidence, ensure_ascii=False, sort_keys=True),
                ),
            )
            connection.commit()
            connection.close()

            collided = candidate_audit(
                database=graph, kind="relic", details=True, limit=10
            )["kinds"][0]
            self.assertEqual(collided["entities_with_multiple_structured_records"], 1)
            self.assertEqual(collided["structured_records_with_multiple_entities"], 1)
            self.assertEqual(collided["unique_one_entity_one_record_candidates"], 0)

    def test_normalized_event_internal_name_remains_a_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)
            connection = sqlite3.connect(wiki)
            connection.execute(
                """
                UPDATE pages SET wikitext=?,plain_text=?
                WHERE source_id='slaythespire-wikigg-en' AND title='The Nest'
                """,
                ("* Internally called <code>N_est</code>.", "Internally called N_est."),
            )
            connection.commit()
            connection.close()

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            self.assertEqual(summary["kinds"]["event"]["accepted"], 0)
            audit = candidate_audit(
                database=graph, kind="event", details=True, limit=10
            )["kinds"][0]
            self.assertEqual(audit["with_structured_candidates"], 1)
            self.assertEqual(audit["unique_one_entity_one_record_candidates"], 1)

    def test_duplicate_event_internal_name_claim_is_a_conflict(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            _fixture_wiki(wiki)
            connection = sqlite3.connect(wiki)
            connection.execute(
                """
                INSERT INTO pages(
                    source_id,page_id,namespace_id,title,url,revision_id,revision_timestamp,
                    revision_sha1,content_model,redirect_target,plain_text,wikitext,
                    acquisition_warnings_json,acquisition_errors_json
                ) VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)
                """,
                (
                    "slaythespire-wikigg-en",
                    16,
                    0,
                    "Duplicate Nest",
                    "https://example.invalid/16",
                    116,
                    "2026-08-20T00:00:00Z",
                    "sha-16",
                    "wikitext",
                    None,
                    "Internally called Nest.",
                    "* Internally called Nest.",
                    "[]",
                    "[]",
                ),
            )
            connection.execute(
                "INSERT INTO categories VALUES (?,?,?)",
                ("slaythespire-wikigg-en", 16, "Events"),
            )
            connection.commit()
            connection.close()

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
            )

            self.assertEqual(summary["conflict_count"], 1)
            self.assertEqual(summary["kinds"]["event"]["accepted"], 0)

    def test_reviewed_identity_accepts_without_hiding_the_candidate(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            reviews = root / "reviews.json"
            _fixture_wiki(wiki)
            connection = sqlite3.connect(wiki)
            connection.execute(
                """
                UPDATE pages SET wikitext=?,plain_text=?
                WHERE source_id='slaythespire-wikigg-en' AND page_id=15
                """,
                ("* Internally called <code>N_est</code>.", "Internally called N_est."),
            )
            connection.commit()
            connection.close()
            _write_reviews(reviews, decisions=["accept"])

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
                review_file=reviews,
            )

            self.assertEqual(summary["identity_review_count"], 1)
            self.assertEqual(summary["reviewed_accepted_edge_count"], 3)
            self.assertEqual(summary["kinds"]["event"]["accepted"], 1)
            event = query_entities(
                database=graph, kind="event", stable_id="Nest", query=None
            )[0]
            self.assertEqual(
                [edge["status"] for edge in event["edges"]].count("candidate"), 3
            )
            reviewed = [
                edge
                for edge in event["edges"]
                if edge["evidence_type"] == "reviewed_identity_override"
            ]
            self.assertEqual(len(reviewed), 3)
            self.assertTrue(all(edge["status"] == "accepted" for edge in reviewed))
            self.assertTrue(
                all(
                    edge["evidence"]["identity_review"]["project"]
                    == "fixture-project"
                    for edge in reviewed
                )
            )
            report = query_identity_reviews(database=graph, decision=None, limit=10)
            self.assertEqual(report["counts"]["accept"], 1)
            self.assertEqual(report["counts"]["applied_accepted_edges"], 3)
            self.assertEqual(len(report["review_file_sha256"]), 64)

    def test_reject_and_defer_reviews_do_not_accept_candidate_edges(self) -> None:
        for decision in ("reject", "defer"):
            with self.subTest(decision=decision), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                repository = root / "repository"
                _fixture_repository(repository)
                wiki = root / "wiki.sqlite3"
                graph = root / "entities.sqlite3"
                reviews = root / "reviews.json"
                _fixture_wiki(wiki)
                connection = sqlite3.connect(wiki)
                connection.execute(
                    """
                    UPDATE pages SET wikitext=?,plain_text=?
                    WHERE source_id='slaythespire-wikigg-en' AND page_id=15
                    """,
                    ("* Internally called N_est.", "Internally called N_est."),
                )
                connection.commit()
                connection.close()
                _write_reviews(reviews, decisions=[decision])

                summary = rebuild_entity_graph(
                    repository=repository,
                    wiki_database=wiki,
                    output_database=graph,
                    review_file=reviews,
                )

                self.assertEqual(summary["kinds"]["event"]["accepted"], 0)
                self.assertEqual(summary["reviewed_accepted_edge_count"], 0)
                report = query_identity_reviews(
                    database=graph, decision=decision, limit=10
                )
                self.assertEqual(report["counts"][decision], 1)
                self.assertEqual(report["reviews"][0]["decision"], decision)

    def test_review_revision_and_record_drift_fail_explicitly(self) -> None:
        for revision_id, internal_name, message in (
            (114, "N_est", "is stale"),
            (115, "Ne_st", "no longer matches"),
        ):
            with self.subTest(message=message), tempfile.TemporaryDirectory() as temporary:
                root = Path(temporary)
                repository = root / "repository"
                _fixture_repository(repository)
                wiki = root / "wiki.sqlite3"
                graph = root / "entities.sqlite3"
                reviews = root / "reviews.json"
                _fixture_wiki(wiki)
                connection = sqlite3.connect(wiki)
                connection.execute(
                    """
                    UPDATE pages SET wikitext=?,plain_text=?
                    WHERE source_id='slaythespire-wikigg-en' AND page_id=15
                    """,
                    ("* Internally called N_est.", "Internally called N_est."),
                )
                connection.commit()
                connection.close()
                _write_reviews(
                    reviews,
                    decisions=["accept"],
                    revision_id=revision_id,
                    internal_name=internal_name,
                )

                with self.assertRaisesRegex(ValueError, message):
                    rebuild_entity_graph(
                        repository=repository,
                        wiki_database=wiki,
                        output_database=graph,
                        review_file=reviews,
                    )

    def test_review_refuses_an_already_accepted_record(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            reviews = root / "reviews.json"
            _fixture_wiki(wiki)
            _write_reviews(
                reviews,
                decisions=["accept"],
                internal_name="Nest",
            )

            with self.assertRaisesRegex(ValueError, "redundant"):
                rebuild_entity_graph(
                    repository=repository,
                    wiki_database=wiki,
                    output_database=graph,
                    review_file=reviews,
                )

    def test_review_can_accept_revision_bound_normalized_title(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            reviews = root / "reviews.json"
            _fixture_wiki(wiki)
            connection = sqlite3.connect(wiki)
            connection.execute(
                """
                UPDATE pages SET title='N est',wikitext='',plain_text=''
                WHERE source_id='slaythespire-wikigg-en' AND page_id=15
                """
            )
            connection.commit()
            connection.close()
            reviews.write_text(
                json.dumps(
                    {
                        "schema": "sts-wiki-entity-identity-reviews-v1",
                        "project": "fixture-project",
                        "reviews": [
                            {
                                "decision": "accept",
                                "entity_key": "event:Nest",
                                "record": {
                                    "record_source_id": "slaythespire-wikigg-en",
                                    "record_page_id": 15,
                                    "record_page_title": "N est",
                                    "record_revision_id": 115,
                                    "record_locator": {"field": "title"},
                                    "record_identity": {
                                        "kind": "event",
                                        "title": "N est",
                                    },
                                },
                                "rationale": "Fixture normalized-title identity review.",
                                "reviewer": "fixture-reviewer",
                                "reviewed_on": "2026-08-20",
                            }
                        ],
                    }
                ),
                encoding="utf-8",
            )

            summary = rebuild_entity_graph(
                repository=repository,
                wiki_database=wiki,
                output_database=graph,
                review_file=reviews,
            )

            self.assertEqual(summary["identity_review_count"], 1)
            self.assertEqual(summary["reviewed_accepted_edge_count"], 1)
            event = query_entities(
                database=graph, kind="event", stable_id="Nest", query=None
            )[0]
            reviewed = [
                edge
                for edge in event["edges"]
                if edge["evidence_type"] == "reviewed_identity_override"
            ]
            self.assertEqual(
                [(edge["source_id"], edge["title"]) for edge in reviewed],
                [("slaythespire-wikigg-en", "N est")],
            )

    def test_review_refuses_a_candidate_record_shared_by_multiple_entities(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            repository = root / "repository"
            _fixture_repository(repository)
            events = repository / "src/state/events/mod.rs"
            events.write_text(
                events.read_text(encoding="utf-8").replace(
                    "    Nest,", "    Nest,\n    NEst,"
                ),
                encoding="utf-8",
            )
            handler = repository / "src/state/events/identity.rs"
            handler.write_text(
                handler.read_text(encoding="utf-8")
                .replace("];", '    ("Fixture Second Nest", EventId::NEst),\n];')
                .replace(
                    '        EventId::Nest => "Nest",',
                    '        EventId::Nest => "Nest",\n'
                    '        EventId::NEst => "N Est",',
                ),
                encoding="utf-8",
            )
            wiki = root / "wiki.sqlite3"
            graph = root / "entities.sqlite3"
            reviews = root / "reviews.json"
            _fixture_wiki(wiki)
            connection = sqlite3.connect(wiki)
            connection.execute(
                """
                UPDATE pages SET wikitext=?,plain_text=?
                WHERE source_id='slaythespire-wikigg-en' AND page_id=15
                """,
                ("* Internally called N_est.", "Internally called N_est."),
            )
            connection.commit()
            connection.close()
            _write_reviews(reviews, decisions=["accept"])

            with self.assertRaisesRegex(ValueError, "ambiguous across candidate entities"):
                rebuild_entity_graph(
                    repository=repository,
                    wiki_database=wiki,
                    output_database=graph,
                    review_file=reviews,
                )


if __name__ == "__main__":
    unittest.main()
