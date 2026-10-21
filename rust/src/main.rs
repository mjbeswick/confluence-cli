mod client;
mod config;

use clap::{Parser, Subcommand};
use client::ConfluenceClient;
use dialoguer::Confirm;
use std::io::IsTerminal;
use std::process::ExitCode;

#[derive(Debug, Parser)]
#[command(
    name = "confluence-cli",
    version,
    about = "Rust migration preview for the Confluence CLI"
)]
struct Cli {
    /// Select a named profile from the Confluence CLI config file.
    #[arg(long, global = true)]
    profile: Option<String>,
    /// Emit JSON on stdout and machine-readable errors on stderr where supported.
    #[arg(long, global = true)]
    json: bool,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Discover agent workflow guidance packaged with the CLI.
    Skills {
        #[command(subcommand)]
        command: SkillsCommand,
    },
    /// Initialize or update a configuration profile.
    Init {
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        site_url: Option<String>,
        #[arg(long)]
        protocol: Option<String>,
        #[arg(long)]
        allow_insecure_http: bool,
        #[arg(long)]
        api_path: Option<String>,
        #[arg(long)]
        auth_type: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        cookie: Option<String>,
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        keychain: bool,
    },
    /// Manage named configuration profiles.
    Profile {
        #[command(subcommand)]
        command: ProfileCommand,
    },
    /// Show local command usage statistics.
    Stats,
    /// Install this CLI's Claude Code skill into the current project.
    InstallSkill {
        #[arg(long, default_value = "./.claude/skills/confluence")]
        dest: String,
        #[arg(short, long)]
        yes: bool,
    },
    /// Search Confluence pages by text (or pass raw CQL with --cql).
    Search {
        query: String,
        #[arg(short, long, default_value_t = 10)]
        limit: u32,
        #[arg(long, default_value = "0")]
        start: String,
        #[arg(long)]
        cql: bool,
    },
    /// List spaces, following pagination up to the requested cap.
    Spaces {
        #[arg(short, long)]
        limit: Option<u32>,
        #[arg(long)]
        all: bool,
    },
    /// Look up one space by key.
    SpaceLookup { space_key: String },
    /// Find a page by title, optionally within a space.
    Find {
        title: String,
        #[arg(long)]
        space: Option<String>,
    },
    /// List direct child pages for a page.
    Children {
        page_id: String,
        #[arg(short, long, default_value_t = 500)]
        limit: u32,
        #[arg(long)]
        recursive: bool,
        #[arg(long)]
        show_id: bool,
        #[arg(long)]
        show_url: bool,
        #[arg(long, value_enum, default_value_t = ChildrenType::Pages)]
        kind: ChildrenType,
    },
    /// Export one page to a local directory.
    Export {
        page_id: String,
        #[arg(long, default_value = ".")]
        dest: String,
        #[arg(long, default_value = "markdown")]
        format: String,
        #[arg(long)]
        file: Option<String>,
        #[arg(long)]
        recursive: bool,
        #[arg(long, default_value_t = 10)]
        max_depth: u32,
        #[arg(long)]
        exclude: Option<String>,
        #[arg(long, default_value = "100")]
        delay_ms: String,
        #[arg(long, default_value = "attachments")]
        attachments_dir: String,
        #[arg(long)]
        pattern: Option<String>,
        #[arg(long)]
        exclude_attachments: Option<String>,
        #[arg(long)]
        referenced_only: bool,
        #[arg(long)]
        skip_attachments: bool,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        overwrite: bool,
    },
    /// Create a Confluence page.
    Create {
        title: String,
        space_key: String,
        #[arg(short, long, conflicts_with = "content")]
        file: Option<String>,
        #[arg(short, long, conflicts_with = "file")]
        content: Option<String>,
        #[arg(long, default_value = "storage")]
        format: String,
        #[arg(long, default_value = "page")]
        page_type: String,
        #[arg(long)]
        front_matter: bool,
        #[arg(long)]
        plantuml_format: Option<String>,
    },
    /// Create a page under an existing parent.
    CreateChild {
        title: String,
        parent_id: String,
        #[arg(short, long, conflicts_with = "content")]
        file: Option<String>,
        #[arg(short, long, conflicts_with = "file")]
        content: Option<String>,
        #[arg(long, default_value = "storage")]
        format: String,
        #[arg(long, default_value = "page")]
        page_type: String,
        #[arg(long)]
        front_matter: bool,
        #[arg(long)]
        plantuml_format: Option<String>,
    },
    /// Update page title and/or body.
    Update {
        page_id: String,
        #[arg(long)]
        title: Option<String>,
        #[arg(short, long, conflicts_with = "content")]
        file: Option<String>,
        #[arg(short, long, conflicts_with = "file")]
        content: Option<String>,
        #[arg(long, default_value = "storage")]
        format: String,
        #[arg(long)]
        front_matter: bool,
        #[arg(long)]
        plantuml_format: Option<String>,
    },
    /// Move a page under a new parent.
    Move {
        page_id: String,
        new_parent_id: String,
        #[arg(long)]
        title: Option<String>,
    },
    /// Copy a page tree under a destination parent.
    CopyTree {
        source_page_id: String,
        target_parent_id: String,
        title: Option<String>,
        #[arg(long)]
        dry_run: bool,
        #[arg(long)]
        max_depth: Option<u32>,
        #[arg(long)]
        exclude: Option<String>,
        #[arg(long, default_value_t = 100)]
        delay_ms: u64,
        #[arg(long, default_value = " (Copy)")]
        copy_suffix: String,
        #[arg(long)]
        fail_on_error: bool,
        #[arg(short, long)]
        quiet: bool,
    },
    /// Delete a page. Destructive use requires --yes.
    Delete {
        page_id: String,
        #[arg(long)]
        yes: bool,
    },
    /// List all content properties for a page.
    PropertyList {
        page_id: String,
        #[arg(short, long)]
        limit: Option<u32>,
        #[arg(long, default_value = "0")]
        start: String,
        #[arg(long)]
        all: bool,
    },
    /// Get one content property.
    PropertyGet { page_id: String, key: String },
    /// Create or update one content property.
    PropertySet {
        page_id: String,
        key: String,
        #[arg(long, conflicts_with = "file")]
        value: Option<String>,
        #[arg(long, conflicts_with = "value")]
        file: Option<String>,
    },
    /// Delete one content property. Requires --yes.
    PropertyDelete {
        page_id: String,
        key: String,
        #[arg(long)]
        yes: bool,
    },
    /// List page comments.
    Comments {
        page_id: String,
        #[arg(short, long)]
        limit: Option<u32>,
        #[arg(long, default_value = "0")]
        start: String,
        #[arg(long)]
        location: Option<String>,
        #[arg(long)]
        depth: Option<String>,
        #[arg(long)]
        all: bool,
        #[arg(short, long, default_value = "text")]
        format: String,
    },
    /// Create a page comment.
    Comment {
        page_id: String,
        #[arg(long)]
        content: Option<String>,
        #[arg(long)]
        file: Option<String>,
        #[arg(long, default_value = "storage")]
        format: String,
        #[arg(long)]
        parent: Option<String>,
        #[arg(long, default_value = "footer")]
        location: String,
        #[arg(long)]
        inline_selection: Option<String>,
    },
    /// Delete a comment. Requires --yes.
    CommentDelete {
        comment_id: String,
        #[arg(long)]
        yes: bool,
    },
    /// Look up one comment thread.
    CommentLookup { comment_id: String },
    /// List or download attachments for a page.
    Attachments {
        page_id: String,
        #[arg(short, long)]
        limit: Option<u32>,
        #[arg(short, long)]
        pattern: Option<String>,
        #[arg(short, long)]
        download: bool,
        #[arg(long, default_value = ".")]
        dest: String,
    },
    /// Look up an attachment by ID.
    AttachmentLookup { attachment_id: String },
    /// Download one attachment's bytes to a local file.
    AttachmentDownload {
        page_id: String,
        attachment_id: String,
        #[arg(long)]
        dest: String,
    },
    /// Upload one or more files as page attachments.
    AttachmentUpload {
        page_id: String,
        #[arg(long, required = true)]
        file: Vec<String>,
        #[arg(long)]
        comment: Option<String>,
        #[arg(long)]
        replace: bool,
        #[arg(long)]
        minor_edit: bool,
    },
    /// Delete a page attachment. Requires --yes.
    AttachmentDelete {
        page_id: String,
        attachment_id: String,
        #[arg(long)]
        yes: bool,
    },
    /// List page versions.
    Versions { page_id: String },
    /// Delete an old page version. Requires --yes.
    VersionDelete {
        page_id: String,
        version_number: u32,
        #[arg(long)]
        yes: bool,
    },
    /// Preview and optionally purge old versions, keeping the newest N.
    VersionsPurge {
        page_id: String,
        #[arg(long)]
        yes: bool,
        #[arg(long, default_value = "0")]
        throttle: String,
    },
    /// Resolve or reopen a comment thread.
    CommentResolve {
        comment_id: String,
        #[arg(long)]
        reopen: bool,
    },
    /// Make an authenticated Confluence API request.
    Api {
        endpoint: String,
        #[arg(short = 'X', long)]
        method: Option<String>,
        #[arg(short = 'f', long = "field", action = clap::ArgAction::Append)]
        fields: Vec<String>,
        #[arg(short = 'H', long = "header", action = clap::ArgAction::Append)]
        headers: Vec<String>,
        #[arg(long)]
        input: Option<String>,
        #[arg(long)]
        jq: Option<String>,
        #[arg(short, long)]
        include: bool,
        #[arg(long)]
        silent: bool,
    },
    /// Get page metadata by numeric ID or Confluence URL.
    Info { page_id: String },
    /// Read a page as storage, HTML, text or Markdown.
    Read {
        page_id: String,
        #[arg(short, long, default_value = "text")]
        format: String,
        #[arg(long, value_delimiter = ',')]
        front_matter: Option<String>,
    },
    /// Retrieve page content for editing.
    Edit {
        page_id: String,
        #[arg(short, long)]
        output: Option<String>,
    },
    /// Convert content formats locally without Confluence credentials.
    Convert {
        #[arg(short = 'i', long)]
        input_file: Option<String>,
        #[arg(short = 'o', long)]
        output_file: Option<String>,
        #[arg(long)]
        input_format: String,
        #[arg(long)]
        output_format: String,
    },
    /// Show local configuration profile.
    Config,
}

#[derive(Debug, Subcommand)]
enum SkillsCommand {
    /// List packaged agent skills.
    List,
    /// Print complete skill guidance.
    Show { name: String },
}

#[derive(Debug, Subcommand)]
enum ProfileCommand {
    /// List available profiles.
    List,
    /// Select the active profile.
    Use { name: String },
    /// Add a profile from the provided options or interactive prompts.
    Add {
        name: String,
        #[arg(long)]
        domain: Option<String>,
        #[arg(long)]
        site_url: Option<String>,
        #[arg(long)]
        protocol: Option<String>,
        #[arg(long)]
        allow_insecure_http: bool,
        #[arg(long)]
        api_path: Option<String>,
        #[arg(long)]
        auth_type: Option<String>,
        #[arg(long)]
        email: Option<String>,
        #[arg(long)]
        token: Option<String>,
        #[arg(long)]
        cookie: Option<String>,
        #[arg(long)]
        read_only: bool,
        #[arg(long)]
        keychain: bool,
    },
    /// Remove a profile.
    Remove { name: String },
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum ChildrenType {
    Pages,
    Folders,
    All,
}

const SKILL: &str = include_str!("../../plugins/confluence/skills/confluence/SKILL.md");
const SKILL_NAME: &str = "confluence";
const SKILL_DESCRIPTION: &str =
    "Use confluence-cli to read, search, create, update, move, delete, and convert Confluence pages and attachments from the terminal.";
const SKILL_PATH: &str = "plugins/confluence/skills/confluence/SKILL.md";

fn main() -> ExitCode {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) =>
        {
            let _ = error.print();
            return ExitCode::SUCCESS;
        }
        Err(error) => {
            let json = std::env::args().any(|arg| arg == "--json");
            if json {
                eprintln!(
                    "{{\"error\":{},\"code\":\"VALIDATION\",\"status\":null,\"details\":null}}",
                    json_string(&error.to_string())
                );
            } else {
                let _ = error.print();
            }
            return ExitCode::from(error.exit_code() as u8);
        }
    };
    match run(cli) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            let json = error.json;
            if json {
                eprintln!(
                    "{{\"error\":{},\"code\":{},\"status\":{},\"details\":{}}}",
                    json_string(&error.message),
                    json_string(error.code),
                    error
                        .status
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "null".into()),
                    error
                        .details
                        .as_ref()
                        .map(|value| value.to_string())
                        .unwrap_or_else(|| "null".into())
                );
            } else {
                eprintln!("Error: {}", error.message);
            }
            ExitCode::from(error.exit_code)
        }
    }
}

fn run(cli: Cli) -> Result<(), CliError> {
    let json_mode = cli.json;
    match cli.command {
        Command::Skills { command } => run_skills(command, cli.json),
        Command::Config => {
            if cli.json {
                return Err(CliError::new(
                    "--json is not supported by config.",
                    "VALIDATION",
                    1,
                    true,
                ));
            }
            let path = config::config_file_path()
                .map_err(|message| CliError::new(message, "VALIDATION", 1, false))?;
            let data = std::fs::read_to_string(&path).map_err(|error| {
                CliError::new(
                    format!("Could not read config: {error}"),
                    "VALIDATION",
                    1,
                    false,
                )
            })?;
            let mut value: serde_json::Value = serde_json::from_str(&data).map_err(|error| {
                CliError::new(
                    format!("Invalid config JSON: {error}"),
                    "VALIDATION",
                    1,
                    false,
                )
            })?;
            redact_secrets(&mut value);
            println!(
                "{}",
                serde_json::to_string_pretty(&value).unwrap_or_default()
            );
            Ok(())
        }
        Command::Convert {
            input_file,
            output_file,
            input_format,
            output_format,
        } => {
            if cli.json {
                return Err(CliError::new(
                    "--json is not supported by \"convert\".",
                    "VALIDATION",
                    1,
                    true,
                ));
            }
            let input = if let Some(path) = input_file {
                std::fs::read_to_string(path).map_err(|error| {
                    CliError::new(
                        format!("Could not read input: {error}"),
                        "VALIDATION",
                        1,
                        false,
                    )
                })?
            } else if std::io::stdin().is_terminal() {
                return Err(CliError::new(
                    "No input provided. Use --input-file or pipe content via stdin.",
                    "VALIDATION",
                    1,
                    false,
                ));
            } else {
                let mut input = String::new();
                std::io::Read::read_to_string(&mut std::io::stdin(), &mut input)
                    .map_err(|error| CliError::new(error.to_string(), "VALIDATION", 1, false))?;
                input
            };
            let output = convert_content(&input, &input_format, &output_format)?;
            if let Some(path) = output_file {
                std::fs::write(path, output).map_err(|error| {
                    CliError::new(
                        format!("Could not write output: {error}"),
                        "UNKNOWN",
                        1,
                        false,
                    )
                })?;
            } else {
                print!("{output}");
                if !output.ends_with('\n') {
                    println!();
                }
            }
            config::track_command("convert", true);
            Ok(())
        }
        Command::Init {
            domain,
            site_url,
            protocol,
            allow_insecure_http,
            api_path,
            auth_type,
            email,
            token,
            cookie,
            read_only,
            keychain,
        } => {
            if cli.json {
                return Err(CliError::new(
                    "--json is not supported by \"init\".",
                    "VALIDATION",
                    1,
                    true,
                ));
            }
            let mut domain = domain;
            let mut auth_type = auth_type;
            let mut email = email;
            let mut token = token;
            let mut cookie = cookie;
            if std::io::stdin().is_terminal() {
                if domain.is_none() {
                    domain = dialoguer::Input::new()
                        .with_prompt("Confluence domain")
                        .interact_text()
                        .ok();
                }
                if auth_type.is_none() {
                    auth_type = dialoguer::Select::new()
                        .with_prompt("Authentication type")
                        .items(["basic", "bearer", "cookie", "none"])
                        .default(0)
                        .interact()
                        .ok()
                        .map(|index| ["basic", "bearer", "cookie", "none"][index].to_owned());
                }
                if auth_type.as_deref() == Some("basic") && email.is_none() {
                    email = dialoguer::Input::new()
                        .with_prompt("Email or username")
                        .interact_text()
                        .ok();
                }
                if matches!(auth_type.as_deref(), Some("basic" | "bearer")) && token.is_none() {
                    token = dialoguer::Password::new()
                        .with_prompt("Confluence API token (input hidden)")
                        .interact()
                        .ok();
                }
                if auth_type.as_deref() == Some("cookie") && cookie.is_none() {
                    cookie = dialoguer::Password::new()
                        .with_prompt("Confluence cookie (input hidden)")
                        .interact()
                        .ok();
                }
            }
            let domain = domain.ok_or_else(|| {
                CliError::new(
                    "--domain is required in non-interactive mode.",
                    "VALIDATION",
                    1,
                    false,
                )
            })?;
            config::initialize(
                &domain,
                site_url.as_deref(),
                protocol.as_deref(),
                allow_insecure_http,
                api_path.as_deref(),
                auth_type.as_deref(),
                email.as_deref(),
                token.as_deref(),
                cookie.as_deref(),
                read_only,
                cli.profile.as_deref(),
                keychain,
            )
            .map_err(|message| CliError::new(message, "VALIDATION", 1, false))?;
            println!("Configuration profile saved.");
            Ok(())
        }
        Command::Profile { command } => {
            if cli.json {
                return Err(CliError::new(
                    "--json is not supported by \"profile\".",
                    "VALIDATION",
                    1,
                    true,
                ));
            }
            match command {
                ProfileCommand::List => {
                    for profile in config::list_profiles()
                        .map_err(|message| CliError::new(message, "VALIDATION", 1, false))?
                    {
                        println!(
                            "{}{} — {}{}",
                            if profile.active { "* " } else { "  " },
                            profile.name,
                            profile.domain,
                            if profile.read_only {
                                " [read-only]"
                            } else {
                                ""
                            }
                        );
                    }
                    Ok(())
                }
                ProfileCommand::Use { name } => {
                    config::set_active_profile(&name)
                        .map_err(|message| CliError::new(message, "VALIDATION", 1, false))?;
                    println!("Active profile set to '{name}'.");
                    Ok(())
                }
                ProfileCommand::Add {
                    name,
                    mut domain,
                    site_url,
                    protocol,
                    allow_insecure_http,
                    api_path,
                    mut auth_type,
                    mut email,
                    mut token,
                    mut cookie,
                    read_only,
                    keychain,
                } => {
                    if !config::is_valid_profile_name(&name) {
                        return Err(CliError::new("Invalid profile name. Use only letters, numbers, hyphens, and underscores.", "VALIDATION", 1, false));
                    }
                    if std::io::stdin().is_terminal() {
                        if domain.is_none() {
                            domain = dialoguer::Input::new()
                                .with_prompt("Confluence domain")
                                .interact_text()
                                .ok();
                        }
                        if auth_type.is_none() {
                            auth_type = dialoguer::Select::new()
                                .with_prompt("Authentication type")
                                .items(["basic", "bearer", "cookie", "none"])
                                .default(0)
                                .interact()
                                .ok()
                                .map(|index| {
                                    ["basic", "bearer", "cookie", "none"][index].to_owned()
                                });
                        }
                        if auth_type.as_deref() == Some("basic") && email.is_none() {
                            email = dialoguer::Input::new()
                                .with_prompt("Email or username")
                                .interact_text()
                                .ok();
                        }
                        if matches!(auth_type.as_deref(), Some("basic" | "bearer"))
                            && token.is_none()
                        {
                            token = dialoguer::Password::new()
                                .with_prompt("Confluence API token (input hidden)")
                                .interact()
                                .ok();
                        }
                        if auth_type.as_deref() == Some("cookie") && cookie.is_none() {
                            cookie = dialoguer::Password::new()
                                .with_prompt("Confluence cookie (input hidden)")
                                .interact()
                                .ok();
                        }
                    }
                    let domain = domain.ok_or_else(|| {
                        CliError::new(
                            "--domain is required in non-interactive mode.",
                            "VALIDATION",
                            1,
                            false,
                        )
                    })?;
                    config::initialize(
                        &domain,
                        site_url.as_deref(),
                        protocol.as_deref(),
                        allow_insecure_http,
                        api_path.as_deref(),
                        auth_type.as_deref(),
                        email.as_deref(),
                        token.as_deref(),
                        cookie.as_deref(),
                        read_only,
                        Some(&name),
                        keychain,
                    )
                    .map_err(|message| CliError::new(message, "VALIDATION", 1, false))?;
                    println!("Profile '{name}' saved.");
                    config::track_command("profile_add", true);
                    Ok(())
                }
                ProfileCommand::Remove { name } => {
                    if !std::io::stdin().is_terminal() {
                        return Err(CliError::new(
                            "Removing a profile requires interactive confirmation.",
                            "VALIDATION",
                            1,
                            false,
                        ));
                    }
                    if !Confirm::new()
                        .with_prompt(format!("Delete profile \"{name}\"?"))
                        .default(false)
                        .interact()
                        .unwrap_or(false)
                    {
                        println!("Cancelled.");
                        return Ok(());
                    }
                    config::delete_profile(&name)
                        .map_err(|message| CliError::new(message, "VALIDATION", 1, false))?;
                    println!("Profile '{name}' removed.");
                    Ok(())
                }
            }
        }
        Command::Stats => {
            if cli.json {
                return Err(CliError::new(
                    "--json is not supported by \"stats\".",
                    "VALIDATION",
                    1,
                    true,
                ));
            }
            let stats = config::read_stats_value();
            if let Some(first) = stats.get("firstUsed").and_then(serde_json::Value::as_str) {
                println!("First used: {}", local_date(first));
            }
            if let Some(last) = stats.get("lastUsed").and_then(serde_json::Value::as_str) {
                println!("Last used: {}", local_date(last));
            }
            let (total, commands) = config::read_stats();
            println!("Command usage ({total} total):");
            for (name, count) in commands {
                println!("  {name}: {count}");
            }
            Ok(())
        }
        Command::InstallSkill { dest, yes } => {
            if cli.json {
                return Err(CliError::new(
                    "--json is not supported by \"install-skill\".",
                    "VALIDATION",
                    1,
                    true,
                ));
            }
            let destination = std::path::PathBuf::from(dest).join("SKILL.md");
            if destination.exists() && !yes {
                let confirmed = Confirm::new()
                    .with_prompt(format!(
                        "Overwrite existing skill file at {}?",
                        destination.display()
                    ))
                    .default(true)
                    .interact()
                    .unwrap_or(false);
                if !confirmed {
                    println!("Cancelled.");
                    return Ok(());
                }
            }
            config::install_skill(&destination)
                .map_err(|message| CliError::new(message, "UNKNOWN", 1, false))?;
            println!("Skill installed at {}", destination.display());
            Ok(())
        }
        command => {
            let config = config::load(cli.profile.as_deref())
                .map_err(|message| CliError::new(message, "VALIDATION", 1, cli.json))?;
            let client = client::ConfluenceClient::new(config)
                .map_err(|message| CliError::new(message, "VALIDATION", 1, cli.json))?;
            match command {
                Command::Search {
                    query,
                    limit,
                    start,
                    cql,
                } => {
                    let start = start
                        .parse::<u32>()
                        .ok()
                        .filter(|value| *value > 0 || start == "0")
                        .ok_or_else(|| {
                            CliError::new(
                                "Start must be a non-negative number.",
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?;
                    if limit == 0 {
                        return Err(CliError::new(
                            "Limit must be a positive number.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let results = client
                        .search(&query, limit, start, cql)
                        .map_err(|e| e.with_json(cli.json))?;
                    config::track_command("search", true);
                    if cli.json {
                        print_json(&serde_json::json!({
                            "query": query,
                            "start": start,
                            "resultCount": results.len(),
                            "results": results
                        }));
                    } else if results.is_empty() {
                        println!("No results found.");
                    } else {
                        println!("Found {} results:", results.len());
                        for (index, result) in results.iter().enumerate() {
                            println!(
                                "{}. {} (ID: {})",
                                start + index as u32 + 1,
                                result.title,
                                result.id
                            );
                            if !result.excerpt.is_empty() {
                                println!("   {}", result.excerpt);
                            }
                        }
                    }
                    Ok(())
                }
                Command::Spaces { limit, all } => {
                    if all && limit.is_some() {
                        return Err(CliError::new(
                            "--all cannot be combined with --limit.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let spaces = client
                        .spaces(if all {
                            None
                        } else {
                            Some(limit.unwrap_or(500))
                        })
                        .map_err(|e| e.with_json(cli.json))?;
                    config::track_command("spaces", true);
                    if cli.json {
                        print_json(
                            &serde_json::json!({ "spaceCount": spaces.len(), "spaces": spaces }),
                        );
                    } else {
                        println!("Available spaces ({}):", spaces.len());
                        for space in spaces {
                            println!("{} - {}", space.key, space.name);
                        }
                    }
                    Ok(())
                }
                Command::SpaceLookup { space_key } => {
                    match client
                        .space_lookup(&space_key)
                        .map_err(|e| e.with_json(cli.json))?
                    {
                        Some(space) => print_json_or_debug(cli.json, &space),
                        None => print_json_or_debug(
                            cli.json,
                            &serde_json::json!({ "found": false, "key": space_key }),
                        ),
                    }
                    config::track_command("space_lookup", true);
                    Ok(())
                }
                Command::Find { title, space } => {
                    let page = client
                        .find(&title, space.as_deref())
                        .map_err(|e| e.with_json(cli.json))?;
                    config::track_command("find", true);
                    if cli.json {
                        print_json(&page);
                    } else {
                        println!(
                            "{} (ID: {})",
                            page["title"].as_str().unwrap_or(&title),
                            page["id"].as_str().unwrap_or_default()
                        );
                        if let Some(url) = page["url"].as_str().filter(|url| !url.is_empty()) {
                            println!("{url}");
                        }
                    }
                    Ok(())
                }
                Command::Children {
                    page_id,
                    limit,
                    recursive,
                    show_id,
                    show_url,
                    kind,
                } => {
                    if limit == 0 {
                        return Err(CliError::new(
                            "Limit must be a positive number.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if matches!(kind, ChildrenType::Folders | ChildrenType::All) {
                        return Err(CliError::new(
                            "Folder listing is Cloud-only and is not implemented in Rust yet.",
                            "VALIDATION",
                            2,
                            cli.json,
                        ));
                    }
                    let rows = if recursive {
                        client
                            .descendants(&page_id, limit)
                            .map_err(|e| e.with_json(cli.json))?
                    } else {
                        client
                            .children(&page_id, limit)
                            .map_err(|e| e.with_json(cli.json))?
                    };
                    config::track_command("children", true);
                    if cli.json {
                        print_json(
                            &serde_json::json!({"pageId": page_id, "childCount": rows.len(), "children": rows}),
                        );
                    } else if rows.is_empty() {
                        println!("No child pages found.");
                    } else {
                        for row in rows {
                            let depth = row["depth"].as_u64().unwrap_or(1);
                            println!(
                                "{}{}{}{}",
                                "  ".repeat(depth.saturating_sub(1) as usize),
                                row["title"].as_str().unwrap_or("Untitled"),
                                if show_id {
                                    format!(" (ID: {})", row["id"].as_str().unwrap_or(""))
                                } else {
                                    String::new()
                                },
                                if show_url {
                                    format!(" — {}", row["url"].as_str().unwrap_or(""))
                                } else {
                                    String::new()
                                }
                            );
                        }
                    }
                    Ok(())
                }
                Command::Export {
                    page_id,
                    dest,
                    format,
                    file,
                    recursive,
                    max_depth,
                    exclude,
                    delay_ms,
                    attachments_dir,
                    pattern,
                    exclude_attachments,
                    referenced_only,
                    skip_attachments,
                    dry_run,
                    overwrite,
                } => {
                    if cli.json {
                        return Err(CliError::new(
                            "--json is not supported by \"export\".",
                            "VALIDATION",
                            1,
                            true,
                        ));
                    }
                    if !["markdown", "html", "storage", "text"].contains(&format.as_str()) {
                        return Err(CliError::new(
                            "Export format must be markdown, html, storage, or text.",
                            "VALIDATION",
                            1,
                            false,
                        ));
                    }
                    let options = client::ExportOptions {
                        format: format.to_lowercase(),
                        filename: file,
                        recursive,
                        max_depth,
                        exclude,
                        delay_ms: delay_ms.trim().parse::<u64>().unwrap_or(100),
                        attachments_dir,
                        pattern,
                        exclude_attachments,
                        referenced_only,
                        skip_attachments,
                        dry_run,
                        overwrite,
                    };
                    let result = client
                        .export(&page_id, &dest, &options)
                        .map_err(|error| error.with_json(cli.json))?;
                    if result["dryRun"] == true {
                        if cli.json {
                            print_json(&result);
                        } else {
                            println!(
                                "Dry run — no files written.\n{} page(s) selected for export.",
                                result["pageCount"]
                            );
                        }
                    } else {
                        if cli.json {
                            print_json(&result);
                        } else {
                            println!(
                                "Exported {}/{} page(s) to {}",
                                result["exported"],
                                result["pageCount"],
                                result["rootDirectory"].as_str().unwrap_or_default()
                            );
                        }
                    }
                    Ok(())
                }
                Command::Create {
                    title,
                    space_key,
                    file,
                    content,
                    format,
                    page_type,
                    front_matter,
                    plantuml_format,
                } => {
                    confirm_write(
                        json_mode,
                        format!("Create page '{title}' in space {space_key}?"),
                    )?;
                    if format == "folder" {
                        return Err(CliError::new(
                            "Invalid content format 'folder'; use --type folder.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if !["auto", "storage", "html", "markdown"].contains(&format.as_str()) {
                        return Err(CliError::new(
                            "Content format must be auto, storage, html, or markdown.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if let Some(format) = plantuml_format.as_deref() {
                        if !["plantuml", "plantumlcloud"].contains(&format) {
                            return Err(CliError::new(
                                "PlantUML format must be plantuml or plantumlcloud.",
                                "VALIDATION",
                                1,
                                cli.json,
                            ));
                        }
                    }
                    let (body, properties) = read_write_content(
                        file.as_deref(),
                        content.as_deref(),
                        page_type == "folder",
                        front_matter,
                    )?;
                    let result = client
                        .create_page(&title, &space_key, &body, &format, &page_type)
                        .map_err(|e| e.with_json(cli.json))?;
                    if !properties.is_empty() {
                        let page_id = result
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default();
                        for (key, value) in properties {
                            client
                                .property_set(page_id, &key, &value)
                                .map_err(|error| error.with_json(cli.json))?;
                        }
                    }
                    if cli.json {
                        print_json(&result);
                    } else {
                        println!(
                            "Page created: {} (ID: {})",
                            result["title"].as_str().unwrap_or(&title),
                            result["id"].as_str().unwrap_or_default()
                        );
                    }
                    config::track_command("create", true);
                    Ok(())
                }
                Command::CreateChild {
                    title,
                    parent_id,
                    file,
                    content,
                    format,
                    page_type,
                    front_matter,
                    plantuml_format,
                } => {
                    confirm_write(
                        json_mode,
                        format!("Create child page '{title}' under {parent_id}?"),
                    )?;
                    let (body, properties) = read_write_content(
                        file.as_deref(),
                        content.as_deref(),
                        false,
                        front_matter,
                    )?;
                    if let Some(format) = plantuml_format.as_deref() {
                        if !["plantuml", "plantumlcloud"].contains(&format) {
                            return Err(CliError::new(
                                "PlantUML format must be plantuml or plantumlcloud.",
                                "VALIDATION",
                                1,
                                cli.json,
                            ));
                        }
                    }
                    let result = client
                        .create_child_page_typed(&title, &parent_id, &body, &format, &page_type)
                        .map_err(|e| e.with_json(cli.json))?;
                    if !properties.is_empty() {
                        let page_id = result
                            .get("id")
                            .and_then(serde_json::Value::as_str)
                            .unwrap_or_default();
                        for (key, value) in properties {
                            client
                                .property_set(page_id, &key, &value)
                                .map_err(|error| error.with_json(cli.json))?;
                        }
                    }
                    if cli.json {
                        print_json(&result);
                    } else {
                        println!(
                            "Page created: {} (ID: {})",
                            result["title"].as_str().unwrap_or(&title),
                            result["id"].as_str().unwrap_or_default()
                        );
                    }
                    config::track_command("create_child", true);
                    Ok(())
                }
                Command::Update {
                    page_id,
                    title,
                    file,
                    content,
                    format,
                    front_matter,
                    plantuml_format,
                } => {
                    confirm_write(json_mode, format!("Update page {page_id}?"))?;
                    let (body, properties) = if file.is_some() || content.is_some() {
                        let (body, properties) = read_write_content(
                            file.as_deref(),
                            content.as_deref(),
                            false,
                            front_matter,
                        )?;
                        (Some(body), properties)
                    } else {
                        (None, Vec::new())
                    };
                    if let Some(format) = plantuml_format.as_deref() {
                        if !["plantuml", "plantumlcloud"].contains(&format) {
                            return Err(CliError::new(
                                "PlantUML format must be plantuml or plantumlcloud.",
                                "VALIDATION",
                                1,
                                cli.json,
                            ));
                        }
                    }
                    if title.is_none() && body.is_none() {
                        return Err(CliError::new(
                            "At least one of --title, --file, or --content is required.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let result = client
                        .update_page(&page_id, title.as_deref(), body.as_deref(), &format)
                        .map_err(|e| e.with_json(cli.json))?;
                    for (key, value) in properties {
                        client
                            .property_set(&page_id, &key, &value)
                            .map_err(|error| error.with_json(cli.json))?;
                    }
                    if cli.json {
                        print_json(&result);
                    } else {
                        println!(
                            "Page updated: {}",
                            result["id"].as_str().unwrap_or(&page_id)
                        );
                    }
                    config::track_command("update", true);
                    Ok(())
                }
                Command::Move {
                    page_id,
                    new_parent_id,
                    title,
                } => {
                    confirm_write(
                        json_mode,
                        format!("Move page {page_id} under {new_parent_id}?"),
                    )?;
                    let result = client
                        .move_page(&page_id, &new_parent_id, title.as_deref())
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        print_json(&result);
                    } else {
                        println!("Page moved: {}", result["id"].as_str().unwrap_or(&page_id));
                    }
                    config::track_command("move", true);
                    Ok(())
                }
                Command::CopyTree {
                    source_page_id,
                    target_parent_id,
                    title,
                    dry_run,
                    max_depth,
                    exclude,
                    delay_ms,
                    copy_suffix,
                    fail_on_error,
                    quiet,
                } => {
                    let excluded = exclude
                        .unwrap_or_default()
                        .split(',')
                        .map(str::trim)
                        .filter(|pattern| !pattern.is_empty())
                        .map(str::to_owned)
                        .collect::<Vec<_>>();
                    let delay_ms = delay_ms.min(3_600_000);
                    let copy_options = client::CopyTreeOptions {
                        max_depth: max_depth.unwrap_or(10),
                        exclude: excluded,
                        delay_ms,
                        copy_suffix,
                        quiet,
                    };
                    let target_info = client
                        .info(&target_parent_id)
                        .map_err(|error| error.with_json(cli.json))?;
                    let (plan, _) = client
                        .copy_tree_plan(&source_page_id, &copy_options)
                        .map_err(|e| e.with_json(cli.json))?;
                    if plan.is_empty() {
                        return Err(CliError::new(
                            "Could not plan the source page tree.",
                            "NOT_FOUND",
                            1,
                            cli.json,
                        ));
                    }
                    if plan.first().and_then(|page| page["spaceKey"].as_str())
                        != target_info.space.as_ref().map(|space| space.key.as_str())
                    {
                        return Err(CliError::new("Cannot copy pages across spaces. Source and target parent must be in the same space.", "VALIDATION", 1, cli.json));
                    }
                    if dry_run {
                        let root_title = title.clone().unwrap_or_else(|| {
                            format!(
                                "{}{}",
                                plan.first()
                                    .and_then(|page| page["title"].as_str())
                                    .unwrap_or("Untitled"),
                                copy_options.copy_suffix
                            )
                        });
                        let preview = serde_json::json!({"dryRun":true,"rootTitle":root_title,"sourcePageId":plan.first().and_then(|page|page["id"].as_str()).unwrap_or(&source_page_id),"sourceVersion":plan.first().and_then(|page|page["version"].as_u64()),"targetParentId":target_info.id,"targetParentVersion":target_info.version,"childCount":plan.len().saturating_sub(1),"pages":plan});
                        if cli.json {
                            print_json(&preview);
                        } else {
                            println!("Copy preview: {} pages would be copied.", plan.len());
                        }
                        return Ok(());
                    }
                    if !cli.json {
                        confirm_write(
                            json_mode,
                            format!(
                                "Copy {} pages from {source_page_id} to {target_parent_id}?",
                                plan.len()
                            ),
                        )?;
                    }
                    let result = client
                        .copy_tree_execute(
                            &source_page_id,
                            &target_parent_id,
                            title.as_deref(),
                            &copy_options,
                            &plan,
                        )
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        print_json(&result);
                    } else if quiet {
                        println!(
                            "Copied {} pages.",
                            result["totalCopied"].as_u64().unwrap_or_default()
                        );
                    } else {
                        println!(
                            "✅ Page tree copy completed\nTotal copied pages: {}",
                            result["totalCopied"].as_u64().unwrap_or_default()
                        );
                    }
                    if fail_on_error
                        && result["failures"]
                            .as_array()
                            .is_some_and(|failures| !failures.is_empty())
                    {
                        if cli.json {
                            std::process::exit(1);
                        }
                        return Err(CliError::new(
                            "Copy completed with failures and --fail-on-error is set.",
                            "API_ERROR",
                            1,
                            false,
                        ));
                    }
                    Ok(())
                }
                Command::Delete { page_id, yes } => {
                    if !yes && !std::io::stdin().is_terminal() {
                        return Err(CliError::new(
                            "Delete requires --yes in non-interactive Rust preview mode.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if !yes
                        && !Confirm::new()
                            .with_prompt(format!("Delete page {page_id}? This cannot be undone."))
                            .default(false)
                            .interact()
                            .unwrap_or(false)
                    {
                        return Ok(());
                    }
                    if !yes
                        && !Confirm::new()
                            .with_prompt(format!("Delete page {page_id}? This cannot be undone."))
                            .default(false)
                            .interact()
                            .unwrap_or(false)
                    {
                        return Ok(());
                    }
                    let result = client
                        .delete_page(&page_id)
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        print_json(&result);
                    } else {
                        println!("Page deleted: {}", page_id);
                    }
                    config::track_command("delete", true);
                    Ok(())
                }
                Command::PropertyList {
                    page_id,
                    limit,
                    start,
                    all,
                } => {
                    let start = start
                        .parse::<u32>()
                        .ok()
                        .filter(|value| *value > 0 || start == "0")
                        .ok_or_else(|| {
                            CliError::new(
                                "Start must be a non-negative number.",
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?;
                    if limit == Some(0) {
                        return Err(CliError::new(
                            "Limit must be a positive number.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let result = client
                        .property_list_page(&page_id, limit, start, all)
                        .map_err(|error| error.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    Ok(())
                }
                Command::PropertyGet { page_id, key } => {
                    let value = client
                        .property_get(&page_id, &key)
                        .map_err(|e| e.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &value);
                    Ok(())
                }
                Command::PropertySet {
                    page_id,
                    key,
                    value,
                    file,
                } => {
                    confirm_write(
                        json_mode,
                        format!("Set property '{key}' on page {page_id}?"),
                    )?;
                    let raw = if let Some(file) = file {
                        std::fs::read_to_string(file).map_err(|error| {
                            CliError::new(
                                format!("Could not read property file: {error}"),
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?
                    } else {
                        value.ok_or_else(|| {
                            CliError::new(
                                "Either --value or --file is required.",
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?
                    };
                    let json: serde_json::Value = serde_json::from_str(&raw).map_err(|error| {
                        CliError::new(
                            format!("Property value must be valid JSON: {error}"),
                            "VALIDATION",
                            1,
                            cli.json,
                        )
                    })?;
                    let result = client
                        .property_set(&page_id, &key, &json)
                        .map_err(|e| e.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    Ok(())
                }
                Command::PropertyDelete { page_id, key, yes } => {
                    if !yes && !std::io::stdin().is_terminal() {
                        return Err(CliError::new(
                            "Deleting a property requires --yes in the Rust preview.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if !yes
                        && !Confirm::new()
                            .with_prompt(format!(
                            "Delete property '{key}' from page {page_id}? This cannot be undone."
                        ))
                            .default(false)
                            .interact()
                            .unwrap_or(false)
                    {
                        return Ok(());
                    }
                    if !yes
                        && !Confirm::new()
                            .with_prompt(format!(
                            "Delete property '{key}' from page {page_id}? This cannot be undone."
                        ))
                            .default(false)
                            .interact()
                            .unwrap_or(false)
                    {
                        return Ok(());
                    }
                    client
                        .property_delete(&page_id, &key)
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        print_json(&serde_json::json!({"pageId":page_id,"key":key,"deleted":true}));
                    } else {
                        println!("Property deleted: {key}");
                    }
                    Ok(())
                }
                Command::Comments {
                    page_id,
                    limit,
                    start,
                    location,
                    depth,
                    all,
                    format,
                } => {
                    if !["text", "markdown", "json"].contains(&format.as_str()) {
                        return Err(CliError::new(
                            "Format must be one of: text, markdown, json",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if limit == Some(0) {
                        return Err(CliError::new(
                            "Limit must be a positive number.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let start = start
                        .parse::<u32>()
                        .ok()
                        .filter(|value| *value > 0 || start == "0")
                        .ok_or_else(|| {
                            CliError::new(
                                "Start must be a non-negative number.",
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?;
                    let locations = location
                        .map(|value| {
                            value
                                .split(',')
                                .map(|item| item.trim().to_lowercase())
                                .filter(|item| !item.is_empty())
                                .collect::<Vec<_>>()
                        })
                        .unwrap_or_default();
                    if let Some(value) = locations
                        .iter()
                        .find(|value| !["inline", "footer", "resolved"].contains(&value.as_str()))
                    {
                        return Err(CliError::new(
                            format!("Invalid location value: {value}"),
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let result = client
                        .comments_page(&page_id, limit, start, &locations, depth.as_deref(), all)
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json || format == "json" {
                        print_json(&result);
                    } else if result["comments"].as_array().is_none_or(Vec::is_empty) {
                        println!("No comments found.");
                    } else {
                        println!("Found {} comments:", result["commentCount"]);
                        println!(
                            "{}",
                            serde_json::to_string_pretty(&result["comments"]).unwrap_or_default()
                        );
                    }
                    Ok(())
                }
                Command::Comment {
                    page_id,
                    content,
                    file,
                    format,
                    parent,
                    location,
                    inline_selection,
                } => {
                    confirm_write(json_mode, format!("Create a comment on page {page_id}?"))?;
                    let content = read_content(file.as_deref(), content.as_deref(), false)?;
                    if !["storage", "html", "markdown", "auto"].contains(&format.as_str()) {
                        return Err(CliError::new(
                            "Comment format must be storage, html, markdown or auto.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if !["footer", "inline"].contains(&location.as_str()) {
                        return Err(CliError::new(
                            "Comment location must be footer or inline.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if location == "inline" && inline_selection.is_none() {
                        return Err(CliError::new(
                            "Inline comments require --inline-selection.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let result = client
                        .create_comment(&page_id, &content, &format, parent.as_deref(), &location)
                        .map_err(|e| e.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    Ok(())
                }
                Command::CommentDelete { comment_id, yes } => {
                    if !yes && !std::io::stdin().is_terminal() {
                        return Err(CliError::new(
                            "Deleting a comment requires --yes in the Rust preview.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if !yes
                        && !Confirm::new()
                            .with_prompt(format!(
                                "Delete comment {comment_id}? This cannot be undone."
                            ))
                            .default(false)
                            .interact()
                            .unwrap_or(false)
                    {
                        return Ok(());
                    }
                    if !yes
                        && !Confirm::new()
                            .with_prompt(format!(
                                "Delete comment {comment_id}? This cannot be undone."
                            ))
                            .default(false)
                            .interact()
                            .unwrap_or(false)
                    {
                        return Ok(());
                    }
                    client
                        .comment_delete(&comment_id)
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        print_json(&serde_json::json!({"commentId":comment_id,"deleted":true}));
                    } else {
                        println!("Comment deleted: {comment_id}");
                    }
                    Ok(())
                }
                Command::Attachments {
                    page_id,
                    limit,
                    pattern,
                    download,
                    dest,
                } => {
                    if limit == Some(0) {
                        return Err(CliError::new(
                            "Limit must be a positive number.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    let value = client
                        .attachments_page(&page_id, limit, pattern.as_deref())
                        .map_err(|error| error.with_json(cli.json))?;
                    if download {
                        let directory = std::path::PathBuf::from(dest);
                        std::fs::create_dir_all(&directory).map_err(|error| {
                            CliError::new(
                                format!("Could not create download directory: {error}"),
                                "UNKNOWN",
                                1,
                                cli.json,
                            )
                        })?;
                        let mut downloaded = Vec::new();
                        for attachment in
                            value["attachments"].as_array().cloned().unwrap_or_default()
                        {
                            let id = attachment
                                .get("id")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or_default();
                            let name = attachment
                                .get("title")
                                .and_then(serde_json::Value::as_str)
                                .unwrap_or("attachment");
                            let safe_name = sanitize_filename(name);
                            let mut path = directory.join(&safe_name);
                            let mut suffix = 1;
                            while path.exists() {
                                path = directory.join(format!(
                                    "{} ({suffix}).{}",
                                    path.file_stem()
                                        .and_then(|value| value.to_str())
                                        .unwrap_or("attachment"),
                                    path.extension()
                                        .and_then(|value| value.to_str())
                                        .unwrap_or("")
                                ));
                                suffix += 1;
                            }
                            let bytes = client
                                .attachment_download(&page_id, id)
                                .map_err(|error| error.with_json(cli.json))?;
                            std::fs::write(&path, bytes).map_err(|error| {
                                CliError::new(error.to_string(), "UNKNOWN", 1, cli.json)
                            })?;
                            downloaded
                                .push(serde_json::json!({"title":name,"id":id,"savedTo":path}));
                        }
                        if cli.json {
                            print_json(
                                &serde_json::json!({"attachmentCount":downloaded.len(),"downloaded":downloaded.len(),"destination":directory,"attachments":downloaded}),
                            );
                        } else {
                            println!(
                                "Downloaded {} attachments to {}",
                                downloaded.len(),
                                directory.display()
                            );
                        }
                    } else {
                        if cli.json {
                            print_json(
                                &serde_json::json!({"attachmentCount":value["attachments"].as_array().map_or(0, Vec::len),"attachments":value["attachments"]}),
                            );
                        } else {
                            emit_json_or_pretty(false, &value);
                        }
                    }
                    Ok(())
                }
                Command::AttachmentLookup { attachment_id } => {
                    let result = client
                        .attachment_lookup(&attachment_id)
                        .map_err(|e| e.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    Ok(())
                }
                Command::AttachmentDownload {
                    page_id,
                    attachment_id,
                    dest,
                } => {
                    let bytes = client
                        .attachment_download(&page_id, &attachment_id)
                        .map_err(|e| e.with_json(cli.json))?;
                    std::fs::write(&dest, bytes).map_err(|error| {
                        CliError::new(
                            format!("Could not write attachment: {error}"),
                            "UNKNOWN",
                            1,
                            cli.json,
                        )
                    })?;
                    if cli.json {
                        print_json(&serde_json::json!({"attachmentId":attachment_id,"path":dest}));
                    } else {
                        println!("Attachment downloaded to {dest}");
                    }
                    Ok(())
                }
                Command::AttachmentUpload {
                    page_id,
                    file,
                    comment,
                    replace,
                    minor_edit,
                } => {
                    confirm_write(
                        json_mode,
                        format!("Upload {} attachment(s) to page {page_id}?", file.len()),
                    )?;
                    let result = client
                        .attachment_upload(
                            &page_id,
                            &file,
                            comment.as_deref(),
                            replace,
                            minor_edit.then_some(true),
                        )
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        let uploaded = result.get("results").and_then(serde_json::Value::as_array).cloned().unwrap_or_default().iter().map(|attachment| serde_json::json!({"id":attachment.get("id"),"title":attachment.get("title"),"version":attachment.get("version").and_then(|value|value.get("number"))})).collect::<Vec<_>>();
                        print_json(&serde_json::json!({"pageId":page_id,"uploaded":uploaded}));
                    } else {
                        emit_json_or_pretty(false, &result);
                    }
                    Ok(())
                }
                Command::AttachmentDelete {
                    page_id,
                    attachment_id,
                    yes,
                } => {
                    if !yes && !std::io::stdin().is_terminal() {
                        return Err(CliError::new(
                            "Deleting an attachment requires --yes in non-interactive mode.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if !yes && !Confirm::new().with_prompt(format!("Delete attachment {attachment_id} from page {page_id}? This cannot be undone.")).default(false).interact().unwrap_or(false) { return Ok(()); }
                    client
                        .attachment_delete(&page_id, &attachment_id)
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        print_json(
                            &serde_json::json!({"pageId":page_id,"attachmentId":attachment_id,"deleted":true}),
                        );
                    } else {
                        println!("Attachment deleted: {attachment_id}");
                    }
                    Ok(())
                }
                Command::Versions { page_id } => {
                    let result = client
                        .versions(&page_id)
                        .map_err(|e| e.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    Ok(())
                }
                Command::VersionDelete {
                    page_id,
                    version_number,
                    yes,
                } => {
                    if !yes && cli.json {
                        return Err(CliError::new("Refusing to delete without confirmation in --json mode. Pass --yes to proceed.", "VALIDATION", 1, true));
                    }
                    if !yes && !std::io::stdin().is_terminal() {
                        return Err(CliError::new(
                            "Deleting a version requires --yes in non-interactive mode.",
                            "VALIDATION",
                            1,
                            false,
                        ));
                    }
                    if !yes && !Confirm::new().with_prompt(format!("Delete version {version_number} from page {page_id}? This cannot be undone.")).default(false).interact().unwrap_or(false) { return Ok(()); }
                    client
                        .version_delete(&page_id, version_number)
                        .map_err(|e| e.with_json(cli.json))?;
                    if cli.json {
                        print_json(
                            &serde_json::json!({"id":page_id,"versionNumber":version_number,"deleted":true}),
                        );
                    } else {
                        println!("Version {version_number} deleted from {page_id}.");
                    }
                    Ok(())
                }
                Command::VersionsPurge {
                    page_id,
                    yes,
                    throttle,
                } => {
                    let preview = client
                        .versions_purge_preview(&page_id, 1)
                        .map_err(|error| error.with_json(cli.json))?;
                    if preview["deletions"]
                        .as_array()
                        .is_none_or(|rows| rows.is_empty())
                    {
                        if cli.json {
                            print_json(
                                &serde_json::json!({"id":preview["pageId"],"deleted":0,"failed":0,"kept":preview["currentVersion"]}),
                            );
                        } else {
                            println!(
                                "Nothing to purge; current version {} will be kept.",
                                preview["currentVersion"]
                            );
                        }
                        return Ok(());
                    }
                    if !yes {
                        confirm_write(
                            json_mode,
                            format!(
                                "Purge {} historical versions from page {page_id}?",
                                preview["deletions"].as_array().map_or(0, Vec::len)
                            ),
                        )?;
                    }
                    let throttle_ms =
                        throttle.trim().parse::<f64>().unwrap_or(0.0).max(0.0) * 1000.0;
                    let result = client
                        .versions_purge_execute(&page_id, &preview, throttle_ms.round() as u64)
                        .map_err(|error| error.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    if result["failed"].as_u64().unwrap_or_default() > 0 {
                        if cli.json {
                            std::process::exit(1);
                        }
                        return Err(CliError::new(
                            "Version purge completed with failures.",
                            "API_ERROR",
                            1,
                            false,
                        ));
                    }
                    Ok(())
                }
                Command::CommentResolve { comment_id, reopen } => {
                    let result = client
                        .comment_resolve(&comment_id, !reopen)
                        .map_err(|e| e.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    Ok(())
                }
                Command::CommentLookup { comment_id } => {
                    let result = client
                        .comment_lookup(&comment_id)
                        .map_err(|error| error.with_json(cli.json))?;
                    emit_json_or_pretty(cli.json, &result);
                    Ok(())
                }
                Command::Api {
                    endpoint,
                    method,
                    fields,
                    headers,
                    input,
                    jq,
                    include,
                    silent,
                } => {
                    let mut parameters = serde_json::Map::new();
                    for raw in fields {
                        let (key, value) = raw.split_once('=').ok_or_else(|| {
                            CliError::new(
                                format!("Invalid field \"{raw}\". Must be key=value."),
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?;
                        parameters
                            .insert(key.to_owned(), serde_json::Value::String(value.to_owned()));
                    }
                    let mut extra_headers = reqwest::header::HeaderMap::new();
                    for raw in headers {
                        let (key, value) = raw.split_once(':').ok_or_else(|| {
                            CliError::new(
                                format!("Invalid header \"{raw}\". Must be key:value."),
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?;
                        let name = reqwest::header::HeaderName::from_bytes(key.trim().as_bytes())
                            .map_err(|_| {
                            CliError::new(
                                format!("Invalid header name '{key}'."),
                                "VALIDATION",
                                1,
                                cli.json,
                            )
                        })?;
                        let value =
                            reqwest::header::HeaderValue::from_str(value.trim()).map_err(|_| {
                                CliError::new("Invalid header value.", "VALIDATION", 1, cli.json)
                            })?;
                        extra_headers.insert(name, value);
                    }
                    let raw_body = if let Some(input) = input {
                        if input == "-" {
                            let mut body = String::new();
                            std::io::Read::read_to_string(&mut std::io::stdin(), &mut body)
                                .map_err(|error| {
                                    CliError::new(error.to_string(), "VALIDATION", 1, cli.json)
                                })?;
                            Some(body)
                        } else {
                            Some(std::fs::read_to_string(input).map_err(|error| {
                                CliError::new(error.to_string(), "VALIDATION", 1, cli.json)
                            })?)
                        }
                    } else {
                        None
                    };
                    let body = raw_body.map(|text| {
                        serde_json::from_str::<serde_json::Value>(&text)
                            .unwrap_or(serde_json::Value::String(text))
                    });
                    let method = method
                        .unwrap_or_else(|| {
                            if body.is_some() || !parameters.is_empty() {
                                "POST".into()
                            } else {
                                "GET".into()
                            }
                        })
                        .to_uppercase();
                    if client.is_read_only()
                        && ["POST", "PUT", "PATCH", "DELETE"].contains(&method.as_str())
                    {
                        return Err(CliError::new(
                            "This profile is in read-only mode. Write operations are not allowed.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if !["GET", "HEAD"].contains(&method.as_str()) {
                        confirm_write(json_mode, format!("Send {method} request to {endpoint}?"))?;
                    }
                    let response = client
                        .raw_request(
                            &method,
                            &endpoint,
                            &extra_headers,
                            &parameters,
                            body.as_ref(),
                        )
                        .map_err(|error| error.with_json(cli.json))?;
                    if silent {
                        config::track_command("api", true);
                        return Ok(());
                    }
                    if include {
                        println!("HTTP {}", response.status);
                        for (key, value) in &response.headers {
                            println!("{key}: {value}");
                        }
                        println!();
                    }
                    if let Some(expression) = jq {
                        let mut child = std::process::Command::new("jq")
                            .arg(expression)
                            .stdin(std::process::Stdio::piped())
                            .stdout(std::process::Stdio::piped())
                            .stderr(std::process::Stdio::piped())
                            .spawn()
                            .map_err(|error| {
                                CliError::new(
                                    if error.kind() == std::io::ErrorKind::NotFound {
                                        "jq is not installed (--jq requires jq in PATH).".to_owned()
                                    } else {
                                        format!("jq failed: {error}")
                                    },
                                    "VALIDATION",
                                    2,
                                    cli.json,
                                )
                            })?;
                        if let Some(mut stdin) = child.stdin.take() {
                            std::io::Write::write_all(&mut stdin, response.body.as_bytes())
                                .map_err(|error| {
                                    CliError::new(error.to_string(), "VALIDATION", 2, cli.json)
                                })?;
                        }
                        let filtered = child.wait_with_output().map_err(|error| {
                            CliError::new(error.to_string(), "VALIDATION", 2, cli.json)
                        })?;
                        if !filtered.status.success() {
                            return Err(CliError::new(
                                format!(
                                    "jq exited with status {}",
                                    filtered.status.code().unwrap_or(1)
                                ),
                                "VALIDATION",
                                2,
                                cli.json,
                            ));
                        }
                        print!("{}", String::from_utf8_lossy(&filtered.stdout));
                    } else {
                        print!("{}", response.body);
                    }
                    if !response.body.ends_with('\n') {
                        println!();
                    }
                    config::track_command("api", true);
                    Ok(())
                }
                Command::Info { page_id } => {
                    let info = client.info(&page_id).map_err(|e| e.with_json(cli.json))?;
                    config::track_command("info", true);
                    if cli.json {
                        print_json(
                            &serde_json::to_value(&info).expect("serializing response cannot fail"),
                        );
                    } else {
                        println!("Page Information:");
                        println!("Title: {}", info.title);
                        println!("ID: {}", info.id);
                        println!("Type: {}", info.page_type);
                        println!("Status: {}", info.status);
                        if let Some(space) = info.space {
                            println!("Space: {} ({})", space.name, space.key);
                        }
                    }
                    Ok(())
                }
                Command::Read {
                    page_id,
                    format,
                    front_matter,
                } => {
                    if cli.json {
                        return Err(CliError::new(
                            "--json is not supported by \"read\". Supported Rust preview commands: info, search, spaces, space-lookup, skills.",
                            "VALIDATION",
                            1,
                            true,
                        ));
                    }
                    if !["text", "storage", "html", "markdown"].contains(&format.as_str()) {
                        return Err(CliError::new(
                            "Format must be text, storage, html, or markdown.",
                            "VALIDATION",
                            1,
                            cli.json,
                        ));
                    }
                    if front_matter.is_some() && format != "markdown" {
                        return Err(CliError::new(
                            "--front-matter requires --format markdown.",
                            "VALIDATION",
                            1,
                            false,
                        ));
                    }
                    let content = client
                        .read(&page_id, &format)
                        .map_err(|e| e.with_json(cli.json))?;
                    let content = if let Some(keys) = front_matter {
                        let id = client::extract_page_id_for_cli(&page_id).ok_or_else(|| {
                            CliError::new(
                                "Could not extract a page ID from the supplied value.",
                                "VALIDATION",
                                1,
                                false,
                            )
                        })?;
                        let mut properties = serde_json::Map::new();
                        for key in keys.split(',').map(str::trim).filter(|key| !key.is_empty()) {
                            if let Ok(property) = client.property_get(&id, key) {
                                if property.get("found") != Some(&serde_json::Value::Bool(false)) {
                                    properties.insert(
                                        key.to_owned(),
                                        property
                                            .get("value")
                                            .cloned()
                                            .unwrap_or(serde_json::Value::Null),
                                    );
                                }
                            }
                        }
                        let yaml = properties
                            .iter()
                            .map(|(key, value)| {
                                format!(
                                    "  {key}: {}\n",
                                    serde_json::to_string(value).unwrap_or_else(|_| "null".into())
                                )
                            })
                            .collect::<String>();
                        format!("---\nproperties:\n{yaml}---\n\n{content}")
                    } else {
                        content
                    };
                    print!("{content}");
                    if !content.ends_with('\n') {
                        println!();
                    }
                    config::track_command("read", true);
                    Ok(())
                }
                Command::Edit { page_id, output } => {
                    let page = client
                        .get_page_for_edit(&page_id)
                        .map_err(|error| error.with_json(false))?;
                    if let Some(path) = output {
                        std::fs::write(&path, &page.content).map_err(|error| {
                            CliError::new(
                                format!("Could not write content file: {error}"),
                                "UNKNOWN",
                                1,
                                false,
                            )
                        })?;
                        println!("Page information:\nTitle: {}\nID: {}\nVersion: {}\nSpace: {} ({})\n\nContent saved to: {}\nEdit the file and use `confluence-cli update` to save changes.", page.title, page.id, page.version, page.space_name, page.space_key, path);
                    } else {
                        println!("Page information:\nTitle: {}\nID: {}\nVersion: {}\nSpace: {} ({})\n\nPage content:\n{}", page.title, page.id, page.version, page.space_name, page.space_key, page.content);
                    }
                    config::track_command("edit", true);
                    Ok(())
                }
                Command::Skills { .. } => {
                    unreachable!("skills dispatched before config resolution")
                }
                Command::Init { .. }
                | Command::Profile { .. }
                | Command::Stats
                | Command::InstallSkill { .. }
                | Command::Convert { .. }
                | Command::Config => {
                    unreachable!("config management dispatched before API configuration")
                }
            }
        }
    }
}

fn run_skills(command: SkillsCommand, json: bool) -> Result<(), CliError> {
    match command {
        SkillsCommand::List => {
            if json {
                print_json(&serde_json::json!({ "skills": [{
                    "name": SKILL_NAME,
                    "description": SKILL_DESCRIPTION,
                    "path": SKILL_PATH
                }] }));
            } else {
                println!("Available skills:");
                println!("  {SKILL_NAME}\t{SKILL_DESCRIPTION}");
                println!("\nShow a skill with: confluence skills show <name>");
            }
        }
        SkillsCommand::Show { name } => {
            if name != SKILL_NAME {
                return Err(CliError::new(
                    format!("Unknown skill '{name}'. Run 'confluence skills list' to see available skills."),
                    "VALIDATION",
                    2,
                    json,
                ));
            }
            if json {
                print_json(&serde_json::json!({
                    "name": SKILL_NAME,
                    "description": SKILL_DESCRIPTION,
                    "path": SKILL_PATH,
                    "content": SKILL
                }));
            } else {
                print!("{SKILL}");
            }
        }
    }
    Ok(())
}

fn print_json(value: &serde_json::Value) {
    println!(
        "{}",
        serde_json::to_string(value).expect("serializing JSON value cannot fail")
    );
}

fn emit_json_or_pretty(json: bool, value: &serde_json::Value) {
    if json {
        print_json(value);
    } else {
        println!(
            "{}",
            serde_json::to_string_pretty(value).unwrap_or_default()
        );
    }
}

fn print_json_or_debug<T: serde::Serialize + std::fmt::Debug>(json: bool, value: &T) {
    let _ = json;
    print_json(&serde_json::to_value(value).expect("serializing response cannot fail"));
}

fn json_string(value: &str) -> String {
    serde_json::to_string(value).expect("serializing string cannot fail")
}

fn local_date(value: &str) -> String {
    chrono::DateTime::parse_from_rfc3339(value)
        .map(|date| date.format("%-m/%-d/%Y").to_string())
        .unwrap_or_else(|_| value.to_owned())
}

fn sanitize_filename(value: &str) -> String {
    let cleaned = value
        .chars()
        .map(|character| {
            if character.is_control()
                || matches!(
                    character,
                    '/' | '\\' | ':' | '*' | '?' | '"' | '<' | '>' | '|'
                )
            {
                '_'
            } else {
                character
            }
        })
        .collect::<String>();
    let cleaned = cleaned.trim().trim_matches('.');
    if cleaned.is_empty() {
        "attachment".into()
    } else {
        cleaned.chars().take(180).collect()
    }
}

fn redact_secrets(value: &mut serde_json::Value) {
    match value {
        serde_json::Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                if ["token", "cookie", "clientKey", "tlsClientKey", "password"]
                    .iter()
                    .any(|secret| key.eq_ignore_ascii_case(secret))
                {
                    *value = serde_json::Value::String("[REDACTED]".into());
                } else {
                    redact_secrets(value);
                }
            }
        }
        serde_json::Value::Array(array) => {
            for value in array {
                redact_secrets(value);
            }
        }
        _ => {}
    }
}

fn read_content(
    file: Option<&str>,
    content: Option<&str>,
    folder: bool,
) -> Result<String, CliError> {
    if let Some(file) = file {
        return std::fs::read_to_string(file).map_err(|error| {
            CliError::new(
                format!("Could not read content file {file}: {error}"),
                "VALIDATION",
                1,
                false,
            )
        });
    }
    if let Some(content) = content {
        return Ok(content.to_owned());
    }
    if folder {
        Ok(String::new())
    } else {
        Err(CliError::new(
            "Either --file or --content option is required",
            "VALIDATION",
            1,
            false,
        ))
    }
}

fn read_write_content(
    file: Option<&str>,
    content: Option<&str>,
    folder: bool,
    front_matter: bool,
) -> Result<(String, Vec<(String, serde_json::Value)>), CliError> {
    let mut raw = read_content(file, content, folder)?;
    if !front_matter {
        return Ok((raw, Vec::new()));
    }
    if !raw.starts_with("---\n") {
        return Ok((raw, Vec::new()));
    }
    let end = raw[4..]
        .find("\n---")
        .map(|offset| offset + 4)
        .ok_or_else(|| {
            CliError::new(
                "Invalid YAML front matter: missing closing ---.",
                "VALIDATION",
                1,
                false,
            )
        })?;
    let front = raw[4..end].to_owned();
    raw = raw[(end + 4).min(raw.len())..]
        .trim_start_matches('\n')
        .to_owned();
    let mut in_properties = false;
    let mut properties = Vec::new();
    for line in front.lines() {
        if line.trim() == "properties:" {
            in_properties = true;
            continue;
        }
        if !in_properties || line.trim().is_empty() {
            continue;
        }
        let (key, value) = line.trim().split_once(':').ok_or_else(|| {
            CliError::new(
                "Invalid properties front matter line.",
                "VALIDATION",
                1,
                false,
            )
        })?;
        let value: serde_json::Value = serde_json::from_str(value.trim())
            .or_else(|_| {
                Ok::<_, serde_json::Error>(serde_json::Value::String(
                    value.trim().trim_matches(['"', '\'']).to_owned(),
                ))
            })
            .map_err(|error| CliError::new(error.to_string(), "VALIDATION", 1, false))?;
        properties.push((key.trim().to_owned(), value));
    }
    Ok((raw, properties))
}

fn confirm_write(json: bool, prompt: String) -> Result<(), CliError> {
    if json {
        return Err(CliError::new(
            "--json cannot be used for a write without an interactive confirmation UI.",
            "VALIDATION",
            1,
            true,
        ));
    }
    if !std::io::stdin().is_terminal() {
        return Err(CliError::new(
            "Write commands require interactive confirmation; refusing non-interactive execution.",
            "VALIDATION",
            1,
            false,
        ));
    }
    if Confirm::new()
        .with_prompt(prompt)
        .default(false)
        .interact()
        .unwrap_or(false)
    {
        Ok(())
    } else {
        Err(CliError::new("Cancelled.", "VALIDATION", 2, false))
    }
}

fn convert_content(input: &str, from: &str, to: &str) -> Result<String, CliError> {
    let valid_in = ["markdown", "storage", "html"];
    let valid_out = ["markdown", "storage", "html", "text"];
    if !valid_in.contains(&from) {
        return Err(CliError::new(
            format!("Invalid input format '{from}'. Valid: markdown, storage, html"),
            "VALIDATION",
            1,
            false,
        ));
    }
    if !valid_out.contains(&to) {
        return Err(CliError::new(
            format!("Invalid output format '{to}'. Valid: markdown, storage, html, text"),
            "VALIDATION",
            1,
            false,
        ));
    }
    if from == to {
        return Err(CliError::new(
            "Input and output formats must be different.",
            "VALIDATION",
            1,
            false,
        ));
    }
    match (from, to) {
        ("markdown", "storage") => Ok(ConfluenceClient::local_markdown_to_storage(input)),
        ("markdown", "html") => Ok(ConfluenceClient::local_markdown_to_html(input)),
        ("html", "storage") => Ok(ConfluenceClient::local_html_to_storage(input)),
        ("storage", "markdown") => Ok(ConfluenceClient::local_html_to_markdown(input)),
        ("storage", "html") => Ok(input.to_owned()),
        ("storage", "text") | ("html", "text") => Ok(ConfluenceClient::local_html_to_text(input)),
        ("html", "markdown") => Ok(ConfluenceClient::local_html_to_markdown(input)),
        _ => Err(CliError::new(
            format!("Conversion from {from} to {to} is not supported in the Rust preview."),
            "VALIDATION",
            1,
            false,
        )),
    }
}

#[derive(Debug)]
struct CliError {
    message: String,
    code: &'static str,
    status: Option<u16>,
    details: Option<serde_json::Value>,
    exit_code: u8,
    json: bool,
}

impl CliError {
    fn new(message: impl Into<String>, code: &'static str, exit_code: u8, json: bool) -> Self {
        Self {
            message: message.into(),
            code,
            status: None,
            details: None,
            exit_code,
            json,
        }
    }
}
