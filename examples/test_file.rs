use nixos_parser::*; // Remplacez par le nom de votre crate

fn main() {
    let config = parse_nix_file("/home/quentin/config/hosts/fw-laptop-16/quentin.nix").unwrap();
    println!("{:#?}", config);
    write_nix_file("./test.nix", &config).unwrap();
}
