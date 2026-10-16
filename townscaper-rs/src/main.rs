//! `townscaper`: the app. Bevy is the host (window, GPU, audio, input); what is drawn and how is the game's
//! and comes from `ts_game` as it is ported (AGENTS.md section 13).

use bevy::prelude::*;
use ts_data::install;

fn main() {
    let title = match install::find_dir().map(|d| install::identify(&d)) {
        Some(Ok(i)) if i.is_reference() => format!("Townscaper (Rust) - Unity {} reference build", i.unity_version),
        Some(Ok(i)) => {
            eprintln!("warning: not the reference build: {:?}", i.differences());
            format!("Townscaper (Rust) - unknown build, Unity {}", i.unity_version)
        }
        Some(Err(e)) => {
            eprintln!("warning: could not read the install: {e}");
            "Townscaper (Rust) - install unreadable".to_string()
        }
        None => {
            eprintln!("warning: no Townscaper install found; set TOWNSCAPER_DIR to your game folder");
            "Townscaper (Rust) - no install".to_string()
        }
    };

    App::new()
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window { title, ..default() }),
            ..default()
        }))
        // STANDIN: flat sky colour, for the game's sky and water (C3, E8)
        .insert_resource(ClearColor(Color::srgb(0.54, 0.74, 0.86)))
        .add_systems(Startup, setup)
        .run();
}

fn setup(mut commands: Commands) {
    // STANDIN: fixed camera, for Placemaker.Ui.OrbitalCamera (G2)
    commands.spawn((Camera3d::default(), Transform::from_xyz(0.0, 12.0, 18.0).looking_at(Vec3::ZERO, Vec3::Y)));
}
