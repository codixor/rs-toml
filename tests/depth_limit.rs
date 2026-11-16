//! The parser bounds the depth of the whole item tree with one cumulative
//! budget (headers, dotted keys and inline values together), so recursive
//! consumers cannot overflow the stack on hostile input.

/// Matches the parser's `MAX_RECURSION_DEPTH`.
const MAX_DEPTH: usize = 128;

/// Stack size for the consumer threads: small enough that an unbounded
/// tree would overflow it in a debug build.
const SMALL_STACK: usize = 512 * 1024;

fn on_small_stack<F: FnOnce() + Send + 'static>(f: F) {
    std::thread::Builder::new()
        .stack_size(SMALL_STACK)
        .spawn(f)
        .unwrap()
        .join()
        .unwrap();
}

fn assert_depth_error(source: &str) {
    let arena = rs_toml::Arena::new();
    let err = rs_toml::parse(source, &arena).unwrap_err();
    assert!(
        err.message(source).contains("Max recursion depth exceeded"),
        "unexpected error: {}",
        err.message(source)
    );
    assert!(!err.span().is_empty(), "depth error must carry a span");
    assert!(err.span().range().end <= source.len());
}

/// `[[a.a...]]` with `segments` segments: every segment is an array of
/// tables, so the header alone is `2 * segments` levels deep once each
/// prefix has been declared.
fn aot_chain(segments: usize) -> String {
    let mut out = String::new();
    for n in 1..=segments {
        out.push_str("[[");
        out.push_str(&vec!["a"; n].join("."));
        out.push_str("]]\n");
    }
    out
}

/// The crafted document from the review: 257-segment array-of-tables
/// header, then a 200-segment dotted key, then 50 nested arrays.
#[test]
fn combined_header_dotted_inline_nesting_is_rejected() {
    let mut source = String::from("[[");
    source.push_str(&vec!["a"; 257].join("."));
    source.push_str("]]\n");
    source.push_str(&vec!["b"; 200].join("."));
    source.push_str(" = ");
    source.push_str(&"[".repeat(50));
    source.push('1');
    source.push_str(&"]".repeat(50));
    source.push('\n');
    assert_depth_error(&source);

    // Each piece within budget on its own, but one level too deep
    // together: 40 header tables + 39 dotted tables + 50 arrays.
    let mut source = String::from("[");
    source.push_str(&vec!["a"; 40].join("."));
    source.push_str("]\n");
    source.push_str(&vec!["b"; 40].join("."));
    source.push_str(" = ");
    source.push_str(&"[".repeat(50));
    source.push('1');
    source.push_str(&"]".repeat(50));
    assert_depth_error(&source);

    // Array-of-tables headers count two levels per segment.
    assert_depth_error(&aot_chain(MAX_DEPTH / 2 + 1));
}

#[test]
fn dotted_key_under_deep_header_is_rejected() {
    let mut source = String::from("[");
    source.push_str(&vec!["a"; MAX_DEPTH].join("."));
    source.push_str("]\nb.c = 1\n");
    assert_depth_error(&source);

    let mut source = String::from("[");
    source.push_str(&vec!["a"; MAX_DEPTH].join("."));
    source.push_str("]\nb = []\n");
    assert_depth_error(&source);
}

/// A document whose deepest container is exactly at the limit: 50 header
/// levels (25 array-of-tables segments), 50 dotted tables, 28 arrays.
fn at_limit_document() -> String {
    let mut source = aot_chain(25);
    source.push_str(&vec!["b"; 51].join("."));
    source.push_str(" = ");
    source.push_str(&"[".repeat(28));
    source.push_str("'leaf'");
    source.push_str(&"]".repeat(28));
    source.push('\n');
    source
}

#[test]
fn at_limit_document_parses() {
    let source = at_limit_document();
    let arena = rs_toml::Arena::new();
    rs_toml::parse(&source, &arena).unwrap();

    // One more array level is rejected.
    let deeper = source.replace("'leaf'", "['leaf']");
    assert_depth_error(&deeper);
}

#[test]
#[cfg_attr(miri, ignore = "deep recursion is slow under Miri")]
fn at_limit_document_is_walkable_on_small_stack() {
    on_small_stack(|| {
        let source = at_limit_document();
        let arena = rs_toml::Arena::new();
        let doc = rs_toml::parse(&source, &arena).unwrap();
        let debug = format!("{:?}", doc.table());
        assert!(debug.contains("leaf"));
        let emitted = rs_toml::to_string(doc.table()).unwrap();
        let arena2 = rs_toml::Arena::new();
        let reparsed = rs_toml::parse(&emitted, &arena2).unwrap();
        assert_eq!(reparsed.table().as_item(), doc.table().as_item());
        let cloned = doc.table().clone_in(&arena2);
        assert_eq!(cloned.as_item(), doc.table().as_item());
    });
}

#[cfg(feature = "serde")]
mod serde_depth {
    use super::{at_limit_document, on_small_stack};
    use std::collections::BTreeMap;

    #[derive(Debug, PartialEq, serde::Deserialize)]
    #[serde(untagged)]
    enum V {
        B(bool),
        I(i64),
        F(f64),
        S(String),
        A(Vec<V>),
        T(BTreeMap<String, V>),
    }

    #[test]
    #[cfg_attr(miri, ignore = "deep recursion is slow under Miri")]
    fn at_limit_document_deserializes_on_small_stack() {
        on_small_stack(|| {
            let source = at_limit_document();
            let value: V = rs_toml::serde::from_str(&source).unwrap();
            let mut depth = 0;
            let mut cur = &value;
            loop {
                match cur {
                    V::T(map) => cur = map.values().next().unwrap(),
                    V::A(items) => cur = items.last().unwrap(),
                    _ => break,
                }
                depth += 1;
            }
            assert_eq!(cur, &V::S("leaf".into()));
            // The root table plus `MAX_DEPTH` nested containers.
            assert_eq!(depth, super::MAX_DEPTH + 1);
        });
    }

    /// Trees built through the mutable API can exceed the parser's limit;
    /// the deserializer stops with an error instead of recursing.
    #[test]
    #[cfg_attr(miri, ignore = "deep recursion is slow under Miri")]
    fn deserializer_rejects_programmatically_deep_tree() {
        on_small_stack(|| {
            let arena = rs_toml::Arena::new();
            let mut item = rs_toml::Item::from(1i64);
            // The outermost array is depth 0, so this nests one level past
            // the limit.
            for _ in 0..=super::MAX_DEPTH + 1 {
                let mut array = rs_toml::Array::new();
                array.push(item, &arena);
                item = array.into_item();
            }
            let err = rs_toml::serde::from_item::<V>(&item).unwrap_err();
            assert!(
                err.message().contains("max recursion depth exceeded"),
                "{err}"
            );
        });
    }
}
