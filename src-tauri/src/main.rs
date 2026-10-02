fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "--snapshot") {
        std::process::exit(codealong_lib::snapshot_cli(&args));
    }
    // A subcommand of the `codealong` command, or one of its global flags, runs it. Anything else
    // starts the app: Finder and `open` pass arguments of their own, which must not turn into a
    // command line.
    if codealong_lib::is_cli(&args) {
        std::process::exit(codealong_lib::cli_main(&args));
    }
    codealong_lib::run();
}
