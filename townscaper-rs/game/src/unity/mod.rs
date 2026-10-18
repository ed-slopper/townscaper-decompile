//! The Unity engine and .NET behaviour the game's results depend on, exact (board row F11):
//! `Mathf`, `Vector3`, `Quaternion`, `UnityEngine.Random`, `Unity.Mathematics`, `List<T>.Sort` order, ...
//! Addresses are of the engine code as compiled into the reference `GameAssembly.dll`, or `UnityPlayer.dll`
//! for native engine code (notes: `docs/notes/unity.md`).

pub mod camera;
pub mod collections;
pub mod conv;
pub mod geometry;
pub mod mathematics;
pub mod mathf;
pub mod physics;
pub mod quaternion;
pub mod random;
pub mod sse;
pub mod transform;
pub mod vector;
