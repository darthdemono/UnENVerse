// SPDX-License-Identifier: PolyForm-Strict-1.0.0
// Copyright Jubair Hasan.
//! Importing a Python config module as bundle variables. The module's contract:
//! only assignments of literals are read and nothing is executed; anything else is
//! kept as source text with a warning that names the line; a name assigned twice is
//! kept under a new key; and redaction is fail-closed, so a value is marked public
//! (safe to print) only when the importer is sure, and a template built from a
//! secret never comes out printable.

use vault_core::bundle_import::{import_python_config, BundleImport, ImportedVar};

fn var<'a>(imp: &'a BundleImport, key: &str) -> &'a ImportedVar {
    imp.vars
        .iter()
        .find(|v| v.key == key)
        .unwrap_or_else(|| panic!("no {key} in {:?}", imp.vars))
}

fn warnings(imp: &BundleImport) -> String {
    imp.warnings.join("\n")
}

// ---- literals

#[test]
fn numbers_booleans_and_none_are_read_without_being_run() {
    let imp = import_python_config(
        "\
a = 7
b = -12
c = 0xFF
d = 0o17
e = 0b101
f = 1.5
g = .5
h = 2e10
i = True
j = False
k = None
l = 1 + 2
m = os.environ['X']
n == 3
o = 1.2.3
p = 0xZZ
 = 4
1x = 5
",
    );
    for (key, value, kind) in [
        ("a", "7", "int"),
        ("b", "-12", "int"),
        ("c", "0xFF", "hex_int"),
        ("d", "0o17", "hex_int"),
        ("e", "0b101", "hex_int"),
        ("f", "1.5", "float"),
        ("g", ".5", "float"),
        ("h", "2e10", "float"),
        ("i", "true", "bool"),
        ("j", "false", "bool"),
        ("k", "", "string"),
    ] {
        let v = var(&imp, key);
        assert_eq!((v.value.as_str(), v.kind.as_str()), (value, kind), "{key}");
    }
    assert_eq!(
        var(&imp, "l").value,
        "1 + 2",
        "an expression is kept as text, not evaluated"
    );
    assert_eq!(var(&imp, "m").value, "os.environ['X']");
    assert!(!var(&imp, "l").public && !var(&imp, "m").public);
    let w = warnings(&imp);
    assert!(
        w.contains("line 11: k is None; imported as an empty string"),
        "{w}"
    );
    assert!(w.contains("line 12: l is not a supported literal"), "{w}");
    assert!(w.contains("line 13: m is not a supported literal"), "{w}");
    assert!(
        w.contains("o is not a supported literal") && w.contains("p is not a supported literal")
    );
    assert!(
        imp.vars
            .iter()
            .all(|v| !["n", "", "1x"].contains(&v.key.as_str())),
        "a comparison, an empty name and an invalid name are not assignments"
    );
}

#[test]
fn strings_are_unescaped_and_an_unknown_escape_falls_back_to_source_text() {
    let imp = import_python_config(
        "a = 'it\\'s \"ok\"\\n\\t\\r\\\\'\nb = r'raw\\nstring'\nc = \"bad \\q escape\"\nd = 'x' + 'y'\ne = 'trailing'  # note\nf = 'unterminated\n",
    );
    assert_eq!(var(&imp, "a").value, "it's \"ok\"\n\t\r\\");
    assert_eq!(
        var(&imp, "b").value,
        "raw\\nstring",
        "a raw string keeps its backslashes"
    );
    assert_eq!(var(&imp, "c").value, "\"bad \\q escape\"");
    assert!(warnings(&imp).contains("line 3: unsupported escape in c"));
    assert_eq!(var(&imp, "d").value, "'x' + 'y'");
    assert_eq!(
        var(&imp, "e").value,
        "trailing",
        "a trailing comment is not part of the value"
    );
    assert!(
        warnings(&imp).contains("f is not a supported literal"),
        "an unterminated string is not silently accepted"
    );
}

// ---- kinds

#[test]
fn ids_links_and_urls_are_told_apart() {
    let imp = import_python_config(
        "\
big = '1234567890123456'
guild_id = '1234567890'
short = '12345'
site = 'https://example.com/a'
spaced = 'https://example.com/a b'
link = '[docs](https://example.com/d)'
badlink = '[docs](ftp://example.com/d)'
nested = '[a]b](https://example.com/d)'
plain = 'hello'
",
    );
    for (k, kind) in [
        ("big", "large_id"),
        ("guild_id", "large_id"),
        ("short", "string"),
        ("site", "url"),
        ("spaced", "string"),
        ("link", "markdown_link"),
        ("badlink", "string"),
        ("nested", "string"),
        ("plain", "string"),
    ] {
        assert_eq!(var(&imp, k).kind, kind, "{k}");
    }
}

// ---- f-strings

#[test]
fn an_fstring_over_earlier_names_becomes_a_template_and_one_with_an_expression_does_not() {
    let imp = import_python_config(
        "\
host = 'db.internal'
port = '5432'
dsn = f'postgres://{host}:{port}/app'
esc = f'{{literal}} {host}'
nothing = f'no holes here'
expr = f'{host.upper()}'
unknown = f'{never_defined}'
stray = f'a } b'
",
    );
    let dsn = var(&imp, "dsn");
    assert_eq!(
        (dsn.value.as_str(), dsn.kind.as_str()),
        ("postgres://{host}:{port}/app", "template")
    );
    assert_eq!(var(&imp, "esc").value, "{{literal}} {host}");
    assert_eq!(var(&imp, "esc").kind, "template");
    assert_eq!(
        var(&imp, "nothing").kind,
        "string",
        "an f-string with nothing to fill is a string"
    );
    for k in ["expr", "unknown", "stray"] {
        assert_eq!(var(&imp, k).kind, "string", "{k}");
        assert!(warnings(&imp).contains(&format!("{k} is an f-string with an expression")));
    }
}

#[test]
fn a_name_assigned_twice_keeps_the_first_and_later_references_use_the_new_key() {
    let imp = import_python_config("x = 'one'\nx = 'two'\nx = 'three'\ny = f'{x}'\n");
    assert_eq!(var(&imp, "x").value, "one");
    assert_eq!(var(&imp, "x_2").value, "two");
    assert_eq!(var(&imp, "x_3").value, "three");
    assert_eq!(
        var(&imp, "y").value,
        "{x_3}",
        "a reference follows the latest binding"
    );
    let w = warnings(&imp);
    assert!(w.contains("line 2: x is assigned more than once") && w.contains("imported as x_2"));
    assert!(w.contains("line 3") && w.contains("imported as x_3"), "{w}");
}

// ---- public: fail-closed

#[test]
fn only_values_the_importer_is_sure_of_are_marked_public() {
    let imp = import_python_config(
        "\
prefix = '!'
title = 'Welcome to the server'
retries = 3
big_number = 1234567
ratio = 0.5
debug = True
mask = 0xFF
site = 'https://example.com/docs?page=2'
doc_link = '[docs](https://example.com/d)'
api_key = 'sk_live_abcdef'
auth_header = 'Bearer x'
my_token = 12
hook = 'https://discord.com/api/webhooks/1/abc'
signed = 'https://example.com/d?token=abc'
userinfo = 'https://user:pw@example.com/'
pathtoken = 'https://example.com/a1b2c3d4e5f6g7h8i9j0k1l2m3'
opaque = 'Zm9vYmFyYmF6cXV4'
mixed = 'abc123def456ghi789jkl'
angle = '<https://example.com/page>'
angle_secret = '<https://example.com/x?key=1>'
",
    );
    let public: Vec<&str> = imp
        .vars
        .iter()
        .filter(|v| v.public)
        .map(|v| v.key.as_str())
        .collect();
    assert_eq!(
        public,
        ["prefix", "title", "retries", "ratio", "debug", "mask", "site", "doc_link", "angle"],
        "everything not listed must stay masked: {:?}",
        imp.vars
    );
}

#[test]
fn a_template_is_public_only_if_every_input_is_and_a_secret_cannot_be_laundered_through_one() {
    let imp = import_python_config(
        "\
host = 'main site'
password = 'hunter2'
user = 'svc'
open_url = f'https://{host}/'
login = f'https://{user}:{password}@{host}/'
leaky = f'{password}'
",
    );
    assert!(var(&imp, "host").public && var(&imp, "open_url").public);
    assert!(!var(&imp, "password").public);
    assert!(!var(&imp, "login").public && !var(&imp, "leaky").public);
}

#[test]
fn the_json_form_omits_public_unless_it_is_true() {
    let imp = import_python_config("open = 'hi there'\nclosed_key = 'x'\n");
    let json = serde_json::to_value(&imp.vars).unwrap();
    assert_eq!(json[0]["public"], true);
    assert!(json[1].get("public").is_none(), "{json}");
}

#[test]
fn source_text_the_importer_did_not_understand_is_never_marked_public() {
    // Each of these reads as harmless prose to a word-shape check, but each is code
    // the importer did not evaluate, and any of them can build a credential.
    let imp = import_python_config(
        "\
joined = 'pw' + 'hunter2'
call = decode('a b', 3)
escaped = 'two words \\q'
expr = f'{name.upper()} words'
",
    );
    for k in ["joined", "call", "escaped", "expr"] {
        assert!(!var(&imp, k).public, "{k}: {:?}", var(&imp, k));
    }
}
