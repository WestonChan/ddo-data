# Corrections

Known mistakes in Maetrim's DDOBuilderV2 files, each replaced with the right value. The ETL embeds every `*.toml` in this directory. Quest corrections apply as soon as his quests are written, before wiki quests are created and before his items' and augments' drop text is linked to quests, so drop text naming a quest by its corrected name links its loot; every other correction applies after all of his files are written. All of them apply before the [wiki overrides](../wiki/README.md) merge.

## The principle

This is the only place our data overrides his. Everywhere else his files win, and the wiki layer only adds facts he has no field for. The wiki is assumed right: when a ddowiki page contradicts a value he carries (a typo, a level no sibling shares, a bonus value, a bonus type, a name, a missing socket, a duplicate row), the wiki's value goes here, with that page as `source`. Report the disagreement upstream at [Maetrim/DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2) as well, so the correction can go away. Every correction records the value he has now, and it expires the moment his value changes: the build then leaves his new value alone and `wiki-check` asks for the entry to be deleted.

Corrections are internal. The ETL applies them and stores each applied one in the `corrections` table of `ddo.db` for provenance and for `wiki-check`'s stale-correction warnings; the API never returns them, so a corrected value reads like any other.

## The shape

One `[[correction]]` table per corrected field, in any `*.toml` file here, split by area: `corrections_quests.toml` (quests, packs, patrons), `corrections_crafting.toml` (crafting augments and their bonuses), `corrections_items.toml` (items, their sockets, socket labels), and `corrections.toml` for the rest.

```toml
[[correction]]
kind = "augment"
name = "The Fury's Rage"
field = "min_level"
from = 318
to = 18
reason = "Every sibling augment in LostPurpose is min level 18; 318 is a typo."
source = "https://ddowiki.com/page/Lost_Purpose"
read = "2026-09-29"
```

- `kind`: the table the row is in: `item`, `augment`, `quest`, `feat`, `enhancement`, `race`, `class`, `adventure_pack`, `patron`, `set_bonus` or `spell`; or `augment_bonus` (one bonus of an augment), `item_socket` (an item's sockets) or `socket_label` (an `augment_slot_types` row).
- `name`: the row, by his exact name (for `augment_bonus` the augment's, for `item_socket` the item's, for `socket_label` the label as `/v1/augment-slot-types` lists it, such as `"crafting: zentarim attuned"`). Augment, feat and enhancement names repeat (across augment families, class files and trees), so a correction of one of those applies to every row with that name. An `item` or `item_socket` correction matches only his items, never an item the wiki supplied.
- `family` (optional, `augment` and `augment_bonus` only): narrows the correction to the rows of that name in that augment family.
- `stat` and `bonus_type` (`augment_bonus` `value` and `bonus_type` only): the bonus to correct, by the `stats.name` and his `bonus_types.name` it has now.
- `field`: a field from the allow-list below.
- `from`: his current value; `to`: the value to write. Each is an integer, a float, a string, or the literal string `"null"` for NULL. Flags are `0` or `1`; a field that refers to another row takes that row's name. A `remove` takes `from = 0` (his files still carry the row) and `to = 1`; an `add` takes `from = "null"`.
- `reason`: why his value is wrong, in one sentence.
- `source`: the `https://` URL that shows the right value.
- `read`: the date the source was read, `YYYY-MM-DD`.

## The allow-list

Only scalar columns can be corrected, never an id or a row's name (except where a rename is the correction):

| `kind` | fields |
|---|---|
| `item` | `minimum_level`, `enhancement_bonus`, `description`, `drop_location`, `race_required`, `accepts_sentience`, `is_minor_artifact`, `slot` (an `equipment_slots.name`), `item_category`, `item_type`, `material` (an `item_materials.name`), `set_bonus` (a set's name; the item's set link moves with it), `name` (a rename; `wiki_url` follows), `remove` |
| `augment` | `min_level`, `description`, `effect_description`, `family`, `name` (a rename), `remove` |
| `quest` | `level`, `epic_level`, `favor`, `is_raid`, `pack` (an `adventure_packs.name`), `patron` (a `patrons.name`), `name` (a rename) |
| `feat`, `enhancement`, `race`, `class`, `spell` | `description` |
| `adventure_pack`, `patron`, `set_bonus` | `name`, which renames the row; a renamed set is renamed in every item's and augment's `set_bonus` too |
| `augment_bonus` | `value` (an integer), `bonus_type` (a `bonus_types.name`), `add` (`to = { stat = "...", bonus_type = "...", value = N }`) |
| `item_socket` | `add` (`to` = the socket label to add) |
| `socket_label` | `name`, which renames the label everywhere it is used; when `to` is already a label the two merge |

- `remove` drops the row and its child rows (bonuses, effects, sockets, quest loot, set links, modifiers; crafting recipes that name it would then fail the wiki merge). Use it only for a row the wiki shows is a duplicate of another he carries.
- `augment_bonus` `value` and `bonus_type` rewrite the bonus on every augment of that name: the augment is pointed at the `bonuses` row with the corrected stat, type and value (found or inserted), so other items and augments that share his bonus row keep it. The augment's `modifiers` are his and are not rewritten. `add` appends a bonus row to the augment; it is stale once the augment already carries a bonus with that stat and type.
- `item_socket` `add` appends an `item_augment_slots` row with that label, which must be one his files use; it is stale once the item carries the label.
- `socket_label` `name` into a new label keeps the prefix (`crafting: `) and updates the variant; a label with a qualifier (`isle of dread: scale (weapon)`) can only merge into an existing label.

A quest's `is_free_to_play` and `legendary_level` come from the wiki's `quests.toml`, not his files, so they are corrected there. Within each of those two passes renames run after every other correction, so every correction (and `pack`, `patron`, `set_bonus` and `item_socket` values) names rows by his names; the wiki merge runs after the renames, so wiki files (crafting `augments` and `slot` fields, `quest_loot`, descriptions, packs) use the corrected name, and the build fails on a wiki file that still names his old spelling. The one exception is a wiki items file (`items*.toml`): wiki items are written before corrections run, so their `augment_slots` and `set_bonus` use his labels and names, and a later `socket_label` rename moves their sockets with his.

## Validation

The build fails, naming the file and the correction, when:

- a file has a table or field other than the ones above;
- `kind` is not one of the fourteen, or `field` is not in that kind's allow-list;
- `family` is set on a kind other than `augment` or `augment_bonus`, or `stat` and `bonus_type` are missing from (or set outside) an `augment_bonus` `value` or `bonus_type` correction;
- the same (`kind`, `name`, qualifier, `field`) appears twice, in one file or across files; the qualifier is the `family`, the bonus's stat and type, or an added socket's label, so one augment can have several of its bonuses corrected;
- `from` or `to` does not fit the field (a string for an integer, `2` for a flag, `"null"` for a field that cannot be NULL), or `to` equals `from`;
- `reason` is empty, `source` is not an `https://` URL, or `read` is not a real `YYYY-MM-DD` date;
- `name` matches no row of that kind (in that `family`) in his files;
- a `slot`, `material`, `pack`, `patron`, `set_bonus`, stat, bonus type or socket label names nothing in his files, or a rename's `to` is already another row's name (for an augment, another augment in the same family).

When `from` equals his stored value (on every row with that name), the build writes `to` and records the correction in the `corrections` table, with its qualifier. When it does not, the build writes nothing for that entry and counts it as stale; a stale correction never fails the build.

## Checking

From the `ddo-data` root:

```bash
cargo xtask wiki-check
```

It builds into memory in a few seconds and prints `correction_applied_count` and `correction_stale_count` after the wiki counts, and one warning per stale correction under `warnings:` (`warning: correction <kind> "<name>".<field> expects <from> but Maetrim now has <value>; delete it from <file>`). `--corrections DIR` checks a directory of drafts in place of the embedded files. The tests use their own corrections in `crates/ddo-etl/tests/fixtures/corrections/`, which only name what the fixture data files carry.
