# Wiki overrides

Facts read from [ddowiki](https://ddowiki.com) that Maetrim's DDOBuilderV2 files have no field for. The ETL embeds every `*.toml` in this directory and merges it after his files are written.

## The principle

Maetrim's files are authoritative for every field they carry. A wiki row may **add** a fact he has no field for; it never replaces one he has. When the wiki and his files disagree about something he carries (a name, a level, a drop location, a loot type), his value stands and the disagreement is reported upstream at [Maetrim/DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2), not patched here.

## Citing the source

Every entry names the page it was read from (`page`, a full `https://ddowiki.com/page/...` URL) and the date it was read (`read`, `YYYY-MM-DD`). Re-reading a page means updating that entry and its `read` date, not adding a second one. Read pages in a real browser, one page per navigation at a human pace; ddowiki's bot challenge blocks `api.php`, curl and scripted clients, and working around it is off limits. Wiki content is CC BY-SA.

## `quest_loot.toml`

One `[[quest]]` table per quest page. `rare` lists the named items the page's loot table marks as rare for that quest. The build already sets `is_rare` where Maetrim's drop text marks the drop itself, a segment (split on `;` and newlines) naming the quest with `(rare)` or `rare drop` and not `rare encounter`, so entries are only needed for rare drops his text leaves unmarked.

```toml
[[quest]]
name = "Book Burning"
page = "https://ddowiki.com/page/Book_Burning"
read = "2026-09-27"
rare = [
  "Buckler of the Golden Age",
  "Docent of the Crownblade",
  "Legendary Buckler of the Golden Age",
  "Legendary Docent of the Crownblade",
  "Legendary Robe of the Warblade",
  "Robe of the Warblade",
]
```

The merge sets `quest_loot.is_rare` on each (quest, item) pair, after the drop text has set its own; it never clears the flag. When Maetrim's drop text did not already link the item to the quest, it adds the link with `loot_type = 'chest'`; an added link is an added fact. It never changes a `loot_type` his text produced.

## Validation

The build fails, naming the file and entry, when:

- a file has a table or field other than the ones above;
- `page` does not start with `https://ddowiki.com/page/`;
- `read` is not a real `YYYY-MM-DD` date;
- the same quest `name` appears twice, in one file or across files;
- `name` matches no quest in Maetrim's `Quests.xml` or `Challenges.xml` (names must match his exactly);
- an item in `rare` matches no item in his `Items/`. Items missing from his files are reported upstream, not stored here.

Validate a file by running the real build from the `ddo-data` root, with `export PATH="/opt/homebrew/opt/rustup/bin:$PATH"` first on the maintainer's Mac:

```bash
cargo run --release -p ddo-etl -- build --source upstream/Output/DataFiles --out /tmp/check.db
```

A clean run prints the report, whose `wiki_quest_loot_entries`, `wiki_rare_drops` and `wiki_quest_loot_links_added` count what was applied, and `drop_text_rare_links` counts the rare links Maetrim's text marked on its own. The tests use their own overrides in `crates/ddo-etl/tests/fixtures/wiki/`, which only name what the fixture data files carry.
