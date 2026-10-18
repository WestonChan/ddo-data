use anyhow::{bail, Context};
use ra_ap_rustc_lexer::{tokenize, FrontmatterAllowed, TokenKind};
use std::path::Path;
use walkdir::WalkDir;

pub fn deny_generic_file_mappers(root: &Path) -> anyhow::Result<()> {
    let mut findings = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        if !matches!(path.strip_prefix(root)?.to_str(), Some("schemas.rs" | "schemas/catalog.rs" | "layout.rs")) {
            let mut offset = 0;
            let mut code_tokens = Vec::new();
            for token in tokenize(production, FrontmatterAllowed::No) {
                let end = offset + token.len as usize;
                if !matches!(
                    token.kind,
                    TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }
                ) {
                    code_tokens.push((&production[offset..end], offset));
                }
                offset = end;
            }
            for window in code_tokens.windows(4) {
                if window[0].0 == "toml"
                    && window[1].0 == ":"
                    && window[2].0 == ":"
                    && matches!(window[3].0, "Value" | "Table")
                {
                    let line = production[..window[0].1].matches('\n').count() + 1;
                    findings.push(format!(
                        "{}:{line}: untyped TOML belongs at the file input boundary; use the declared record",
                        path.display()
                    ));
                }
            }
        }
        let mut token_start = 0;
        for token in tokenize(&source, FrontmatterAllowed::No) {
            let token_end = token_start + token.len as usize;
            let identifier = &source[token_start..token_end];
            if token_start < production.len() && matches!(token.kind, TokenKind::Literal { .. }) {
                let sql = identifier.to_ascii_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
                for table in [
                    "pack_exceptions",
                    "corrections",
                    "citations",
                    "removed_entities",
                    "entity_notes",
                    "entity_identity",
                    "redirects",
                    "removed_slugs",
                ] {
                    let inserted = sql.contains(&format!("insert into {table} ("));
                    let selected = sql
                        .split_once("select ")
                        .and_then(|(_, query)| query.split_once(&format!(" from {table}")))
                        .is_some_and(|(columns, _)| columns.contains(',') && !columns.contains("{}"));
                    if inserted || selected {
                        let line = source[..token_start].matches('\n').count() + 1;
                        findings.push(format!(
                            "{}:{line}: {table} columns must come from database_record!",
                            path.display()
                        ));
                    }
                }
            }
            if matches!(token.kind, TokenKind::Ident | TokenKind::RawIdent)
                && (matches!(
                    identifier.trim_start_matches("r#"),
                    "HydratedEntity"
                        | "DIRECT_CHILD_TABLES"
                        | "NAMED_CHILD_REFERENCES"
                        | "TreeRowFields"
                        | "insert_tree_row"
                        | "hydrate_entity"
                        | "export_database_from_metadata"
                        | "export_database_with_wiki_overrides"
                ) || (identifier.len() > 3
                    && identifier.ends_with("Row")
                    && identifier.chars().next().is_some_and(char::is_uppercase)))
            {
                let line = source[..token_start].matches('\n').count() + 1;
                findings.push(format!(
                    "{}:{line}: retired data-file identifier {identifier}; use a domain type and its declaration",
                    path.display()
                ));
            }
            token_start = token_end;
        }
    }
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn annotation_columns_must_come_from_declarations() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("annotations.rs");
        for table in [
            "pack_exceptions",
            "corrections",
            "citations",
            "removed_entities",
            "entity_notes",
            "entity_identity",
            "redirects",
            "removed_slugs",
        ] {
            for sql in [
                format!("INSERT INTO {table} (name, field) VALUES (?1, ?2)"),
                format!("SELECT name, field FROM {table}"),
            ] {
                std::fs::write(&path, format!("fn insert_annotation() {{ db.prepare({sql:?}); }}")).unwrap();
                let error = deny_generic_file_mappers(folder.path()).unwrap_err().to_string();
                assert!(error.contains(table) && error.contains("annotations.rs:1"), "{error}");
            }
        }
    }

    #[test]
    fn retired_mapper_identifiers_fail_with_file_and_line() {
        let folder = tempfile::tempdir().unwrap();
        for identifier in [
            "HydratedEntity",
            "ModifierRow",
            "FeatRowRow",
            "DIRECT_CHILD_TABLES",
            "NAMED_CHILD_REFERENCES",
            "TreeRowFields",
            "insert_tree_row",
            "hydrate_entity",
            "export_database_from_metadata",
            "export_database_with_wiki_overrides",
        ] {
            let path = folder.path().join("mapping.rs");
            std::fs::write(&path, format!("\nstruct {identifier};\n")).unwrap();
            let error = deny_generic_file_mappers(folder.path()).unwrap_err().to_string();
            assert!(error.contains("mapping.rs:2"), "{error}");
            assert!(error.contains(identifier), "{error}");
        }
    }

    #[test]
    fn untyped_catalog_reading_stays_at_the_schema_boundary() {
        let folder = tempfile::tempdir().unwrap();
        let boundary = folder.path().join("schemas/catalog.rs");
        std::fs::create_dir_all(boundary.parent().unwrap()).unwrap();
        let source = "fn inspect_file(document: toml::Value) {}\n";
        std::fs::write(&boundary, source).unwrap();
        deny_generic_file_mappers(folder.path()).unwrap();
        std::fs::write(folder.path().join("catalog.rs"), source).unwrap();
        assert!(deny_generic_file_mappers(folder.path()).unwrap_err().to_string().contains("catalog.rs:1"));
    }

    #[test]
    fn renamed_untyped_export_mapping_fails() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("export.rs");
        std::fs::write(&path, "fn renamed_mapper() { let fields = toml::Value::try_from(record); }\n").unwrap();
        assert!(deny_generic_file_mappers(folder.path()).unwrap_err().to_string().contains("export.rs:1"));
    }

    #[test]
    fn retired_names_in_strings_are_not_mapping_code() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("message.rs"), "const MESSAGE: &str = \"HydratedEntity\";\n").unwrap();
        deny_generic_file_mappers(folder.path()).unwrap();
    }
}
