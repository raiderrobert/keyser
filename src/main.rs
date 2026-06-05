mod keychain;

use std::io::{self, BufRead, Write};
use std::os::unix::process::CommandExt;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        print_help(&args[0]);
        std::process::exit(2);
    }

    let exit_code = match args[1].as_str() {
        "--set" | "-s" => cmd_set(&args[2..]),
        "--list" | "-l" => cmd_list(&args[2..]),
        "--unset" => cmd_unset(&args[2..]),
        arg if !arg.starts_with('-') => cmd_exec(&args[1..]),
        _ => {
            print_help(&args[0]);
            2
        }
    };

    std::process::exit(exit_code);
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

/// --set / -s: save one or more env vars into a namespace
fn cmd_set(args: &[String]) -> i32 {
    let mut noecho = false;
    let mut require_passphrase: Option<bool> = None;
    let mut positional = Vec::new();

    for arg in args {
        match arg.as_str() {
            "--noecho" | "-n" => noecho = true,
            "--require-passphrase" | "-p" => require_passphrase = Some(true),
            "--no-require-passphrase" | "-P" => require_passphrase = Some(false),
            _ => positional.push(arg.as_str()),
        }
    }

    if positional.len() < 2 {
        eprintln!("Usage: keyser --set [-n] [-p|-P] NAMESPACE ENV [ENV ..]");
        return 1;
    }

    let namespace = positional[0];
    let keys = &positional[1..];
    let is_tty = atty::is(atty::Stream::Stdin);

    let stdin = io::stdin();
    let mut reader = stdin.lock();

    for key in keys {
        let value = if is_tty {
            if noecho {
                match rpassword::prompt_password(format!("{key}: ")) {
                    Ok(v) => v,
                    Err(e) => {
                        eprintln!("Failed to read value for {key}: {e}");
                        return 1;
                    }
                }
            } else {
                eprint!("{key}: ");
                io::stderr().flush().ok();
                let mut line = String::new();
                if let Err(e) = reader.read_line(&mut line) {
                    eprintln!("Failed to read value for {key}: {e}");
                    return 1;
                }
                line.trim_end_matches('\n').to_string()
            }
        } else {
            // Piped stdin: read one line per key
            let mut line = String::new();
            if let Err(e) = reader.read_line(&mut line) {
                eprintln!("Failed to read value for {key}: {e}");
                return 1;
            }
            line.trim_end_matches('\n').to_string()
        };

        if let Err(e) = keychain::save_value(namespace, key, &value, require_passphrase) {
            eprintln!("Failed to save {key}: {e}");
            return 1;
        }
    }

    0
}

/// --list / -l: list namespaces or keys in a namespace
fn cmd_list(args: &[String]) -> i32 {
    let mut show_value = false;
    let mut namespace = None;

    for arg in args {
        match arg.as_str() {
            "--show-value" | "-v" => show_value = true,
            _ => namespace = Some(arg.as_str()),
        }
    }

    match namespace {
        None => {
            // List all namespaces
            match keychain::search_namespaces() {
                Ok(ns) => {
                    for n in ns {
                        println!("{n}");
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Failed to list namespaces: {e}");
                    1
                }
            }
        }
        Some(ns) => {
            // List keys in namespace
            match keychain::search_values(ns) {
                Ok(pairs) => {
                    for (key, value) in pairs {
                        if show_value {
                            println!("{key}={value}");
                        } else {
                            println!("{key}");
                        }
                    }
                    0
                }
                Err(e) => {
                    eprintln!("Failed to list keys: {e}");
                    1
                }
            }
        }
    }
}

/// --unset: delete one or more keys from a namespace
fn cmd_unset(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("Usage: keyser --unset NAMESPACE ENV [ENV ..]");
        return 1;
    }

    let namespace = &args[0];
    let keys = &args[1..];

    for key in keys {
        if let Err(e) = keychain::delete_value(namespace, key) {
            eprintln!("Failed to delete {key}: {e}");
            return 1;
        }
    }

    0
}

/// Exec mode: load env vars from namespace(s) and exec a command
fn cmd_exec(args: &[String]) -> i32 {
    if args.len() < 2 {
        eprintln!("Usage: keyser NAMESPACE CMD [ARG ..]");
        return 1;
    }

    let namespaces_arg = &args[0];
    let cmd = &args[1];
    let cmd_args = &args[2..];

    // Split namespace by commas
    let namespaces: Vec<&str> = namespaces_arg.split(',').collect();

    for ns in &namespaces {
        match keychain::search_values(ns) {
            Ok(pairs) => {
                for (key, value) in pairs {
                    std::env::set_var(&key, &value);
                }
            }
            Err(keychain::KeyserError::ItemNotFound) => {
                eprintln!("keyser: warning: namespace '{}' not found, skipping", ns);
            }
            Err(e) => {
                eprintln!("keyser: failed to load namespace '{}': {}", ns, e);
                return 1;
            }
        }
    }

    let err = std::process::Command::new(cmd).args(cmd_args).exec();
    // exec() only returns on error
    eprintln!("keyser: failed to exec '{}': {}", cmd, err);
    1
}
