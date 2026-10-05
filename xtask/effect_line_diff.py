import argparse
import collections
import difflib
import json
import sqlite3


def detail_rows(path):
    rows = {}
    with open(path, encoding="utf-8") as source:
        for record in source:
            owner_kind, owner_id, payload = record.rstrip("\n").split("\t", 2)
            rows[(owner_kind, owner_id)] = json.loads(payload)
    return rows


def effect_lines(detail):
    yield "", detail.get("effects", [])
    for tier in detail.get("tiers", []):
        yield str(tier["equipped_count"]), tier.get("effects", [])


def paired_lines(old_lines, new_lines):
    matches = difflib.SequenceMatcher(
        a=[line["name"] for line in old_lines],
        b=[line["name"] for line in new_lines],
        autojunk=False,
    )
    for operation, old_start, old_end, new_start, new_end in matches.get_opcodes():
        if operation == "equal":
            for offset in range(old_end - old_start):
                yield old_lines[old_start + offset], new_start + offset, new_lines[new_start + offset]
        elif operation == "replace":
            paired_count = min(old_end - old_start, new_end - new_start)
            for offset in range(paired_count):
                yield old_lines[old_start + offset], new_start + offset, new_lines[new_start + offset]
            for offset in range(paired_count, old_end - old_start):
                yield old_lines[old_start + offset], None, None
            for offset in range(paired_count, new_end - new_start):
                yield None, new_start + offset, new_lines[new_start + offset]
        elif operation == "delete":
            for old_line in old_lines[old_start:old_end]:
                yield old_line, None, None
        elif operation == "insert":
            for new_index in range(new_start, new_end):
                yield None, new_index, new_lines[new_index]


def comparison(before_path, after_path, db_path):
    before = detail_rows(before_path)
    after = detail_rows(after_path)
    with sqlite3.connect(db_path) as db:
        effect_names = dict(db.execute("SELECT id, name FROM effects"))
    counts = collections.Counter()
    differences = []
    for owner_key, old_detail in before.items():
        if owner_key not in after:
            counts["missing-owner"] += 1
            continue
        old_groups = dict(effect_lines(old_detail))
        new_groups = dict(effect_lines(after[owner_key]))
        for tier in old_groups.keys() | new_groups.keys():
            for old_line, line_index, new_line in paired_lines(old_groups.get(tier, []), new_groups.get(tier, [])):
                if old_line is None:
                    counts["added-line"] += 1
                    continue
                if new_line is None:
                    counts["removed-line"] += 1
                    continue
                report_key = (*owner_key, tier, line_index)
                effect_name = effect_names.get(new_line["effect_id"])
                if old_line["name"] != new_line["name"]:
                    counts["renamed-line"] += 1
                    differences.append((*report_key, "name", old_line["name"], new_line["name"]))
                if new_line["name"] != effect_name:
                    counts["qualified-name"] += 1
                    differences.append((*report_key, "qualified-name", effect_name, new_line["name"]))
                if old_line.get("text") != new_line.get("verbose_name"):
                    counts["rendered-line"] += 1
                    differences.append(
                        (*report_key, "verbose_name", old_line.get("text"), new_line.get("verbose_name"))
                    )
                if old_line.get("description") != new_line.get("description"):
                    category = "moved-paragraph" if old_line.get("description") is None else "changed-description"
                    counts[category] += 1
                    differences.append(
                        (*report_key, category, old_line.get("description"), new_line.get("description"))
                    )
    return counts, differences


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("before")
    parser.add_argument("after")
    parser.add_argument("--db", required=True)
    parser.add_argument("--out", required=True)
    arguments = parser.parse_args()
    counts, differences = comparison(arguments.before, arguments.after, arguments.db)
    with open(arguments.out, "w", encoding="utf-8") as report:
        report.write("owner_kind\towner_id\ttier\tline_index\tcategory\tbefore\tafter\n")
        for difference in differences:
            report.write("\t".join(json.dumps(field, ensure_ascii=False) for field in difference) + "\n")
    print(json.dumps(dict(sorted(counts.items())), indent=2))


if __name__ == "__main__":
    main()
