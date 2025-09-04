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
    let replay = broodrep::Replay::new(file)?;
    
    display_replay_info(&replay);
    
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
    println!("  Duration:      {}", duration);
    
    if let Some(start_time) = replay.start_time() {
        println!("  Started:       {}", start_time.format("%Y-%m-%d %H:%M:%S"));
    }
    
    println!("  Title:         {}", replay.game_title());
    let (width, height) = replay.map_dimensions();
    println!("  Map:           {} ({}x{})", replay.map_name(), width, height);
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
            println!("  [{}] {} ({}, {}, Team {})", 
                     i + 1, 
                     player.name, 
                     player.race, 
                     player.player_type, 
                     player.team);
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

fn format_duration(frames: u32, speed: broodrep::GameSpeed) -> String {
    let total_duration = speed.time_per_step() * frames;
    let total_seconds = total_duration.as_secs();
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{}:{:02} ({} frames at {})", minutes, seconds, frames, speed)
}
