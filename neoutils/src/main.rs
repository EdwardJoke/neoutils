use clap::Parser;

mod human;

#[derive(Parser)]
#[command(name = "neoutils", about = "Neoutils toolkit installer", version)]
struct Cli {
    #[arg(
        long,
        value_delimiter = ',',
        help = "Components to install (nav, cup, ram, all)"
    )]
    components: Option<Vec<String>>,

    #[arg(
        long,
        default_value = "cargo-install",
        help = "Install method (cargo-install, curl)"
    )]
    method: String,
}

fn main() {
    let cli = Cli::parse();

    match cli.components {
        Some(comps) => llm_mode(&comps, &cli.method),
        None => human::run(),
    }
}

fn llm_mode(components: &[String], method: &str) {
    let selected = resolve_components(components);
    if selected.is_empty() {
        eprintln!("Error: no valid components. Available: nav, cup, ram, all");
        std::process::exit(1);
    }

    match method {
        "cargo-install" => {
            println!("cargo install {}", selected.join(" "));
        }
        "curl" => {
            let names: Vec<&str> = selected
                .iter()
                .map(|s| match *s {
                    "navcli" => "nav",
                    "cupcli" => "cup",
                    "ramcli" => "ram",
                    _ => unreachable!(),
                })
                .collect();
            println!(
                "curl -sSL https://neoutils.dev/install.sh | bash -s -- --components {}",
                names.join(",")
            );
        }
        _ => {
            eprintln!(
                "Error: unknown method '{}'. Use 'cargo-install' or 'curl'",
                method
            );
            std::process::exit(1);
        }
    }
}

fn resolve_components(components: &[String]) -> Vec<&'static str> {
    let mut result = Vec::new();
    for comp in components {
        match comp.to_lowercase().as_str() {
            "all" => return vec!["navcli", "cupcli", "ramcli"],
            "nav" => {
                if !result.contains(&"navcli") {
                    result.push("navcli");
                }
            }
            "cup" => {
                if !result.contains(&"cupcli") {
                    result.push("cupcli");
                }
            }
            "ram" => {
                if !result.contains(&"ramcli") {
                    result.push("ramcli");
                }
            }
            _ => {
                eprintln!("Warning: unknown component '{}', skipping", comp);
            }
        }
    }
    result
}
