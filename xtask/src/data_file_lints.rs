use anyhow::{bail, Context};
use ddo_etl::tree::{AppliedCorrection, Citation, EntityIdentity, PackException, Redirect, RemovedEntry, RetiredSlug};
use ra_ap_rustc_lexer::{tokenize, FrontmatterAllowed, TokenKind};
use std::path::Path;
use walkdir::WalkDir;

const ANNOTATION_TABLES: [&str; 7] = [
    PackException::TABLE,
    AppliedCorrection::TABLE,
    Citation::TABLE,
    RemovedEntry::TABLE,
    EntityIdentity::TABLE,
    Redirect::TABLE,
    RetiredSlug::TABLE,
];

const DECLARED_ANNOTATION_TABLES: [&str; 3] = [AppliedCorrection::TABLE, Citation::TABLE, RemovedEntry::TABLE];

pub fn deny_annotation_table_literals(root: &Path) -> anyhow::Result<()> {
    let mut findings = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let production_length = source.split("#[cfg(test)]").next().unwrap_or(&source).len();
        let mut token_start = 0;
        let mut previous_code = "";
        for token in tokenize(&source, FrontmatterAllowed::No) {
            let token_end = token_start + token.len as usize;
            let text = &source[token_start..token_end];
            if token_start < production_length
                && matches!(token.kind, TokenKind::Literal { .. })
                && previous_code != "in"
                && DECLARED_ANNOTATION_TABLES.iter().any(|table| text == format!("{table:?}"))
            {
                let line = source[..token_start].matches('\n').count() + 1;
                findings.push(format!(
                    "{}:{line}: {text} names an annotation table; use its declared TABLE constant",
                    path.display()
                ));
            }
            if !matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }
            ) {
                previous_code = text;
            }
            token_start = token_end;
        }
    }
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

const CITATION_FIELD_READERS: &str = "tree/annotations.rs";

fn reads_citation_field(literal: &str) -> bool {
    let sql = literal.to_ascii_lowercase().split_whitespace().collect::<Vec<_>>().join(" ");
    if !sql.contains("select ") {
        return false;
    }
    let words: Vec<&str> =
        sql.split(|character: char| !is_word_character(character)).filter(|word| !word.is_empty()).collect();
    let Some(table_position) =
        words.windows(2).position(|pair| matches!(pair[0], "from" | "join") && pair[1] == Citation::TABLE)
    else {
        return false;
    };
    let after_table = &words[table_position + 2..];
    let alias = match after_table.first() {
        Some(&"as") => after_table.get(1),
        other => other,
    }
    .copied()
    .filter(|word| !is_sql_keyword(word));
    sql.match_indices("field").any(|(position, _)| {
        let before = &sql[..position];
        let after = &sql[position + "field".len()..];
        let part_of_word = |character: Option<char>| character.is_some_and(is_word_character);
        if part_of_word(before.chars().next_back()) || part_of_word(after.chars().next()) {
            return false;
        }
        match before.strip_suffix('.') {
            None => true,
            Some(qualified_name) => {
                let owner = qualified_name.rsplit(|character: char| !is_word_character(character)).next();
                owner == Some(Citation::TABLE) || owner == alias
            }
        }
    })
}

fn is_word_character(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

fn is_sql_keyword(word: &str) -> bool {
    matches!(
        word,
        "where"
            | "on"
            | "join"
            | "left"
            | "inner"
            | "cross"
            | "group"
            | "order"
            | "limit"
            | "union"
            | "using"
            | "and"
            | "or"
    )
}

pub fn deny_citation_field_reads(roots: &[&Path]) -> anyhow::Result<()> {
    let mut findings = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root) {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "rs") || path.ends_with(CITATION_FIELD_READERS) {
                continue;
            }
            let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            let production_length = source.split("#[cfg(test)]").next().unwrap_or(&source).len();
            let mut token_start = 0;
            for token in tokenize(&source, FrontmatterAllowed::No) {
                let token_end = token_start + token.len as usize;
                if token_start < production_length
                    && matches!(token.kind, TokenKind::Literal { .. })
                    && reads_citation_field(&source[token_start..token_end])
                {
                    let line = source[..token_start].matches('\n').count() + 1;
                    findings.push(format!(
                        "{}:{line}: reads a citation's field; only {CITATION_FIELD_READERS} may",
                        path.display()
                    ));
                }
                token_start = token_end;
            }
        }
    }
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

pub fn deny_unlisted_records(tree_root: &Path) -> anyhow::Result<()> {
    let registry_path = tree_root.join("schemas.rs");
    let registry =
        std::fs::read_to_string(&registry_path).with_context(|| format!("reading {}", registry_path.display()))?;
    let registry = registry.split_once("declared_records! {").map_or("", |(_, listed)| listed);
    let mut findings = Vec::new();
    for entry in WalkDir::new(tree_root) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "rs") || path.ends_with("records.rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let production = source.split("#[cfg(test)]").next().unwrap_or(&source);
        let tokens = tokenize(production, FrontmatterAllowed::No).scan(0, |offset, token| {
            let start = *offset;
            *offset += token.len as usize;
            Some((token.kind, &production[start..*offset]))
        });
        let code: Vec<&str> = tokens
            .filter(|(kind, _)| {
                !matches!(kind, TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. })
            })
            .map(|(_, text)| text)
            .collect();
        for window in code.windows(4) {
            if window[0] == "struct" && window[2] == "in" && !registry.contains(&format!("::{},", window[1])) {
                findings.push(format!(
                    "{}: record {} is declared with database_record! but missing from declared_records! in schemas.rs",
                    path.display(),
                    window[1]
                ));
            }
        }
    }
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

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
                for table in ANNOTATION_TABLES {
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
        for table in ANNOTATION_TABLES {
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
    fn annotation_table_names_come_from_their_declarations() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("layout.rs");
        for table in DECLARED_ANNOTATION_TABLES {
            std::fs::write(&path, format!("fn path() {{ let table = {table:?}; }}\n")).unwrap();
            let error = deny_annotation_table_literals(folder.path()).unwrap_err().to_string();
            assert!(error.contains("layout.rs:1") && error.contains(table), "{error}");
        }
        std::fs::write(&path, format!("struct Entry in {:?} {{}}\n", Citation::TABLE)).unwrap();
        deny_annotation_table_literals(folder.path()).unwrap();
    }

    #[test]
    fn a_citation_field_is_read_only_by_the_annotation_module() {
        let folder = tempfile::tempdir().unwrap();
        let wiki = folder.path().join("wiki");
        std::fs::create_dir_all(&wiki).unwrap();
        let reader = wiki.join("database.rs");
        for sql in [
            "SELECT c.url FROM citations c WHERE c.field = 'description'",
            "SELECT url FROM citations WHERE field = 'description'",
            "SELECT d.id FROM descriptions d JOIN citations AS owner ON owner.owner_id = d.id WHERE owner.field <> ''",
            "SELECT citations.field FROM citations",
        ] {
            std::fs::write(&reader, format!("fn read_descriptions() {{ db.prepare({sql:?}); }}\n")).unwrap();
            let error = deny_citation_field_reads(&[folder.path()]).unwrap_err().to_string();
            assert!(error.contains("database.rs:1") && error.contains("citation"), "{sql}: {error}");
        }
        for sql in [
            "SELECT url FROM citations WHERE owner_id = 1",
            "SELECT field FROM corrections WHERE name = ''",
            "SELECT c.url FROM citations c JOIN corrections k ON k.name = c.url WHERE k.field = 'description'",
            "INSERT INTO citations (owner_id, field) VALUES (1, '')",
        ] {
            std::fs::write(&reader, format!("fn read_descriptions() {{ db.prepare({sql:?}); }}\n")).unwrap();
            deny_citation_field_reads(&[folder.path()]).unwrap_or_else(|error| panic!("{sql}: {error}"));
        }
        std::fs::create_dir_all(folder.path().join("tree")).unwrap();
        std::fs::write(
            folder.path().join("tree/annotations.rs"),
            "fn read() { db.prepare(\"SELECT c.field FROM citations c\"); }\n",
        )
        .unwrap();
        std::fs::write(&reader, "fn read_descriptions() {}\n#[cfg(test)]\nmod tests { fn t() { db.prepare(\"SELECT field FROM citations\"); } }\n").unwrap();
        deny_citation_field_reads(&[folder.path()]).unwrap();
    }

    #[test]
    fn a_record_missing_from_the_registry_is_reported() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("schemas.rs"), "declared_records! {\n    super::kinds::Listed,\n}\n")
            .unwrap();
        std::fs::write(
            folder.path().join("kind.rs"),
            "database_record! {\n    pub struct Listed in \"things\" {}\n}\ndatabase_record! {\n    pub struct Forgotten in \"things\" {}\n}\n",
        )
        .unwrap();
        let error = deny_unlisted_records(folder.path()).unwrap_err().to_string();
        assert!(error.contains("Forgotten") && !error.contains("Listed"), "{error}");
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
