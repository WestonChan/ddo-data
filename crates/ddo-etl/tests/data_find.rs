use std::process::Command;

#[test]
fn find_reads_names_slugs_and_variants_without_a_database() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/data");
    for (query, expected, count) in [
        ("BOOK BURN", "quest  Book Burning  packs/magic-of-myth-drannor/quests/book-burning/quest.toml", None),
        ("Book BurninX", "quest  Book Burning  packs/magic-of-myth-drannor/quests/book-burning/quest.toml", Some(5)),
        (
            "5-fortitude-save-negative-opposition-ethereal",
            "augments/5-fortitude-save-negative-opposition-ethereal.toml",
            Some(1),
        ),
    ] {
        let output =
            Command::new(env!("CARGO_BIN_EXE_ddo-etl")).args(["find", query, "--data"]).arg(&root).output().unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        let text = String::from_utf8(output.stdout).unwrap();
        assert!(text.contains(expected), "{text}");
        if let Some(count) = count {
            assert_eq!(text.lines().count(), count, "{text}");
        }
    }
}
