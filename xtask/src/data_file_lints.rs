use anyhow::{bail, Context};
use ddo_etl::corrections::CorrectionValue;
use ddo_etl::tree::{AppliedCorrection, Citation, EntityIdentity, PackException, Redirect, RemovedEntry, RetiredSlug};
use ra_ap_rustc_lexer::{tokenize, FrontmatterAllowed, LiteralKind, TokenKind};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
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

pub fn deny_test_file_basename_lookups(roots: &[&Path]) -> anyhow::Result<()> {
    let mut findings = Vec::new();
    for root in roots {
        let test_files = declared_test_files(root)?;
        for entry in WalkDir::new(root) {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "rs") {
                continue;
            }
            let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            let is_test_file = path.strip_prefix(root)?.starts_with("tests") || test_files.contains(path);
            let tokens = code_tokens(&source);
            for offset in basename_lookup_offsets(&tokens, test_code_start(&tokens, is_test_file)) {
                let line = source[..offset].matches('\n').count() + 1;
                findings.push(format!(
                    "{}:{line}: test input selected by basename; join the exact path instead",
                    path.display()
                ));
            }
        }
    }
    findings.sort();
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

fn code_tokens(source: &str) -> Vec<(usize, &str)> {
    let mut position = 0;
    tokenize(source, FrontmatterAllowed::No)
        .filter_map(|token| {
            let start = position;
            position += token.len as usize;
            (!matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }
            ))
            .then_some((start, &source[start..position]))
        })
        .collect()
}

fn is_test_attribute(tokens: &[(usize, &str)]) -> bool {
    tokens.iter().map(|token| token.1).eq(["#", "[", "cfg", "(", "test", ")", "]"])
}

fn test_code_start(tokens: &[(usize, &str)], is_test_file: bool) -> usize {
    if is_test_file {
        0
    } else {
        tokens.windows(7).find(|attribute| is_test_attribute(attribute)).map_or(usize::MAX, |attribute| attribute[0].0)
    }
}

fn declared_test_files(root: &Path) -> anyhow::Result<BTreeSet<PathBuf>> {
    let mut test_files = BTreeSet::new();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        let tokens = code_tokens(&source);
        for declaration in tokens.windows(10) {
            if !is_test_attribute(&declaration[..7]) || declaration[7].1 != "mod" || declaration[9].1 != ";" {
                continue;
            }
            let parent = path.parent().expect("a source file has a parent");
            let stem = path.file_stem().expect("a source file has a stem");
            let module_root = if ["lib", "main", "mod"].iter().any(|name| stem == *name) {
                parent.to_path_buf()
            } else {
                parent.join(stem)
            };
            let module = module_root.join(declaration[8].1.trim_start_matches("r#"));
            test_files.insert(module.with_extension("rs"));
            if module.is_dir() {
                for descendant in WalkDir::new(&module) {
                    test_files.insert(descendant?.into_path());
                }
            }
        }
    }
    Ok(test_files)
}

fn selects_one_entry(tokens: &[(usize, &str)]) -> bool {
    if tokens.len() < 3 || tokens[0].1 != "." {
        return false;
    }
    match tokens[1].1 {
        "next" | "last" | "nth" => tokens[2].1 == "(",
        "take" => tokens.get(2..5).is_some_and(|call| call.iter().map(|token| token.1).eq(["(", "1", ")"])),
        "collect" => {
            let Some(open) = tokens.iter().position(|token| token.1 == "(") else {
                return false;
            };
            tokens.get(open + 1..open + 3).is_some_and(|call| call[0].1 == ")" && call[1].1 == "[")
        }
        _ => false,
    }
}

fn basename_predicate(tokens: &[(usize, &str)]) -> bool {
    tokens.windows(4).any(|call| {
        if call[0].1 != "." || call[2].1 != "(" {
            return false;
        }
        match call[1].1.trim_start_matches("r#") {
            "file_name" | "file_stem" => call[3].1 == ")",
            "ends_with" => {
                let literal = call[3].1;
                let Some(opening) = literal.find('"') else {
                    return false;
                };
                let Some(closing) = literal.rfind('"').filter(|&closing| closing > opening) else {
                    return false;
                };
                let content = &literal[opening + 1..closing];
                let decoded =
                    if literal.starts_with('r') { Some(content.to_owned()) } else { decoded_string_content(content) };
                decoded.is_some_and(|name| !name.is_empty() && !name.contains(['/', '\\']))
            }
            _ => false,
        }
    })
}

fn basename_lookup_offsets(tokens: &[(usize, &str)], test_start: usize) -> Vec<usize> {
    let mut offsets = Vec::new();
    for (index, call) in tokens.windows(3).enumerate() {
        let method = call[1].1.trim_start_matches("r#");
        if call[0].0 < test_start
            || call[0].1 != "."
            || call[2].1 != "("
            || !matches!(method, "find" | "find_map" | "rfind" | "position" | "filter" | "filter_map" | "skip_while")
        {
            continue;
        }
        let mut depth = 1;
        let Some(end) = (index + 3..tokens.len()).find(|&cursor| {
            match tokens[cursor].1 {
                "(" => depth += 1,
                ")" => depth -= 1,
                _ => {}
            }
            depth == 0
        }) else {
            continue;
        };
        if matches!(method, "filter" | "filter_map" | "skip_while") && !selects_one_entry(&tokens[end + 1..]) {
            continue;
        }
        if basename_predicate(&tokens[index + 3..end]) {
            offsets.push(call[0].0);
        }
    }
    offsets
}

pub fn deny_missing_target_literals(roots: &[&Path]) -> anyhow::Result<()> {
    let missing_target = CorrectionValue::missing_target();
    let marker = missing_target.as_text().expect("a missing target is text");
    let mut findings = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root) {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "rs")
                || path.ends_with("ddo-etl/src/corrections/missing_target.rs")
            {
                continue;
            }
            let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            let mut position = 0;
            for token in tokenize(&source, FrontmatterAllowed::No) {
                let start = position;
                position += token.len as usize;
                let TokenKind::Literal { kind, suffix_start } = token.kind else {
                    continue;
                };
                if !matches!(
                    kind,
                    LiteralKind::Str { terminated: true }
                        | LiteralKind::ByteStr { terminated: true }
                        | LiteralKind::RawStr { n_hashes: Some(_) }
                        | LiteralKind::RawByteStr { n_hashes: Some(_) }
                ) {
                    continue;
                }
                let literal = &source[start..start + suffix_start as usize];
                let opening = literal.find('"').expect("a string literal has an opening quote");
                let closing = literal.rfind('"').expect("a terminated string literal has a closing quote");
                let content = &literal[opening + 1..closing];
                let decoded = match kind {
                    LiteralKind::RawStr { .. } | LiteralKind::RawByteStr { .. } => Some(content.to_owned()),
                    _ => decoded_string_content(content),
                };
                if decoded.as_deref() == Some(marker) {
                    let line = source[..start].matches('\n').count() + 1;
                    findings.push(format!(
                        "{}:{line}: use CorrectionValue::missing_target or its target predicates instead of the marker literal",
                        path.display()
                    ));
                }
            }
        }
    }
    findings.sort();
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

fn decoded_string_content(content: &str) -> Option<String> {
    let mut characters = content.chars().peekable();
    let mut decoded = String::new();
    while let Some(character) = characters.next() {
        if character != '\\' {
            decoded.push(character);
            continue;
        }
        let escaped = match characters.next()? {
            '\\' => '\\',
            '"' => '"',
            '\'' => '\'',
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '0' => '\0',
            'x' => {
                let high = characters.next()?.to_digit(16)?;
                let low = characters.next()?.to_digit(16)?;
                char::from_u32(high * 16 + low)?
            }
            'u' => {
                if characters.next()? != '{' {
                    return None;
                }
                let mut digits = String::new();
                loop {
                    match characters.next()? {
                        '}' => break,
                        '_' => {}
                        digit => digits.push(digit),
                    }
                }
                char::from_u32(u32::from_str_radix(&digits, 16).ok()?)?
            }
            newline @ ('\n' | '\r') => {
                if newline == '\r' && characters.next()? != '\n' {
                    return None;
                }
                while characters.next_if(|next| matches!(next, ' ' | '\t' | '\n' | '\r')).is_some() {}
                continue;
            }
            _ => return None,
        };
        decoded.push(escaped);
    }
    Some(decoded)
}

pub fn deny_copied_step_cost_fallbacks(roots: &[&Path]) -> anyhow::Result<()> {
    let mut findings = Vec::new();
    for root in roots {
        for entry in WalkDir::new(root) {
            let entry = entry?;
            let path = entry.path();
            if path.extension().is_none_or(|extension| extension != "rs") || path.ends_with("ddo-model/src/schema.rs") {
                continue;
            }
            let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
            for (index, line) in source.lines().enumerate() {
                let names_step_costs = line.contains("crafting_step_costs");
                let names_any_tier = line.contains("'any'");
                if names_step_costs && names_any_tier {
                    findings.push(format!(
                        "{}:{}: step-cost fallback belongs in the model's crafting cost views",
                        path.display(),
                        index + 1
                    ));
                }
            }
        }
    }
    findings.sort();
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

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
    let test_files = declared_test_files(tree_root)?;
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
        let tokens = code_tokens(&source);
        let test_start = test_code_start(&tokens, test_files.contains(path));
        let code: Vec<_> = tokens.iter().take_while(|token| token.0 < test_start).map(|token| token.1).collect();
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
    fn test_file_lookups_require_exact_paths() {
        let folder = tempfile::tempdir().unwrap();
        let test_root = folder.path().join("tests");
        std::fs::create_dir_all(&test_root).unwrap();
        let path = test_root.join("lookup.rs");
        let mut missed = Vec::new();
        for expression in [
            "walk.find(|entry| entry.file_name() == name)",
            "walk.find(|entry| name == entry.file_name())",
            "walk.find(|entry| { entry.file_name().to_str() == Some(name) })",
            "walk.find(|entry| entry.path().file_stem().is_some_and(|stem| stem == name))",
            "walk.find_map(|entry| (entry.file_name() == name).then(|| entry.path()))",
            "walk.rfind(|entry| entry.file_name() == name)",
            r#"walk.find(|entry| entry.path().ends_with("item.toml"))"#,
            "walk.filter(|entry| entry.file_name() == name).collect::<Vec<_>>()[0]",
            "walk.filter(|entry| entry.file_name() == name).take(1)",
            "walk.skip_while(|entry| entry.file_name() != name).next()",
            "walk.position(|entry| entry.file_name() == name)",
            "walk.filter(|entry| entry.file_name() == name).next()",
            "walk.filter_map(|entry| (entry.file_name() == name).then_some(entry)).next()",
            "walk\n.find(\n|entry| entry\n.file_name() == name\n)",
        ] {
            std::fs::write(&path, format!("fn input() {{\n{expression}\n}}\n")).unwrap();
            match deny_test_file_basename_lookups(&[folder.path()]) {
                Ok(()) => missed.push(expression),
                Err(error) => {
                    let message = error.to_string();
                    assert!(message.contains("lookup.rs:") && message.contains("join the exact path"), "{message}");
                }
            }
        }
        assert!(missed.is_empty(), "unreported basename lookups:\n{}", missed.join("\n"));
        for expression in [
            "root.join(\"packs/shared/gear/item.toml\")",
            "walk.filter(|entry| entry.file_name() != excluded).collect::<Vec<_>>()",
            "walk.map(|entry| entry.file_name()).collect::<Vec<_>>()",
            "rows.find(|row| row.file_name == name)",
            "walk.find(|entry| entry.path() == expected_path)",
            r#"walk.find(|entry| entry.path().ends_with("gear/item.toml"))"#,
            "r#\"walk.find(|entry| entry.file_name() == name)\"#",
        ] {
            std::fs::write(&path, format!("fn input() {{ {expression} }}")).unwrap();
            deny_test_file_basename_lookups(&[folder.path()]).unwrap();
        }
        std::fs::remove_file(path).unwrap();
        let unit_path = folder.path().join("unit.rs");
        let lookup = "fn input() { walk.find(|entry| entry.file_name() == name); }";
        std::fs::write(&unit_path, lookup).unwrap();
        deny_test_file_basename_lookups(&[folder.path()]).unwrap();
        std::fs::write(&unit_path, format!("#[cfg(test)]\nmod tests {{ {lookup} }}")).unwrap();
        assert!(deny_test_file_basename_lookups(&[folder.path()]).unwrap_err().to_string().contains("unit.rs:2"));
    }

    #[test]
    fn checkout_ancestors_do_not_make_production_files_tests() {
        let folder = tempfile::tempdir().unwrap();
        let crate_root = folder.path().join("tests/project");
        std::fs::create_dir_all(crate_root.join("src")).unwrap();
        std::fs::write(crate_root.join("src/lib.rs"), "fn input() { walk.find(|entry| entry.file_name() == name); }")
            .unwrap();
        deny_test_file_basename_lookups(&[&crate_root]).unwrap();
    }

    #[test]
    fn declared_test_modules_are_scanned_without_repeating_the_attribute() {
        let folder = tempfile::tempdir().unwrap();
        let module_root = folder.path().join("src/delta");
        std::fs::create_dir_all(&module_root).unwrap();
        std::fs::write(module_root.join("mod.rs"), "#[cfg(test)] mod tests;\n#[cfg(test)] mod merge_tests;").unwrap();
        for module in ["tests", "merge_tests"] {
            std::fs::write(
                module_root.join(format!("{module}.rs")),
                "fn input() { walk.find(|entry| entry.file_name() == name); }",
            )
            .unwrap();
        }
        let error = deny_test_file_basename_lookups(&[folder.path()]).unwrap_err().to_string();
        assert!(error.contains("delta/tests.rs:1") && error.contains("delta/merge_tests.rs:1"), "{error}");
    }

    #[test]
    fn test_records_are_excluded_from_the_registry_lint() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("schemas.rs"), "declared_records! {}").unwrap();
        std::fs::write(folder.path().join("mod.rs"), "#[cfg(test)] mod examples;").unwrap();
        std::fs::write(folder.path().join("examples.rs"), "database_record! { pub struct Example in \"things\" {} }")
            .unwrap();
        deny_unlisted_records(folder.path()).unwrap();
    }

    #[test]
    fn missing_target_literals_belong_to_the_declaring_module() {
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("corrections.rs");
        let missing_target = CorrectionValue::missing_target();
        let missing = missing_target.as_text().unwrap();
        let literal = format!("{missing:?}");
        let mut missed = Vec::new();
        for expression in [
            format!("x == Some({literal})"),
            format!("x != Some({literal})"),
            format!("text == {literal}"),
            format!("Some({literal}) == x"),
            format!("{literal} == text"),
            format!("matches!(x, Some({literal}))"),
            format!("match text {{ {literal} => true, _ => false }}"),
            format!("text.eq({literal})"),
            format!("text == &{literal}"),
            format!("text.starts_with({literal})"),
            format!("r##\"{missing}\"##"),
            format!("b{literal}"),
            format!("br#\"{missing}\"#"),
            format!("CorrectionValue::Text({literal}.into())"),
            format!("\"{}\"", missing.replacen(' ', "\\x20", 1)),
            format!("\"{}\"", missing.replace('b', "\\u{62}")),
            format!("\"{}\"", missing.replace("bonus", "\\\n    bonus")),
        ] {
            std::fs::write(&path, format!("fn read() {{\n    {expression}\n}}\n")).unwrap();
            match deny_missing_target_literals(&[folder.path()]) {
                Ok(()) => missed.push(expression),
                Err(error) => assert!(error.to_string().contains("corrections.rs:2"), "{error}"),
            }
        }
        assert!(missed.is_empty(), "unreported marker literals:\n{}", missed.join("\n"));
        for expression in [
            "CorrectionValue::missing_target()",
            "value.has_matching_target()",
            "\"a missing target needs correction\"",
            "\"\"",
        ] {
            std::fs::write(&path, format!("fn read() {{ {expression} }}\n")).unwrap();
            deny_missing_target_literals(&[folder.path()]).unwrap();
        }
        let declaring_path = folder.path().join("crates/ddo-etl/src/corrections/missing_target.rs");
        std::fs::create_dir_all(declaring_path.parent().unwrap()).unwrap();
        std::fs::write(&declaring_path, format!("const MARKER: &str = {literal};")).unwrap();
        deny_missing_target_literals(&[folder.path()]).unwrap();
    }

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
