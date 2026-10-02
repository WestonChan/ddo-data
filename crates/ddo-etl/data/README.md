# ETL data files

The vocabularies the ETL maps Maetrim's DDOBuilderV2 files through. Each is embedded in the binary at compile time, so a change here needs a rebuild, and a value one of them cannot place fails the build on purpose.

- `buff_map.toml`: his item buff types, as stats, bonus types and effects.
- `effect_map.toml`: his effect types, as stats and bonus types.
- `legacy_drop_sources.toml`: the names in his drop text of quests that no longer exist, so the items only they dropped are flagged legacy (below).
- `corrections/`: his values a ddowiki page contradicts; see [`corrections/README.md`](corrections/README.md).
- `wiki/`: facts read from ddowiki that his files have no field for; see [`wiki/README.md`](wiki/README.md).

## Items the build leaves out or flags

His `Items/` files carry some items that a player cannot get. The build writes none of an item whose slots are all cosmetic, and records it in `excluded_items` with its reason (`cosmetic-only slots`), so a consumer can tell an item left out on purpose from one that is missing.

An item that once dropped but no longer does is written in full and flagged instead: `items.is_legacy = 1`. The build flags an item when its `DropLocation` has at least one segment (the text between `;` or line breaks) and every segment names a text in `legacy_drop_sources.toml` and no quest, quest chain, saga or adventure pack of his or of a wiki file. An item with any other segment is current, and its legacy segments simply link nothing. A legacy text that later becomes a quest's name again (a quest correction renaming one, say) is a current quest in every segment that names it.

## `legacy_drop_sources.toml`

The case the file was made for: Update 56 retired the quests Temple of Elemental Evil Part One and Part Two and replaced them with the Temple of Elemental Evil area quests, whose loot tables differ. His files no longer carry the two quests, but 553 of his items still say `Temple of Elemental Evil Part One, and Temple of Elemental Evil Part Two any chest`, `Temple of Elemental Evil Part Two, Elemental nodes chests` and the like, and those are the items flagged.

```toml
[[legacy]]
text = "Temple of Elemental Evil Part One"
reason = "The quest was retired in Update 56, replaced by the Temple of Elemental Evil area quests, which have their own loot tables."
source = "https://ddowiki.com/page/Temple_of_Elemental_Evil_Part_One"
read = "2026-10-02"
```

- `text`: the name as his segments spell it. It is matched as quest names are: longest first, as whole words, ignoring the case of every letter after the first.
- `reason`: why the source no longer drops loot, in one sentence.
- `source`: the `https://` URL that shows it.
- `read`: the date the source was read, `YYYY-MM-DD`.

A name his text uses for a source that still exists under another name is not a legacy source. Animus says `Devil Assault (quest), End Chest`, while his quests are `Devil Assault (Normal)`, `(Hard)` and `(Elite)`: the segment links to the pack Devil Assault, and the wiki loot file links the item to the three quests.

The build fails, naming the entry, when the file has a field other than these, `text` or `reason` is empty, `source` is not an `https://` URL, `read` is not a real date, or the same `text` appears twice ignoring case. `cargo xtask wiki-check` prints `legacy_source_flagged_count`, the items flagged, and `wiki-batch`'s `unlinked_drop_segments.txt` leaves out every segment that names a legacy text.
