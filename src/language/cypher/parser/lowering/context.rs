use crate::language::cypher::parser::{CypherParseError, Result};

#[derive(Debug, Default)]
pub(crate) struct ParseLowering {
    errors: Vec<CypherParseError>,
}

impl ParseLowering {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn fail(&mut self, err: CypherParseError) {
        self.errors.push(err);
    }

    pub(crate) fn finish<T>(mut self, value: T) -> Result<T> {
        if let Some(err) = self.errors.drain(..).next() {
            Err(err)
        } else {
            Ok(value)
        }
    }

    pub(crate) fn into_result(mut self) -> Result<()> {
        if let Some(err) = self.errors.drain(..).next() {
            Err(err)
        } else {
            Ok(())
        }
    }
}

pub(crate) fn unsupported<T>(message: impl Into<String>) -> Result<T> {
    Err(CypherParseError::Unsupported(message.into()))
}

pub(crate) fn missing<T>(message: impl Into<String>) -> Result<T> {
    Err(CypherParseError::Parse(message.into()))
}
