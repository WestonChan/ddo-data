# Corrections

Known mistakes in Maetrim's DDOBuilderV2 files, each replaced with the right value. The ETL embeds every `*.toml` in this directory and applies it after all of his files are written and before the [wiki overrides](../wiki/README.md) merge.

## The principle

This is the only place our data overrides his. Everywhere else his files win, and the wiki layer only adds facts he has no field for. A correction is for a value that is plainly wrong (a typo, a level no sibling shares), not for a disagreement of judgement; report those upstream at [Maetrim/DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2). Every correction cites a source for the right value and records the value he has now, and it expires the moment his value changes: the build then leaves his new value alone and `wiki-check` asks for the entry to be deleted. Report the mistake upstream as well, so the correction can go away.

Corrections are internal. The ETL applies them and stores each applied one in the `corrections` table of `ddo.db` for provenance and for `wiki-check`'s stale-correction warnings; the API never returns them, so a corrected value reads like any other.

## The shape

One `[[correction]]` table per corrected field, in any `*.toml` file here (`corrections.toml` for now).

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

- `kind`: the table the row is in: `item`, `augment`, `quest`, `feat`, `enhancement`, `race`, `class`, `adventure_pack`, `patron`, `set_bonus` or `spell`.
- `name`: the row, by his exact name. Augment, feat and enhancement names repeat (across augment families, class files and trees), so a correction of one of those applies to every row with that name. An `item` correction matches only his items, never an item the wiki supplied.
- `field`: a field from the allow-list below.
- `from`: his current value; `to`: the value to write. Each is an integer, a float, a string, or the literal string `"null"` for NULL. Flags are `0` or `1`; a field that refers to another row takes that row's name.
- `reason`: why his value is wrong, in one sentence.
- `source`: the `https://` URL that shows the right value.
- `read`: the date the source was read, `YYYY-MM-DD`.

## The allow-list

Only scalar columns can be corrected, never an id or a row's name (except where a rename is the correction):

| `kind` | fields |
|---|---|
| `item` | `minimum_level`, `enhancement_bonus`, `description`, `drop_location`, `race_required`, `accepts_sentience`, `is_minor_artifact`, `slot` (an `equipment_slots.name`), `item_category`, `item_type`, `material` (an `item_materials.name`), `set_bonus` (a set's name; the item's set link moves with it) |
| `augment` | `min_level`, `description`, `effect_description`, `family` |
| `quest` | `level`, `epic_level`, `favor`, `is_raid`, `pack` (an `adventure_packs.name`), `patron` (a `patrons.name`) |
| `feat`, `enhancement`, `race`, `class`, `spell` | `description` |
| `adventure_pack`, `patron`, `set_bonus` | `name`, which renames the row; a renamed set is renamed in every item's and augment's `set_bonus` too |

A quest's `is_free_to_play` and `legendary_level` come from the wiki's `quests.toml`, not his files, so they are corrected there. Renames run after every other correction, so `pack`, `patron` and `set_bonus` values name rows by his names; the wiki merge runs after the renames, so wiki files use the corrected name.

## Validation

The build fails, naming the file and the correction, when:

- a file has a table or field other than the ones above;
- `kind` is not one of the eleven, or `field` is not in that kind's allow-list;
- the same (`kind`, `name`, `field`) appears twice, in one file or across files;
- `from` or `to` does not fit the field (a string for an integer, `2` for a flag, `"null"` for a field that cannot be NULL), or `to` equals `from`;
- `reason` is empty, `source` is not an `https://` URL, or `read` is not a real `YYYY-MM-DD` date;
- `name` matches no row of that kind in his files;
- a `slot`, `material`, `pack`, `patron` or `set_bonus` value names nothing in his files, or a rename's `to` is already another row's name.

When `from` equals his stored value (on every row with that name), the build writes `to` and records the correction in the `corrections` table. When it does not, the build writes nothing for that entry and counts it as stale; a stale correction never fails the build.

## Checking

From the `ddo-data` root:

```bash
cargo xtask wiki-check
```

It builds into memory in a few seconds and prints `correction_applied_count` and `correction_stale_count` after the wiki counts, and one warning per stale correction under `warnings:` (`warning: correction <kind> "<name>".<field> expects <from> but Maetrim now has <value>; delete it from <file>`). `--corrections DIR` checks a directory of drafts in place of the embedded files. The tests use their own corrections in `crates/ddo-etl/tests/fixtures/corrections/`, which only name what the fixture data files carry.
