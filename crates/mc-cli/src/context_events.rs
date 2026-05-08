//! `mc model context-events` — manage context events for explanation chains.
//!
//! Phase 7A.5 (ADR-0022 Decision 11): CLI verb for adding, listing, and
//! removing context events in `.mosaic/context-events.yaml`.

use mc_narrative::context_events::{read_context_events, write_context_events, ContextEvent};
use std::collections::BTreeMap;
use std::path::Path;

pub struct ContextEventsCommand {
    pub path: String,
    pub action: Action,
}

pub enum Action {
    List,
    Add {
        period: String,
        event_type: String,
        description: String,
        scope: BTreeMap<String, String>,
    },
    Remove {
        id: String,
    },
}

pub fn parse(args: &[String]) -> Result<ContextEventsCommand, String> {
    if args.is_empty() {
        return Err("`mc model context-events` requires a model directory path".into());
    }

    let mut path: Option<String> = None;
    let mut action: Option<Action> = None;
    let mut period: Option<String> = None;
    let mut event_type: Option<String> = None;
    let mut description: Option<String> = None;
    let mut scope = BTreeMap::new();
    let mut remove_id: Option<String> = None;

    let mut iter = args.iter();
    while let Some(arg) = iter.next() {
        match arg.as_str() {
            "--list" => action = Some(Action::List),
            "--add" => { /* parsed from other flags below */ }
            "--remove" => match iter.next() {
                Some(v) => remove_id = Some(v.clone()),
                None => return Err("--remove requires an event ID".into()),
            },
            "--period" => match iter.next() {
                Some(v) => period = Some(v.clone()),
                None => return Err("--period requires a value".into()),
            },
            "--type" => match iter.next() {
                Some(v) => event_type = Some(v.clone()),
                None => return Err("--type requires a value".into()),
            },
            "--description" => match iter.next() {
                Some(v) => description = Some(v.clone()),
                None => return Err("--description requires a value".into()),
            },
            "--scope" => match iter.next() {
                Some(v) => {
                    // Parse "Key=Value" pairs.
                    for pair in v.split(',') {
                        if let Some((k, val)) = pair.split_once('=') {
                            scope.insert(k.trim().to_string(), val.trim().to_string());
                        }
                    }
                }
                None => return Err("--scope requires a value like 'Channel=Display'".into()),
            },
            other if !other.starts_with("--") && path.is_none() => {
                path = Some(other.to_string());
            }
            other => return Err(format!("unknown argument: {other:?}")),
        }
    }

    let path = path.ok_or("`mc model context-events` requires a model directory path")?;

    // Determine action.
    let action = if let Some(id) = remove_id {
        Action::Remove { id }
    } else if let Some(p) = period {
        let et = event_type.ok_or("--add requires --type")?;
        let desc = description.ok_or("--add requires --description")?;
        Action::Add {
            period: p,
            event_type: et,
            description: desc,
            scope,
        }
    } else {
        action.unwrap_or(Action::List)
    };

    Ok(ContextEventsCommand { path, action })
}

pub fn run(cmd: ContextEventsCommand) -> i32 {
    let model_path = Path::new(&cmd.path);
    let model_dir = if model_path.is_dir() {
        model_path.to_path_buf()
    } else {
        model_path.parent().unwrap_or(Path::new(".")).to_path_buf()
    };

    match cmd.action {
        Action::List => {
            let events = read_context_events(&model_dir);
            if events.is_empty() {
                println!("No context events found.");
                return 0;
            }
            let header = format!(
                "{:<20} {:<12} {:<20} {}",
                "ID", "Period", "Type", "Description"
            );
            println!("{header}");
            println!("{}", "-".repeat(72));
            for event in &events {
                let scope_str = if event.scope.is_empty() {
                    String::new()
                } else {
                    let parts: Vec<String> = event
                        .scope
                        .iter()
                        .map(|(k, v)| format!("{k}={v}"))
                        .collect();
                    format!(" [{}]", parts.join(", "))
                };
                println!(
                    "{:<20} {:<12} {:<20} {}{}",
                    event.id, event.period, event.event_type, event.description, scope_str
                );
            }
            println!("\n{} event(s) total.", events.len());
        }
        Action::Add {
            period,
            event_type,
            description,
            scope,
        } => {
            let mut events = read_context_events(&model_dir);

            // Generate ID: ce-{period}-{NNN}
            let seq = events.iter().filter(|e| e.period == period).count() + 1;
            let id = format!("ce-{period}-{seq:03}");

            let event = ContextEvent {
                id: id.clone(),
                period,
                scope,
                event_type,
                description,
                source: None,
                expires_at: None,
            };

            events.push(event);

            if let Err(e) = write_context_events(&model_dir, &events) {
                eprintln!("error: could not write context events: {e}");
                return 1;
            }
            println!("Added context event: {id}");
        }
        Action::Remove { id } => {
            let mut events = read_context_events(&model_dir);
            let before = events.len();
            events.retain(|e| e.id != id);
            if events.len() == before {
                eprintln!("Event ID {id:?} not found.");
                return 1;
            }
            if let Err(e) = write_context_events(&model_dir, &events) {
                eprintln!("error: could not write context events: {e}");
                return 1;
            }
            println!("Removed context event: {id}");
        }
    }

    0
}
