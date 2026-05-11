//! Generated ANTLR parser bindings.
//!
//! These modules are committed because generation is deterministic but slow,
//! and the handwritten parser integration should not depend on Java at build
//! time. Regenerate with `scripts/regenerate_antlr_parsers.sh` after changing a
//! `.g4` file.

pub mod generated {
    pub mod cypher {
        #![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
        #![allow(unused_imports, unused_mut, unused_parens, unused_variables)]

        pub mod cypherbaselistener;
        pub mod cypherbasevisitor;
        pub mod cypherlexer;
        pub mod cypherlistener;
        pub mod cypherparser;
        pub mod cyphervisitor;
    }

    pub mod gremlin {
        #![allow(non_snake_case, non_camel_case_types, non_upper_case_globals)]
        #![allow(unused_imports, unused_mut, unused_parens, unused_variables)]

        pub mod gremlinbaselistener;
        pub mod gremlinbasevisitor;
        pub mod gremlinlexer;
        pub mod gremlinlistener;
        pub mod gremlinparser;
        pub mod gremlinvisitor;
    }
}
