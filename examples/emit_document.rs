use std::collections::BTreeMap;

fn main() {
    let mut values = BTreeMap::new();
    values.insert("name", "workspace");
    values.insert("mode", "release");

    let output = rs_toml::to_string(&values).expect("emit should succeed");
    println!("{output}");
}
