fn main() {
    if let Err(error) = udjat_harness::cli::run() {
        eprintln!("Error: {error}");
        std::process::exit(1);
    }
}
