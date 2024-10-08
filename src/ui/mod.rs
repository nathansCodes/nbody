use core::f32;

use bevy::{prelude::*, render::camera::CameraUpdateSystem, utils::hashbrown::HashMap};
use bevy_asset_loader::prelude::*;
use bevy_egui::{
    egui::{self, load::SizedTexture, Frame, Pos2, Sense},
    EguiContexts, EguiPlugin, EguiSet,
};

use crate::{
    assets::system::System,
    controls::SimCamera,
    sim::{
        ClearTrajectories, Follow, Hover, Mass, Name, Radius, SimData, SimSnapshot, SimState,
        Trajectory, TrajectoryVisibility,
    },
    AppData, AppEvent, AppState,
};

#[derive(Resource)]
pub struct UiState {
    show_inspector: bool,
    is_active: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            show_inspector: true,
            is_active: false,
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

pub fn ui_is_active(ui_state: Res<UiState>) -> bool {
    ui_state.is_active
}

fn reset_state(mut ui_state: ResMut<UiState>) {
    ui_state.is_active = false;
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

    state.is_active |= response.response.contains_pointer();
}

#[derive(Component)]
pub struct Inspect;

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
    focused: Query<Entity, With<Follow>>,
    inspected: Query<Entity, With<Inspect>>,
    mut state: ResMut<UiState>,
    mut clear_traj_evw: EventWriter<ClearTrajectories>,
    mut sim_data: ResMut<SimData>,
    mut cmds: Commands,
) {
    if !state.show_inspector {
        return;
    }

    let ctx = contexts.ctx_mut();

    let mut reset_trajectories = false;

    let response = egui::SidePanel::left("Inspector")
        .min_width(200.0)
        .default_width(300.0)
        .max_width(600.0)
        .show(ctx, |ui| {
            let inspected_maybe = inspected.get_single();

            egui::CollapsingHeader::new("Simulation arguments")
                .default_open(true)
                .show(ui, |ui| {
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        ui.label("Gravitational constant:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            if ui
                                .add(
                                    egui::DragValue::new(&mut sim_data.gravitational_const)
                                        .speed(0.0001),
                                )
                                .changed()
                            {
                                reset_trajectories = true;
                            }
                        });
                    });
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        ui.label("Trajectory length:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.add(
                                egui::DragValue::new(&mut sim_data.trajectory_len)
                                    .range(1..=usize::MAX),
                            );
                        });
                    });
                    ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                        ui.label("Speed:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            ui.add(egui::DragValue::new(&mut sim_data.speed).range(1..=usize::MAX));
                        });
                    });
                });

            egui::CollapsingHeader::new("Celestial Bodies")
                .default_open(true)
                .show(ui, |ui| {
                    let mut sorted = bodies.iter_mut().collect::<Vec<_>>();

                    sorted.sort_by(|(_, a, ..), (_, b, ..)| a.0.cmp(&b.0));

                    for (entity, ref mut name, ref mut vis, ..) in sorted.iter_mut() {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                            let button = ui.button(name.0.clone());
                            if button.clicked() {
                                if let Ok(inspected_entity) = inspected_maybe {
                                    cmds.entity(inspected_entity).remove::<Inspect>();
                                }
                                cmds.entity(*entity).insert(Inspect);
                            }

                            if button.double_clicked() {
                                if let Ok(focused_entity) = focused.get_single() {
                                    cmds.entity(focused_entity).remove::<Follow>();
                                }
                                cmds.entity(*entity).insert(Follow);
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
                    mut trajectory,
                    mat_handle,
                    mut transform,
                ) = bodies.get_mut(inspected_entity).unwrap();

                let SimSnapshot {
                    ref mut position,
                    ref mut velocity,
                } = trajectory.front_mut().expect("Trajectory empty");

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

                egui::CollapsingHeader::new("Properties")
                    .default_open(true)
                    .show(ui, |ui| {
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                            ui.label("Name:");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                ui.add(
                                    egui::TextEdit::singleline(&mut name.0)
                                        .desired_width(100.0)
                                        .horizontal_align(egui::Align::Max),
                                );
                            });
                        });

                        ui.label("Color:");
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                            if ui.color_edit_button_rgb(&mut color_tmp).changed() {
                                state.is_active = true;
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

                        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                            ui.label("Mass:");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                ui.add(
                                    egui::DragValue::new(&mut mass_tmp)
                                        .max_decimals(2)
                                        .speed(0.05),
                                )
                            });
                        });
                        ui.with_layout(egui::Layout::left_to_right(egui::Align::TOP), |ui| {
                            ui.label("Radius:");
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::TOP), |ui| {
                                let r_drag = ui.add(
                                    egui::DragValue::new(&mut radius.0)
                                        .max_decimals(2)
                                        .speed(0.05),
                                );
                                if r_drag.changed() {
                                    transform.scale = Vec3::new(radius.0, radius.0, radius.0);
                                }
                            });
                        });

                        if ui.button("Remove").clicked() {
                            cmds.entity(entity).despawn();
                            reset_trajectories = true;
                        }
                    });

                let pos_tmp = Vec2::from_array(pos_tmp);
                let vel_tmp = Vec2::from_array(vel_tmp);

                if mass_tmp != mass.0 || pos_tmp != *position || vel_tmp != *velocity {
                    mass.0 = mass_tmp;
                    *position = pos_tmp;
                    *velocity = vel_tmp;
                    transform.translation = pos_tmp.extend(0.0);
                    reset_trajectories = true;
                }
            }
        });

    state.is_active |= response.response.contains_pointer();
    state.is_active |= ctx.dragging_something_else(response.response.id);

    if reset_trajectories {
        clear_traj_evw.send(ClearTrajectories);
    }
}

#[allow(clippy::too_many_arguments, clippy::type_complexity)]
fn hover_indicator(
    camera: Query<(&Camera, &OrthographicProjection, &GlobalTransform), With<SimCamera>>,
    bodies: Query<
        (Entity, &Trajectory, &Transform, &Radius, Option<&Inspect>),
        Or<(With<Follow>, With<Hover>, With<Inspect>)>,
    >,
    images: Res<Images>,
    mut contexts: EguiContexts,
) {
    let quarter_circle = contexts
        .image_id(&images.handles["icons/quarter_circle.png"])
        .unwrap();

    let (cam, cam_projection, cam_transform) = camera.single();

    for (entity, trajectory, transform, Radius(radius), maybe_inspect) in bodies.iter() {
        let SimSnapshot { velocity, position } = trajectory.front().unwrap();

        let scale = f32::max(radius * cam_projection.scale, *radius / 6.0);

        let ctx = contexts.ctx_mut();

        let screen_space_pos = cam
            .world_to_viewport(cam_transform, transform.translation)
            .unwrap_or(Vec2::ZERO);

        let screen_space_scale =
            scale / cam_projection.scale * if maybe_inspect.is_some() { 8.0 } else { 10.0 };

        let pos = Pos2::new(
            screen_space_pos.x - screen_space_scale,
            screen_space_pos.y - screen_space_scale,
        );

        egui::Area::new(egui::Id::new(entity))
            .fixed_pos([
                screen_space_pos.x - screen_space_scale,
                screen_space_pos.y - screen_space_scale,
            ])
            .default_size([screen_space_scale * 2.0, screen_space_scale * 2.0])
            .order(egui::Order::Background)
            .constrain(false)
            .show(ctx, |ui| {
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Min), |ui| {
                    ui.allocate_ui_at_rect(egui::Rect::from_pos(pos), |ui| {
                        ui.vertical(|ui| {
                            ui.add(
                                egui::Image::new(SizedTexture::new(
                                    quarter_circle,
                                    egui::Vec2::new(
                                        screen_space_scale * 0.7,
                                        screen_space_scale * 0.7,
                                    ),
                                ))
                                .rotate(f32::consts::PI, egui::Vec2::splat(0.5)),
                            );
                            ui.add_space(screen_space_scale * 0.6);
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Min), |ui| {
                                ui.add(
                                    egui::Image::new(SizedTexture::new(
                                        quarter_circle,
                                        egui::Vec2::new(
                                            screen_space_scale * 0.7,
                                            screen_space_scale * 0.7,
                                        ),
                                    ))
                                    .rotate(f32::consts::PI / 2.0, egui::Vec2::splat(0.5)),
                                );
                            });
                        })
                    });

                    ui.allocate_ui_at_rect(
                        egui::Rect::from_pos(Pos2::new(
                            screen_space_pos.x + screen_space_scale * 0.3,
                            pos.y,
                        )),
                        |ui| {
                            ui.vertical(|ui| {
                                ui.add(
                                    egui::Image::new(SizedTexture::new(
                                        quarter_circle,
                                        egui::Vec2::new(
                                            screen_space_scale * 0.7,
                                            screen_space_scale * 0.7,
                                        ),
                                    ))
                                    .rotate(f32::consts::PI * 1.5, egui::Vec2::splat(0.5)),
                                );
                                ui.add_space(screen_space_scale * 0.6);
                                ui.with_layout(
                                    egui::Layout::right_to_left(egui::Align::Min),
                                    |ui| {
                                        ui.add(egui::Image::new(SizedTexture::new(
                                            quarter_circle,
                                            egui::Vec2::new(
                                                screen_space_scale * 0.7,
                                                screen_space_scale * 0.7,
                                            ),
                                        )));
                                    },
                                );
                            })
                        },
                    );

                    ui.add_space(screen_space_scale*0.2);

                    ui.vertical(|ui| {
                        ui.add(
                            egui::Label::new(format!(
                                "Position: {:.2}; {:.2}",
                                position.x, position.y
                            ))
                            .sense(Sense {
                                click: false,
                                drag: false,
                                focusable: false,
                            })
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                        ui.add(
                            egui::Label::new(format!(
                                "Velocity: {:.2}; {:.2}",
                                velocity.x, velocity.y
                            ))
                            .sense(Sense {
                                click: false,
                                drag: false,
                                focusable: false,
                            })
                            .wrap_mode(egui::TextWrapMode::Extend),
                        );
                    });
                })
            });
    }
}

fn sim_controls(
    mut contexts: EguiContexts,
    sim_state: Res<State<SimState>>,
    mut next_sim_state: ResMut<NextState<SimState>>,
    images: Res<Images>,
) {
    let pause_icon = contexts
        .image_id(&images.handles["icons/pause.png"])
        .unwrap();
    let play_icon = contexts
        .image_id(&images.handles["icons/play.png"])
        .unwrap();
    let step_icon = contexts
        .image_id(&images.handles["icons/step.png"])
        .unwrap();

    let ctx = contexts.ctx_mut();

    egui::CentralPanel::default()
        .frame(Frame::none())
        .show(ctx, |ui| {
            ui.add_space(5.0);
            ui.columns(3, |cols| {
                cols[1].horizontal_top(|ui| {
                    let min = ui.min_rect().min;
                    let max = ui.max_rect().max;
                    ui.allocate_ui_at_rect(
                        egui::Rect {
                            min: egui::Pos2::new(min.x + (max.x - min.x) / 2.0 - 32.0, min.y),
                            max: egui::Pos2::new(min.x + (max.x - min.x) / 2.0 + 32.0, max.y),
                        },
                        |ui| {
                            let play_pause = ui.add(
                                egui::ImageButton::new(egui::load::SizedTexture::new(
                                    match sim_state.get() {
                                        SimState::Paused | SimState::Step => play_icon,
                                        _ => pause_icon,
                                    },
                                    (24.0, 24.0),
                                ))
                                .rounding(8.0),
                            );

                            if play_pause.clicked() {
                                match sim_state.get() {
                                    SimState::Paused => next_sim_state.set(SimState::Playing),
                                    SimState::Playing => next_sim_state.set(SimState::Paused),
                                    _ => (),
                                }
                            }

                            let step = ui.add(
                                egui::ImageButton::new(egui::load::SizedTexture::new(
                                    step_icon,
                                    (24.0, 24.0),
                                ))
                                .rounding(8.0),
                            );

                            if step.clicked() {
                                next_sim_state.set(SimState::Step);
                            }
                        },
                    );
                });
            });
        });
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
            .configure_sets(
                Update,
                UiSet
                    .run_if(in_state(LoadState::Done))
                    .after(EguiSet::InitContexts)
                    .after(TransformSystem::TransformPropagate)
                    .after(CameraUpdateSystem),
            )
            .add_systems(
                Update,
                (
                    hover_indicator,
                    (
                        reset_state,
                        menu_bar,
                        inspector.run_if(in_state(AppState::Simulating)),
                        sim_controls.run_if(in_state(AppState::Simulating)),
                    )
                        .chain(),
                )
                    .in_set(UiSet),
            );
    }
}
