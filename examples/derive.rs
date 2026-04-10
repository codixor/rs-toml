use rs_toml::Toml;

#[derive(Debug, Toml)]
#[toml(Toml)]
struct Config {
    name: String,
    port: u16,
}

fn main() {
    let source = "name = 'app'\nport = 8080\n";
    let parsed: Config = rs_toml::from_str(source).expect("derive load should work");
    let emitted = rs_toml::to_string(&parsed).expect("derive emit should work");

    println!("{parsed:?}");
    println!("{emitted}");
}
