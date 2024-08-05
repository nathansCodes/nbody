use core::f32;

use bevy::{prelude::*, utils::hashbrown::HashMap, window::PrimaryWindow};
use bevy_asset_loader::prelude::*;
use bevy_egui::{egui, EguiContexts, EguiPlugin};

use crate::{
    assets::system::System,
    sim::{
        ClearQueues, Focused, HoverIndicator, Mass, Name, TrajectoryVisibility, Radius, SimData,
        Trajectory, SimSnapshot,
    },
    AppData, AppEvent, AppState,
};

#[derive(Resource)]
pub struct UiState {
    show_inspector: bool,
    is_hovered: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_inspector: true,
            is_hovered: false,
        }
    }
}

#[derive(AssetCollection, Resource)]
struct Images {
    #[asset(path = "icons", collection(typed, mapped))]
    handles: HashMap<String, Handle<Image>>,
}

#[derive(States, Clone, PartialEq, Eq, Hash, Debug)]
enum LoadState {
    Loading,
    Done,
}

pub fn ui_is_hovered(ui_state: Res<UiState>) -> bool {
    ui_state.is_hovered
}

fn reset_state(mut ui_state: ResMut<UiState>) {
    ui_state.is_hovered = false;
}

fn register_images(mut contexts: EguiContexts, images: Res<Images>) {
    for image in images.handles.values() {
        contexts.add_image(image.clone());
    }
}

fn menu_bar(
    mut contexts: EguiContexts,
    app_data: Res<AppData>,
    mut ev_writer: EventWriter<AppEvent>,
    systems: Res<Assets<System>>,
    mut state: ResMut<UiState>,
) {
    let ctx = contexts.ctx_mut();

    let response = egui::TopBottomPanel::top("top_panel").show(ctx, |ui| {
        egui::menu::bar(ui, |ui| {
            egui::menu::menu_button(ui, "Load System", |ui| {
                for id in app_data.systems.clone() {
                    let sys = systems.get(id).expect("Invalid Asset Id");

                    if ui.button(sys.display_name.clone()).clicked() {
                        ev_writer.send(AppEvent::LoadSystem { id });
                        ui.close_menu();
                    }
                }
            });

            egui::menu::menu_button(ui, "View", |ui| {
                egui::menu::menu_button(ui, "Windows", |ui| {
                    if ui.button("Inspector").clicked() {
                        state.show_inspector = !state.show_inspector;
                    }
                });
            });
        });
    });

    state.is_hovered |= response.response.contains_pointer();
}

#[derive(Component)]
pub struct Inspected;

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn inspector(
    mut contexts: EguiContexts,
    mut bodies: Query<(
        Entity,
        &mut Name,
        &mut TrajectoryVisibility,
        &mut Mass,
        &mut Radius,
        &mut Trajectory,
        &Handle<ColorMaterial>,
        &mut Transform,
    )>,
    mut materials: ResMut<Assets<ColorMaterial>>,
    focused: Query<Entity, With<Focused>>,
    inspected: Query<Entity, With<Inspected>>,
    mut state: ResMut<UiState>,
    mut clear_queues_evw: EventWriter<ClearQueues>,
    mut sim_data: ResMut<SimData>,
    mut cmds: Commands,
) {
    if !state.show_inspector {
        return;
    }

    let ctx = contexts.ctx_mut();

    let mut reset_queues = false;

    let response = egui::SidePanel::left("Inspector")
        .exact_width(300.0)
        .show(ctx, |ui| {
            let inspected_maybe = inspected.get_single();

            ui.collapsing("Simulation arguments", |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                    ui.label("Gravitational constant:");
                    ui.add(egui::DragValue::new(&mut sim_data.gravitational_const));
                });
                reset_queues = true;
            });

            ui.collapsing("Celestial Bodies", |ui| {
                let mut sorted = bodies.iter_mut().collect::<Vec<_>>();

                sorted.sort_by(|(_, a, ..), (_, b, ..)| a.0.cmp(&b.0));

                for (entity, ref mut name, ref mut vis, ..) in sorted.iter_mut() {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        let button = ui.button(name.0.clone());
                        if button.clicked() {
                            if let Ok(inspected_entity) = inspected_maybe {
                                cmds.entity(inspected_entity).remove::<Inspected>();
                            }
                            cmds.entity(*entity).insert(Inspected);
                        }

                        if button.double_clicked() {
                            if let Ok(focused_entity) = focused.get_single() {
                                cmds.entity(focused_entity).remove::<Focused>();
                            }
                            cmds.entity(*entity).insert(Focused);
                        }

                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.checkbox(&mut vis.0, "Trajectory Visible");
                        });
                    });
                }
            });

            if let Ok(inspected_entity) = inspected_maybe {
                let (
                    entity,
                    mut name,
                    _,
                    mut mass,
                    mut radius,
                    mut queue,
                    mat_handle,
                    mut transform,
                ) = bodies.get_mut(inspected_entity).unwrap();

                let SimSnapshot {
                    ref mut position,
                    ref mut velocity,
                } = queue.front_mut().expect("Queue empty");

                ui.separator();

                let color = if let Some(material) = materials.get_mut(mat_handle) {
                    &mut material.color
                } else {
                    panic!()
                };

                let color_linear = color.to_srgba();

                let mut pos_tmp = [position.x, position.y];
                let mut vel_tmp = [velocity.x, velocity.y];
                let mut mass_tmp = mass.0;
                let mut color_tmp = [color_linear.red, color_linear.green, color_linear.blue];

                ui.collapsing("Properties", |ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        ui.label("Name:");
                        ui.text_edit_singleline(&mut name.0);
                    });

                    ui.label("Color:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        if ui.color_edit_button_rgb(&mut color_tmp).changed() {
                            state.is_hovered = true;
                            *color = Color::srgb(color_tmp[0], color_tmp[1], color_tmp[2]);
                        }
                    });

                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        ui.label("Position:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut pos_tmp[1])
                                    .max_decimals(2)
                                    .speed(0.05),
                            );
                            ui.add(
                                egui::DragValue::new(&mut pos_tmp[0])
                                    .max_decimals(2)
                                    .speed(0.05),
                            );
                        });
                    });
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        ui.label("Velocity:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut vel_tmp[1])
                                    .max_decimals(2)
                                    .speed(0.05),
                            );
                            ui.add(
                                egui::DragValue::new(&mut vel_tmp[0])
                                    .max_decimals(2)
                                    .speed(0.05),
                            );
                        });
                    });

                    ui.separator();

                    ui.label("Mass:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        ui.add(
                            egui::DragValue::new(&mut mass_tmp)
                                .max_decimals(2)
                                .speed(0.05),
                        );
                    });
                    ui.label("Radius:");
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                        ui.add(
                            egui::DragValue::new(&mut radius.0)
                                .max_decimals(2)
                                .speed(0.05),
                        );
                    });

                    if ui.button("Remove").clicked() {
                        cmds.entity(entity).despawn();
                        reset_queues = true;
                    }
                });

                let pos_tmp = Vec2::from_array(pos_tmp);
                let vel_tmp = Vec2::from_array(vel_tmp);

                if mass_tmp != mass.0 || pos_tmp != *position || vel_tmp != *velocity {
                    mass.0 = mass_tmp;
                    *position = pos_tmp;
                    *velocity = vel_tmp;
                    transform.translation = pos_tmp.extend(0.0);
                    reset_queues = true;
                }
            }
        });

    state.is_hovered |= response.response.contains_pointer();

    if reset_queues {
        clear_queues_evw.send(ClearQueues);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hover_indicator(
    windows: Query<&Window, With<PrimaryWindow>>,
    camera: Query<(&Camera, &OrthographicProjection, &GlobalTransform)>,
    bodies: Query<(Entity, &Trajectory, &Radius)>,
    already_focused: Query<Entity, With<Focused>>,
    already_inspected: Query<Entity, With<Inspected>>,
    mut q_hover_indicator: Query<
        (&mut Transform, &mut Visibility),
        (With<HoverIndicator>, Without<Trajectory>),
    >,
    mouse: Res<ButtonInput<MouseButton>>,
    images: Res<Images>,
    mut contexts: EguiContexts,
    mut cmds: Commands,
) {
    let hover_indicator = q_hover_indicator.get_single_mut();

    if hover_indicator.is_err() {
        cmds.spawn((
            HoverIndicator,
            Transform::default(),
            Visibility::Hidden,
            GlobalTransform::default(),
            InheritedVisibility::default(),
        ))
        .with_children(|parent| {
            parent.spawn(SpriteBundle {
                texture: images.handles["icons/quarter_circle.png"].clone(),
                transform: Transform::from_xyz(-6.0, 6.0, 0.0)
                    .with_scale(Vec3::new(0.1, 0.1, 1.0))
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, f32::consts::PI)),
                ..default()
            });
            parent.spawn(SpriteBundle {
                texture: images.handles["icons/quarter_circle.png"].clone(),
                transform: Transform::from_xyz(6.0, 6.0, 0.0)
                    .with_scale(Vec3::new(0.1, 0.1, 1.0))
                    .with_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        0.0,
                        0.0,
                        f32::consts::PI / 2.0,
                    )),
                ..default()
            });
            parent.spawn(SpriteBundle {
                texture: images.handles["icons/quarter_circle.png"].clone(),
                transform: Transform::from_xyz(6.0, -6.0, 0.0)
                    .with_scale(Vec3::new(0.1, 0.1, 1.0))
                    .with_rotation(Quat::from_euler(EulerRot::XYZ, 0.0, 0.0, 0.0)),
                ..default()
            });
            parent.spawn(SpriteBundle {
                texture: images.handles["icons/quarter_circle.png"].clone(),
                transform: Transform::from_xyz(-6.0, -6.0, 0.0)
                    .with_scale(Vec3::new(0.1, 0.1, 1.0))
                    .with_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        0.0,
                        0.0,
                        1.5 * f32::consts::PI,
                    )),
                ..default()
            });
        });
        return;
    }

    let (mut indicator_transform, mut indicator_vis) = hover_indicator.unwrap();
    *indicator_vis = Visibility::Hidden;

    let (cam, cam_projection, cam_transform) = camera.single();

    if let Some(cursor_pos) = windows.single().cursor_position() {
        for (entity_id, queue, Radius(radius)) in bodies.iter() {
            let SimSnapshot { velocity, position } = queue.front().unwrap();
            // convert to world space
            let cursor_pos = cam.viewport_to_world_2d(cam_transform, cursor_pos).unwrap();

            if cursor_pos.x > position.x - radius
                && cursor_pos.x < position.x + radius
                && cursor_pos.y > position.y - radius
                && cursor_pos.y < position.y + radius
            {
                let scale = f32::max(radius * cam_projection.scale, *radius / 6.0);
                indicator_transform.translation = position.extend(0.0);
                indicator_transform.scale = Vec3::new(scale, scale, 0.0);
                *indicator_vis = Visibility::Visible;

                if mouse.pressed(MouseButton::Middle) {
                    if let Ok(entity_id) = already_focused.get_single() {
                        cmds.entity(entity_id).remove::<Focused>();
                    }

                    cmds.entity(entity_id).insert(Focused);
                }

                if mouse.pressed(MouseButton::Left) || mouse.pressed(MouseButton::Middle) {
                    if let Ok(entity_id) = already_inspected.get_single() {
                        cmds.entity(entity_id).remove::<Inspected>();
                    }

                    cmds.entity(entity_id).insert(Inspected);
                }

                let ctx = contexts.ctx_mut();

                let screen_space_pos = cam
                    .world_to_viewport(cam_transform, indicator_transform.translation)
                    .unwrap_or(Vec2::ZERO);

                let screen_space_scale = scale / cam_projection.scale * 10.0;

                egui::Area::new(egui::Id::new("Indicator"))
                    .fixed_pos([
                        screen_space_pos.x + screen_space_scale * 1.5,
                        screen_space_pos.y - screen_space_scale,
                    ])
                    .order(egui::Order::Background)
                    .constrain(false)
                    .show(ctx, |ui| {
                        ui.add(
                            egui::Label::new(format!(
                                "Position: {:.2}; {:.2}",
                                position.x, position.y
                            ))
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                        ui.add(
                            egui::Label::new(format!(
                                "Velocity: {:.2}; {:.2}",
                                velocity.x, velocity.y
                            ))
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
            }
        }
    }
}

#[derive(SystemSet, PartialEq, Eq, Hash, Debug, Clone)]
struct UiSet;

pub struct UiPlugin;

impl Plugin for UiPlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(EguiPlugin)
            .insert_resource(UiState::default())
            .insert_resource(Images {
                handles: HashMap::new(),
            })
            .insert_state(LoadState::Loading)
            .add_loading_state(
                LoadingState::new(LoadState::Loading)
                    .load_collection::<Images>()
                    .continue_to_state(LoadState::Done),
            )
            .add_systems(OnEnter(LoadState::Done), register_images)
            .configure_sets(Update, UiSet.run_if(in_state(LoadState::Done)))
            .add_systems(
                Update,
                (
                    (
                        reset_state,
                        (menu_bar, inspector.run_if(in_state(AppState::Simulating))),
                    )
                        .chain(),
                    hover_indicator,
                )
                    .in_set(UiSet),
            );
    }
}
