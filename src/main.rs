mod keychain;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_help(&args[0]);
        std::process::exit(2);
    }
}

fn print_help(name: &str) {
    let version = env!("CARGO_PKG_VERSION");
    eprintln!(
        "{name} version {version}\n\n\
         Usage:\n  \
           Add variables\n    \
             {name} (--set|-s) [--[no-]require-passphrase|-p|-P] [--noecho|-n] NAMESPACE ENV [ENV ..]\n  \
           Execute with variables\n    \
             {name} NAMESPACE CMD [ARG ...]\n  \
           List namespaces\n    \
             {name} --list\n  \
           Remove variables\n    \
             {name} --unset NAMESPACE ENV [ENV ..]\n\n\
         Options:\n  \
           --set (-s):       Add keychain item of environment variable ENV for namespace NAMESPACE.\n  \
           --noecho (-n):    Enable noecho mode when prompting values.\n  \
           --require-passphrase (-p), --no-require-passphrase (-P):\n                    \
             Replace the item's ACL list to require passphrase (or not)."
    );
}
