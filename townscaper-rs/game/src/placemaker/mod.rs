//! `Placemaker.*`: the game. Submodules are added as their board rows are ported:
//! grid (E3), graphs (E4), modules (E5), wave function collapse (E6), meshes (E7), world loop (E8),
//! bakes (E9), props (E10), building (G1), camera (G2), saving (G3), life (G5), audio (G6), UI (G7).

pub mod ao_baker;
pub mod big_mesh;
pub mod boot;
pub mod click_effect;
pub mod border_drawer;
pub mod graphs;
pub mod grid_generator;
pub mod hover_data;
pub mod hover_hightlight;
pub mod maker;
pub mod materials;
pub mod module_builder;
pub mod modules;
pub mod prop_achievements;
pub mod prop_mesh_animator;
pub mod prop_placer;
pub mod props;
pub mod rapid_button;
pub mod quads;
pub mod sandbanker;
pub mod text_save_system;
pub mod texture_png_master;
pub mod save_binary;
pub mod save_camera;
pub mod save_data;
pub mod save_system;
pub mod save_xml;
pub mod shadow_baker;
pub mod timing;
pub mod wfc;
pub mod world_master;
pub mod world_meshes;
pub mod ui;
pub mod voxel_bob_effect;
