use std::env;
use std::io::{self, Write};
use std::process::ExitCode;

const SKILL: &str = include_str!("../../plugins/confluence/skills/confluence/SKILL.md");
const SKILL_NAME: &str = "confluence";
const SKILL_DESCRIPTION: &str =
    "Use confluence-cli to read, search, create, update, move, delete, and convert Confluence pages and attachments from the terminal.";
const SKILL_PATH: &str = "plugins/confluence/skills/confluence/SKILL.md";

fn main() -> ExitCode {
    match run(env::args().skip(1)) {
        Ok(()) => ExitCode::SUCCESS,
        Err((message, json)) => {
            if json {
                eprintln!(
                    "{{\"error\":{},\"code\":\"VALIDATION\"}}",
                    json_string(&message)
                );
            } else {
                eprintln!("Error: {message}");
            }
            ExitCode::from(2)
        }
    }
}

fn run(args: impl IntoIterator<Item = String>) -> Result<(), (String, bool)> {
    let mut positional = Vec::new();
    let mut json = false;
    let mut help = false;

    for arg in args {
        match arg.as_str() {
            "--json" => json = true,
            "-h" | "--help" => help = true,
            _ if arg.starts_with('-') => {
                return Err((format!("Unknown option '{arg}'."), json));
            }
            _ => positional.push(arg),
        }
    }

    if help {
        print_help();
        return Ok(());
    }

    match positional.as_slice() {
        [command, action] if command == "skills" && action == "list" => list_skills(json),
        [command, action, name] if command == "skills" && action == "show" => {
            show_skill(name, json)
        }
        [command] if command == "skills" => Err((
            "Usage: confluence-rs skills <list|show <name>>".to_owned(),
            json,
        )),
        [] => {
            print_help();
            Ok(())
        }
        _ => Err((
            "This Rust preview currently supports only 'skills list' and 'skills show <name>'. See docs/RUST-MIGRATION.md.".to_owned(),
            json,
        )),
    }
}

fn list_skills(json: bool) -> Result<(), (String, bool)> {
    if json {
        println!(
            "{{\"skills\":[{{\"name\":{},\"description\":{},\"path\":{}}}]}}",
            json_string(SKILL_NAME),
            json_string(SKILL_DESCRIPTION),
            json_string(SKILL_PATH)
        );
    } else {
        println!("Available skills:");
        println!("  {SKILL_NAME}\t{SKILL_DESCRIPTION}");
        println!("\nShow a skill with: confluence-rs skills show <name>");
    }
    Ok(())
}

fn show_skill(name: &str, json: bool) -> Result<(), (String, bool)> {
    if name != SKILL_NAME {
        return Err((
            format!(
                "Unknown skill '{name}'. Run 'confluence-rs skills list' to see available skills."
            ),
            json,
        ));
    }

    if json {
        println!(
            "{{\"name\":{},\"description\":{},\"path\":{},\"content\":{}}}",
            json_string(SKILL_NAME),
            json_string(SKILL_DESCRIPTION),
            json_string(SKILL_PATH),
            json_string(SKILL)
        );
    } else {
        print!("{SKILL}");
        if !SKILL.ends_with('\n') {
            println!();
        }
    }
    io::stdout()
        .flush()
        .map_err(|error| (error.to_string(), json))
}

fn print_help() {
    println!(
        "confluence-rs {} — Rust migration preview",
        env!("CARGO_PKG_VERSION")
    );
    println!("\nUSAGE:\n    confluence-rs <COMMAND> [OPTIONS]");
    println!("\nCOMMANDS:\n    skills list             List packaged agent skills\n    skills show <name>      Print a skill's complete guidance");
    println!("\nOPTIONS:\n    --json                  Emit machine-readable output\n    -h, --help              Print help");
    println!("\nThis preview does not yet replace the Node.js confluence command.");
}

fn json_string(value: &str) -> String {
    let mut output = String::with_capacity(value.len() + 2);
    output.push('"');
    for character in value.chars() {
        match character {
            '"' => output.push_str("\\\""),
            '\\' => output.push_str("\\\\"),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character <= '\u{1f}' => {
                use std::fmt::Write as FmtWrite;
                write!(output, "\\u{:04x}", character as u32)
                    .expect("writing to String cannot fail");
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_string_escapes_json_control_characters() {
        assert_eq!(
            json_string("quote \" slash \\ newline\n"),
            "\"quote \\\" slash \\\\ newline\\n\""
        );
        assert_eq!(json_string("\u{1}").as_str(), "\"\\u0001\"");
    }

    #[test]
    fn skill_body_is_packaged_and_named() {
        assert!(SKILL.starts_with("---\nname: confluence\n"));
        assert!(SKILL.contains("## Installation"));
    }

    #[test]
    fn unknown_skill_is_rejected() {
        let error = show_skill("missing", false).unwrap_err();
        assert!(error.0.contains("Unknown skill 'missing'"));
    }
}
