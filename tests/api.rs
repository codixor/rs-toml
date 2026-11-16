use std::collections::BTreeMap;

#[test]
fn parse_and_access_document() {
    let arena = rs_toml::Arena::new();
    let doc = rs_toml::parse("title = 'demo'\n[server]\nport = 8080\n", &arena).unwrap();

    assert_eq!(doc["title"].as_str(), Some("demo"));
    assert_eq!(doc["server"]["port"].as_i64(), Some(8080));
}

#[test]
fn typed_load_and_emit_round_trip() {
    let source = "name = 'demo'\nmode = 'debug'\n";
    let values: BTreeMap<String, String> = rs_toml::from_str(source).unwrap();
    let emitted = rs_toml::to_string(&values).unwrap();

    assert!(emitted.contains("name = \"demo\""));
    assert!(emitted.contains("mode = \"debug\""));
}

#[test]
fn formatting_preserved_from_source() {
    let source = "key = \"old\"\n";
    let arena = rs_toml::Arena::new();
    let doc = rs_toml::parse(source, &arena).unwrap();

    let mut values = BTreeMap::new();
    values.insert("key", "updated");

    let emitted = rs_toml::Formatting::preserved_from(&doc)
        .format(&values)
        .unwrap();
    assert!(emitted.contains("key = \"updated\""));
}

// Dotted keys and header segments count toward the nesting limit, so input
// cannot build a tree deep enough to overflow later recursive walks.
fn assert_depth_error(source: &str) {
    let arena = rs_toml::Arena::new();
    let err = rs_toml::parse(source, &arena).unwrap_err();
    assert!(
        err.to_string().contains("Max recursion depth exceeded"),
        "{err}"
    );
}

#[test]
fn deep_dotted_key_is_rejected() {
    assert_depth_error(&format!("{}a = 1\n", "a.".repeat(100_000)));
}

#[test]
fn deep_table_header_is_rejected() {
    assert_depth_error(&format!("[{}a]\n", "a.".repeat(100_000)));
    assert_depth_error(&format!("[[{}a]]\n", "a.".repeat(100_000)));
}

#[test]
fn deep_dotted_key_in_inline_table_is_rejected() {
    assert_depth_error(&format!("x = {{ {}a = 1 }}\n", "a.".repeat(100_000)));
}

#[test]
fn dotted_keys_count_toward_value_depth() {
    // 200 dotted segments plus 100 nested arrays exceed the limit together.
    let source = format!(
        "{}a = {}1{}\n",
        "a.".repeat(200),
        "[".repeat(100),
        "]".repeat(100)
    );
    assert_depth_error(&source);
}

#[test]
fn moderate_nesting_still_parses() {
    let arena = rs_toml::Arena::new();
    let source = format!(
        "{}a = 1\n[{}b]\nc = 2\n",
        "a.".repeat(100),
        "h.".repeat(100)
    );
    let doc = rs_toml::parse(&source, &arena).unwrap();
    assert!(format!("{:?}", doc.table()).contains('1'));
}

#[test]
fn oversized_capacity_returns_none() {
    let arena = rs_toml::Arena::new();
    assert!(rs_toml::Table::try_with_capacity(rs_toml::MAX_CAPACITY + 1, &arena).is_none());
    assert!(rs_toml::Array::try_with_capacity(rs_toml::MAX_CAPACITY + 1, &arena).is_none());
    assert!(rs_toml::Table::try_with_capacity(8, &arena).is_some());
}

/// Errors are moved across threads by callers (for example a service
/// loading configuration on a worker); keep them `Send + Sync`.
#[test]
fn public_error_types_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync + 'static>() {}
    assert_send_sync::<rs_toml::Error>();
    assert_send_sync::<rs_toml::FromTomlError>();
    assert_send_sync::<rs_toml::ToTomlError>();
    assert_send_sync::<rs_toml::ErrorKind<'static>>();
    #[cfg(feature = "serde")]
    assert_send_sync::<rs_toml::serde::Error>();
}

/// Conversion errors are resolved to real paths before they are returned,
/// so an error can outlive the document and arena and still be read on
/// another thread.
#[test]
fn conversion_error_outlives_document_on_another_thread() {
    let err = rs_toml::from_str::<std::collections::BTreeMap<String, u8>>("a = 1\nb = 'x'\n")
        .unwrap_err();
    let handle = std::thread::spawn(move || {
        let error = &err.errors[0];
        error.path().map(ToString::to_string)
    });
    assert_eq!(handle.join().unwrap().as_deref(), Some("b"));
}
