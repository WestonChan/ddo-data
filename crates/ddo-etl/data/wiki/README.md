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

One `[[quest]]` table per quest, carrying the quest facts Maetrim's `Quests.xml` has no field for. The loader reads a file by its name: `quest_loot*.toml` holds loot entries, `quests*.toml` holds these, `crafting*.toml` holds crafting systems, `descriptions*.toml` holds descriptions and `items*.toml` holds items, so a quest may appear once in each of the first two; any other name fails the build. The first 548 entries were read from the one index page, [Quests by level and XP](https://ddowiki.com/page/Quests_by_level_and_XP); a later read of a quest's own page updates its entry, `page` and `read` and adds the per-page fields.

```toml
[[quest]]
name = "A Blood Pact"
page = "https://ddowiki.com/page/Quests_by_level_and_XP"
read = "2026-09-28"
free_to_play = false
legendary_level = 37
```

- `free_to_play` (required): whether the quest itself is free to play, which the wiki marks per quest; `adventure_packs.is_free_to_play` stays Maetrim's per-pack flag.
- `legendary_level` (optional): the legendary version's level. Heroic and epic levels are Maetrim's and are not repeated here.
- `zone`, `bestowed_by`, `flagging` (optional, per-page reads): the page's "Takes place in" value, the quest giver, and free text on what must be run first.

The merge fills `quests.is_free_to_play`, `legendary_level`, `zone`, `bestowed_by` and `flagging`. It never writes the columns Maetrim's files fill (`level`, `epic_level`, `pack_id`, `patron_id`, `favor`, `is_raid`, `difficulties`). Besides the checks above, the build fails when `free_to_play` is missing or a key is not one of those listed; quest duration and XP are deliberately not kept, so a `duration` or `xp` key fails as an unknown field. The report's `wiki_quest_entry_count` counts what was applied.

## `crafting.toml`

One `[[system]]` table per wiki crafting page, carrying what the wiki adds to Maetrim's crafting data: the ingredients and what each recipe costs. His files already model a system's *options* as augments (`augments.family` groups them: `Slavelords_Heroic`, `Greensteel_Legendary`, `Alchemical`, ...) and an item's crafting sockets as `item_augment_slots`, so a system here only points at his families and names his augments; it never adds an augment, an item or an innate item bonus. Upgrade and ingredient-only systems (altar tiers, "add an augment slot" rows, material steps) have no augment family: he models an upgrade as a socket on the item (`upgrade: tier 2`, `crafting: slavelords extra`), so their recipes either grant that socket with `grants_slot` or carry a `note`, and an upgraded state is never written as a new item. `crafting*.toml` files hold these; split a large system into its own `crafting_<system>.toml` when that reads better. An illustrative entry, in the shape the reading agents write:

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
- `families` (optional): the augment families this system's options live in, each the name of one of his `Augments/*.Augments.xml` files before `.Augments.xml`. Leave it out (or empty) for a system with no augment family; then every recipe's `augments` must be empty.
- `npc` (optional): the crafting NPC or station, free text.
- `[[system.ingredient]]`: `name`, `tier` (`heroic`, `epic`, `legendary` or `any`), and optional free-text `bind` and `source` as the wiki writes them. A name may repeat at different tiers, never at the same one.
- `[[system.recipe]]`, one per row of the page's recipe tables, in page order: `tier` as above; `slot` (optional), the socket label as `/v1/augment-slot-types` lists it; `option`, the wiki's label for the row; `augments`, the names of his augments in the system's families that the row yields; `grants_slot` (optional), the socket label, as `/v1/augment-slot-types` lists it, that the row adds to the item (`colorless` for a "Colorless Augment Slot" row, `upgrade: tier 2` for an altar's second tier); `note` (optional) free text; `cost`, one `{ ingredient, quantity }` per ingredient.

An augment name resolves to every augment of that name in any of the system's families, because names repeat within a family (Green Steel has one `+5 Fortitude Save` per element combination). A cost's `ingredient` names one declared in the same system; when that name is declared at several tiers, the one at the recipe's tier is used, then the one at `any`. `augments` may be empty when the wiki row has no augment counterpart (cleansing an item, a material step, adding a socket), but then `grants_slot` must name the socket it adds or `note` must say why.

The merge writes `crafting_systems`, `crafting_system_families`, `crafting_ingredients`, `crafting_recipes` (with `sort_order` the recipe's position in the entry and `grants_slot_id` the granted socket's `augment_slot_types` row), `crafting_recipe_augments` and `crafting_recipe_ingredients`. Besides the citation checks above, the build fails, naming the system and the value, when a system has a field or table other than the ones above; a required field is missing; a system without `families` has a recipe listing `augments`; a tier is not one of the four; an ingredient repeats at one tier; a recipe has no `augments`, no `grants_slot` and no `note`; a cost names an undeclared ingredient, repeats one, or has a quantity below 1; or `pack`, a family, a `slot` or `grants_slot` label or an augment name matches nothing in Maetrim's files. The report's `wiki_crafting_system_count`, `wiki_crafting_recipe_count` and `wiki_crafting_ingredient_count` count what was applied.

To check a draft without embedding it, point the check at a directory holding only that file: `cargo xtask wiki-check --wiki /path/to/draft-dir`. It reads that directory in place of the embedded files, builds into memory, and prints the wiki counts or the validation error in a few seconds. After the counts, a `warnings:` section lists each augment in a system's `families` that no recipe in that system yields (`warning: <system>: <family> augment "<name>" has no recipe`); a warning never fails the check, and many are expected (drop-only augments, a shared family such as `Named`). A second group reads Maetrim's socket labels, not the wiki files: labels that collide once case, spaces and punctuation are dropped and `zentarim`/`upgradable` are respelled `zhentarim`/`upgradeable` (`warning: socket labels differ only by spelling: "<a>" / "<b>"`). A recipe's `slot` must still name the exact label the recipe's augments use. `cargo xtask wiki-batch` writes the item, augment and quest name lists a reading agent matches against, and `blank_descriptions.txt` (below), under `target/wiki-batch/`.

## `descriptions.toml`

One `[[entry]]` table per item, augment, race, feat or enhancement whose description Maetrim's files leave blank, carrying the description text from its wiki page.

```toml
[[entry]]
kind = "item"
name = "Royal Guard Mask"
page = "https://ddowiki.com/page/Item:Royal_Guard_Mask"
read = "2026-09-29"
description = "..."
```

- `kind` (required): `item`, `augment`, `race`, `feat` or `enhancement`.
- `name` (required, unique per `kind` across files): the name exactly as Maetrim's `items.name`, `augments.name`, `races.name`, `feats.name` or `enhancements.name` spells it. An augment, feat or enhancement name applies to every row with that name, since names repeat across augment families, class files and enhancement trees.
- `description` (required): the page's description text, not empty and with no leading or trailing whitespace.

The merge writes the description only where Maetrim's is empty (or, for an augment, where his text is the `Drops in: ?` placeholder or ends in a `Drops in: ?` line, which is then the only line replaced), so a wiki description never replaces text he wrote; for such an augment, write the drop line in his form (`Drops in: The House of Gems, end chest`). Besides the citation checks above, the build fails, naming the entry, when a field is missing or unknown, `kind` is not one of the five, `description` is empty or untrimmed, a (`kind`, `name`) pair repeats, or `name` matches no row of that kind. The report's `wiki_description_entry_count` counts the entries applied, `wiki_description_filled_count` the rows filled and `wiki_description_skipped_count` the rows left alone because he already had a description. `cargo xtask wiki-batch` writes the work list, `blank_descriptions.txt`: one `kind<TAB>name<TAB>suggested page URL` line per name still blank after the current overrides.

## `items.toml`

One `[[item]]` table per wiki item page for a named item Maetrim's `Items/` does not have, so the item exists in the database until his files carry it. This is the one file type that adds rows he could have written himself, which is why it has its own rule: the moment his files carry an item of the same name, his row wins and the wiki entry is dropped from the build and reported so it can be deleted here. A tiered item is recorded once per level variant, each its own entry named `<Name> (Level N)`, matching the way his files name tiered items.

```toml
[[item]]
name = "Longsword of the Oozing Hunger"
page = "https://ddowiki.com/page/Item:Longsword_of_the_Oozing_Hunger"
read = "2026-09-29"
slot = "Main Hand"
category = "Weapon"
item_type = "Longsword"
minimum_level = 29
enhancement_bonus = 15
material = "Steel"
drop_location = "Sealed Altar, crafted from ..."
quests = [{ name = "Some Quest", loot_type = "chest" }]
augment_slots = ["red", "colorless"]
bonuses = [{ stat = "Strength", bonus_type = "Enhancement", value = 15 }]
effects = [{ name = "Vorpal", description = "...", value = 3, target = "..." }]

[item.weapon]
damage_dice_count = 2
damage_dice_sides = 8
damage_dice_bonus = 0
critical_threat_range = 2
critical_multiplier = 2
handedness = "One-handed"
dr_bypass = ["Magic", "Slash"]
```

- `name` (required, unique across files), `slot` (an `equipment_slots.name`, as `/v1/equipment-slots` lists it), `category` (`Armor`, `Shield`, `Weapon`, `Jewelry` or `Clothing`), `minimum_level` and `drop_location` (the page's source line, free text) are required.
- `item_type`: for a weapon or shield, a weapon type as `/v1/weapon-types` lists it (required); for armor, the armor type (`Cloth`, `Light`, `Medium`, `Heavy`, `Docent`), or omitted; otherwise free text or omitted.
- `enhancement_bonus`, `material` (a material his items carry), `race_required`, `description` (flavour text) and `set` (a set in his `SetBonuses.xml` or `FiligreeSets/`) are optional; `accepts_sentience` and `is_minor_artifact` default to `false`.
- `quests`: each `{ name, loot_type }`, the name spelled as his `Quests.xml` or `Challenges.xml` has it and `loot_type` one of `chest`, `raid`, `reward`.
- `augment_slots`: socket labels as `/v1/augment-slot-types` lists them, in the item's order.
- `bonuses`: each `{ stat, bonus_type, value }` with optional `value2`; the stat and bonus type as `/v1/stats` and `/v1/bonus-types` name them.
- `effects`: each `{ name }` with optional `description`, `value` and `target`. A name he already uses reuses his effect and his description; a new name creates the effect with the given description. Effect names are matched to his ignoring case, spaces and hyphens, so `Nightmare Guard` reuses his `NightmareGuard` and his description.
- `[item.weapon]` (required for `Weapon`, allowed for `Shield`): `handedness` (`One-handed`, `Two-handed`, `Off-hand` or `Thrown`) is required, the weapon type being the item's `item_type`; `damage_dice_count`, `damage_dice_sides`, `critical_threat_range` (the count of threatening faces: `19-20` is 2), `critical_multiplier`, `damage_dice_bonus`, `damage_multiplier` (the `[W]` multiplier, when the page gives one) and `dr_bypass` are optional, and each one left out is written as NULL. Dice are optional for rune arms (`slot = "Runearm"`, `item_type = "Rune Arm"`, `handedness = "Off-hand"`), which carry none, as none of Maetrim's do; give them for every other weapon the page lists them for.
- `[item.armor]` (required for `Armor` and `Shield`, only for them): `armor_type` (`Shield` for shields, otherwise one of the armor types, matching `item_type`), and optional `armor_bonus`, `max_dex_bonus`, `arcane_spell_failure`, `armor_check_penalty` and `shield_bonus`.

The merge runs after his items, sets and sockets are written. An entry whose `name` equals one of his item names writes nothing and is counted in `wiki_item_superseded_count`; every other entry writes an `items` row with `source = 'wiki'` and `wiki_url` its `page`, plus its weapon or armor stats, bonuses, effects, sockets, quest links (never rare) and set link, and is counted in `wiki_item_written_count`. An entry whose name matches one of his once case, punctuation, spaces and a trailing `(level N)` are ignored is still written, and is counted in `wiki_item_probable_duplicate_count` so a person can decide whether it is the same item. Besides the citation checks above, the build fails, naming the file, the item and the value, when a field is missing or unknown, a value is outside the vocabularies above, a weapon lacks `[item.weapon]`, an item carries a table its category does not allow, or a quest, material, set or socket label matches nothing in his files. `cargo xtask wiki-check` prints the three counts and, under `warnings:`, one line per superseded entry (`warning: wiki item "<name>" is now in Maetrim's files; delete it from <file>`) one per probable duplicate (`warning: wiki item "<name>" may duplicate Maetrim's "<his name>"`), and one per written entry whose `minimum_level` and `drop_location` equal one of his items' and whose name is that item's name followed by a space and a parenthesised suffix, as a looks variant is (`warning: wiki item "<name>" looks like a variant of Maetrim's "<his name>" (same level and drop location)`). `cargo xtask wiki-batch` writes `wiki_source_items.txt`, the names still supplied by the wiki; its `item_names.txt` lists only his items. The API reports each item's `source`, and `/v1/version` counts them as `wiki_items`.
