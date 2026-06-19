fn main() {
    if let Err(error) = session_weaver::cli::run() {
        eprintln!("error: {error:#}");
        std::process::exit(1);
    }
}
