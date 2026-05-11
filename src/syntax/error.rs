use std::{cell::RefCell, rc::Rc};

use antlr4rust::{
    error_listener::ErrorListener, errors::ANTLRError, recognizer::Recognizer,
    token_factory::TokenFactory,
};

use crate::{GraphError, Result};

#[derive(Clone, Default)]
pub(crate) struct SyntaxErrors {
    messages: Rc<RefCell<Vec<String>>>,
}

impl SyntaxErrors {
    pub(crate) fn listener(&self) -> Self {
        self.clone()
    }

    pub(crate) fn into_result(self) -> Result<()> {
        let messages = self.messages.borrow();
        if messages.is_empty() {
            Ok(())
        } else {
            Err(GraphError::Parse(messages.join("; ")))
        }
    }
}

impl<'a, T> ErrorListener<'a, T> for SyntaxErrors
where
    T: Recognizer<'a>,
{
    fn syntax_error(
        &self,
        _recognizer: &T,
        offending_symbol: Option<&<T::TF as TokenFactory<'a>>::Inner>,
        line: isize,
        column: isize,
        msg: &str,
        _error: Option<&ANTLRError>,
    ) {
        let offending = offending_symbol
            .map(ToString::to_string)
            .unwrap_or_else(|| "<unknown>".to_owned());
        self.messages
            .borrow_mut()
            .push(format!("line {line}:{column} {msg} near {offending}"));
    }
}
