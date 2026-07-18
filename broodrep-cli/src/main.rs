use anyhow::Result;
use clap::Parser;
use std::fs::File;

#[derive(Parser)]
#[command(name = "broodrep-cli")]
#[command(about = "A StarCraft 1 replay file parser")]
#[command(version)]
struct Args {
    /// Path to the StarCraft 1 replay file (.rep)
    replay_file: std::path::PathBuf,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let file = File::open(&args.replay_file)?;
    let mut replay = broodrep::Replay::new(file)?;

    display_replay_info(&replay);
    display_commands(&mut replay);

    Ok(())
}

fn display_replay_info(replay: &broodrep::Replay<std::fs::File>) {
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
    let players: Vec<_> = replay.players().collect();
    if !players.is_empty() {
        println!("Players:");
        for (i, player) in players.iter().enumerate() {
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
    let observers: Vec<_> = replay.observers().collect();
    if !observers.is_empty() {
        println!("Observers:");
        for observer in observers {
            println!("  [Obs] {}", observer.name);
        }
        println!();
    }
}

fn display_commands(replay: &mut broodrep::Replay<std::fs::File>) {
    match replay.get_commands() {
        Ok(Some(commands)) => {
            println!("Commands:");
            println!("  Total:         {}", commands.len());

            // Count commands by type
            let mut type_counts = std::collections::HashMap::<&str, usize>::new();
            for cmd in &commands {
                let name = match &cmd.command {
                    broodrep::Command::Select { .. } | broodrep::Command::Select121 { .. } => {
                        "Select"
                    }
                    broodrep::Command::SelectAdd { .. }
                    | broodrep::Command::SelectAdd121 { .. } => "Select Add",
                    broodrep::Command::SelectRemove { .. }
                    | broodrep::Command::SelectRemove121 { .. } => "Select Remove",
                    broodrep::Command::RightClick { .. }
                    | broodrep::Command::RightClick121 { .. } => "Right Click",
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
            }

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
