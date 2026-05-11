use antlr4rust::tree::ParseTree;

pub(crate) fn raw_text<'input, T>(ctx: &T) -> String
where
    T: ParseTree<'input>,
{
    ctx.get_text()
}

pub(crate) fn normalized_keyword(text: &str) -> String {
    text.trim().to_ascii_uppercase()
}

pub(crate) fn strip_backticks(text: &str) -> &str {
    let trimmed = text.trim();
    trimmed
        .strip_prefix('`')
        .and_then(|s| s.strip_suffix('`'))
        .unwrap_or(trimmed)
}
