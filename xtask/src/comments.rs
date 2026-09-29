use ra_ap_rustc_lexer::{tokenize, FrontmatterAllowed, TokenKind};
use std::ops::Range;

pub struct Comment {
    pub byte_range: Range<usize>,
    pub line_number: usize,
}

pub fn comments_in(source_code: &str) -> Vec<Comment> {
    let mut comments = Vec::new();
    let mut token_start = 0;
    for token in tokenize(source_code, FrontmatterAllowed::No) {
        let token_end = token_start + token.len as usize;
        if matches!(token.kind, TokenKind::LineComment { .. } | TokenKind::BlockComment { .. }) {
            let line_number = source_code[..token_start].matches('\n').count() + 1;
            comments.push(Comment { byte_range: token_start..token_end, line_number });
        }
        token_start = token_end;
    }
    comments
}

pub fn without_comments(source_code: &str) -> String {
    let mut uncommented_code = source_code.to_string();
    for comment in comments_in(source_code).into_iter().rev() {
        uncommented_code.replace_range(comment_removal_range(source_code, comment.byte_range), "");
    }
    uncommented_code
}

fn comment_removal_range(source_code: &str, comment_range: Range<usize>) -> Range<usize> {
    let bytes = source_code.as_bytes();
    let mut removal_start = comment_range.start;
    while removal_start > 0 && matches!(bytes[removal_start - 1], b' ' | b'\t') {
        removal_start -= 1;
    }
    let is_first_on_line = removal_start == 0 || bytes[removal_start - 1] == b'\n';

    let mut removal_end = comment_range.end;
    while removal_end < bytes.len() && matches!(bytes[removal_end], b' ' | b'\t') {
        removal_end += 1;
    }
    let is_last_on_line = removal_end == bytes.len() || bytes[removal_end] == b'\n';

    match (is_first_on_line, is_last_on_line) {
        (true, true) => removal_start..(removal_end + 1).min(bytes.len()),
        (false, true) => removal_start..comment_range.end,
        _ => comment_range.start..removal_end,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_line_block_and_doc_comments_with_line_numbers() {
        let source_code = "// a\nfn f() {}\n/* b */\n/// c\nfn g() {}\n";
        let line_numbers: Vec<usize> = comments_in(source_code).iter().map(|comment| comment.line_number).collect();
        assert_eq!(line_numbers, vec![1, 3, 4]);
    }

    #[test]
    fn ignores_comment_syntax_inside_strings() {
        let source_code = "let a = \"// not a comment\";\nlet b = r#\"/* nor this */\"#;\n";
        assert!(comments_in(source_code).is_empty());
    }

    #[test]
    fn strips_whole_line_comments_with_their_line() {
        assert_eq!(without_comments("// a\nfn f() {}\n"), "fn f() {}\n");
        assert_eq!(without_comments("    // a\n    let x = 1;\n"), "    let x = 1;\n");
    }

    #[test]
    fn strips_trailing_comments_and_the_space_before_them() {
        assert_eq!(without_comments("let x = 1; // a\n"), "let x = 1;\n");
    }

    #[test]
    fn strips_inline_block_comments() {
        assert_eq!(without_comments("let x = /* a */ 1;\n"), "let x = 1;\n");
    }

    #[test]
    fn strips_doc_comments_and_module_docs() {
        let source_code = "//! module\n\n/// item\n/// more\npub fn f() {}\n";
        assert_eq!(without_comments(source_code), "\npub fn f() {}\n");
    }

    #[test]
    fn strips_multiline_block_comments() {
        let source_code = "/*\n * a\n */\nfn f() {}\n";
        assert_eq!(without_comments(source_code), "fn f() {}\n");
    }

    #[test]
    fn strips_adjacent_comments_in_one_pass() {
        assert_eq!(without_comments("// a\n// b\nfn f() {}\n"), "fn f() {}\n");
    }
}
