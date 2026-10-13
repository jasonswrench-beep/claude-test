// SPDX-License-Identifier: Apache-2.0
//! A script of commands for a demo or a test, instead of commands typed on standard input.
//!
//! The form is `command@milliseconds`, separated by semicolons, with the time counted from the start
//! of the session (the countdown included): `pause@7000;resume@9000;marker:look here@11000;stop@13000`.

use openstage_capture::Input;
use openstage_capture_protocol::Command;

/// Turn a demo script into timed commands, in time order.
pub fn parse(script: &str) -> Result<Vec<(u64, Input)>, String> {
    let mut items = Vec::new();
    for part in script.split(';').map(str::trim).filter(|p| !p.is_empty()) {
        let (what, at) = part
            .rsplit_once('@')
            .ok_or_else(|| format!("\"{part}\" needs a time, like {part}@5000 (milliseconds)."))?;
        let at: u64 = at
            .trim()
            .parse()
            .map_err(|_| format!("\"{}\" is not a number of milliseconds (in \"{part}\").", at.trim()))?;
        let what = what.trim();
        let command = match what {
            "pause" => Command::Pause,
            "resume" => Command::Resume,
            "stop" => Command::Stop,
            "restart" => Command::Restart,
            other if other == "marker" || other.starts_with("marker:") => {
                let label = other.strip_prefix("marker").unwrap_or("").trim_start_matches(':').trim();
                Command::Marker { label: if label.is_empty() { None } else { Some(label.to_string()) } }
            }
            other => {
                return Err(format!(
                    "\"{other}\" is not a command. Use pause, resume, stop, restart, or marker (in \"{part}\")."
                ))
            }
        };
        items.push((at, Input::Command(command)));
    }
    items.sort_by_key(|(at, _)| *at);
    Ok(items)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cmds(script: &str) -> Vec<(u64, Command)> {
        parse(script)
            .unwrap()
            .into_iter()
            .map(|(t, i)| match i {
                Input::Command(c) => (t, c),
                other => panic!("not a command: {other:?}"),
            })
            .collect()
    }

    #[test]
    fn a_script_becomes_timed_commands_in_time_order() {
        let got = cmds("stop@13000; pause@7000 ;resume@9000;marker:look here@11000;marker@12000");
        assert_eq!(
            got,
            vec![
                (7000, Command::Pause),
                (9000, Command::Resume),
                (11000, Command::Marker { label: Some("look here".into()) }),
                (12000, Command::Marker { label: None }),
                (13000, Command::Stop),
            ]
        );
    }

    #[test]
    fn restart_is_understood_and_empty_scripts_are_fine() {
        assert_eq!(cmds("restart@500"), vec![(500, Command::Restart)]);
        assert!(parse("").unwrap().is_empty());
        assert!(parse(" ; ;").unwrap().is_empty());
    }

    #[test]
    fn mistakes_are_explained_in_plain_words() {
        assert!(parse("pause").unwrap_err().contains("needs a time"));
        assert!(parse("pause@soon").unwrap_err().contains("not a number of milliseconds"));
        assert!(parse("explode@100").unwrap_err().contains("is not a command"));
    }
}
