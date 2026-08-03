use anyhow::Result;
use clap::Parser;
use std::{fs::File, ops::ControlFlow};

#[derive(Parser)]
#[command(name = "broodrep-cli")]
#[command(about = "A StarCraft 1 replay file parser")]
#[command(version)]
struct Args {
    /// Path to the StarCraft 1 replay file (.rep)
    replay_file: std::path::PathBuf,

    /// Read commands and display a summary by command type
    #[arg(long)]
    commands: bool,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let file = File::open(&args.replay_file)?;
    let mut replay = broodrep::Replay::new(file)?;

    display_replay_info(&replay);
    if args.commands {
        display_commands(&mut replay);
    }

    Ok(())
}

fn display_replay_info(replay: &broodrep::Replay<File>) {
    println!("StarCraft 1 Replay Information");
    println!("=============================");
    println!();

    // Game Information Section
    println!("Game Information:");
    println!("  Format:        {}", replay.format());
    println!("  Engine:        {}", replay.engine());

    let duration = format_duration(replay.frames(), replay.game_speed());
    println!("  Duration:      {duration}");

    if let Some(start_time) = replay.start_time() {
        println!(
            "  Started:       {}",
            start_time.format("%Y-%m-%d %H:%M:%S")
        );
    }

    println!("  Title:         {}", replay.game_title());
    let (width, height) = replay.map_dimensions();
    println!("  Map:           {} ({width}x{height})", replay.map_name(),);
    println!();

    // Game Settings Section
    println!("Game Settings:");
    println!("  Speed:         {}", replay.game_speed());
    println!("  Type:          {}", replay.game_type());
    println!("  Host:          {}", replay.host_name());
    println!();

    // Players Section
    let mut players = replay.players().enumerate().peekable();
    if players.peek().is_some() {
        println!("Players:");
        for (i, player) in players {
            println!(
                "  [{}] {} ({}, {}, Team {})",
                i + 1,
                player.name,
                player.race,
                player.player_type,
                player.team
            );
        }
        println!();
    }

    // Observers Section
    let mut observers = replay.observers().peekable();
    if observers.peek().is_some() {
        println!("Observers:");
        for observer in observers {
            println!("  [Obs] {}", observer.name);
        }
        println!();
    }
}

fn display_commands(replay: &mut broodrep::Replay<File>) {
    let mut total = 0;
    let mut type_counts = std::collections::HashMap::<&str, usize>::new();

    match replay.visit_commands(|cmd| {
        total += 1;
        let name = match &cmd.command {
            broodrep::Command::Select { .. } | broodrep::Command::Select121 { .. } => "Select",
            broodrep::Command::SelectAdd { .. } | broodrep::Command::SelectAdd121 { .. } => {
                "Select Add"
            }
            broodrep::Command::SelectRemove { .. } | broodrep::Command::SelectRemove121 { .. } => {
                "Select Remove"
            }
            broodrep::Command::RightClick { .. } | broodrep::Command::RightClick121 { .. } => {
                "Right Click"
            }
            broodrep::Command::TargetedOrder { .. }
            | broodrep::Command::TargetedOrder121 { .. } => "Targeted Order",
            broodrep::Command::Build { .. } => "Build",
            broodrep::Command::Train { .. } => "Train",
            broodrep::Command::Hotkey { .. } => "Hotkey",
            broodrep::Command::Stop { .. } => "Stop",
            broodrep::Command::HoldPosition { .. } => "Hold Position",
            broodrep::Command::Chat { .. } => "Chat",
            broodrep::Command::KeepAlive => "Keep Alive",
            broodrep::Command::LeaveGame { .. } => "Leave Game",
            _ => "Other",
        };
        *type_counts.entry(name).or_default() += 1;
        ControlFlow::Continue(())
    }) {
        Ok(Some(ControlFlow::Continue(()))) => {
            println!("Commands:");
            println!("  Total:         {total}");

            let mut sorted: Vec<_> = type_counts.into_iter().collect();
            sorted.sort_by_key(|item| std::cmp::Reverse(item.1));
            for (name, count) in &sorted {
                println!("    {name}: {count}");
            }
            println!();
        }
        Ok(None) => {
            println!("Commands:        (section not present)");
            println!();
        }
        Ok(Some(ControlFlow::Break(()))) => unreachable!("visitor never breaks"),
        Err(e) => {
            println!("Commands:        (error: {e})");
            println!();
        }
    }
}

fn format_duration(frames: u32, speed: broodrep::GameSpeed) -> String {
    let total_duration = speed.time_per_step() * frames;
    let total_seconds = total_duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes}:{seconds:02} ({frames} frames at {speed})")
}
