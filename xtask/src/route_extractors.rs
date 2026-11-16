use anyhow::{bail, Context};
use ra_ap_rustc_lexer::{tokenize, FrontmatterAllowed, TokenKind};
use std::path::Path;
use walkdir::WalkDir;

const ROUTE_EXTRACTORS: &[(&str, &str)] = &[
    ("State", "axum::extract::State"),
    ("ApiQuery", "crate::query::ApiQuery"),
    ("ApiFilterQuery", "crate::query::ApiFilterQuery"),
    ("EntityPath", "crate::entity_slugs::EntityPath"),
];

const ROUTER_ASSEMBLY_METHODS: &[&str] = &["merge", "nest", "nest_service"];
const ROUTER_MIDDLEWARE_METHODS: &[&str] = &["layer", "route_layer"];
const ROUTE_REGISTRATION_METHODS: &[&str] = &["route", "route_service", "fallback", "fallback_service"];
const ROUTER_ASSEMBLY_RULE: &str = "router assembly must merge a sibling module's router() in mod.rs";

pub fn deny_unchecked_route_extractors(root: &Path) -> anyhow::Result<()> {
    let mut findings = Vec::new();
    for entry in WalkDir::new(root) {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let source = std::fs::read_to_string(path).with_context(|| format!("reading {}", path.display()))?;
        for (offset, message) in route_extractor_findings(&source, path) {
            let line = source[..offset].matches('\n').count() + 1;
            findings.push(format!("{}:{line}: {message}", path.display()));
        }
    }
    findings.sort();
    if !findings.is_empty() {
        bail!("{}", findings.join("\n"));
    }
    Ok(())
}

#[derive(Clone, Copy)]
struct SourceToken<'source> {
    offset: usize,
    text: &'source str,
    is_identifier: bool,
}

fn source_tokens(source: &str) -> Vec<SourceToken<'_>> {
    let mut position = 0;
    tokenize(source, FrontmatterAllowed::No)
        .filter_map(|token| {
            let start = position;
            position += token.len as usize;
            (!matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }
            ))
            .then_some(SourceToken {
                offset: start,
                text: source[start..position].trim_start_matches("r#"),
                is_identifier: matches!(token.kind, TokenKind::Ident | TokenKind::RawIdent),
            })
        })
        .collect()
}

fn matching_delimiter(tokens: &[SourceToken<'_>], start: usize) -> usize {
    let Some(opening) = tokens.get(start) else {
        return tokens.len();
    };
    let closing = match opening.text {
        "(" => ")",
        "[" => "]",
        "{" => "}",
        "<" => ">",
        _ => return start,
    };
    let mut depth = 1;
    for (index, token) in tokens.iter().enumerate().skip(start + 1) {
        if token.text == opening.text {
            depth += 1;
        } else if token.text == closing {
            depth -= 1;
            if depth == 0 {
                return index;
            }
        }
    }
    tokens.len()
}

fn method_arguments_start(tokens: &[SourceToken<'_>], mut index: usize) -> Option<usize> {
    if tokens.get(index..index + 3).is_some_and(|parts| token_text(parts) == "::<") {
        index = matching_delimiter(tokens, index + 2) + 1;
    }
    tokens.get(index).filter(|token| token.text == "(").map(|_| index)
}

fn split_tokens<'tokens, 'source>(
    tokens: &'tokens [SourceToken<'source>],
    separator: &str,
) -> Vec<&'tokens [SourceToken<'source>]> {
    let mut pieces = Vec::new();
    let mut start = 0;
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].text {
            "(" | "[" | "{" | "<" => index = matching_delimiter(tokens, index),
            text if text == separator => {
                if start < index {
                    pieces.push(&tokens[start..index]);
                }
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if start < tokens.len() {
        pieces.push(&tokens[start..]);
    }
    pieces
}

fn token_text(tokens: &[SourceToken<'_>]) -> String {
    tokens.iter().map(|token| token.text).collect()
}

fn is_reserved_name(name: &str) -> bool {
    ROUTE_EXTRACTORS.iter().any(|(extractor, _)| *extractor == name) || name == "axum"
}

struct ScopedName {
    name: String,
    scope: Vec<usize>,
}

struct ImportedName {
    binding: ScopedName,
    path: String,
    is_renamed: bool,
    offset: usize,
}

struct HandlerArgument {
    handler: String,
    argument: String,
    type_path: String,
    scope: Vec<usize>,
    offset: usize,
}

#[derive(Default)]
struct RouteSource {
    has_literal_router: bool,
    sibling_modules: Vec<String>,
    router_assemblies: Vec<(usize, String, String)>,
    module_scopes: Vec<Vec<usize>>,
    imports: Vec<ImportedName>,
    handlers: Vec<ScopedName>,
    shadows: Vec<ScopedName>,
    arguments: Vec<HandlerArgument>,
    registrations: Vec<(usize, ScopedName)>,
    implementations: Vec<(usize, Vec<usize>, String)>,
    findings: Vec<(usize, String)>,
}

fn route_extractor_findings(source: &str, path: &Path) -> Vec<(usize, String)> {
    let mut parsed = RouteSource::default();
    parsed.inspect(&source_tokens(source), &[], false);
    for import in &parsed.imports {
        let original = import.path.rsplit("::").next().unwrap_or_default();
        if is_reserved_name(&import.binding.name) || is_reserved_name(original) {
            let is_canonical = ROUTE_EXTRACTORS.iter().any(|(_, path)| *path == import.path);
            if import.is_renamed || !is_canonical {
                parsed.findings.push((
                    import.offset,
                    format!("extractor import must use its canonical path without a rename: {}", import.path),
                ));
            }
        }
    }
    for import in &parsed.imports {
        if import.path.rsplit("::").next() == Some("routes")
            && (import.path != "utoipa_axum::routes" || import.is_renamed)
        {
            parsed.findings.push((
                import.offset,
                "route registration macro must use utoipa_axum::routes without a rename".to_string(),
            ));
        }
    }
    for argument in &parsed.arguments {
        let is_allowed = ROUTE_EXTRACTORS.iter().any(|(name, canonical)| {
            argument.type_path == *canonical
                || (argument.type_path == *name
                    && parsed.imports.iter().any(|import| {
                        import.path == *canonical
                            && !import.is_renamed
                            && parsed.is_visible(&import.binding.scope, &argument.scope)
                    }))
        });
        if !is_allowed {
            parsed.findings.push((argument.offset, format!(
                "handler {} argument {} must use State, ApiQuery, ApiFilterQuery or EntityPath from its canonical module",
                argument.handler, argument.argument
            )));
        }
    }
    for (offset, registration) in &parsed.registrations {
        let handler = parsed
            .handlers
            .iter()
            .filter(|handler| {
                handler.name == registration.name && parsed.is_visible(&handler.scope, &registration.scope)
            })
            .max_by_key(|handler| handler.scope.len());
        let is_shadowed = handler.is_some_and(|handler| {
            parsed.shadows.iter().chain(parsed.imports.iter().map(|import| &import.binding)).any(|binding| {
                binding.name == registration.name
                    && binding.scope.len() >= handler.scope.len()
                    && parsed.is_visible(&binding.scope, &registration.scope)
            })
        });
        if handler.is_none() || is_shadowed {
            parsed.findings.push((
                *offset,
                format!(
                    "route registration {} must name an async fn defined in the same file and scope",
                    registration.name
                ),
            ));
        }
    }
    for (offset, scope, trait_name) in &parsed.implementations {
        if ["FromRequest", "FromRequestParts"].contains(&trait_name.as_str())
            || parsed.imports.iter().any(|import| {
                import.binding.name == *trait_name
                    && scope.starts_with(&import.binding.scope)
                    && ["FromRequest", "FromRequestParts"]
                        .contains(&import.path.rsplit("::").next().unwrap_or_default())
            })
        {
            parsed
                .findings
                .push((*offset, "request extractor implementation belongs outside route modules".to_string()));
        }
    }
    for (offset, method, argument) in parsed.router_assemblies {
        let sibling = parsed.sibling_modules.iter().find(|module| argument == format!("{module}::router()"));
        if path.file_name().is_none_or(|name| name != "mod.rs") || method != "merge" || sibling.is_none() {
            parsed.findings.push((offset, ROUTER_ASSEMBLY_RULE.to_string()));
        } else if let Some(sibling) = sibling {
            let parent = path.parent().unwrap_or_else(|| Path::new(""));
            let sibling_source = std::fs::read_to_string(parent.join(format!("{sibling}.rs")))
                .or_else(|_| std::fs::read_to_string(parent.join(sibling).join("mod.rs")));
            let mut sibling_route = RouteSource::default();
            if let Ok(source) = sibling_source {
                sibling_route.inspect(&source_tokens(&source), &[], false);
            }
            if !sibling_route.has_literal_router {
                parsed.findings.push((
                    offset,
                    format!("router assembly requires a literal router function in sibling module {sibling}"),
                ));
            }
        }
    }
    parsed.findings
}

impl RouteSource {
    fn is_visible(&self, binding_scope: &[usize], use_scope: &[usize]) -> bool {
        use_scope.starts_with(binding_scope)
            && !self
                .module_scopes
                .iter()
                .any(|module| use_scope.starts_with(module) && module.len() > binding_scope.len())
    }

    fn inspect(&mut self, tokens: &[SourceToken<'_>], scope: &[usize], is_macro: bool) {
        let mut index = 0;
        while index < tokens.len() {
            let token = tokens[index];
            match token.text {
                "#" if tokens.get(index + 1).is_some_and(|next| next.text == "[") => {
                    let mut path_overrides = Vec::new();
                    let mut is_test = false;
                    while tokens.get(index).is_some_and(|part| part.text == "#")
                        && tokens.get(index + 1).is_some_and(|part| part.text == "[")
                    {
                        let end = matching_delimiter(tokens, index + 1);
                        let attribute = &tokens[index + 2..end];
                        is_test |= token_text(attribute) == "cfg(test)";
                        let is_path_override = attribute.first().is_some_and(|part| part.text == "path")
                            || (attribute.first().is_some_and(|part| part.text == "cfg_attr")
                                && attribute.get(1).is_some_and(|part| part.text == "(")
                                && split_tokens(&attribute[2..matching_delimiter(attribute, 1)], ",")
                                    .iter()
                                    .skip(1)
                                    .any(|parts| parts.first().is_some_and(|part| part.text == "path")));
                        if is_path_override {
                            path_overrides.push(tokens[index].offset);
                        }
                        index = end + 1;
                    }
                    if is_test {
                        index = after_item(tokens, index);
                    } else {
                        self.findings.extend(
                            path_overrides
                                .into_iter()
                                .map(|offset| (offset, "route module path overrides are not allowed".to_string())),
                        );
                    }
                    continue;
                }
                "use" => {
                    let end = tokens[index..]
                        .iter()
                        .position(|part| part.text == ";")
                        .map_or(tokens.len(), |length| index + length);
                    self.imports.extend(imported_names(&tokens[index + 1..end], "", scope));
                    index = end;
                }
                "type" | "struct" | "enum" | "union" | "trait" | "mod" => {
                    if token.text == "mod"
                        && scope.is_empty()
                        && tokens.get(index + 2).is_some_and(|part| part.text == ";")
                    {
                        self.sibling_modules.push(tokens[index + 1].text.to_string());
                    }
                    if let Some(name) = tokens.get(index + 1).filter(|name| is_reserved_name(name.text)) {
                        self.findings.push((name.offset, format!("extractor declaration cannot shadow {}", name.text)));
                    }
                }
                "impl" => {
                    let mut start = index + 1;
                    if tokens.get(start).is_some_and(|part| part.text == "<") {
                        start = matching_delimiter(tokens, start) + 1;
                    }
                    let (trait_path, _) = leading_path(tokens.get(start..).unwrap_or_default());
                    self.implementations.push((
                        token.offset,
                        scope.to_vec(),
                        trait_path.rsplit("::").next().unwrap_or_default().to_string(),
                    ));
                }
                "async" => {
                    let function =
                        index + 1 + usize::from(tokens.get(index + 1).is_some_and(|part| part.text == "unsafe"));
                    if tokens.get(function).is_some_and(|part| part.text == "fn") {
                        self.inspect_handler(&tokens[function + 1..], scope, is_macro);
                    }
                }
                "fn" if index == 0 || !matches!(tokens[index - 1].text, "async" | "unsafe") => {
                    if let Some(name) = tokens.get(index + 1).filter(|name| name.is_identifier) {
                        if name.text == "router" && scope.is_empty() && !is_macro {
                            self.has_literal_router = true;
                        }
                        self.shadows.push(ScopedName { name: name.text.to_string(), scope: scope.to_vec() });
                    }
                }
                "let" => {
                    for binding in tokens[index + 1..].iter().take_while(|part| !matches!(part.text, "=" | ";")) {
                        if binding.is_identifier {
                            self.shadows.push(ScopedName { name: binding.text.to_string(), scope: scope.to_vec() });
                        }
                    }
                }
                method
                    if (ROUTER_ASSEMBLY_METHODS.contains(&method) || ROUTER_MIDDLEWARE_METHODS.contains(&method))
                        && index >= 2
                        && tokens[index - 1].text == ":"
                        && tokens[index - 2].text == ":" =>
                {
                    self.findings.push((
                        token.offset,
                        if ROUTER_MIDDLEWARE_METHODS.contains(&method) {
                            "route middleware belongs outside route modules"
                        } else {
                            ROUTER_ASSEMBLY_RULE
                        }
                        .to_string(),
                    ));
                }
                "." => {
                    if let Some(method) = tokens.get(index + 1) {
                        let Some(opening) = method_arguments_start(tokens, index + 2) else {
                            index += 1;
                            continue;
                        };
                        if ROUTER_MIDDLEWARE_METHODS.contains(&method.text) {
                            self.findings
                                .push((method.offset, "route middleware belongs outside route modules".to_string()));
                        } else if ROUTE_REGISTRATION_METHODS.contains(&method.text) {
                            self.findings.push((
                                method.offset,
                                "route registration must use routes! with a local async handler".to_string(),
                            ));
                        } else if ROUTER_ASSEMBLY_METHODS.contains(&method.text) {
                            let end = matching_delimiter(tokens, opening);
                            let argument = token_text(&tokens[opening + 1..end]);
                            self.router_assemblies.push((method.offset, method.text.to_string(), argument));
                        } else if method.text == "routes" {
                            let end = matching_delimiter(tokens, opening);
                            let argument = &tokens[opening + 1..end];
                            if argument.len() < 3 || argument[0].text != "routes" || argument[1].text != "!" {
                                self.findings.push((
                                    method.offset,
                                    "route registration must use routes! with a local async handler".to_string(),
                                ));
                            }
                        }
                    }
                }
                "routes"
                    if tokens.get(index + 1).is_some_and(|next| next.text == "!")
                        && tokens.get(index + 2).is_some_and(|next| matches!(next.text, "(" | "{" | "[")) =>
                {
                    let opening = index + 2;
                    let end = matching_delimiter(tokens, opening);
                    for argument in split_tokens(&tokens[opening + 1..end], ",") {
                        if argument.len() == 1 && argument[0].is_identifier {
                            self.registrations.push((
                                token.offset,
                                ScopedName { name: argument[0].text.to_string(), scope: scope.to_vec() },
                            ));
                        } else {
                            self.findings.push((
                                token.offset,
                                format!(
                                    "route registration {} must be one bare handler identifier",
                                    token_text(argument)
                                ),
                            ));
                        }
                    }
                    index = end;
                }
                "macro_rules"
                    if tokens.get(index + 1).is_some_and(|next| next.text == "!")
                        && tokens.get(index + 3).is_some_and(|next| matches!(next.text, "(" | "{" | "[")) =>
                {
                    let opening = index + 3;
                    let end = matching_delimiter(tokens, opening);
                    self.inspect(&tokens[opening + 1..end], scope, true);
                    index = end;
                }
                "!" if tokens.get(index + 1).is_some_and(|next| matches!(next.text, "(" | "{" | "[")) => {
                    let end = matching_delimiter(tokens, index + 1);
                    self.inspect(&tokens[index + 2..end], scope, true);
                    index = end;
                }
                "{" => {
                    let end = matching_delimiter(tokens, index);
                    let mut nested_scope = scope.to_vec();
                    nested_scope.push(token.offset);
                    if index >= 2 && tokens[index - 2].text == "mod" {
                        self.module_scopes.push(nested_scope.clone());
                    }
                    self.inspect(&tokens[index + 1..end], &nested_scope, is_macro);
                    index = end;
                }
                _ => {}
            }
            index += 1;
        }
    }

    fn inspect_handler(&mut self, tokens: &[SourceToken<'_>], scope: &[usize], is_macro: bool) {
        let name_index = usize::from(tokens.first().is_some_and(|name| name.text == "$"));
        let Some(name) = tokens.get(name_index).filter(|name| name.is_identifier) else {
            return;
        };
        let mut opening = name_index + 1;
        if tokens.get(opening).is_some_and(|part| part.text == "<") {
            let end = matching_delimiter(tokens, opening);
            for parameter in split_tokens(&tokens[opening + 1..end], ",") {
                if let Some(parameter) = parameter.first().filter(|parameter| is_reserved_name(parameter.text)) {
                    self.findings
                        .push((parameter.offset, format!("extractor declaration cannot shadow {}", parameter.text)));
                }
            }
            opening = end + 1;
        }
        if !tokens.get(opening).is_some_and(|part| part.text == "(") {
            return;
        }
        if !is_macro {
            self.handlers.push(ScopedName { name: name.text.to_string(), scope: scope.to_vec() });
        }
        let end = matching_delimiter(tokens, opening);
        for argument in split_tokens(&tokens[opening + 1..end], ",") {
            let type_start = argument_type_start(argument);
            let (type_path, _) = leading_path(&argument[type_start..]);
            self.arguments.push(HandlerArgument {
                handler: name.text.to_string(),
                argument: token_text(argument),
                type_path,
                scope: scope.to_vec(),
                offset: argument[0].offset,
            });
        }
    }
}

fn after_item(tokens: &[SourceToken<'_>], mut index: usize) -> usize {
    while index < tokens.len() {
        match tokens[index].text {
            "#" if tokens.get(index + 1).is_some_and(|part| part.text == "[") => {
                index = matching_delimiter(tokens, index + 1)
            }
            "(" | "[" => index = matching_delimiter(tokens, index),
            "{" => return matching_delimiter(tokens, index) + 1,
            ";" => return index + 1,
            _ => {}
        }
        index += 1;
    }
    index
}

fn leading_path(tokens: &[SourceToken<'_>]) -> (String, usize) {
    let mut index = usize::from(tokens.first().is_some_and(|part| part.text == ":")) * 2;
    let mut segments = Vec::new();
    while let Some(token) = tokens.get(index).filter(|token| token.is_identifier) {
        segments.push(token.text);
        index += 1;
        if tokens.get(index).is_none_or(|part| part.text != ":")
            || tokens.get(index + 1).is_none_or(|part| part.text != ":")
        {
            break;
        }
        index += 2;
    }
    (segments.join("::"), index)
}

fn argument_type_start(tokens: &[SourceToken<'_>]) -> usize {
    let mut index = 0;
    while index < tokens.len() {
        match tokens[index].text {
            "(" | "[" | "{" | "<" => index = matching_delimiter(tokens, index),
            ":" if tokens.get(index + 1).is_some_and(|part| part.text == ":") => index += 1,
            ":" => return index + 1,
            _ => {}
        }
        index += 1;
    }
    tokens.len()
}

fn imported_names(tokens: &[SourceToken<'_>], prefix: &str, scope: &[usize]) -> Vec<ImportedName> {
    let mut imports = Vec::new();
    for branch in split_tokens(tokens, ",") {
        if let Some(opening) = branch.iter().position(|part| part.text == "{") {
            let end = matching_delimiter(branch, opening);
            let nested_prefix = format!("{prefix}{}", token_text(&branch[..opening]));
            imports.extend(imported_names(&branch[opening + 1..end], &nested_prefix, scope));
        } else {
            let rename = branch.iter().position(|part| part.text == "as");
            let path_end = rename.unwrap_or(branch.len());
            let path = format!("{prefix}{}", token_text(&branch[..path_end])).trim_start_matches("::").to_string();
            let name = rename
                .and_then(|position| branch.get(position + 1))
                .map_or_else(|| path.rsplit("::").next().unwrap_or_default(), |token| token.text)
                .to_string();
            imports.push(ImportedName {
                binding: ScopedName { name, scope: scope.to_vec() },
                path,
                is_renamed: rename.is_some(),
                offset: branch[0].offset,
            });
        }
    }
    imports
}

#[cfg(test)]
mod tests {
    use super::deny_unchecked_route_extractors;

    const IMPORTS: &str = "use axum::extract::State;
use crate::query::{ApiQuery, ApiFilterQuery};
use crate::entity_slugs::EntityPath;";

    #[test]
    fn route_extractor_check_rejects_every_unchecked_request_path() {
        let mut probes = Vec::new();
        for argument in [
            "uri: axum::http::Uri",
            "request: axum::extract::Request",
            "parts: axum::http::request::Parts",
            "headers: axum::http::HeaderMap",
            "OriginalUri(uri): axum::extract::OriginalUri",
            "RawQuery(query): axum::extract::RawQuery",
            "Query(filters): axum::extract::Query<Filters>",
            "Extension(query): axum::extract::Extension<String>",
            "tuple: (State<AppState>, axum::http::Uri)",
            "state: &State<AppState>",
            "request: impl FromRequestParts<AppState>",
            "State(uri): axum::http::Uri",
            "state: other::State<AppState>",
        ] {
            probes.push((argument.to_string(), format!("async fn listing({argument}) {{}}"), "argument"));
        }
        for (label, source, expected) in [
            ("alias", "type Raw = axum::http::Uri; async fn listing(raw: Raw) {}", "argument"),
            ("renamed import", "use axum::http::Uri as EntityPath;", "import"),
            ("renamed extractor", "use crate::query::ApiQuery as Raw;", "import"),
            ("nested import", "use axum::{http::{Uri as EntityPath}};", "import"),
            ("re-export", "pub use other::EntityPath;", "import"),
            ("alias declaration", "type EntityPath = axum::http::Uri;", "declaration"),
            ("struct declaration", "struct ApiQuery(axum::http::Uri);", "declaration"),
            ("enum declaration", "enum ApiFilterQuery { Raw(axum::http::Uri) }", "declaration"),
            ("raw identifier", "type r#EntityPath = axum::http::Uri;", "declaration"),
            ("trait implementation", "impl axum::extract::FromRequestParts<AppState> for Raw {}", "implementation"),
            ("request implementation", "impl FromRequest<AppState> for Raw {}", "implementation"),
            ("renamed trait", "use axum::extract::FromRequestParts as ReadRequest; impl ReadRequest<AppState> for Raw {}", "implementation"),
            ("foreign handler", "fn router() { routes!(crate::other::listing); }", "registration"),
            ("sync handler", "fn listing(uri: Uri) -> impl Future<Output = ()> { async {} } fn router() { routes!(listing); }", "registration"),
            ("missing handler", "fn router() { routes!(listing); }", "registration"),
            ("macro handler", "macro_rules! listing { () => { async fn listing(uri: Uri) {} } } listing!(); fn router() { routes!(listing); }", "registration"),
            ("macro named handler", "macro_rules! listing { ($name:ident) => { async fn $name(uri: Uri) {} } } listing!(listing); fn router() { routes!(listing); }", "registration"),
            ("multiple registrations", "async fn listing(State(state): State<AppState>) {} fn router() { routes!(listing, missing); }", "registration"),
            ("multiline registration", "fn router() { routes!\n(\ncrate::other::listing\n); }", "registration"),
            ("gated handler", "#[cfg(feature = \"extra\")] async fn listing(uri: Uri) {}", "argument"),
            ("nested module", "mod nested { async fn listing(uri: Uri) {} }", "argument"),
            ("after tests", "#[cfg(test)] mod tests { async fn ignored(uri: Uri) {} } async fn listing(uri: Uri) {}", "argument"),
            ("test-only decoy", "#[cfg(test)] async fn listing() {} fn router() { routes!(listing); }", "registration"),
            ("nested decoy", "mod nested { async fn listing() {} } use elsewhere::listing; fn router() { routes!(listing); }", "registration"),
            ("generic shadow", "async fn listing<State>(State(uri): State) {}", "declaration"),
            ("namespace shadow", "mod axum { pub mod extract { pub type State = Uri; } }", "declaration"),
            ("namespace alias", "use other as axum;", "import"),
            ("nested glob shadow", "mod nested { use other::*; async fn listing(state: State<AppState>) {} }", "argument"),
            ("direct route", "fn router() { OpenApiRouter::new().route(\"/probe\", get(|uri: Uri| async {})); }", "registration"),
            ("associated merge", "fn router() { OpenApiRouter::merge(router, foreign::router()); }", "router assembly"),
            ("associated nest service", "fn router() { OpenApiRouter::nest_service(router, \"/raw\", service); }", "router assembly"),
            ("assembly function alias", "fn router() { let attach = OpenApiRouter::merge; attach(router, foreign::router()); }", "router assembly"),
            ("constructed routes", "fn router() { OpenApiRouter::new().routes(other::routes()); }", "registration"),
            ("renamed registration macro", "use utoipa_axum::routes as register; fn router() { register!(other); }", "registration"),
            ("shadowed handler", "async fn listing() {} fn router() { let listing = |uri: Uri| async {}; routes!(listing); }", "registration"),
            ("shadowed handler import", "async fn listing() {} fn router() { use other::listing; routes!(listing); }", "registration"),
            ("shadowed sync handler", "async fn listing() {} fn router() { fn listing(uri: Uri) -> impl Future<Output = ()> { async {} } routes!(listing); }", "registration"),
            ("macro parameter handler", "macro_rules! make_handler { ($name:ident) => { async fn $name(uri: Uri) {} } }", "argument"),
        ] {
            probes.push((label.to_string(), source.to_string(), expected));
        }
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("probe.rs");
        let mut missed = Vec::new();
        for (label, source, expected) in probes {
            std::fs::write(&file, format!("{IMPORTS}\n{source}")).unwrap();
            match deny_unchecked_route_extractors(folder.path()) {
                Ok(()) => missed.push(format!("{label}: no finding reported")),
                Err(error) => {
                    let message = error.to_string();
                    assert!(message.contains("probe.rs:4:"), "{label}: {message}");
                    assert!(message.contains(expected), "{label}: {message}");
                    println!("{label}: {message}");
                }
            }
        }
        assert!(missed.is_empty(), "{}", missed.join("\n"));
    }

    #[test]
    fn route_method_fields_and_incomplete_tokens_never_panic() {
        let mut failures = Vec::new();
        for method in super::ROUTER_ASSEMBLY_METHODS
            .iter()
            .chain(super::ROUTER_MIDDLEWARE_METHODS)
            .chain(super::ROUTE_REGISTRATION_METHODS)
            .chain([&"routes"])
        {
            for suffix in ["", ";", ".", ")", ",", "::<"] {
                let source = format!("settings.{method}{suffix}");
                match std::panic::catch_unwind(|| {
                    super::route_extractor_findings(&source, std::path::Path::new("listing.rs"))
                }) {
                    Ok(findings) if findings.is_empty() => {}
                    Ok(findings) => failures.push(format!("{source}: unexpected findings {findings:?}")),
                    Err(_) => failures.push(format!("{source}: panicked")),
                }
            }
        }
        for source in [
            "#[cfg(test)] fn ignored() {}",
            "use crate::{query::{ApiQuery,},};",
            "impl<T> FromRequestParts<T> for Extractor {}",
            "async fn handler<T>(State(state): State<T>,) {}",
            "fn router() { router.routes(routes!(handler,)); }",
            "macro_rules! handler { () => { async fn handler() {} } }",
        ] {
            for end in source.char_indices().map(|(index, _)| index).chain([source.len()]) {
                let prefix = &source[..end];
                if std::panic::catch_unwind(|| {
                    super::route_extractor_findings(prefix, std::path::Path::new("listing.rs"))
                })
                .is_err()
                {
                    failures.push(format!("{prefix}: panicked"));
                }
            }
        }
        assert!(failures.is_empty(), "{}", failures.join("\n"));
    }

    #[test]
    fn router_assembly_rejects_unread_modules_and_route_middleware() {
        let folder = tempfile::tempdir().unwrap();
        let module_file = folder.path().join("mod.rs");
        let sibling_file = folder.path().join("raw.rs");
        let mut missed = Vec::new();
        for (label, declaration, sibling, assembly, expected) in [
            (
                "path override",
                "#[path = \"../../raw_routes.rs\"] mod raw;",
                "pub(super) fn router() {}",
                ".merge(raw::router())",
                "module path",
            ),
            ("macro router", "mod raw;", "crate::db::raw_route_module!();", ".merge(raw::router())", "literal router"),
            (
                "re-exported router",
                "mod raw;",
                "pub(super) use crate::raw::router;",
                ".merge(raw::router())",
                "literal router",
            ),
            (
                "test-only router",
                "mod raw;",
                "#[cfg(test)] fn router() {} crate::db::raw_route_module!();",
                ".merge(raw::router())",
                "literal router",
            ),
            (
                "nested router",
                "mod raw;",
                "mod nested { fn router() {} } pub(super) use crate::raw::router;",
                ".merge(raw::router())",
                "literal router",
            ),
            (
                "router layer",
                "mod raw;",
                "fn router() {}",
                ".layer(axum::middleware::from_fn(|request: axum::extract::Request, next| async {}))",
                "route middleware",
            ),
            ("route layer", "mod raw;", "fn router() {}", ".route_layer(middleware)", "route middleware"),
        ] {
            std::fs::write(&module_file, format!("{declaration}\nfn router() {{ OpenApiRouter::new(){assembly}; }}"))
                .unwrap();
            std::fs::write(&sibling_file, sibling).unwrap();
            match deny_unchecked_route_extractors(folder.path()) {
                Ok(()) => missed.push(format!("{label}: no finding reported")),
                Err(error) => {
                    let message = error.to_string();
                    assert!(message.contains("mod.rs:"), "{label}: {message}");
                    assert!(message.contains(expected), "{label}: {message}");
                    println!("{label}: {message}");
                }
            }
        }
        assert!(missed.is_empty(), "{}", missed.join("\n"));
    }

    #[test]
    fn router_assembly_accepts_only_sibling_router_merges_in_module_files() {
        let folder = tempfile::tempdir().unwrap();
        std::fs::write(folder.path().join("child.rs"), "fn router() {}").unwrap();
        let mut missed = Vec::new();
        for (filename, assembly, accepted) in [
            ("mod.rs", ".merge(child::router())", true),
            ("mod.rs", ".merge(\n child :: router ( ) \n)", true),
            ("mod.rs", ".merge(crate::db::raw_router())", false),
            ("mod.rs", ".merge(foreign::router())", false),
            ("mod.rs", ".merge(raw_router!())", false),
            ("mod.rs", ".merge(raw_router())", false),
            ("mod.rs", ".merge(router)", false),
            ("mod.rs", ".nest(\"/raw\", child::router())", false),
            (
                "mod.rs",
                ".nest_service(\"/raw\", axum::routing::get(|uri: axum::http::Uri| async move { uri.to_string() }))",
                false,
            ),
            ("listing.rs", ".merge(child::router())", false),
            ("listing.rs", ".merge(raw_router())", false),
            ("listing.rs", ".nest(\"/raw\", child::router())", false),
            (
                "listing.rs",
                ".nest_service(\"/raw\", axum::routing::get(|uri: axum::http::Uri| async move { uri.to_string() }))",
                false,
            ),
        ] {
            let file = folder.path().join(filename);
            std::fs::write(&file, format!("mod child;\nfn router() {{\nOpenApiRouter::new(){assembly};\n}}")).unwrap();
            let result = deny_unchecked_route_extractors(folder.path());
            std::fs::remove_file(file).unwrap();
            match (accepted, result) {
                (true, Err(error)) => panic!("{filename} {assembly}: {error}"),
                (false, Ok(())) => missed.push(format!("{filename} {assembly}: no finding reported")),
                (false, Err(error)) => {
                    let message = error.to_string();
                    assert!(message.contains(&format!("{filename}:3:")), "{message}");
                    assert!(message.contains("router assembly"), "{message}");
                    println!("{message}");
                }
                (true, Ok(())) => {}
            }
        }
        assert!(missed.is_empty(), "{}", missed.join("\n"));
    }

    #[test]
    fn route_extractor_check_accepts_declared_handlers_and_skips_only_test_items() {
        let folder = tempfile::tempdir().unwrap();
        let file = folder.path().join("listing.rs");
        for source in [
            "async fn listing(State(state): State<AppState>, ApiQuery(query, filters): ApiQuery<Filters>) {} fn router() { routes!(listing); }",
            "async fn listing(EntityPath(id): EntityPath, ApiFilterQuery(filters): ApiFilterQuery<Filters>) {} fn router() { routes!(listing); }",
            "async fn listing(state: axum::extract::State<AppState>, filters: crate::query::ApiQuery<Filters>) {} fn router() { routes!(listing); }",
            "async fn first() {} async fn second() {} fn router() { routes!\n(\nfirst,\nsecond,\n); }",
            "#[cfg(feature = \"extra\")] async fn listing(State(state): State<AppState>) {} fn router() { routes!(listing); }",
            "mod nested { use axum::extract::State; async fn listing(State(state): State<AppState>) {} fn router() { routes!(listing); } }",
            "#[cfg(test)] mod tests { async fn listing(uri: Uri) {} type EntityPath = Uri; } async fn listing() {} fn router() { routes!(listing); }",
            "#[cfg(test)] #[tokio::test] async fn ignored(uri: Uri) {} async fn listing() {} fn router() { routes!(listing); }",
            "#[path = \"test_routes.rs\"] #[cfg(test)] mod tests; async fn listing() {} fn router() { routes!(listing); }",
            "#[cfg(test)] #[path = \"test_routes.rs\"] mod tests; async fn listing() {} fn router() { routes!(listing); }",
            "const TEXT: &str = r#\"async fn fake(uri: Uri) {} routes!(missing)\"#;",
            "macro_rules! records { ($name:ident) => { struct $name; } } records!(Row);",
        ] {
            std::fs::write(&file, format!("{IMPORTS}\n{source}")).unwrap();
            deny_unchecked_route_extractors(folder.path()).unwrap_or_else(|error| panic!("{source}: {error}"));
        }
    }

    #[test]
    fn route_extractor_check_covers_nested_source_files() {
        let folder = tempfile::tempdir().unwrap();
        let nested = folder.path().join("v2");
        std::fs::create_dir(&nested).unwrap();
        std::fs::write(nested.join("listing.rs"), "async fn listing(uri: Uri) {}").unwrap();
        let error = deny_unchecked_route_extractors(folder.path()).unwrap_err().to_string();
        assert!(error.contains("v2/listing.rs:1:"), "{error}");
    }

    #[test]
    fn every_route_handler_uses_declared_extractors() {
        deny_unchecked_route_extractors(&crate::workspace_root().join("crates/ddo-api/src/routes")).unwrap();
    }
}
