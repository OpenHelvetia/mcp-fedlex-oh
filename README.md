# mcp-fedlex

> **Kurz auf Deutsch.** Dieser MCP-Server macht das Schweizer Bundesrecht in Fedlex für KI-Systeme nutzbar: SR-Nummer zu ELI auflösen, die an einem Datum geltende Fassung bestimmen, Artikel im Wortlaut lesen, ein Zitat gegen den Normtext prüfen, die kanonische Fundstelle schreiben — 35 Werkzeuge, jede Antwort mit Beleg. Der Server spricht das Model Context Protocol (MCP) über stdin/stdout und lässt sich mit jedem MCP-Client verbinden. Alle Tests laufen offline gegen aufgezeichnete Antworten. Die Daten bleiben beim Bund; dieses Repository ist die Schnittstelle. Anleitung unten auf Englisch, Schritt für Schritt.

**mcp-fedlex** is an MCP server by the association [OpenHelvetia](https://openhelvetia.swiss) over the Confederation's [Fedlex](https://www.fedlex.admin.ch) infrastructure: Swiss federal law through the public SPARQL endpoint and the official Akoma Ntoso texts. It gives an AI system — or any MCP client — 35 tools that close the bitemporal citation loop: resolve an SR number to an ELI, list the versions of an act, determine the consolidation in force at a date, read an article eId-precise, check a quote against the norm text that was read, write the canonical citation.

**What it is not.** It is not a copy of the data and not a service you have to trust: every answer names the row, the IRI or the text it was read from, and the server derives no figure of its own. It keeps no state, needs no account, and stores nothing about you.

---

## Contents

1. [Before you start](#1-before-you-start)
2. [Get it running in five minutes](#2-get-it-running-in-five-minutes)
3. [Connect an MCP client](#3-connect-an-mcp-client)
4. [Your first call, by hand](#4-your-first-call-by-hand)
5. [The tools](#5-the-tools)
6. [Command-line flags](#6-command-line-flags)
7. [Where the data comes from, and how the server treats it](#7-where-the-data-comes-from-and-how-the-server-treats-it)
8. [What is in this repository](#8-what-is-in-this-repository)
9. [How it is verified](#9-how-it-is-verified)
10. [When something does not work](#10-when-something-does-not-work)
11. [Where this repository comes from](#11-where-this-repository-comes-from)
12. [Contributing, security, licence](#12-contributing-security-licence)

---

## 1. Before you start

You need three things. Nothing else.

| Need | Why | How to get it |
|---|---|---|
| **Rust, stable** (rustc and cargo) | the server is a Rust program and is built from source | <https://rustup.rs> — one command, then open a new terminal and run `cargo --version` |
| **Git** | to clone this repository | macOS: comes with Xcode command-line tools (`xcode-select --install`); Linux: your package manager; Windows: <https://git-scm.com> |
| **About 2 GB of disk and a few minutes** | the first build compiles the dependencies once; later builds take seconds | — |

Network: the **tests and the fixture mode need none**. Only the live mode talks to the Confederation's endpoint.

Operating systems: Linux and macOS are what the association builds on. Windows works in principle (Rust is portable) but is not tested here; use WSL if in doubt.

## 2. Get it running in five minutes

Copy each block into a terminal, one after the other. Every command runs from the folder you cloned into.

**Clone**

```bash
git clone https://github.com/OpenHelvetia/mcp-fedlex.git
cd mcp-fedlex
```

**Build and run the tests** (offline; the first build takes a few minutes)

```bash
cargo test --locked --manifest-path mcp/servers/fedlex/Cargo.toml
```

What you should see at the end of each test binary: a line like `test result: ok. … passed; 0 failed` and no `FAILED`. Tests marked `ignored` are deliberate live recording runs that only the association runs.

**Start the server in fixture mode** (offline, answers from the recorded files)

```bash
cargo run --locked --manifest-path mcp/servers/fedlex/Cargo.toml -- --fixtures mcp/servers/fedlex/tests/fixtures
```

The server now waits for an MCP client on stdin/stdout. It prints nothing by itself — that is correct. Stop it with Ctrl+C.

**Start the server in live mode** (talks to the Confederation's public endpoint)

```bash
cargo run --locked --manifest-path mcp/servers/fedlex/Cargo.toml
```

Live mode is polite by default: at most two upstream requests per second with a burst of four (see §6).

## 3. Connect an MCP client

The server speaks MCP over **stdio**: the client starts the process and talks to it through its input and output. Any MCP client that supports stdio servers works. Two examples.

**Claude Desktop** — add this to `claude_desktop_config.json` (macOS: `~/Library/Application Support/Claude/claude_desktop_config.json`), replacing `/ABSOLUTE/PATH/TO/mcp-fedlex` with the folder you cloned into, then restart Claude Desktop:

```json
{
  "mcpServers": {
    "mcp-fedlex": {
      "command": "cargo",
      "args": ["run", "--quiet", "--locked", "--manifest-path", "/ABSOLUTE/PATH/TO/mcp-fedlex/mcp/servers/fedlex/Cargo.toml"]
    }
  }
}
```

For an offline demo add `"--", "--fixtures", "/ABSOLUTE/PATH/TO/mcp-fedlex/mcp/servers/fedlex/tests/fixtures"` to the `args` list.

**Any stdio-capable client** — the command is the same as in §2; the binary itself lives at `mcp/servers/fedlex/target/debug/oh-mcp-fedlex` after a build (`target/release/oh-mcp-fedlex` after `cargo build --release`).

## 4. Your first call, by hand

You do not need a client to see the server answer. The block below sends three MCP messages over stdin — `initialize`, the `initialized` notification, and a `tools/list` — and prints what comes back. It works offline in fixture mode.

```bash
(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"hand","version":"0"}}}'; sleep 1; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}'; sleep 2) | cargo run --quiet --locked --manifest-path mcp/servers/fedlex/Cargo.toml -- --fixtures mcp/servers/fedlex/tests/fixtures
```

You should see two JSON lines: the `initialize` result naming the server, and the `tools/list` result with 35 tools. Then call one:

```bash
(printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"hand","version":"0"}}}'; sleep 1; printf '%s\n' '{"jsonrpc":"2.0","method":"notifications/initialized"}' '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"fedlex.resolve_sr","arguments":{"sr":"832.10"}}}'; sleep 10) | cargo run --quiet --locked --manifest-path mcp/servers/fedlex/Cargo.toml -- --fixtures mcp/servers/fedlex/tests/fixtures
```

The answer resolves SR 832.10 (the Health Insurance Act, KVG) to its ELI, its titles in German, French and Italian and its in-force status — read from the graph, with the IRI it came from.

If the second line does not appear, the server was still answering when the pipe closed: raise the `sleep 10` or use a real client (§3).

## 5. The tools

35 tools. Every name carries the domain prefix, so the same server can stand behind the association's gateway unchanged. Each tool's exact input and output shape is in the contract `mcp/servers/fedlex/TOOLSET-v1.md`; the one-line purpose:

| Tool | What it does |
|---|---|
| `fedlex.check_in_force` | Check whether an act was in force on a date; false is a valid answer, never an error: use for «gilt das noch?» questions. norm. |
| `fedlex.check_quote` | Check a quote (Zitat) against the norm text of one element: use before citing, to prove the wording is in what was read (Belegkette). norm. |
| `fedlex.cite` | Cite an element as its canonical Fundstelle («Art. 7 Abs. 1 Bst. b LSV») from eli_version and eId: use to label a read place in the Belegkette. norm. |
| `fedlex.compare_versions` | Compare an element or every article between two Fassungen of an act — added, removed, changed paragraphs with wording: use for «was hat sich geändert?». norm. |
| `fedlex.detect_foreign_content` | Detect what the text tools hide in a Fassung: sections in another language (xml:lang) and <foreign> islands (formulas, graphics): use before quoting. norm. |
| `fedlex.explore_node` | Explore a JOLux node's edges (predicates and neighbours, both directions, capped): use to debug what the graph holds about an IRI; never as proof. hint. |
| `fedlex.extract_tables` | Extract the tables of a consolidation or of one element (annex limit values, tariffs) as header and rows: use when a norm is a table, not prose. norm. |
| `fedlex.find_related_topic` | Find acts in the same field of law via the legal taxonomy, by ELI or SR: use to discover neighbouring Erlasse; candidates only. hint. |
| `fedlex.find_treaties` | Find treaty processes (Staatsverträge) by a title word, partner country IRI or bilaterality: use to locate a treaty before get_treaty_info. hint. |
| `fedlex.get_article_history` | Trace which amendments and consolidations changed one Artikel (eId) of an act, with dates: use for «seit wann gilt Art. X so?». norm. |
| `fedlex.get_citations` | List an act's relations: cites\|cited_by (formal citations, act level) or in\|out (foreseen impacts, mostly consultation drafts): use to see who cites X. norm. |
| `fedlex.get_consultation_documents` | List the position statements and result reports of one consultation IRI: use after get_consultations to read the genesis record. norm. |
| `fedlex.get_consultations` | List the consultations (Vernehmlassungen) of an act's drafts or of one draft, with status and dates: use for the genesis, never for law in force. hint. |
| `fedlex.get_drafts` | List the legislative drafts (Entwürfe, eli/proj) an act came from, with the Curia Vista number: use as the entry to consultations and materials. norm. |
| `fedlex.get_fga_documents` | List the Federal Gazette (BBl) documents of an act's genesis — Botschaft, reports — with genre and date: use for materials, never for law in force. norm. |
| `fedlex.get_law_metadata` | Read the JOLux profile of an act (titles de/fr/it, status, dates, identifier) for an ELI: use to confirm a search hit before you cite. norm. |
| `fedlex.get_memorial` | List the AS/BBl issue (memorial) an oc publication appeared in and the acts of that issue: use after get_oc_act to locate the volume. norm. |
| `fedlex.get_modifications` | List the amendment notes («Fassung gemäss …», AS refs) per element of a consolidation: use for «wann und wodurch wurde Art. X geändert?». norm. |
| `fedlex.get_oc_act` | Resolve an act's binding AS/RO publication (oc ELI, date, genre, office, memorial) from its consolidation ELI: use to cite the Amtliche Sammlung. norm. |
| `fedlex.get_references` | List the references (Verweise) an act's text makes, with ELI where linked, optionally within one eId: use to follow cross-references. hint. |
| `fedlex.get_structure` | Outline one consolidation (sections, articles with eId, num and heading): use when you know the act but not the article number. norm. |
| `fedlex.get_subdivisions` | List the subdivisions JOLux knows for an act (amended elements only, a gap catalogue): use to see which Artikel carry amendments; outline: get_structure. norm. |
| `fedlex.get_taxonomy` | Classify an act in the systematic collection (SR branch chain, notation, labels de/fr/it): use for «zu welchem Rechtsgebiet gehört X?». norm. |
| `fedlex.get_treaty_info` | Read a treaty process profile (title, signature, bilateral, partner countries, approving decree) for an eli/treaty IRI: use after find_treaties. norm. |
| `fedlex.list_annexes` | List the annexes (Anhänge) of a consolidation with titles and path eIds (annex_u1/…): use before reading an Anhang with read_article. norm. |
| `fedlex.list_expressions` | List the language versions and manifestations (XML, PDF) of one consolidation: use before reading to see whether a Fassung is PDF-only. norm. |
| `fedlex.list_versions` | List every dated consolidation (Fassung) of an act, future ones included: use to pick the eli_version the reading tools need. norm. |
| `fedlex.parse_reference` | Parse a citation («Art. 7 Abs. 1 lit. b LSV») into act, article eId and path proposal: use to turn a quoted Fundstelle into what read_article can open. hint. |
| `fedlex.read_article` | Read one element (Artikel, Absatz, Anhang) of a dated consolidation by eId, e.g. art_6 or annex_u1/lvl_u1: use to quote a norm. norm. |
| `fedlex.read_document` | Read a whole small act or Verordnung as capped Markdown (truncated flag, continuation offset): use for short acts; quote via read_article. norm. |
| `fedlex.resolve_consolidation_at` | Resolve which consolidation (Fassung) of an act governed on an ISO date: use before reading text for a past or future Stichtag. norm. |
| `fedlex.resolve_sr` | Resolve an SR number (e.g. 832.10) to the act's ELI, titles and in-force status: use when a question names an SR; predecessors stay visible. norm. |
| `fedlex.resolve_vocabulary_label` | Look up a Fedlex vocabulary term (enforcement-status, language, …) by label or IRI: use to decode a coded value from another answer. hint. |
| `fedlex.search_law` | Search acts by title keyword or official abbreviation (KVG, StPO, OR): use when you know the name but not the SR or ELI; in-force acts rank first. hint. |
| `fedlex.search_text` | Find where a word occurs inside ONE consolidation (hits with eId and Artikel): use before read_article when the article is unknown. hint. |

## 6. Command-line flags

| Flag | Meaning | Default |
|---|---|---|
| `--fixtures <dir>` | answer from recorded files in `<dir>` instead of the network; the tests use `mcp/servers/fedlex/tests/fixtures` | off (live) |
| `--endpoint <url>` | the SPARQL endpoint to talk to in live mode | `https://fedlex.data.admin.ch/sparqlendpoint` |
| `--upstream-rate <n>` | polite brake: at most `n` upstream requests per second | `2` |
| `--upstream-burst <n>` | polite brake: how many requests may go out at once before the rate applies | `4` |

There is no `--help`: the binary starts serving the moment it runs, because an MCP client expects exactly that. This table is the reference.

## 7. Where the data comes from, and how the server treats it

The holding is the public SPARQL endpoint `https://fedlex.data.admin.ch/sparqlendpoint` (the JOLux graph of Swiss federal legislation) and the Akoma Ntoso XML versions in the same filestore, both published by the Federal Chancellery. The server reads them; it hosts nothing.

Four rules the code enforces and the tests pin:

- **Every answer names its source** — the IRI, the row, the version, the element it was read from.
- **Nothing is derived.** The server never invents a version or a date: the consolidation in force at a date is resolved from the graph's own validity intervals, and `check_quote` compares a quote against the text it actually read and reports the difference rather than a guess.
- **States are answers, never faults.** An act that was not in force on a date answers `false`, an unknown act answers `not-found`, an empty list for a known act is an answer — none of these is an error.
- **No state, no account, no memory.** The server keeps nothing between calls and writes nothing to disk.

Licence of the data: what the Confederation publishes under its own terms (<https://www.admin.ch/gov/en/start/terms-and-conditions.html>); the server passes it on and adds nothing.

## 8. What is in this repository

| Path | What |
|---|---|
| `mcp/servers/fedlex/` | the server: sources, tests, 102 recorded fixtures (94 SPARQL answers, 8 Akoma Ntoso documents), `TOOLSET-v1.md` (the contract), `REFERENCE.md`, `ENGINE.md`, `engine.manifest.json`, `engine-conformance/` |
| `mcp/servers/common/` | what the association's domain MCP servers share: the polite brake and the semantic fixture store |
| `third_party/mcp-fedlex/` | three library crates vendored from the upstream fedlex reference workspace (`fedlex-core`, `fedlex-jolux`, `fedlex-akn`), Apache-2.0, byte-identical at the pin named in `PROVENANCE.md` |
| `docs/reference/fedlex-data-rules.md` | the rulebook: 123 rules (80 JOLux, 43 Akoma Ntoso) the conformance table is gated against |
| `LICENSE`, `NOTICE` | Apache-2.0 and the attributions |

The folder layout mirrors the corpus, so the relative paths inside the crates (`../common`, `../../../third_party/…`) resolve unchanged.

## 9. How it is verified

The rulebook `docs/reference/fedlex-data-rules.md` holds 123 measured rules; its conformance table says for each rule whether it is honoured, untested, violated or not applicable (99 · 15 · 3 · 6), and `tests/rules_table.rs` (7 tests) refuses a row that names a dead test or a status outside the five permitted ones. 114 tests run: 31 in the library, 76 end-to-end against the recorded fixtures, 7 in the rule gate; 15 more are the live recording runs, marked `#[ignore]`. The fixtures are keyed semantically — tool plus parameters, never query bytes — and `INDEX.txt` lists exactly 102 of them with key and recording date, so a rewritten query has to be re-recorded instead of silently missing the fixture. The three vendored upstream crates are byte-identical at their pin, with a SHA-256 per crate in `PROVENANCE.md`.

Run everything yourself with the test command in §2. Nothing in the test suite reaches the network; the live recording runs are marked `#[ignore]` and are the association's job.

## 10. When something does not work

| You see | What it means | What to do |
|---|---|---|
| `error: package … requires rustc 1.xx` or an `edition` error | your Rust is too old | `rustup update stable`, open a new terminal |
| the build fails on the first run with a network error | cargo could not download dependencies | check your connection or proxy; the *tests* are offline, the *first build* is not |
| the server starts and prints nothing | correct — it waits for an MCP client on stdin | connect a client (§3) or use the hand-made call (§4) |
| a tool answers with a refusal naming `retry_after_ms` | the polite brake in live mode | wait that long, or start with a higher `--upstream-rate` if the endpoint's operator allows it |
| a tool answers `not-found` | the act, cube or IRI does not exist at the source | that is an answer, not an error; check the identifier |
| live mode: connection errors | the Confederation's endpoint is unreachable or slow | try again later; fixture mode keeps working offline |

Anything else: open an issue in this repository with the command you ran and the output.

## 11. Where this repository comes from

The association develops all its modules in one corpus, on its own GitLab, where every change runs through a gate (formatting, Clippy without warnings, all tests, seal and drift checks). This repository is **assembled from that corpus** by the publication lane (`tools/publish-module.sh` there): it takes the crate and exactly the files its build and tests need, runs the tests in the assembled tree, and pushes here. Each publication is one commit whose message names the corpus commit.

This copy was published from corpus commit `e73cd7c` on 2026-09-03.

On the association's website the module has a card with its state, evidence and dependencies — <https://openhelvetia.swiss/en/directory/building-blocks/fedlex-engine/> — and a guide: <https://openhelvetia.swiss/en/docs/infrastructure/module-fedlex-engine/>. The module is the association's own entry in its directory; the entry page names the endpoint and the probe.

## 12. Contributing, security, licence

- **Issues** here are welcome: a wrong answer, a missing tool, an unclear sentence in this README. Please include the tool call and what came back.
- **Changes** go through the corpus and arrive here with the next publication; a pull request here is read and carried over by hand.
- **Security reports**, in confidence: security@openhelvetia.swiss. The association answers within a working week.
- **Licence:** Apache-2.0 for the association's code (`LICENSE`, attribution in `NOTICE`).
