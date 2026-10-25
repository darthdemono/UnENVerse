// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! The PHP array reader, held to PHP's own rules for the literals it claims to
//! understand (the PHP manual's string and integer syntax) and to its stated
//! promises: nothing is executed; whatever needs PHP to evaluate becomes `null`
//! with a warning naming the line; and a hostile file is bounded in depth and size.

use serde_json::{json, Value};
use vault_core::php_config::{parse, parse_var};

fn value(src: &str) -> Value {
    parse(src).unwrap_or_else(|e| panic!("{src}: {e}")).value
}

#[test]
fn both_array_syntaxes_lists_and_maps_are_read() {
    assert_eq!(value("$C = array('a', 'b');"), json!(["a", "b"]));
    assert_eq!(value("$C = ['a', 'b',];"), json!(["a", "b"]));
    assert_eq!(value("$C = ARRAY ( 'k' => 'v' );"), json!({ "k": "v" }));
    assert_eq!(value("$C = [0 => 'x', 1 => 'y'];"), json!(["x", "y"]));
    assert_eq!(
        value("$C = [1 => 'x', 2 => 'y'];"),
        json!({ "1": "x", "2": "y" }),
        "keys that are not exactly 0..n make a map"
    );
    assert_eq!(value("$C = ['a' => 1, 'b'];"), json!({ "a": 1, "1": "b" }));
    assert_eq!(
        value("$C = array('n' => array('deep' => [true, false, null, TRUE, Null]));"),
        json!({ "n": { "deep": [true, false, null, true, null] } })
    );
    assert_eq!(value("$C = [];"), json!([]));
}

#[test]
fn the_three_comment_styles_are_ignored_but_not_inside_strings() {
    let src = "<?php\n// line\n# hash\n/* block\n over lines */\n$C = [\n 'url' => 'http://a.test/#frag', // trailing\n /* inline */ 'b' => \"x // y\", # tail\n];";
    assert_eq!(
        value(src),
        json!({ "url": "http://a.test/#frag", "b": "x // y" })
    );
    assert_eq!(
        parse("$C = [ /* never closed").unwrap_err(),
        "The array is never closed"
    );
}

#[test]
fn single_quoted_strings_follow_phps_two_escapes() {
    assert_eq!(
        value(r"$C = ['it\'s', 'back\\slash', 'keep\nthis'];"),
        json!(["it's", "back\\slash", "keep\\nthis"])
    );
    assert_eq!(value("$C = ['multi\nline'];"), json!(["multi\nline"]));
}

#[test]
fn double_quoted_strings_follow_phps_escape_table() {
    let src = r#"$C = ["a\nb\tc\rd\ve\ef\fg", "q\"q", "d\$x", "back\\", "\101\x41\x4a", "\xZ", "\q", "\8", "$name stays"];"#;
    assert_eq!(
        value(src),
        json!([
            "a\nb\tc\rd\u{b}e\u{1b}f\u{c}g",
            "q\"q",
            "d$x",
            "back\\",
            "AAJ",
            "\\xZ",
            "\\q",
            "\\8",
            "$name stays"
        ])
    );
    // Octal takes at most three digits and wraps at a byte; non-ASCII passes through.
    assert_eq!(
        value(r#"$C = ["\1011", "café ✓"];"#),
        json!(["A1", "café ✓"])
    );
}

/// BUG: a signed exponent (`2.5E-2`, `1e+6`) was cut at the sign: the float failed to parse
/// and became `null`, and the `-2` that remained was read as a stray value.
#[test]
fn numbers_follow_phps_integer_and_float_syntax() {
    assert_eq!(
        value("$C = [0, 42, -7, +3, 1_000, 3.5, -0.25, 1e3, 2.5E-2, 1e+2];"),
        json!([0, 42, -7, 3, 1000, 3.5, -0.25, 1000.0, 0.025, 100.0])
    );
}

/// BUG: `0755` was read as the decimal 755 and `0x1A` stopped after the `0`, leaving
/// `x1A` to be misread as the next value. PHP reads them as octal 493 and hex 26, and
/// a file mode or a bit mask in a config is exactly where those appear.
#[test]
fn hex_octal_and_binary_integers_mean_what_php_means() {
    assert_eq!(
        value("$C = ['mode' => 0755, 'mask' => 0x1A, 'flags' => 0b101, 'oct' => 0o17, 'neg' => -0x10, 'zero' => 0, 'after' => 'ok'];"),
        json!({ "mode": 493, "mask": 26, "flags": 5, "oct": 15, "neg": -16, "zero": 0, "after": "ok" })
    );
    assert_eq!(value("$C = [0755, 'next'];"), json!([493, "next"]));
}

#[test]
fn what_needs_php_to_evaluate_becomes_null_with_a_warning_naming_the_line() {
    let src = "$C = [\n 'a' => getenv('X'),\n 'b' => 'one' . 'two',\n 'c' => DIR . '/x',\n 'd' => $var,\n 'e' => [1, foo(2, 3), 4],\n 'ok' => 'fine',\n];";
    let parsed = parse(src).unwrap();
    assert_eq!(
        parsed.value,
        json!({ "a": null, "b": null, "c": null, "d": null, "e": [1, null, 4], "ok": "fine" })
    );
    let w = parsed.warnings.join("\n");
    for line in [2, 3, 4, 5, 6] {
        assert!(
            w.contains(&format!("line {line}:")),
            "line {line} missing in:\n{w}"
        );
    }
    assert!(w.contains("concatenation") && w.contains("needs PHP"));
    assert_eq!(parsed.warnings.len(), 5, "{w}");
}

#[test]
fn the_wanted_assignment_is_found_in_a_script_that_does_other_things_first() {
    let src = "<?php\n$OTHER = ['no'];\n$CONFIGX = ['no'];\n$x == 1;\n$CONFIG = array('yes' => true); $CONFIG2 = ['no'];";
    assert_eq!(
        parse_var(src, "CONFIG").unwrap().value,
        json!({ "yes": true })
    );
    // The first `=` is another assignment's, and a string that mentions `=` is skipped.
    assert_eq!(
        parse("<?php $note = 'a=b'; ").unwrap_err(),
        "Expected an array on line 1"
    );
    assert_eq!(
        parse_var("<?php $A = ['x'];", "CONFIG").unwrap_err(),
        "No `$CONFIG = array(...)` assignment found"
    );
    assert_eq!(
        parse("nothing here").unwrap_err(),
        "No `$CONFIG = array(...)` assignment found"
    );
    assert_eq!(
        parse_var("$CONFIG == [];", "CONFIG").unwrap_err(),
        "No `$CONFIG = array(...)` assignment found"
    );
}

#[test]
fn damaged_input_fails_with_a_reason_naming_the_line() {
    for (src, needle) in [
        ("$C = array 'a');", "Expected `(` after `array` on line 1"),
        ("$C = 5;", "Expected an array on line 1"),
        (
            "$C = [\n'a' => 'never closed",
            "The string opened on line 2 is never closed",
        ),
        ("$C = ['a' => \"bad \\", "never closed"),
        ("$C = ['a', 'b'", "The array is never closed"),
    ] {
        let err = parse(src).unwrap_err();
        assert!(err.contains(needle), "{src:?}: {err}");
    }
}

#[test]
fn a_hostile_file_cannot_nest_or_grow_without_limit() {
    let deep = format!("$C = {}{}", "[".repeat(40), "]".repeat(40));
    assert!(parse(&deep).unwrap_err().contains("nest deeper than 32"));
    let fine = format!("$C = {}{};", "[".repeat(30), "]".repeat(30));
    assert!(parse(&fine).is_ok());

    let many = format!("$C = [{}];", "1,".repeat(100_001));
    assert_eq!(
        parse(&many).unwrap_err(),
        "The file has too many entries to read"
    );
}
