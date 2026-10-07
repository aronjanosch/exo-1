use avian3d::prelude::*;
use bevy::prelude::*;

fn main() {
    App::new().add_plugins((DefaultPlugins, PhysicsPlugins::default())).run();
}
