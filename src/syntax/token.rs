#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GraphToken {
    pub token_type: i32,
    pub symbolic_name: Option<&'static str>,
    pub literal_name: Option<&'static str>,
    pub text: String,
    pub line: isize,
    pub column: isize,
    pub channel: i32,
}

pub(crate) fn token_name(names: &[Option<&'static str>], token_type: i32) -> Option<&'static str> {
    if token_type < 0 {
        return None;
    }
    names
        .get(token_type as usize)
        .and_then(|name| name.as_ref().copied())
}
