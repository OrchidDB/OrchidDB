
    use super::integer_items_as_float_text;

    #[test]
    fn rewrites_integer_items_only() {
        assert_eq!(integer_items_as_float_text("[1,9]"), "[1.000000,9.000000]");
        assert_eq!(
            integer_items_as_float_text("[10,11,12]"),
            "[10.000000,11.000000,12.000000]"
        );
        assert_eq!(integer_items_as_float_text("[-3]"), "[-3.000000]");
        assert_eq!(
            integer_items_as_float_text("[1.500000,2]"),
            "[1.500000,2.000000]"
        );
        assert_eq!(integer_items_as_float_text("[]"), "[]");
        assert_eq!(integer_items_as_float_text("[,]"), "[,]");
        assert_eq!(integer_items_as_float_text("[a1,2b]"), "[a1,2b]");
        assert_eq!(
            integer_items_as_float_text("[[1,2],[3]]"),
            "[[1.000000,2.000000],[3.000000]]"
        );
    }
