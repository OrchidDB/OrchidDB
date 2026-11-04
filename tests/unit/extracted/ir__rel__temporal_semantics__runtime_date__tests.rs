
    use super::*;
    #[test]
    fn canonical_grammar_calendar_and_range_are_proven_without_constructing_dates() {
        for text in ["0001-01-01", "9999-12-31", "2000-02-29", "2024-02-29"] { assert!(canonical_date(text), "{text}"); }
        for text in ["0000-01-01", "1900-02-29", "2023-02-29", "2024-04-31", "2024-00-01", "2024-01-00", "2024-1-01", "20240101", "2024-W01-1", "2024-001", "+10000-01-01", "é024-01-01"] { assert!(!canonical_date(text), "{text}"); }
    }
