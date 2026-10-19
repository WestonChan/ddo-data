mod comments;

use anyhow::{bail, ensure, Context, Result};
use clap::{Parser, Subcommand};
use comments::{comments_in, without_comments};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;
use xtask::api_validation::validate_api;
use xtask::golden_refresh::{golden_merge_changes, golden_sample, merge_golden, SampleRequest};
use xtask::integrity::{integrity_report, IntegrityOptions};
use xtask::response_examples::{write_response_examples, write_v1_detail_snapshot, EXAMPLE_REQUESTS};
use xtask::wiki_tooltips::check_wiki_tooltips;
use xtask::workspace_root;

#[derive(Parser)]
struct Cli {
    #[command(subcommand)]
    command: Task,
}

#[derive(Subcommand)]
enum Task {
    Lint,
    NoComments {
        #[arg(long)]
        fix: bool,
    },
    RefreshExamples {
        #[arg(long)]
        db: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    SnapshotV1Details {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        out: PathBuf,
    },
    ValidateApi {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        all_details: bool,
    },
    CheckRedirects {
        #[arg(long)]
        db: PathBuf,
        #[arg(long)]
        deployed: Option<PathBuf>,
        #[arg(long)]
        today: Option<String>,
    },
    DiffDb {
        first: PathBuf,
        second: PathBuf,
        #[arg(long)]
        by_slug: bool,
        #[arg(
            long,
            help = "Crawl this API using its /v1/dump.sqlite identities (fetched and cached under target/ by dataset SHA); the first database remains the table baseline"
        )]
        against_api: Option<String>,
    },
    RoundTrip {
        #[arg(long = "data")]
        data_dir: PathBuf,
    },
    WikiTooltips {
        #[arg(long)]
        db: PathBuf,
    },
    #[command(
        about = "Write a browser reading list of served effect lines",
        long_about = "Choose modern owners that add missing or stale golden coverage, or name owners and effects for a targeted browser re-read. The command reads a built database and writes target/tooltip_sample.json by default; it never fetches the wiki."
    )]
    GoldenSample {
        #[arg(long, help = "Built database to sample")]
        db: PathBuf,
        #[arg(long, default_value_t = 20, help = "Maximum number of automatically selected owners")]
        count: usize,
        #[arg(long, default_value_t = 90, help = "Skip ordinary samples read within this many days")]
        fresh_days: i64,
        #[arg(long = "item", help = "Include an exact item name; repeatable")]
        items: Vec<String>,
        #[arg(long = "augment", help = "Include an exact augment name; repeatable")]
        augments: Vec<String>,
        #[arg(long = "set", help = "Include an exact set name; repeatable")]
        sets: Vec<String>,
        #[arg(long = "effect", help = "Sample modern owners carrying an exact effect name; repeatable")]
        effects: Vec<String>,
        #[arg(
            long = "effect-like",
            help = "Sample modern owners carrying an effect matching a SQL LIKE pattern; repeatable"
        )]
        effect_likes: Vec<String>,
        #[arg(long, help = "Write the reading list to this JSON path")]
        out: Option<PathBuf>,
    },
    #[command(
        about = "Merge a browser reader's golden entries",
        long_about = "Validate browser-read golden entries and replace matching owners in xtask/data/tooltip_golden.json. A re-read replaces its entire owner entry, so old recorded differences are dropped unless the input still records them."
    )]
    GoldenMerge {
        #[arg(help = "JSON file containing items, augments or sets in golden entry shape")]
        file: PathBuf,
    },
    #[command(about = "Check database integrity")]
    CheckDb {
        db: PathBuf,
        #[arg(long = "allow-empty-table")]
        allowed_empty_tables: Vec<String>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Task::Lint => {
            xtask::data_file_lints::deny_generic_file_mappers(&workspace_root().join("crates/ddo-etl/src/tree"))?;
            xtask::data_file_lints::deny_annotation_table_literals(&workspace_root().join("crates/ddo-etl/src"))?;
            xtask::data_file_lints::deny_citation_field_reads(&[
                &workspace_root().join("crates/ddo-etl/src"),
                &workspace_root().join("xtask/src"),
            ])?;
            xtask::data_file_lints::deny_unlisted_records(&workspace_root().join("crates/ddo-etl/src/tree"))?;
            deny_clippy_warnings()?;
            ddo_etl::tree::schemas::validate_source_fields(&workspace_root().join("data"))?;
            ddo_etl::tree::schemas::validate_column_coverage()?;
            deny_invalid_data_files()?;
            deny_data_schema_drift()?;
            deny_contract_or_scale_regressions()?;
            deny_comments()
        }
        Task::NoComments { fix: false } => deny_comments(),
        Task::NoComments { fix: true } => strip_workspace_comments(),
        Task::RefreshExamples { db: db_path, out: examples_dir } => {
            refresh_response_examples(db_path.as_deref(), examples_dir)
        }
        Task::SnapshotV1Details { db, out } => {
            println!("wrote {} v1 details to {}", write_v1_detail_snapshot(&db, &out)?, out.display());
            Ok(())
        }
        Task::ValidateApi { db, all_details } => {
            let (routes, responses) = validate_api(&db, all_details)?;
            println!("validated {responses} real responses across {routes} JSON routes");
            Ok(())
        }
        Task::CheckRedirects { db, deployed, today } => {
            let flags = rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY;
            let db = rusqlite::Connection::open_with_flags(db, flags)?;
            let deployed = deployed.map(|path| rusqlite::Connection::open_with_flags(path, flags)).transpose()?;
            let violations =
                xtask::redirect_checks::redirect_check_violations(&db, deployed.as_ref(), today.as_deref())?;
            ensure!(violations.is_empty(), "{}", violations.join("\n"));
            println!("redirect dates and deployed slugs: ok");
            Ok(())
        }
        Task::DiffDb { first, second, by_slug, against_api } => {
            let report = if by_slug {
                xtask::diff_db::diff_databases_by_slug_against_api(&first, &second, against_api.as_deref())?
            } else {
                xtask::diff_db::diff_databases_against_api(&first, &second, against_api.as_deref())?
            };
            println!("{report}");
            ensure!(report.is_equal(), "database differences found");
            Ok(())
        }
        Task::RoundTrip { data_dir } => xtask::round_trip::verify_round_trip(&data_dir),
        Task::WikiTooltips { db } => {
            let report = check_wiki_tooltips(&db)?;
            println!("{report}");
            Ok(())
        }
        Task::GoldenSample { db, count, fresh_days, items, augments, sets, effects, effect_likes, out } => {
            let sample =
                golden_sample(&db, &SampleRequest { count, fresh_days, items, augments, sets, effects, effect_likes })?;
            let out = out.unwrap_or_else(|| workspace_root().join("target/tooltip_sample.json"));
            if let Some(parent) = out.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(&out, format!("{}\n", serde_json::to_string_pretty(&sample)?))?;
            println!(
                "wrote {} items, {} augments and {} sets to {}",
                sample["items"].as_array().unwrap().len(),
                sample["augments"].as_array().unwrap().len(),
                sample["sets"].as_array().unwrap().len(),
                out.display()
            );
            Ok(())
        }
        Task::GoldenMerge { file } => {
            let golden_path = workspace_root().join("xtask/data/tooltip_golden.json");
            let old: serde_json::Value = serde_json::from_slice(&std::fs::read(&golden_path)?)?;
            let incoming: serde_json::Value = serde_json::from_slice(&std::fs::read(&file)?)?;
            let changes = golden_merge_changes(&old, &incoming)?;
            let merged = merge_golden(old.clone(), incoming)?;
            std::fs::write(&golden_path, format!("{}\n", serde_json::to_string_pretty(&merged)?))?;
            println!("merged {} into {}\n{}", file.display(), golden_path.display(), changes.join("\n"));
            Ok(())
        }
        Task::CheckDb { db: db_path, allowed_empty_tables } => check_database_integrity(&db_path, allowed_empty_tables),
    }
}

fn deny_invalid_data_files() -> Result<()> {
    let data_root = workspace_root().join("data");
    for entry in WalkDir::new(&data_root) {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type().is_file() || path.extension().is_none_or(|extension| extension != "toml") {
            continue;
        }
        for (index, line) in std::fs::read_to_string(path)?.lines().enumerate() {
            if let Some((field, _)) = line.split_once(" = ") {
                let field = field.trim();
                ensure!(
                    !field.ends_with("_id")
                        && !matches!(
                            field,
                            "id" | "sort_order" | "owner_kind" | "source_kind" | "table" | "provenance"
                        ),
                    "{}:{}: data field {field} belongs in a name relation or folder",
                    path.display(),
                    index + 1
                );
            }
        }
    }
    let mut db = rusqlite::Connection::open_in_memory()?;
    ddo_etl::tree::build_from_data(&data_root, &mut db)?;
    let violations = xtask::redirect_checks::redirect_check_violations(&db, None, None)?;
    ensure!(violations.is_empty(), "{}", violations.join("\n"));
    Ok(())
}

fn deny_data_schema_drift() -> Result<()> {
    for (folder, expected_files) in ddo_etl::tree::schemas::generated_folder_contents()? {
        let folder_root = workspace_root().join("data").join(folder);
        for entry in std::fs::read_dir(&folder_root).with_context(|| format!("reading {}", folder_root.display()))? {
            let path = entry?.path();
            let file_name = path.file_name().and_then(|name| name.to_str()).context("generated name is not UTF-8")?;
            ensure!(expected_files.contains_key(file_name), "{} is obsolete; run ddo-etl export", path.display());
        }
        for (file_name, expected) in expected_files {
            let path = folder_root.join(file_name);
            let actual = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
            ensure!(actual == expected, "{} is stale; run ddo-etl export", path.display());
        }
    }
    ddo_etl::tree::schemas::validate_taplo_config(&workspace_root())?;
    Ok(())
}

fn refresh_response_examples(db_path: Option<&Path>, examples_dir: Option<PathBuf>) -> Result<()> {
    let examples_dir = examples_dir.unwrap_or_else(|| workspace_root().join("crates/ddo-api/docs/examples/v1"));
    let built_db_file;
    let db_path = match db_path {
        Some(db_path) => db_path,
        None => {
            built_db_file = tempfile::NamedTempFile::new()?;
            let data_dir = workspace_root().join("data");
            println!("building {} into {}", data_dir.display(), built_db_file.path().display());
            let mut db = rusqlite::Connection::open(built_db_file.path())?;
            ddo_etl::tree::build_from_data(&data_dir, &mut db)?;
            built_db_file.path()
        }
    };
    for written_example in write_response_examples(db_path, EXAMPLE_REQUESTS, &examples_dir)? {
        println!("{:<28} {:>6} bytes", written_example.file_name, written_example.size_bytes);
    }
    Ok(())
}

fn check_database_integrity(db_path: &Path, allowed_empty_tables: Vec<String>) -> Result<()> {
    let db = rusqlite::Connection::open_with_flags(db_path, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)
        .with_context(|| format!("opening {}", db_path.display()))?;
    let options = IntegrityOptions { allowed_empty_tables };
    let report = integrity_report(&db, &options)?;
    println!("{report}");
    let failed_check_names = report.failed_hard_check_names();
    if !failed_check_names.is_empty() {
        bail!("{} HARD integrity check(s) failed: {}", failed_check_names.len(), failed_check_names.join(", "));
    }
    Ok(())
}

fn deny_clippy_warnings() -> Result<()> {
    let clippy_status = Command::new("cargo")
        .args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
        .status()
        .context("running cargo clippy")?;
    if !clippy_status.success() {
        bail!("clippy failed");
    }
    Ok(())
}

fn deny_contract_or_scale_regressions() -> Result<()> {
    for (crate_name, test_target, test_name) in [
        ("ddo-etl", &["--lib"][..], "tree::build::file_name_tests::pseudo_pack_names_cannot_be_real_packs"),
        (
            "ddo-etl",
            &["--lib"][..],
            "tree::build::file_name_tests::pack_release_prefixes_preserve_identity_and_reject_mismatched_folders",
        ),
        (
            "ddo-etl",
            &["--lib"][..],
            "tree::layout::tests::filigree_sets_keep_all_members_together_and_resolve_existing_aliases",
        ),
        ("ddo-etl", &["--lib"][..], "tree::layout::tests::source_records_and_rewards_share_their_resolved_folders"),
        ("ddo-api", &["--test", "api"][..], "list_parameter_docs_share_descriptions_and_match_runtime_sort_fields"),
        ("ddo-api", &["--test", "api"][..], "item_pack_query_count_does_not_grow_with_rows_or_sources"),
        ("ddo-api", &["--test", "api"][..], "item_full_page_query_count_does_not_grow_with_rows"),
        ("ddo-api", &["--test", "api"][..], "loot_source_references_match_entity_routes"),
        ("ddo-api", &["--test", "api"][..], "loot_source_schemas_share_required_nullable_references"),
        ("ddo-api", &["--test", "api"][..], "feat_bonus_references_match_the_granting_effect"),
        ("ddo-api", &["--test", "api"][..], "typed_response_schemas_resolve_every_reference_and_describe_fields"),
        ("ddo-api", &["--test", "api"][..], "every_json_route_response_matches_its_openapi_schema"),
        ("ddo-etl", &["--lib"][..], "tree::tests::complete_export_query_count_does_not_grow_with_entities"),
        ("ddo-etl", &["--lib"][..], "tree::schemas::tests::every_entity_record_declares_citations"),
        (
            "ddo-etl",
            &["--lib"][..],
            "tree::kinds::enhancement_tree::tests::every_enhancement_column_except_identity_is_correctable",
        ),
        ("ddo-etl", &["--test", "import_ddobuilder"][..], "every_declared_enhancement_field_agrees_between_builders"),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "augment_family_rename_preserves_the_upstream_correction_name",
        ),
        ("ddo-etl", &["--test", "import_ddobuilder"][..], "annotation_sources_and_correction_kinds_are_typed"),
        ("ddo-model", &["--test", "schema"][..], "annotations_require_exactly_one_page_or_in_game_source"),
        (
            "ddo-etl",
            &["--test", "corrections"][..],
            "correctable_fields_pin_each_kinds_names_columns_shapes_and_nullability",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "every_entity_preserves_citations_with_notes_through_the_data_tree",
        ),
        ("ddo-etl", &["--lib"][..], "tree::schemas::tests::item_and_augment_sources_use_singular_tables"),
        ("ddo-etl", &["--lib"][..], "tree::schemas::tests::pack_local_entity_names_do_not_override_folder_kinds"),
        ("ddo-etl", &["--lib"][..], "tree::schemas::tests::effect_names_cannot_take_reserved_vocabulary_filenames"),
        ("ddo-etl", &["--lib"][..], "tree::schemas::catalog::tests::catalog_names_and_slugs_match_the_real_export"),
        ("ddo-etl", &["--test", "data_find"][..], "find_reads_names_slugs_and_variants_without_a_database"),
        ("ddo-etl", &["--lib"][..], "tree::schemas::tests::data_paths_share_reader_editor_and_icon_kinds"),
        (
            "ddo-etl",
            &["--lib"][..],
            "tree::build::file_name_tests::reserved_record_names_always_use_the_parent_identity",
        ),
        (
            "ddo-etl",
            &["--lib"][..],
            "tree::build::file_name_tests::pack_local_file_names_cannot_borrow_the_kind_folder_identity",
        ),
        ("ddo-etl", &["--lib"][..], "tree::pack_layout::tests::every_member_must_resolve_to_one_pack"),
        (
            "ddo-etl",
            &["--lib"][..],
            "tree::pack_layout::tests::pack_derivation_uses_real_sources_without_following_derived_owners",
        ),
        ("ddo-etl", &["--lib"][..], "tree::pack_layout::tests::unsourced_versions_use_only_unanimous_sourced_siblings"),
        ("ddo-etl", &["--lib"][..], "tree::pack_layout::tests::crafting_recipe_outputs_participate_in_pack_derivation"),
        (
            "ddo-etl",
            &["--lib"][..],
            "icons::icon_index_tests::copied_icons_preserve_upstream_case_and_report_mismatched_references",
        ),
        ("ddo-etl", &["--lib"][..], "delta::tests::every_table_has_one_merge_rule"),
        ("ddo-etl", &["--lib"][..], "delta::merge_tests::unique_positions_follow_entity_or_array_merge_policy"),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "exported_ddobuilder_tree_builds_with_dc_icons_from_other_kinds",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "canonical_fixture_reproduces_all_tables_with_xml_citations_a_subset_of_ours",
        ),
        ("ddo-etl", &["--test", "import_ddobuilder"][..], "source_citations_do_not_change_drop_provenance"),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "overlay_references_follow_only_renames_active_in_each_xml_build",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "merged_socket_labels_follow_the_writer_and_have_no_import_delta",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "separate_corrections_with_one_target_fail_naming_both_on_every_kind",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "database_overlay_preserves_authored_array_order_and_effect_titles",
        ),
        ("ddo-etl", &["--test", "import_ddobuilder"][..], "overlay_catalog_identity_survives_insertion_order"),
        (
            "ddo-etl",
            &["--lib"][..],
            "tree::xml_slugs::overlay_identity_tests::overlay_identity_uses_catalog_slugs_including_variants",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "previous_xml_skips_later_named_wiki_facts_but_new_xml_stays_strict",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "canonical_wiki_rows_do_not_appear_in_an_unrelated_import_delta",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "import_reports_mapping_audits_without_changing_success_status",
        ),
        ("xtask", &["--test", "integrity"][..], "database_checks_need_no_xml_or_data_files"),
        ("ddo-etl", &["--test", "upstream"][..], "served_dataset_commands_expose_only_canonical_inputs"),
        ("xtask", &["--test", "diff_db"][..], "database_comparison_exposes_no_difference_allowance"),
        ("ddo-etl", &["--test", "retired_inputs"][..], "retired_inputs_and_tools_are_gone_from_the_workspace"),
        (
            "ddo-etl",
            &["--test", "data_rejections"][..],
            "structural_rejections_fail_the_data_build_naming_their_file_once",
        ),
        ("ddo-etl", &["--test", "data_rejections"][..], "a_citation_field_must_be_empty_or_a_field_of_its_record"),
        ("ddo-etl", &["--test", "templates"][..], "every_template_builds_under_its_placement_and_exports_byte_equal"),
        ("ddo-etl", &["--test", "import_ddobuilder"][..], "import_keeps_the_generated_templates"),
        (
            "ddo-etl",
            &["--test", "data_rejections"][..],
            "a_file_named_off_its_slug_fails_the_build_naming_the_expected_slug",
        ),
        ("ddo-etl", &["--test", "data_rejections"][..], "an_unresolved_reference_fails_the_build_naming_its_file"),
        ("ddo-etl", &["--lib"][..], "tree::schemas::tests::a_ddl_column_no_declaration_writes_is_reported"),
        ("ddo-etl", &["--lib"][..], "tree::records::tests::an_undeclared_use_of_a_parameterized_record_is_rejected"),
        ("ddo-etl", &["--test", "import_ddobuilder"][..], "his_blank_never_erases_ours_and_the_import_says_so"),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "his_added_description_applies_where_ours_had_filled_a_blank",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "merged_socket_labels_resolve_when_his_files_adopt_the_name_for_every_spelling",
        ),
        (
            "ddo-etl",
            &["--test", "import_ddobuilder"][..],
            "a_merge_is_not_resolved_while_one_listed_spelling_remains_in_his_files",
        ),
        ("ddo-etl", &["--test", "corrections"][..], "a_socket_label_merge_renames_every_listed_spelling_into_one_row"),
    ] {
        let test_status = Command::new("cargo")
            .args(["test", "-p", crate_name])
            .args(test_target)
            .args([test_name, "--", "--exact"])
            .status()
            .with_context(|| format!("checking contract or scale guard: {test_name}"))?;
        if !test_status.success() {
            bail!("contract or scale guard failed: {test_name}");
        }
    }
    Ok(())
}

fn deny_comments() -> Result<()> {
    let mut comment_count = 0;
    for path in rust_source_paths(workspace_root()) {
        let source_code = std::fs::read_to_string(&path)?;
        for comment in comments_in(&source_code) {
            println!("{}:{}: comment not allowed", path.display(), comment.line_number);
            comment_count += 1;
        }
    }
    if comment_count > 0 {
        bail!("{comment_count} comment(s) found; run `cargo xtask no-comments --fix`");
    }
    Ok(())
}

fn strip_workspace_comments() -> Result<()> {
    for path in rust_source_paths(workspace_root()) {
        let source_code = std::fs::read_to_string(&path)?;
        let comment_count = comments_in(&source_code).len();
        if comment_count == 0 {
            continue;
        }
        std::fs::write(&path, without_comments(&source_code))?;
        println!("stripped {comment_count} comment(s) from {}", path.display());
    }
    Ok(())
}

fn rust_source_paths(search_root: PathBuf) -> impl Iterator<Item = PathBuf> {
    WalkDir::new(search_root)
        .into_iter()
        .filter_entry(|entry| !matches!(entry.file_name().to_str(), Some("target" | "upstream" | ".git")))
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "rs"))
        .map(|entry| entry.into_path())
}
