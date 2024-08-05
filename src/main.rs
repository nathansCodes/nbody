use bevy::{asset::LoadedFolder, prelude::*};

mod assets;
mod controls;
mod sim;
mod ui;
pub mod utils;

#[derive(States, Clone, Debug, PartialEq, Eq, Hash)]
pub enum AppState {
    MainMenu,
    Loading,
    Simulating,
    SwitchSim {
        next_sim_id: AssetId<assets::system::System>,
    },
}

#[derive(Event)]
pub enum AppEvent {
    LoadSystem { id: AssetId<assets::system::System> },
    ReloadSystem,
}

#[derive(Resource, Default)]
pub struct AppData {
    systems_metadata_folder: Handle<LoadedFolder>,
    systems: Vec<AssetId<assets::system::System>>,
    system_assets: Option<Handle<LoadedFolder>>,
}

fn load_systems(mut data: ResMut<AppData>, asset_server: Res<AssetServer>) {
    data.systems_metadata_folder = asset_server.load_folder("systems_meta");
}

fn check_system_load_state(
    app_data: Res<AppData>,
    mut next_app_state: ResMut<NextState<AppState>>,
    asset_server: Res<AssetServer>,
) {
    if let Some(folder) = &app_data.system_assets {
        if asset_server.is_loaded_with_dependencies(folder) {
            next_app_state.set(AppState::Simulating);
        }
    }
}

fn recieve_asset_events(
    mut app_data: ResMut<AppData>,
    mut sys_events: EventReader<AssetEvent<assets::system::System>>,
) {
    for ev in sys_events.read() {
        if let AssetEvent::LoadedWithDependencies { id } = ev {
            app_data.systems.push(*id);
        }
    }
}

fn load_next_sim(
    mut app_data: ResMut<AppData>,
    asset_server: Res<AssetServer>,
    app_state: Res<State<AppState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    systems: Res<Assets<assets::system::System>>,
    mut sim_data: ResMut<sim::SimData>,
) {
    if let AppState::SwitchSim { next_sim_id } = app_state.get() {
        let system = systems.get(*next_sim_id).expect("Invalid Asset Id");
        sim_data.gravitational_const = system.gravitational_const;
        sim_data.trajectory_pos = 1;

        next_app_state.set(AppState::Loading);
        app_data.system_assets =
            Some(asset_server.load_folder(format!("systems/{}", system.folder)));
    }
}

fn recieve_app_events(
    mut ev_reader: EventReader<AppEvent>,
    mut app_data: ResMut<AppData>,
    app_state: Res<State<AppState>>,
    mut next_app_state: ResMut<NextState<AppState>>,
    asset_server: Res<AssetServer>,
    systems: Res<Assets<assets::system::System>>,
    mut sim_data: ResMut<sim::SimData>,
) {
    for ev in ev_reader.read() {
        if let AppEvent::LoadSystem { id } = ev {
            match app_state.get() {
                AppState::MainMenu => {
                    let system = systems.get(*id).expect("Invalid Asset Id");
                    sim_data.gravitational_const = system.gravitational_const;

                    next_app_state.set(AppState::Loading);
                    app_data.system_assets =
                        Some(asset_server.load_folder(format!("systems/{}", system.folder)));
                }
                AppState::Simulating => {
                    next_app_state.set(AppState::SwitchSim { next_sim_id: *id });
                }
                _ => (),
            }
        }
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins.set(AssetPlugin {
            watch_for_changes_override: Some(true),
            ..default()
        }))
        .init_asset::<assets::system::System>()
        .init_asset_loader::<assets::system::SystemLoader>()
        .add_event::<AppEvent>()
        .insert_state(AppState::MainMenu)
        .insert_resource(AppData::default())
        .add_systems(Startup, load_systems)
        .add_plugins(sim::SimulationPlugin)
        .add_plugins(ui::UiPlugin)
        .add_plugins(controls::ControlsPlugin)
        .add_systems(
            Update,
            (
                recieve_asset_events,
                recieve_app_events,
                (sim::recieve_asset_events, check_system_load_state)
                    .chain()
                    .run_if(in_state(AppState::Loading)),
            ),
        )
        .run();
}
