use rs_toml::Arena;

const MARKER: &str = "sample-colour";

#[test]
fn duplicate_key_diagnostics_hide_input() {
    let source = format!("{MARKER} = 1\n{MARKER} = 2");
    let arena = Arena::new();
    let err = rs_toml::parse(&source, &arena).unwrap_err();
    assert!(!format!("{err} {err:?}").contains(MARKER));
    assert!(!err.message(&source).contains(MARKER));
    assert!(!err.message_with_path(&source).contains(MARKER));
    let path = err.path().unwrap();
    assert!(!path.to_string().contains(MARKER));
    let rs_toml::PathComponent::Key(key) = path[0] else {
        panic!("expected the duplicate key");
    };
    assert_eq!(&source[key.span.range()], MARKER);
    assert_eq!(key.name, "<key>");
    assert!(std::error::Error::source(&err).is_none());
}

use rs_toml::{Category, FormatError, Limits};
use std::collections::BTreeMap;

#[test]
fn converter_and_emitter_diagnostics_discard_arbitrary_text() {
    let error = rs_toml::Error::custom(String::from(MARKER), rs_toml::Span::new(2, 3));
    assert_eq!(error.category(), Category::Data);
    assert_eq!(error.position().unwrap().offset, Some(2));
    assert!(!format!("{error} {error:?}").contains(MARKER));
    assert!(!error.message("").contains(MARKER));
    let error = rs_toml::ToTomlError::from(std::borrow::Cow::Owned(MARKER.to_owned()));
    assert_eq!(error.category(), Category::Data);
    assert!(error.position().is_none());
    assert!(!format!("{error} {error:?} {}", error.message).contains(MARKER));
    assert!(std::error::Error::source(&error).is_none());
}

#[test]
fn byte_limits_and_recovery_stop_at_the_first_excess_byte() {
    let source = "#é\r\nx=1";
    let arena = Arena::new();
    let mut limits = Limits {
        max_document_bytes: source.len() as u64,
        ..Limits::default()
    };
    rs_toml::parse_with(source, &arena, &limits).unwrap();
    limits.max_document_bytes -= 1;
    let err = rs_toml::parse_with(source, &arena, &limits).unwrap_err();
    let pos = err.position().unwrap();
    assert_eq!(err.category(), Category::Limit);
    assert_eq!(pos.offset, Some(7));
    assert_eq!((pos.line.unwrap().get(), pos.column.unwrap().get()), (2, 3));
    let doc = rs_toml::parse_recoverable_with(source, &arena, &limits);
    assert_eq!(doc.errors()[0].position(), Some(pos));
    limits.max_document_bytes = 0;
    rs_toml::parse_with("", &arena, &limits).unwrap();
    let err = rs_toml::parse_with(" ", &arena, &limits).unwrap_err();
    assert_eq!(err.position().unwrap().offset, Some(0));
}

#[test]
fn containers_share_one_depth_budget() {
    let arena = Arena::new();
    for (source, depth) in [
        ("", 1),
        ("x=1", 1),
        ("[t]\na.b=[]", 4),
        ("[[a]]", 3),
        ("[[a]]\n[[a.b]]\nx=[{}]", 7),
        ("a={b.c=[]}", 4),
    ] {
        let mut limits = Limits {
            max_depth: depth,
            max_expansion: 0,
            ..Limits::default()
        };
        rs_toml::parse_with(source, &arena, &limits).unwrap();
        limits.max_depth -= 1;
        assert_eq!(
            rs_toml::parse_with(source, &arena, &limits)
                .unwrap_err()
                .category(),
            Category::Limit,
            "{source}"
        );
    }
    let source = format!("a={}0{}", "[".repeat(129), "]".repeat(129));
    let limits = Limits {
        max_depth: u32::MAX,
        ..Limits::default()
    };
    assert_eq!(
        rs_toml::parse_with(&source, &arena, &limits)
            .unwrap_err()
            .category(),
        Category::Limit
    );
}

#[test]
fn token_limits_count_encoded_bodies_and_exclude_delimiters() {
    let arena = Arena::new();
    for (source, body, bytes) in [
        ("''=''", 1, 0),
        ("k=12_345", 2, 6),
        ("k=-1.25e+02", 2, 9),
        ("k=+inf", 2, 4),
        ("k=false", 2, 5),
        ("k=1979-05-27 07:32:00Z", 2, 20),
        ("k=12:30:01.23", 2, 11),
        ("k='é'", 3, 2),
        ("k=\"\\u0061\"", 3, 6),
        ("k=\"\"\"\nAB\"\"\"\"\"", 5, 5),
        ("k='''\r\nab'''", 5, 4),
        ("a='''A'B''C'''", 5, 6),
        ("k=\"\"\"\\\n  x\"\"\"", 5, 5),
        ("k.abc=1", 2, 3),
        ("[\"éé\"]", 2, 4),
        ("k=[1, # ab\n2]", 7, 3),
        ("k={ #123\n a=1}", 5, 3),
    ] {
        let mut limits = Limits {
            max_token_bytes: bytes,
            ..Limits::default()
        };
        rs_toml::parse_with(source, &arena, &limits).unwrap_or_else(|e| panic!("{source:?}: {e}"));
        if bytes > 0 {
            limits.max_token_bytes -= 1;
            let e = rs_toml::parse_with(source, &arena, &limits).unwrap_err();
            assert_eq!(e.category(), Category::Limit, "{source}");
            assert_eq!(
                e.position().unwrap().offset,
                Some((body + bytes - 1) as u64),
                "{source}"
            );
        }
    }
    let limits = Limits {
        max_token_bytes: 1,
        ..Limits::default()
    };
    let recovered = rs_toml::parse_recoverable_with("x=1\nlong=0\ny=2", &arena, &limits);
    assert_eq!(recovered.errors().len(), 1);
    assert_eq!(recovered.errors()[0].category(), Category::Limit);
    assert_eq!(recovered["x"].as_i64(), Some(1));
    assert_eq!(recovered["y"].as_i64(), None);
}

#[test]
fn native_and_typed_positions_use_encoded_byte_columns() {
    let arena = Arena::new();
    let source = "#c\r\n\"é\"=☃";
    let e = rs_toml::parse(source, &arena).unwrap_err();
    assert_eq!(e.category(), Category::Syntax);
    assert!(!format!("{e} {e:?} {:?}", e.primary_label()).contains('☃'));
    let p = e.position().unwrap();
    assert_eq!(
        (
            p.offset.unwrap(),
            p.line.unwrap().get(),
            p.column.unwrap().get()
        ),
        (9, 2, 6)
    );
    let e = rs_toml::parse("a=[", &arena).unwrap_err();
    assert_eq!(e.category(), Category::Eof);
    assert_eq!(e.position().unwrap().offset, Some(3));
    let source = "#c\r\n\"é\"='wrong'";
    let e =
        rs_toml::from_str_with::<BTreeMap<String, u16>>(source, &Limits::default()).unwrap_err();
    assert_eq!(e.category(), Category::Data);
    assert_eq!(e.position(), Some(p));
    assert_eq!(e.errors[0].span().range(), 9..16);
    let limits = Limits {
        max_document_bytes: 0,
        ..Limits::default()
    };
    assert_eq!(
        rs_toml::from_str_with::<BTreeMap<String, u16>>(source, &limits)
            .unwrap_err()
            .category(),
        Category::Limit
    );
}

#[test]
#[cfg(feature = "serde")]
fn serde_custom_errors_and_unknown_keys_are_redacted_and_located() {
    use serde::Deserialize;
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Strict {}
    #[derive(Debug)]
    struct Reject;
    impl<'de> Deserialize<'de> for Reject {
        fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
            Err(serde::de::Error::custom(String::deserialize(d)?))
        }
    }
    #[derive(Debug, Deserialize)]
    struct Envelope {
        #[serde(rename = "x")]
        _value: Reject,
    }
    let source = format!("{MARKER} = 1");
    let e = rs_toml::serde::from_str_with::<Strict>(&source, &Limits::default()).unwrap_err();
    assert_eq!(e.category(), Category::Data);
    assert_eq!(e.position().unwrap().offset, Some(0));
    assert_eq!(e.span(), Some(0..MARKER.len()));
    assert!(!format!("{e} {e:?}").contains(MARKER));
    assert!(std::error::Error::source(&e).is_none());
    let source = format!("#é\r\nx='{MARKER}'");
    let e = rs_toml::serde::from_str::<Envelope>(&source).unwrap_err();
    assert_eq!(e.category(), Category::Data);
    assert_eq!(e.position().unwrap().column.unwrap().get(), 3);
    assert!(!format!("{e} {e:?}").contains(MARKER));
    let limits = Limits {
        max_token_bytes: 1,
        ..Limits::default()
    };
    let e = rs_toml::serde::from_str_with::<Strict>("aa=1", &limits).unwrap_err();
    assert_eq!(e.category(), Category::Limit);
    assert_eq!(e.position().unwrap().offset, Some(1));
}

#[test]
fn recovered_positions_track_multibyte_lines_and_earlier_header_spans() {
    let arena = Arena::new();
    let source = "#é\r\n'é' = ☃\n[α]\n[ok]\nx = 1\nx = 2\n[ok]\n";
    let doc = rs_toml::parse_recoverable(source, &arena);
    assert!(doc.errors().len() >= 4);
    for error in doc.errors() {
        let pos = error.position().unwrap();
        let offset = error.span().start as usize;
        assert_eq!(pos.offset.unwrap(), offset as u64);
        let before = &source.as_bytes()[..offset];
        assert_eq!(
            pos.line.unwrap().get() as usize,
            before.split(|&b| b == b'\n').count()
        );
        let line_start = before
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1);
        assert_eq!(pos.column.unwrap().get() as usize, offset - line_start + 1);
    }
}

#[test]
fn recovery_enforces_tokens_in_discarded_input() {
    let arena = Arena::new();
    let limits = Limits {
        max_token_bytes: 2,
        ..Limits::default()
    };
    for (text, first_excess) in [
        ("a=? #abcd\nb=1", 7),
        ("a=? 'abcd'\nb=1", 7),
        ("a=? \"\"\"abcd\"\"\"\nb=1", 9),
    ] {
        let doc = rs_toml::parse_recoverable_with(text, &arena, &limits);
        let errors = doc.errors();
        assert_eq!(errors.len(), 2, "{text}");
        assert_eq!(errors[0].category(), Category::Syntax);
        assert_eq!(errors[1].category(), Category::Limit);
        assert_eq!(errors[1].position().unwrap().offset, Some(first_excess));
        assert!(doc["b"].item().is_none());
    }
    for text in ["a=? #ab\nb=1", "a=? 'ab'\nb=1"] {
        let doc = rs_toml::parse_recoverable_with(text, &arena, &limits);
        assert_eq!(doc.errors().len(), 1);
        assert_eq!(doc["b"].as_i64(), Some(1));
    }
}

#[test]
fn limits_win_before_later_datetime_or_escape_errors() {
    let arena = Arena::new();
    for (source, max, offset) in [
        ("t=1979-05-27 12:", 10, 12),
        ("t=12:34:56.12345678901234567890", 10, 12),
        ("k=\"\\u0Z\"", 2, 5),
    ] {
        let limits = Limits {
            max_token_bytes: max,
            ..Limits::default()
        };
        let e = rs_toml::parse_with(source, &arena, &limits).unwrap_err();
        assert_eq!(e.category(), Category::Limit, "{source}");
        assert_eq!(e.position().unwrap().offset, Some(offset));
    }
}

#[test]
fn incomplete_decimal_requires_more_input() {
    let arena = Arena::new();
    for source in [
        "r=1.",
        "r=1e",
        "r=1e+",
        "r=1.0e-",
        "r=0x",
        "r=+",
        "r=12:",
        "r=2026-11-",
        "r=2026-11-25T09:00:00+",
    ] {
        let e = rs_toml::parse(source, &arena).unwrap_err();
        assert_eq!(e.category(), Category::Eof, "{source}");
        assert_eq!(e.position().unwrap().offset, Some(source.len() as u64));
        assert_eq!(
            rs_toml::parse(&format!("{source}\n"), &arena)
                .unwrap_err()
                .category(),
            Category::Syntax
        );
    }
}

#[test]
fn table_helper_reports_source_coordinates_and_missing_item_once() {
    let arena = Arena::new();
    let mut doc = rs_toml::parse("# a comment\nport='wrong'", &arena).unwrap();
    assert!(doc.table_helper().required::<u16>("port").is_err());
    let pos = doc.errors()[0].position().unwrap();
    assert_eq!(pos.line.unwrap().get(), 2);
    assert_eq!(pos.column.unwrap().get(), 6);
    assert!(doc.table_helper().required_item("missing").is_err());
    assert_eq!(doc.errors().len(), 2);
}

#[test]
fn manual_and_bulk_errors_preserve_reverse_field_order() {
    struct Fields;
    impl<'de> rs_toml::FromToml<'de> for Fields {
        fn from_toml(
            ctx: &mut rs_toml::Context<'de>,
            item: &rs_toml::Item<'de>,
        ) -> Result<Self, rs_toml::Failed> {
            let mut fields = item.table_helper(ctx)?;
            fields.optional::<u8>("last");
            fields.optional::<u8>("first");
            Ok(Self)
        }
    }
    let arena = Arena::new();
    let input = "first='a'\nlast='b'\n";
    let mut doc = rs_toml::parse(input, &arena).unwrap();
    let mut helper = doc.table_helper();
    helper.optional::<u8>("last");
    helper.optional::<u8>("first");
    let manual: Vec<_> = doc.errors().iter().map(FormatError::position).collect();
    let mut doc = rs_toml::parse(input, &arena).unwrap();
    let (_, errors) = doc.to_allowing_errors::<Fields>().unwrap();
    let bulk: Vec<_> = errors.errors.iter().map(FormatError::position).collect();
    assert_eq!(bulk, manual);
    assert_eq!(bulk[0].unwrap().line.unwrap().get(), 2);
    assert_eq!(bulk[1].unwrap().line.unwrap().get(), 1);
}

#[cfg(feature = "serde")]
#[test]
fn serde_validated_keys_and_detached_items_keep_leaf_spans() {
    #[derive(Debug, serde::Deserialize, Ord, PartialOrd, Eq, PartialEq)]
    #[serde(try_from = "String")]
    struct Key(String);
    impl TryFrom<String> for Key {
        type Error = &'static str;
        fn try_from(value: String) -> Result<Self, Self::Error> {
            if value == "ok" {
                Ok(Self(value))
            } else {
                Err("invalid name")
            }
        }
    }
    let arena = Arena::new();
    let doc = rs_toml::parse("ok=1\nbad='value'", &arena).unwrap();
    let error = rs_toml::serde::from_document::<BTreeMap<Key, u8>>(&doc).unwrap_err();
    assert_eq!(error.span(), Some(5..8));
    let error = rs_toml::serde::from_item::<Key>(doc["bad"].item().unwrap()).unwrap_err();
    assert_eq!(error.span(), Some(9..16));
    assert_eq!(error.position().unwrap().offset, Some(9));
}
