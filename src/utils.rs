use bevy::{prelude::*, window::PrimaryWindow};

pub fn cleanup_recursive<C: Component>(to_clean: Query<Entity, With<C>>, mut cmds: Commands) {
    cmds.entity(to_clean.single()).despawn_recursive();
}

pub fn cleanup<C: Component>(to_clean: Query<Entity, With<C>>, mut cmds: Commands) {
    for e in &to_clean {
        cmds.entity(e).despawn_recursive();
    }
}

pub fn primary_window_exists(primary_window: Query<(), (With<Window>, With<PrimaryWindow>)>) -> bool {
    primary_window.get_single().is_ok()
}

