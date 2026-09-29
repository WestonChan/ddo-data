# Wiki overrides

Facts read from [ddowiki](https://ddowiki.com) that Maetrim's DDOBuilderV2 files have no field for. The ETL embeds every `*.toml` in this directory and merges it after his files are written.

## The principle

Maetrim's files are authoritative for every field they carry. A wiki row may **add** a fact he has no field for; it never replaces one he has. When the wiki and his files disagree about something he carries (a name, a level, a drop location, a loot type), his value stands and the disagreement is reported upstream at [Maetrim/DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2), not patched here.

## Citing the source

Every entry names the page it was read from (`page`, a full `https://ddowiki.com/page/...` URL) and the date it was read (`read`, `YYYY-MM-DD`). Re-reading a page means updating that entry and its `read` date, not adding a second one. Read pages in a real browser, one page per navigation at a human pace; ddowiki's bot challenge blocks `api.php`, curl and scripted clients, and working around it is off limits. Wiki content is CC BY-SA.

## `quest_loot.toml`

One `[[quest]]` table per quest page. `rare` lists the named items the page's loot table marks as rare for that quest. The build already sets `is_rare` where Maetrim's drop text marks the drop itself, a segment (split on `;` and newlines) naming the quest with `(rare)` or `rare drop` and not `rare encounter`. An entry lists every rare drop the page marks for the quest, whether or not his text marks it too, so the file doubles as the record of what was read.

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

Validate a file from the `ddo-data` root:

```bash
cargo xtask wiki-check
```

A clean run prints the report, whose `wiki_quest_loot_entry_count`, `wiki_rare_drop_count` and `wiki_added_quest_loot_link_count` count what was applied, and `drop_text_rare_link_count` counts the rare links Maetrim's text marked on its own. The tests use their own overrides in `crates/ddo-etl/tests/fixtures/wiki/`, which only name what the fixture data files carry.

## `quests.toml`

One `[[quest]]` table per quest, carrying the quest facts Maetrim's `Quests.xml` has no field for. The loader reads a file by its name: `quest_loot*.toml` holds loot entries, `quests*.toml` holds these and `crafting*.toml` holds crafting systems, so a quest may appear once in each of the first two; any other name fails the build. The first 548 entries were read from the one index page, [Quests by level and XP](https://ddowiki.com/page/Quests_by_level_and_XP); a later read of a quest's own page updates its entry, `page` and `read` and adds the per-page fields.

```toml
[[quest]]
name = "A Blood Pact"
page = "https://ddowiki.com/page/Quests_by_level_and_XP"
read = "2026-09-28"
duration = "Long"
free_to_play = false
legendary_level = 37
xp.heroic = { casual = 3480, normal = 6100, hard = 6399, elite = 6700 }
xp.legendary = { casual = 23256, normal = 39615, hard = 40469, elite = 41325 }
```

- `free_to_play` (required): whether the quest itself is free to play, which the wiki marks per quest; `adventure_packs.is_free_to_play` stays Maetrim's per-pack flag.
- `duration` (optional): `Short`, `Medium`, `Long` or `Very long`, as the wiki spells them.
- `legendary_level` (optional): the legendary version's level. Heroic and epic levels are Maetrim's and are not repeated here.
- `xp` (optional): `heroic`, `epic` and `legendary` tables, each with optional `casual`, `normal`, `hard` and `elite` base XP. Leave out a tier the quest does not run at and a difficulty the tier lacks.
- `zone`, `bestowed_by`, `flagging` (optional, per-page reads): the page's "Takes place in" value, the quest giver, and free text on what must be run first.

The merge fills `quests.duration`, `is_free_to_play`, `legendary_level`, `zone`, `bestowed_by` and `flagging`, and writes one `quest_xp` row per tier. It never writes the columns Maetrim's files fill (`level`, `epic_level`, `pack_id`, `patron_id`, `favor`, `is_raid`, `difficulties`). Besides the checks above, the build fails when `free_to_play` is missing, `duration` is not one of the four values, or an XP value is negative. The report's `wiki_quest_entry_count` and `wiki_quest_xp_row_count` count what was applied.

## `crafting.toml`

One `[[system]]` table per wiki crafting page, carrying what the wiki adds to Maetrim's crafting data: the ingredients and what each recipe costs. His files already model a system's *options* as augments (`augments.family` groups them: `Slavelords_Heroic`, `Greensteel_Legendary`, `Alchemical`, ...) and an item's crafting sockets as `item_augment_slots`, so a system here only points at his families and names his augments; it never adds an augment, an item or an innate item bonus. `crafting*.toml` files hold these; split a large system into its own `crafting_<system>.toml` when that reads better. An illustrative entry, in the shape the reading agents write:

```toml
[[system]]
name = "Slave Lords Crafting"
page = "https://ddowiki.com/page/Slave_Lords_Crafting"
read = "2026-09-27"
pack = "Against the Slave Lords"
families = ["Slavelords_Heroic", "Slavelords_Legendary"]
npc = "Ingredient Bag Vendor"

[[system.ingredient]]
name = "Broken Shackle"
tier = "heroic"
bind = "Unbound"
source = "Slave Pits of the Undercity (Part 1)"

[[system.recipe]]
tier = "heroic"
slot = "crafting: slavelords prefix"
option = "Attributes +5"
augments = ["Charisma +5", "Constitution +5", "Dexterity +5"]
cost = [{ ingredient = "Broken Shackle", quantity = 50 }]
```

- `name` (required, unique across files): the system as the page names it.
- `pack` (optional): the adventure pack or expansion, spelled as Maetrim's `adventure_packs.name` (what `/v1/adventure-packs` lists).
- `families` (required, not empty): the augment families this system's options live in, each the name of one of his `Augments/*.Augments.xml` files before `.Augments.xml`.
- `npc` (optional): the crafting NPC or station, free text.
- `[[system.ingredient]]`: `name`, `tier` (`heroic`, `epic`, `legendary` or `any`), and optional free-text `bind` and `source` as the wiki writes them. A name may repeat at different tiers, never at the same one.
- `[[system.recipe]]`, one per row of the page's recipe tables, in page order: `tier` as above; `slot` (optional), the socket label as `/v1/augment-slot-types` lists it; `option`, the wiki's label for the row; `augments`, the names of his augments in the system's families that the row yields; `note` (optional) free text; `cost`, one `{ ingredient, quantity }` per ingredient.

An augment name resolves to every augment of that name in any of the system's families, because names repeat within a family (Green Steel has one `+5 Fortitude Save` per element combination). A cost's `ingredient` names one declared in the same system; when that name is declared at several tiers, the one at the recipe's tier is used, then the one at `any`. `augments` may be empty when the wiki row has no augment counterpart (cleansing an item, a material step), but then `note` must say so.

The merge writes `crafting_systems`, `crafting_system_families`, `crafting_ingredients`, `crafting_recipes` (with `sort_order` the recipe's position in the entry), `crafting_recipe_augments` and `crafting_recipe_ingredients`. Besides the citation checks above, the build fails, naming the system and the value, when a system has a field or table other than the ones above; a required field is missing; `families` is empty; a tier is not one of the four; an ingredient repeats at one tier; a recipe has no `augments` and no `note`; a cost names an undeclared ingredient, repeats one, or has a quantity below 1; or `pack`, a family, a `slot` label or an augment name matches nothing in Maetrim's files. The report's `wiki_crafting_system_count`, `wiki_crafting_recipe_count` and `wiki_crafting_ingredient_count` count what was applied.

To check a draft without embedding it, point the check at a directory holding only that file: `cargo xtask wiki-check --wiki /path/to/draft-dir`. It reads that directory in place of the embedded files, builds into memory, and prints the wiki counts or the validation error in a few seconds. `cargo xtask wiki-batch` writes the item, augment and quest name lists a reading agent matches against, under `target/wiki-batch/`.
