use std::collections::BTreeMap;

fn main() {
    let source = r#"
name = "demo"
mode = "debug"
"#;

    let parsed: BTreeMap<String, String> =
        rs_toml::from_str(source).expect("typed load should work");
    let emitted = rs_toml::to_string(&parsed).expect("typed emit should work");

    println!("{emitted}");
}
