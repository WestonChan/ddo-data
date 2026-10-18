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

The report is the pull-request body of the future automation, so it is complete on its own: a header with both shas, a counts table (one row per table: added, removed, changed, conflicts, declined removals, resolved notes), then one line per item that needs a person, each starting with the data-relative file:

- `CONFLICT`: he and we both changed the field (or the array, or the correction). Ours stays. The line shows his previous value, his new value and ours. Decide: keep ours (nothing to do), or take his (edit the file to his value and drop the `[[note]]` if one carried ours).
- `DECLINED REMOVAL`: he removed a row we annotated, edited or still reference. The row stays and the line says why. Keep it, or remove it by hand together with what referenced it.
- `RESOLVED NOTE`: he moved to the value our `[[note]]` holds. The note has done its job; delete it so the file no longer records a difference that is gone.
- `NEEDS RESEARCH`: an adventure pack he added has no `update` or `released`. It landed in a folder named only by its slug. Read [Modules and updates](https://ddowiki.com/page/Modules_and_updates), add `update`, `released` and a `[[citation]]` to its `pack.toml`, then rebuild through the database (`build --data data --out ddo.db`, `export --db ddo.db --out data`) so the folder gets its update tag.
- `ICON CONFLICT` / `ICON DECLINED`: both sides changed an image's bytes, or he removed one we still reference. Ours stays.
- `CHANGED UPSTREAM, ABSENT IN OURS`: he changed an entity we deleted. Nothing was applied; the line is for the record.

Every edit after the import is an ordinary data-file edit: change the TOML, add the `[[note]]` or `[[citation]]` that records why, rebuild.

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

`check-redirects` names every slug the live API serves that this build no longer has: his renames and deletions. Answer each in `data/redirects.toml` before landing, or the deploy stops on it. A rename is a `[[redirect]]` row (`kind`, `from` the old slug, `to` the new one, `since` today); a deletion is a `[[removed]]` row (`kind`, `slug`, `since`, `reason`). A renamed or removed enhancement tree or crafting system covers its selections or ingredients. If the build instead fails because a redirect's `to` no longer exists (he deleted the target), retarget the row or turn it into a `[[removed]]` row.

## 6. Review his changes against the wiki

`git diff --stat main -- data` and the report's counts say which entities changed. For each changed or added item, set, feat, spell, enhancement or class, a reader checks the ddowiki page and records what it finds in the entity's own file: a `[[citation]]` for a fact the page confirms or adds, a `[[note]]` (with `kind`, `field`, `upstream` holding his value, `reason`, `url`, `read`) where the page shows his value is wrong and ours should differ. In ddo-tools the `wiki-reader` agent reads item pages and `wiki-reader-tables` reads table-shaped pages; `maetrim-update` dispatches them from the report. Rebuild after their edits; the build names any file that does not validate.

## 7. Commit and land

Two commits: the import (`Import DDOBuilder <short sha> into the data files`, body: the report's counts and every line that needed a decision, with what was decided) and the wiki review (what was read, what was recorded). Then from the main checkout: `git merge --ff-only import-<sha>`, push, and the deploy workflow builds the API from `data/`. Remove the worktree and the branch.

## When his build fails

Until V9 chunk 2 removes them, the embedded `crates/ddo-etl/data/corrections/` and `crates/ddo-etl/data/wiki/` folders are still inputs to his XML build. An entry there naming something he removed stops the import at "building the new DataFiles". Delete that embedded entry (the data files keep our note, so the removal is declined and reported if we annotated it) and run again. A `missing image files` stop means his XML names an image his folders lack: add an icon-null correction like the ones `docs/data-icons.md` lists.
