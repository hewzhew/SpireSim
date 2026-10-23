use std::ffi::OsString;

use clap::{Arg, Command, CommandFactory, FromArgMatches};
use sts_presentation::Locale;

use crate::Cli;

fn text(locale: Locale, key: &str) -> String {
    message(locale, key, &[])
}

pub(super) fn message(locale: Locale, key: &str, parameters: &[(&str, &str)]) -> String {
    locale
        .text("cli", key, parameters)
        .expect("embedded CLI message")
}

fn optional_text(locale: Locale, key: &str) -> Option<String> {
    locale.text("cli", key, &[]).ok()
}

fn command_key(path: &[String]) -> String {
    if path.is_empty() {
        "_root".to_owned()
    } else {
        path.join("/")
    }
}

fn argument_metadata(arg: &Arg, locale: Locale) -> String {
    if !arg.get_action().takes_values() {
        return String::new();
    }
    let mut values = Vec::new();
    if arg.get_action().takes_values() && !arg.is_hide_default_value_set() {
        let defaults = arg
            .get_default_values()
            .iter()
            .map(|value| value.to_string_lossy())
            .collect::<Vec<_>>()
            .join(" ");
        if !defaults.is_empty() {
            values.push(
                locale
                    .text("cli", "common.defaultValue", &[("values", &defaults)])
                    .expect("CLI default-value message"),
            );
        }
    }
    if !arg.is_hide_possible_values_set() {
        let choices = arg
            .get_possible_values()
            .into_iter()
            .filter(|value| !value.is_hide_set())
            .map(|value| value.get_name().to_owned())
            .collect::<Vec<_>>()
            .join(", ");
        if !choices.is_empty() {
            values.push(
                locale
                    .text("cli", "common.possibleValues", &[("values", &choices)])
                    .expect("CLI possible-values message"),
            );
        }
    }
    values.join(" ")
}

fn bind_command(mut command: Command, locale: Locale, path: Vec<String>) -> Command {
    let key = command_key(&path);
    command = command.about(text(locale, &format!("commands.{key}.about")));
    if let Some(about) = optional_text(locale, &format!("commands.{key}.long_about")) {
        command = command.long_about(about);
    }
    command = command.mut_args(|mut arg| {
        let id = arg.get_id().as_str().to_owned();
        let prefix = format!("commands.{key}.args.{id}");
        arg = match (key.as_str(), id.as_str()) {
            ("history", "first" | "last" | "count") => {
                arg.value_parser(move |value: &str| crate::parse_positive_usize(value, locale))
            }
            ("game-facts", "cards")
            | ("combat-query", "eventually_generated_card" | "eventually_lethal_payoff") => arg
                .value_parser(move |value: &str| crate::parse_construction_card_v1(value, locale)),
            ("game-facts", "relics") => {
                arg.value_parser(move |value: &str| crate::parse_game_fact_relic_v1(value, locale))
            }
            ("game-facts", "potions") => {
                arg.value_parser(move |value: &str| crate::parse_game_fact_potion_v1(value, locale))
            }
            _ => arg,
        };
        let help = if id == "locale" {
            Some(text(locale, "common.language"))
        } else {
            optional_text(locale, &format!("{prefix}.help"))
        };
        let long_help = optional_text(locale, &format!("{prefix}.long_help"));
        let metadata = if locale == Locale::ZhCn {
            argument_metadata(&arg, locale)
        } else {
            String::new()
        };
        if let Some(help) = help {
            arg = arg.help(if metadata.is_empty() {
                help
            } else {
                format!("{help} {metadata}")
            });
        } else if !metadata.is_empty() {
            arg = arg.help(metadata.clone());
        }
        if let Some(help) = long_help {
            arg = arg.long_help(if metadata.is_empty() {
                help
            } else {
                format!("{help}\n{metadata}")
            });
        }
        if locale == Locale::ZhCn && arg.get_action().takes_values() {
            arg = arg.hide_default_value(true).hide_possible_values(true);
        }
        arg
    });
    command.mut_subcommands(|child| {
        let mut next = path.clone();
        next.push(child.get_name().to_owned());
        bind_command(child, locale, next)
    })
}

fn bind_generated_help(mut command: Command, locale: Locale) -> Command {
    if command.get_name() == "help" {
        command = command.about(text(locale, "common.helpCommand"));
    }
    if locale == Locale::ZhCn {
        command = command
            .subcommand_help_heading(text(locale, "common.commands"))
            .help_template(format!(
                "{{before-help}}{{about-with-newline}}\n{} {{usage}}\n\n{{all-args}}{{after-help}}",
                text(locale, "common.usage")
            ));
    }
    command
        .mut_args(|mut arg| {
            if arg.get_id() == "help" {
                if arg.get_long_help().is_some() {
                    arg = arg
                        .help(text(locale, "common.helpMore"))
                        .long_help(text(locale, "common.helpSummary"));
                } else {
                    arg = arg.help(text(locale, "common.help"));
                }
            }
            if locale == Locale::ZhCn {
                let heading = if arg.is_positional() {
                    "common.arguments"
                } else {
                    "common.options"
                };
                arg = arg.help_heading(text(locale, heading));
            }
            arg
        })
        .mut_subcommands(|child| bind_generated_help(child, locale))
}

pub(super) fn command(locale: Locale) -> Command {
    let mut command = bind_command(Cli::command(), locale, Vec::new());
    command.build();
    bind_generated_help(command, locale)
}

// Only the stable option syntax is inspected before clap handles --help.
// Values after -- remain positional actions and cannot choose a language.
fn requested_locale(arguments: &[OsString]) -> Locale {
    let mut locale = Locale::En;
    let mut arguments = arguments.iter().skip(1);
    while let Some(argument) = arguments.next() {
        if argument == "--" {
            break;
        }
        let value = if argument == "--locale" {
            arguments.next().and_then(|value| value.to_str())
        } else {
            argument
                .to_str()
                .and_then(|value| value.strip_prefix("--locale="))
        };
        if let Some(value) = value.and_then(|value| value.parse().ok()) {
            locale = value;
        }
    }
    locale
}

pub(super) fn try_parse_from(arguments: Vec<OsString>) -> Result<Cli, clap::Error> {
    let locale = requested_locale(&arguments);
    let matches = command(locale).try_get_matches_from(arguments)?;
    Cli::from_arg_matches(&matches)
}

pub(super) fn parse() -> Cli {
    try_parse_from(std::env::args_os().collect()).unwrap_or_else(|error| error.exit())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Command as Action;

    fn args(values: &[&str]) -> Vec<OsString> {
        values.iter().map(OsString::from).collect()
    }

    #[test]
    fn cli_value_parser_languages_preserve_accepted_values_and_rejections() {
        for locale in ["en", "zh-CN"] {
            for tail in [
                vec![
                    "history",
                    "--session",
                    "fixture",
                    "--journey",
                    "--first",
                    "0",
                ],
                vec![
                    "history",
                    "--session",
                    "fixture",
                    "--journey",
                    "--last",
                    "bad",
                ],
                vec![
                    "history",
                    "--session",
                    "fixture",
                    "--journey",
                    "--after-depth",
                    "0",
                    "--count",
                    "0",
                ],
                vec!["game-facts", "--card", "{{opaque}}"],
                vec!["game-facts", "--relic", "unknown"],
                vec!["game-facts", "--potion", "unknown"],
                vec![
                    "combat-query",
                    "--session",
                    "fixture",
                    "--eventually-generated-card",
                    "unknown",
                ],
                vec![
                    "combat-query",
                    "--session",
                    "fixture",
                    "--eventually-lethal-payoff",
                    "unknown",
                ],
            ] {
                let mut values = vec!["run_play", "--locale", locale];
                values.extend(tail);
                let error = try_parse_from(args(&values)).err().expect("invalid value");
                assert_eq!(error.kind(), clap::error::ErrorKind::ValueValidation);
                if locale == "zh-CN" {
                    assert!(
                        error
                            .to_string()
                            .chars()
                            .any(|c| ('\u{4e00}'..='\u{9fff}').contains(&c)),
                        "{error}"
                    );
                }
            }
            let parsed = try_parse_from(args(&[
                "run_play",
                "--locale",
                locale,
                "game-facts",
                "--card",
                "IronWave",
                "--relic",
                "OrangePellets",
                "--potion",
                "EssenceOfSteel",
            ]))
            .unwrap();
            let Action::GameFacts {
                cards,
                relics,
                potions,
            } = parsed.command
            else {
                panic!("game facts");
            };
            assert_eq!(cards, vec![crate::CardId::IronWave]);
            assert_eq!(relics, vec![crate::RelicId::OrangePellets]);
            assert_eq!(potions, vec![crate::PotionId::EssenceOfSteel]);
            let parsed = try_parse_from(args(&[
                "run_play",
                "--locale",
                locale,
                "history",
                "--session",
                "fixture",
                "--journey",
                "--first",
                "1",
            ]))
            .unwrap();
            let Action::History { first, .. } = parsed.command else {
                panic!("history");
            };
            assert_eq!(first, Some(1));
        }
    }

    fn metadata(command: &Command, path: Vec<String>, rows: &mut Vec<serde_json::Value>) {
        rows.push(serde_json::json!({
            "path": path, "name": command.get_name(), "hidden": command.is_hide_set(),
            "about": command.get_about().map(ToString::to_string),
            "long_about": command.get_long_about().map(ToString::to_string),
            "args": command.get_arguments().map(|arg| serde_json::json!({
                "id": arg.get_id().as_str(), "hidden": arg.is_hide_set(),
                "long": arg.get_long(), "short": arg.get_short(), "required": arg.is_required_set(),
                "global": arg.is_global_set(), "action": format!("{:?}", arg.get_action()),
                "num_args": format!("{:?}", arg.get_num_args()),
                "default_values": arg.get_default_values().iter().map(|value| value.to_string_lossy()).collect::<Vec<_>>(),
                "conflicts": command.get_arg_conflicts_with(arg).iter().map(|other| other.get_id().as_str()).collect::<Vec<_>>(),
                "help": arg.get_help().map(ToString::to_string),
                "long_help": arg.get_long_help().map(ToString::to_string),
                "heading": arg.get_help_heading(),
                "possible_values": arg.get_possible_values().into_iter().map(|value| serde_json::json!({
                    "name": value.get_name(), "help": value.get_help().map(ToString::to_string)
                })).collect::<Vec<_>>()
            })).collect::<Vec<_>>()
        }));
        for child in command
            .get_subcommands()
            .filter(|child| child.get_name() != "help")
        {
            let mut next = path.clone();
            next.push(child.get_name().to_owned());
            metadata(child, next, rows);
        }
    }

    #[test]
    fn cli_help_prose_is_owned_by_resources_instead_of_derive_attributes() {
        fn visit(command: &Command) {
            assert!(command.get_about().is_none(), "{}", command.get_name());
            assert!(command.get_long_about().is_none(), "{}", command.get_name());
            for arg in command.get_arguments().filter(|arg| arg.get_id() != "help") {
                assert!(
                    arg.get_help().is_none(),
                    "{}: {}",
                    command.get_name(),
                    arg.get_id()
                );
                assert!(
                    arg.get_long_help().is_none(),
                    "{}: {}",
                    command.get_name(),
                    arg.get_id()
                );
            }
            for child in command
                .get_subcommands()
                .filter(|child| child.get_name() != "help")
            {
                visit(child);
            }
        }
        let mut command = Cli::command();
        command.build();
        visit(&command);
    }

    #[test]
    fn cli_help_all_current_commands_and_arguments_have_language_independent_schema() {
        let mut schemas = Vec::new();
        for locale in [Locale::En, Locale::ZhCn] {
            let mut rows = Vec::new();
            metadata(&command(locale), Vec::new(), &mut rows);
            if locale == Locale::En {
                println!("CLI_HELP_SCHEMA={}", serde_json::to_string(&rows).unwrap());
            }
            for row in &mut rows {
                row.as_object_mut().unwrap().remove("about");
                row.as_object_mut().unwrap().remove("long_about");
                for arg in row["args"].as_array_mut().unwrap() {
                    for key in ["help", "long_help", "heading"] {
                        arg.as_object_mut().unwrap().remove(key);
                    }
                }
            }
            schemas.push(rows);
        }
        assert_eq!(schemas[0], schemas[1]);
    }

    #[test]
    fn cli_help_language_changes_headings_but_keeps_values_and_defaults() {
        fn visit(mut command: Command, locale: Locale) -> usize {
            let short = command.render_help().to_string();
            let long = command.render_long_help().to_string();
            for help in [&short, &long] {
                assert!(help.contains(if locale == Locale::En {
                    "Usage:"
                } else {
                    "用法："
                }));
                if locale == Locale::ZhCn {
                    for english in [
                        "Options:",
                        "Arguments:",
                        "Print help",
                        "[default:",
                        "[possible values:",
                    ] {
                        assert!(!help.contains(english), "{}: {english}", command.get_name());
                    }
                }
            }
            let mut count = 1;
            for child in command
                .get_subcommands()
                .filter(|child| child.get_name() != "help")
            {
                count += visit(child.clone(), locale);
            }
            count
        }
        assert!(visit(command(Locale::En), Locale::En) > 10);
        assert_eq!(
            visit(command(Locale::ZhCn), Locale::ZhCn),
            visit(command(Locale::En), Locale::En)
        );
        let help = command(Locale::ZhCn).render_help().to_string();
        assert!(help.contains("--locale"));
        assert!(help.contains("[默认：en]"));
    }

    #[test]
    fn cli_help_root_language_reaches_existing_leaf_commands_and_honors_terminator() {
        for values in [
            vec![
                "run_play",
                "--locale",
                "zh-CN",
                "map",
                "--session",
                "fixture",
                "full",
            ],
            vec![
                "run_play",
                "map",
                "--session",
                "fixture",
                "full",
                "--locale=zh-CN",
            ],
            vec![
                "run_play",
                "--locale=zh-CN",
                "agent-shell",
                "--session",
                "fixture",
                "--manual",
            ],
            vec![
                "run_play",
                "inventory",
                "--session",
                "fixture",
                "--locale",
                "zh-CN",
            ],
            vec![
                "run_play",
                "info",
                "--session",
                "fixture",
                "--locale",
                "zh-CN",
                "card",
                "Bash",
            ],
        ] {
            assert_eq!(try_parse_from(args(&values)).unwrap().locale, Locale::ZhCn);
        }
        let parsed = try_parse_from(args(&[
            "run_play",
            "act",
            "--session",
            "fixture",
            "--",
            "--locale",
            "zh-CN",
        ]))
        .unwrap();
        assert_eq!(parsed.locale, Locale::En);
        let Action::Act { actions, .. } = parsed.command else {
            panic!("act command")
        };
        assert_eq!(actions, ["--locale", "zh-CN"]);
        for locale in ["en", "zh-CN"] {
            let error = try_parse_from(args(&["run_play", "--locale", locale, "--help"]))
                .err()
                .expect("help response");
            assert_eq!(error.kind(), clap::error::ErrorKind::DisplayHelp);
        }
    }
}
