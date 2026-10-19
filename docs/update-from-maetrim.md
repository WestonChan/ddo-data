# Updating from Maetrim's DDOBuilderV2

When Maetrim pushes to [DDOBuilderV2](https://github.com/Maetrim/DDOBuilderV2), his changes come into `data/` through `ddo-etl import-ddobuilder`, a three-way merge done in the database: his previous files (A), his new files (B) and our data files (C) are each built, his delta A→B is applied to C row by row, and the export writes C back as TOML. Nothing we changed is lost; every place both sides changed is a conflict for a person to settle. This page is the step list. The `maetrim-update` agent in ddo-tools follows it; a maintainer can follow it by hand.

## 1. Check out his two commits

The previous commit is recorded in `data/upstream/commit.toml`. The new one is his `HEAD`, or whichever commit you are importing.

```bash
export PATH="/opt/homebrew/opt/rustup/bin:$PATH"
PREVIOUS=$(grep -o '[0-9a-f]\{40\}' data/upstream/commit.toml)
NEW=$(git ls-remote https://github.com/Maetrim/DDOBuilderV2 HEAD | cut -f1)
for sha in "$PREVIOUS" "$NEW"; do
  dir=~/.cache/ddo-data/upstream-${sha:0:8}
  [ -d "$dir" ] || {
    git clone --filter=blob:none --sparse https://github.com/Maetrim/DDOBuilderV2 "$dir"
    git -C "$dir" sparse-checkout set Output/DataFiles
    git -C "$dir" checkout --quiet "$sha"
  }
done
```

Keep the checkouts under `~/.cache/ddo-data/`, not `/tmp`: the import is proven against them, and `/tmp` is cleaned.

## 2. Work in a worktree and commit first

```bash
git worktree add -b import-<new sha prefix> /tmp/ddo-data-import-<new sha prefix> main
cd /tmp/ddo-data-import-<new sha prefix>
```

The import rewrites `data/` in place and git is the rollback, so start from a clean, committed tree.

## 2.5. Look at what he changed first

Before building anything, his git history says what changed and whether an import is worth running:

```bash
git -C ~/.cache/ddo-data/upstream-${NEW:0:8} fetch --quiet origin "$PREVIOUS"
git -C ~/.cache/ddo-data/upstream-${NEW:0:8} diff --stat "$PREVIOUS..$NEW" -- Output/DataFiles
git -C ~/.cache/ddo-data/upstream-${NEW:0:8} diff "$PREVIOUS..$NEW" -- Output/DataFiles/Feats.xml
```

An empty stat means nothing to import. A non-empty one names the files whose entities the wiki review (step 6) should expect to see changed. The diff is his format, line by line; it says what happened, not where it lands in our files. The import (step 3) is what turns it into row and column changes in our schema, rebases the corrections and applies it, so the two are a pair: read the diff to decide and to scope, run the import to apply.

## 3. Run the import

```bash
cargo run --release -p ddo-etl -- import-ddobuilder \
  --previous ~/.cache/ddo-data/upstream-${PREVIOUS:0:8}/Output/DataFiles --previous-sha "$PREVIOUS" \
  --source   ~/.cache/ddo-data/upstream-${NEW:0:8}/Output/DataFiles      --sha "$NEW" \
  --data data | tee import-report.txt
```

It stops before writing anything when `--previous` is not the recorded commit, when his files do not build (a correction or wiki entry names something he removed, or his XML names an image his folders lack), when his row references something the data files no longer have, or when the staged files would not rebuild to the merged database. Those messages name the entry; fix the cause and run it again.

It writes `data/` and prints the report when the merge is clean or has only conflicts. Exit 0 means no conflicts; exit 2 means the report lists at least one. `data/upstream/commit.toml` records the new commit either way.

## 4. Read the report

The report is the pull-request body of the future automation, so it is complete on its own: a header with both shas, a counts table (one row per table: added, removed, changed, conflicts, declined removals, resolved corrections), then one line per item that needs a person, each starting with the data-relative file:

- `CONFLICT`: he and we both changed the field (or the array, or the correction). Ours stays. The line shows his previous value, his new value and ours. Decide: keep ours (nothing to do), or take his (edit the file to his value and drop the `[[ddobuilder_correction]]` if one carried ours).
- `DECLINED REMOVAL`: he removed a row we annotated, edited or still reference. The row stays and the line says why. Keep it, or remove it by hand together with what referenced it.
- `RESOLVED CORRECTION`: he moved to the value our `[[ddobuilder_correction]]` holds. The correction has done its job; delete it so the file no longer records a difference that is gone.
- `NEEDS RESEARCH`: an adventure pack he added has no `update` or `released`. It landed in a folder named only by its slug. Read [Modules and updates](https://ddowiki.com/page/Modules_and_updates), add `update`, `released` and a `[[citation]]` to its `pack.toml`, then rebuild through the database (`build --data data --out ddo.db`, `export --db ddo.db --out data`) so the folder gets its update tag.
- `ICON CONFLICT` / `ICON DECLINED`: both sides changed an image's bytes, or he removed one we still reference. Ours stays.
- `CHANGED UPSTREAM, ABSENT IN OURS`: he changed an entity we deleted. Nothing was applied; the line is for the record.
- `BLANKED UPSTREAM, OURS KEPT`: he emptied a value he used to have. His blank never erases ours, so ours stays. Check whether the value is still right.
- `SKIPPED WIKI FACT`: a wiki fact in the data files names something his previous files spelled differently or lacked. It was left out of his previous build only, and nothing in `data/` changed because of it.
- The mapping audits (`UNCLASSIFIED BUFF TYPE`, `PARTIALLY MAPPED EFFECT`): his XML uses a buff family or effect target that `crates/ddo-etl/data/effect_map.toml` does not map. They never change the exit code. Map them there when the effect matters.

How his values meet ours:
- **His blank never erases ours.** In both of his builds, a value he leaves empty (NULL or empty text, but not 0 or false) is taken from ours before the comparison. A value he adds where his file was blank is applied as his change.
- **Five quest columns are always ours.** These are free-to-play, legendary level, zone, quest giver and flagging (`OURS_ONLY_COLUMNS`), because his files never carry them.
- **A merge of several of his spellings is one correction.** It is a single `[[ddobuilder_correction]]` whose `upstream` lists them all, for example the heroism socket labels. Two separate corrections with the same target fail the build.
- **The 12 SunAndMoon gems are corrections.** His descriptions for them end in "Drops in: ?", and each carries a description correction with the wiki's drops line. When he edits one of those descriptions, the correction goes stale and is reported as a `CONFLICT`. An edit of any other field of those gems applies cleanly.

Every edit after the import is an ordinary data-file edit: change the TOML, add the `[[ddobuilder_correction]]` or `[[citation]]` that records why, rebuild.

## 5. Prove it

```bash
cargo xtask round-trip --data data
cargo run --release -p ddo-etl -- build --data data --out ddo.db
cargo xtask check-db ddo.db
cargo xtask validate-api --db ddo.db --all-details
cargo xtask wiki-tooltips --db ddo.db
curl -sf https://ddo-data.fly.dev/v1/dump.sqlite -o deployed.sqlite
cargo xtask check-redirects --db ddo.db --deployed deployed.sqlite
cargo test --workspace && cargo lint && cargo fmt --all --check
```

`wiki-tooltips` or `check-db` may flag his new content itself (a tooltip line the golden set does not know, a quest without a pack). Those are findings about his data, handled by the same rules as any other build, not by the import.

`check-redirects` names every slug the live API serves that this build no longer has: his renames and deletions. Answer each in `data/redirects.toml` before landing, or the deploy stops on it. A rename is a `[[redirect]]` row (`kind`, `from` the old slug, `to` the new one, `since` today); a deletion (the report's `REMOVED`) is a `[[retired]]` row (`kind`, `slug`, `since`, `reason`). A renamed or retired enhancement tree or crafting system covers its selections or ingredients. If the build instead fails because a redirect's `to` no longer exists (he deleted the target), retarget the row or turn it into a `[[retired]]` row.

## 6. Review his changes against the wiki

`git diff --stat main -- data` and the report's counts say which entities changed. For each changed or added item, set, feat, spell, enhancement or class, a reader checks the ddowiki page and records what it finds in the entity's own file: a `[[citation]]` for a fact the page confirms or adds, a `[[ddobuilder_correction]]` (with `kind`, `field`, `upstream` holding his value, `reason`, `url`, `read`) where the page shows his value is wrong and ours should differ. In ddo-tools the `wiki-reader` agent reads item pages and `wiki-reader-tables` reads table-shaped pages; `maetrim-update` dispatches them from the report. Rebuild after their edits; the build names any file that does not validate.

## 7. Commit and land

Two commits: the import (`Import DDOBuilder <short sha> into the data files`, body: the report's counts and every line that needed a decision, with what was decided) and the wiki review (what was read, what was recorded). Then from the main checkout: `git merge --ff-only import-<sha>`, push, and the deploy workflow builds the API from `data/`. Remove the worktree and the branch.

## When his build fails

His XML is built with the wiki facts and corrections the data files hold. In his new build (B), a wiki fact naming something he removed stops the import at "building the new DataFiles" and names the data file holding the fact. Settle it in that file: remove or re-point the fact, and run again. In his previous build (A), such a fact is only left out and listed as `SKIPPED WIKI FACT`, because his previous files predate names the data files already use. A `missing image files` stop means his XML names an image his folders lack: add an icon-null correction like the ones `docs/data-icons.md` lists.
