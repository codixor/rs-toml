use criterion::{Criterion, criterion_group, criterion_main};
use std::collections::BTreeMap;
use std::hint::black_box;

fn parse_small(c: &mut Criterion) {
    let source = "name = 'demo'\nmode = 'debug'\n";
    c.bench_function("rs-toml/parse_small", |b| {
        b.iter(|| {
            let arena = rs_toml::Arena::new();
            let doc = rs_toml::parse(black_box(source), &arena).unwrap();
            black_box(doc["name"].as_str());
        });
    });
}

fn emit_small(c: &mut Criterion) {
    let mut values = BTreeMap::new();
    values.insert("name", "demo");
    values.insert("mode", "debug");
    c.bench_function("rs-toml/emit_small", |b| {
        b.iter(|| black_box(rs_toml::to_string(black_box(&values)).unwrap()));
    });
}

fn typed_round_trip(c: &mut Criterion) {
    let source = "name = 'demo'\nmode = 'debug'\n";
    c.bench_function("rs-toml/typed_round_trip", |b| {
        b.iter(|| {
            let parsed: BTreeMap<String, String> = rs_toml::from_str(black_box(source)).unwrap();
            black_box(rs_toml::to_string(&parsed).unwrap());
        });
    });
}

criterion_group!(toml_core, parse_small, emit_small, typed_round_trip);
criterion_main!(toml_core);
