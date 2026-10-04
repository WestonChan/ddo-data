import argparse
import collections
import difflib
import json
import sqlite3


OWNER_TABLES = {
    "items": ("item_effects", "item_id"),
    "augments": ("augment_effects", "augment_id"),
    "feats": ("feat_effects", "feat_id"),
    "sets": ("set_bonus_tier_effects", "tier_id"),
    "options": ("item_augment_slot_option_effects", "option_id"),
}

LEGACY_OWNER_TABLES = {
    "items": ("item_bonuses", "item_id"),
    "augments": ("augment_bonuses", "augment_id"),
    "feats": ("feat_bonuses", "feat_id"),
    "sets": ("set_bonus_tier_bonuses", "tier_id"),
    "options": ("item_augment_slot_option_bonuses", "option_id"),
}


def has_effect_bonuses(db):
    return db.execute("SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'effect_bonuses'").fetchone() is not None


def read_details(path):
    details = {}
    with open(path, encoding="utf-8") as lines:
        for line in lines:
            kind, owner_id, payload = line.split("\t", 2)
            details[(kind, int(owner_id))] = json.loads(payload)
    return details


def family_names(db, owner_kind, owner_id, legacy=False):
    if legacy:
        table, owner_column = LEGACY_OWNER_TABLES[owner_kind]
        query = f"""SELECT stat.name FROM {table} link
            JOIN bonuses bonus ON bonus.id = link.bonus_id
            JOIN stats stat ON stat.id = bonus.stat_id
            WHERE link.{owner_column} = ? ORDER BY link.sort_order"""
        return [row[0] for row in db.execute(query, (owner_id,))]
    table, owner_column = OWNER_TABLES[owner_kind]
    query = f"""SELECT e.name FROM {table} link
        JOIN effects e ON e.id = link.effect_id
        JOIN effect_bonuses stat ON stat.effect_id = e.id
        WHERE link.{owner_column} = ? AND
          (CASE stat.amount_from WHEN 0 THEN stat.constant
            WHEN 1 THEN COALESCE(link.value, e.default_value)
            ELSE COALESCE(link.value2, e.default_value2) END) IS NOT NULL
        ORDER BY link.sort_order, stat.sort_order"""
    return [row[0] for row in db.execute(query, (owner_id,))]


def changed_tier_families(before_db, after_db, tier_id, before_legacy):
    current_query = """SELECT e.name, link.value, link.value2, link.bonus_type_id
        FROM set_bonus_tier_effects link JOIN effects e ON e.id = link.effect_id
        WHERE link.tier_id = ?"""
    legacy_query = """SELECT stat.name, bonus.value, bonus.value2, bonus.bonus_type_id
        FROM set_bonus_tier_bonuses link JOIN bonuses bonus ON bonus.id = link.bonus_id
        JOIN stats stat ON stat.id = bonus.stat_id WHERE link.tier_id = ?"""
    previous_query = legacy_query if before_legacy else current_query
    previous = set(before_db.execute(previous_query, (tier_id,)))
    current = set(after_db.execute(current_query, (tier_id,)))
    names = {row[0] for row in previous ^ current}
    if not names:
        names = {row[0] for row in previous | current}
    return sorted(names) or ["(tier prose)"]


def row_key(row, effect):
    if effect:
        return row.get("description") or row.get("name")
    return row.get("stat"), row.get("bonus_type")


def compared_row(row, effect):
    compared = {key: value for key, value in row.items() if key != "id"}
    if effect:
        compared.pop("value", None)
        compared.pop("target", None)
    return compared


def structured_effects(owner):
    previous = owner.get("enchantments")
    if isinstance(previous, list):
        return previous
    current = owner.get("effects")
    if isinstance(current, list) and (not current or all("bonuses" in line for line in current)):
        return current
    return None


def row_changes(before, after, before_families, after_families, owner_label, location, effect, findings):
    if not effect and (len(before) != len(before_families) or len(after) != len(after_families)):
        raise ValueError(f"{owner_label} {location}: bonus rows and family rows differ")
    matcher = difflib.SequenceMatcher(
        None,
        [row_key(row, effect) for row in before],
        [row_key(row, effect) for row in after],
        autojunk=False,
    )
    pairs = []
    for tag, before_start, before_end, after_start, after_end in matcher.get_opcodes():
        if tag == "equal":
            pairs.extend(zip(range(before_start, before_end), range(after_start, after_end)))
        else:
            pairs.extend((old_index, None) for old_index in range(before_start, before_end))
            pairs.extend((None, new_index) for new_index in range(after_start, after_end))
    for old_index, new_index in pairs:
        old = before[old_index] if old_index is not None else None
        new = after[new_index] if new_index is not None else None
        if old is not None and new is not None and compared_row(old, effect) == compared_row(new, effect):
            continue
        family = (
            new.get("name", "(unnamed)") if effect else after_families[new_index]
        ) if new is not None else (old.get("name", "(unnamed)") if effect else before_families[old_index])
        changed_fields = sorted(set(compared_row(old or {}, effect)) | set(compared_row(new or {}, effect)))
        if old is not None and new is not None:
            changed_fields = [field for field in changed_fields if compared_row(old, effect).get(field) != compared_row(new, effect).get(field)]
        findings.append({
            "family": family,
            "owner": owner_label,
            "location": location,
            "fields": changed_fields if old is not None and new is not None else ["added" if new is not None else "removed"],
            "before": old,
            "after": new,
        })


def compare_details(before, after, before_db, after_db):
    if before.keys() != after.keys():
        raise ValueError(f"owner keys differ: {len(before.keys() - after.keys())} removed, {len(after.keys() - before.keys())} added")
    before_legacy = not has_effect_bonuses(before_db)
    findings = []
    for owner_kind, owner_id in sorted(before):
        old, new = before[(owner_kind, owner_id)], after[(owner_kind, owner_id)]
        owner_label = f"{owner_kind}/{owner_id} {new['name']}"
        old_effects, new_effects = structured_effects(old), structured_effects(new)
        if old_effects is not None and new_effects is not None:
            row_changes(old_effects, new_effects, [], [], owner_label, "effects", True, findings)
        elif owner_kind in ("items", "augments", "feats"):
            row_changes(old["bonuses"], new["bonuses"], family_names(before_db, owner_kind, owner_id, before_legacy), family_names(after_db, owner_kind, owner_id), owner_label, "bonuses", False, findings)
        if owner_kind == "items" and old_effects is None:
            row_changes(old["effects"], new["effects"], [], [], owner_label, "effects", True, findings)
        if owner_kind == "items":
            for old_slot, new_slot in zip(old["augment_slots"], new["augment_slots"]):
                for old_option, new_option in zip(old_slot["options"], new_slot["options"]):
                    option_id = new_option["id"]
                    old_option_effects, new_option_effects = structured_effects(old_option), structured_effects(new_option)
                    if old_option_effects is not None and new_option_effects is not None:
                        row_changes(old_option_effects, new_option_effects, [], [], owner_label, f"option/{option_id} effects", True, findings)
                    else:
                        row_changes(old_option["bonuses"], new_option["bonuses"], family_names(before_db, "options", option_id, before_legacy), family_names(after_db, "options", option_id), owner_label, f"option/{option_id} bonuses", False, findings)
        if owner_kind == "sets":
            for old_tier, new_tier in zip(old["tiers"], new["tiers"]):
                tier_id = new_tier["id"]
                old_tier_effects, new_tier_effects = structured_effects(old_tier), structured_effects(new_tier)
                if old_tier_effects is not None and new_tier_effects is not None:
                    row_changes(old_tier_effects, new_tier_effects, [], [], owner_label, f"tier/{new_tier['equipped_count']} effects", True, findings)
                else:
                    row_changes(old_tier["bonuses"], new_tier["bonuses"], family_names(before_db, "sets", tier_id, before_legacy), family_names(after_db, "sets", tier_id), owner_label, f"tier/{new_tier['equipped_count']} bonuses", False, findings)
                if old_tier.get("description") != new_tier.get("description"):
                    for family in changed_tier_families(before_db, after_db, tier_id, before_legacy):
                        findings.append({"family": family, "owner": owner_label, "location": f"tier/{new_tier['equipped_count']} description", "fields": ["description"], "before": old_tier.get("description"), "after": new_tier.get("description")})
        for field in old.keys() | new.keys():
            if field in ("bonuses", "effects", "enchantments", "augment_slots", "tiers"):
                continue
            if old.get(field) != new.get(field):
                findings.append({"family": "(other fields)", "owner": owner_label, "location": field, "fields": [field], "before": old.get(field), "after": new.get(field)})
    return findings


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--before", required=True)
    parser.add_argument("--after", required=True)
    parser.add_argument("--db", required=True)
    parser.add_argument("--before-db")
    parser.add_argument("--out", required=True)
    parser.add_argument("--report")
    args = parser.parse_args()
    with sqlite3.connect(args.db) as after_db, sqlite3.connect(args.before_db or args.db) as before_db:
        findings = compare_details(read_details(args.before), read_details(args.after), before_db, after_db)
    with open(args.out, "w", encoding="utf-8") as output:
        for finding in findings:
            output.write(json.dumps(finding, ensure_ascii=False, sort_keys=True) + "\n")
    if args.report:
        by_family = collections.defaultdict(list)
        for finding in findings:
            by_family[finding["family"]].append(finding)
        with open(args.report, "w", encoding="utf-8") as report:
            report.write(f"# V1 detail changes by family\n\n{len(findings)} changed rows in {len(by_family)} family groups.\n")
            for family, family_findings in sorted(by_family.items()):
                report.write(f"\n## {family} ({len(family_findings)})\n")
                for finding in family_findings:
                    old = json.dumps(finding["before"], ensure_ascii=False, sort_keys=True)
                    new = json.dumps(finding["after"], ensure_ascii=False, sort_keys=True)
                    report.write(
                        f"\n- {finding['owner']} · {finding['location']} · {', '.join(finding['fields'])}: "
                        f"{old} → {new}\n"
                    )
    counts = collections.Counter((finding["family"], finding["location"].split("/")[0]) for finding in findings)
    print(f"{len(findings)} row changes in {len({family for family, _ in counts})} family groups")
    for (family, location), count in sorted(counts.items()):
        print(f"{family}\t{location}\t{count}")


if __name__ == "__main__":
    main()
