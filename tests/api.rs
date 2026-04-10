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
