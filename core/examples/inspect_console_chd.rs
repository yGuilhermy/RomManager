fn main() {
    let path = std::env::args_os().nth(1).expect("Informe o CHD de teste");
    let info = rom_core::console::chd::inspect(std::path::Path::new(&path))
        .expect("Metadados CHD inválidos");
    println!("{}", serde_json::to_string_pretty(&info).unwrap());
}
