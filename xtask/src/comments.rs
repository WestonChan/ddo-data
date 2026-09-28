use ra_ap_rustc_lexer::{tokenize, FrontmatterAllowed, TokenKind};
use std::ops::Range;

pub struct Comment {
    pub range: Range<usize>,
    pub line: usize,
}

pub fn find_comments(source: &str) -> Vec<Comment> {
    let mut comments = Vec::new();
    let mut offset = 0;
    for token in tokenize(source, FrontmatterAllowed::No) {
        let len = token.len as usize;
        if matches!(token.kind, TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }) {
            let line = source[..offset].matches('\n').count() + 1;
            comments.push(Comment { range: offset..offset + len, line });
        }
        offset += len;
    }
    comments
}

pub fn strip_comments(source: &str) -> String {
    let mut out = source.to_string();
    for comment in find_comments(source).into_iter().rev() {
        out.replace_range(removal_range(source, comment.range), "");
    }
    out
}

fn removal_range(text: &str, range: Range<usize>) -> Range<usize> {
    let bytes = text.as_bytes();
    let mut from = range.start;
    while from > 0 && matches!(bytes[from - 1], b' ' | b'\t') {
        from -= 1;
    }
    let owns_line_start = from == 0 || bytes[from - 1] == b'\n';

    let mut to = range.end;
    while to < bytes.len() && matches!(bytes[to], b' ' | b'\t') {
        to += 1;
    }
    let owns_line_end = to == bytes.len() || bytes[to] == b'\n';

    match (owns_line_start, owns_line_end) {
        (true, true) => from..(to + 1).min(bytes.len()),
        (false, true) => from..range.end,
        _ => range.start..to,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_line_block_and_doc_comments_with_line_numbers() {
        let source = "// a\nfn f() {}\n/* b */\n/// c\nfn g() {}\n";
        let lines: Vec<usize> = find_comments(source).iter().map(|c| c.line).collect();
        assert_eq!(lines, vec![1, 3, 4]);
    }

    #[test]
    fn ignores_comment_syntax_inside_strings() {
        let source = "let a = \"// not a comment\";\nlet b = r#\"/* nor this */\"#;\n";
        assert!(find_comments(source).is_empty());
    }

    #[test]
    fn strips_whole_line_comments_with_their_line() {
        assert_eq!(strip_comments("// a\nfn f() {}\n"), "fn f() {}\n");
        assert_eq!(strip_comments("    // a\n    let x = 1;\n"), "    let x = 1;\n");
    }

    #[test]
    fn strips_trailing_comments_and_the_space_before_them() {
        assert_eq!(strip_comments("let x = 1; // a\n"), "let x = 1;\n");
    }

    #[test]
    fn strips_inline_block_comments() {
        assert_eq!(strip_comments("let x = /* a */ 1;\n"), "let x = 1;\n");
    }

    #[test]
    fn strips_doc_comments_and_module_docs() {
        let source = "//! module\n\n/// item\n/// more\npub fn f() {}\n";
        assert_eq!(strip_comments(source), "\npub fn f() {}\n");
    }

    #[test]
    fn strips_multiline_block_comments() {
        let source = "/*\n * a\n */\nfn f() {}\n";
        assert_eq!(strip_comments(source), "fn f() {}\n");
    }

    #[test]
    fn strips_adjacent_comments_in_one_pass() {
        assert_eq!(strip_comments("// a\n// b\nfn f() {}\n"), "fn f() {}\n");
    }
}
