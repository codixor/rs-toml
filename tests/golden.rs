use std::collections::BTreeMap;
use std::ops::Range;

#[test]
fn emit_matches_basic_golden() {
    let mut values = BTreeMap::new();
    values.insert("mode", "debug");
    values.insert("name", "demo");

    let emitted = rs_toml::to_string(&values).unwrap();
    let expected = include_str!("golden/emit_basic.toml");

    assert_eq!(emitted, expected);
}

#[test]
fn parse_round_trip_matches_golden_shape() {
    let source = include_str!("golden/parse_basic.toml");
    let values: BTreeMap<String, String> = rs_toml::from_str(source).unwrap();

    assert_eq!(values.get("name").map(String::as_str), Some("demo"));
    assert_eq!(values.get("mode").map(String::as_str), Some("debug"));
}

#[test]
fn temporal_fixture_emits_stably() {
    let source = include_str!("golden/temporal_format.toml");
    let arena = rs_toml::Arena::new();
    let document = rs_toml::parse(source, &arena).unwrap();
    let scratch = rs_toml::Arena::new();
    let emitted = String::from_utf8(
        rs_toml::Formatting::default().format_table_to_bytes(document.into_table(), &scratch),
    )
    .unwrap();

    assert_eq!(emitted, source);
}

#[test]
fn preserved_formatting_reprojection_matches_golden() {
    let source = include_str!("golden/reprojection_preserved_input.toml");
    let expected = include_str!("golden/reprojection_preserved_expected.toml");
    let arena = rs_toml::Arena::new();
    let formatting_from = rs_toml::parse(source, &arena).unwrap();
    let mut table = rs_toml::parse(source, &arena).unwrap().into_table();

    let server = table.get_mut("server").unwrap().as_table_mut().unwrap();
    *server.get_mut("port").unwrap() = rs_toml::Item::from(9090i64);

    let scratch = rs_toml::Arena::new();
    let emitted = String::from_utf8(
        rs_toml::Formatting::preserved_from(&formatting_from)
            .with_span_projection_identity()
            .format_table_to_bytes(table, &scratch),
    )
    .unwrap();

    assert_eq!(emitted, expected);
}

#[test]
fn span_fixture_maps_back_to_value_slices() {
    let source = include_str!("golden/span_surface.toml");
    let arena = rs_toml::Arena::new();
    let document = rs_toml::parse(source, &arena).unwrap();

    let title = document["title"].item().unwrap();
    let title_span: Range<usize> = title.span().into();
    assert_eq!(&source[title_span], "\"demo\"");

    let port = document["server"]["port"].item().unwrap();
    let port_span: Range<usize> = port.span().into();
    assert_eq!(&source[port_span], "8080");
}
