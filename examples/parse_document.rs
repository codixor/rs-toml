use rs_toml::{Arena, parse};

fn main() {
    let source = r#"
title = "demo"
[server]
port = 8080
"#;

    let arena = Arena::new();
    let doc = parse(source, &arena).expect("parse should succeed");

    println!("title={:?}", doc["title"].as_str());
    println!("port={:?}", doc["server"]["port"].as_i64());
}
