//! The Unity engine and .NET behaviour the game's results depend on, exact (board row F11):
//! `Mathf`, `Vector3`, `Quaternion`, `UnityEngine.Random`, `Unity.Mathematics`, `List<T>.Sort` order, ...
//! Addresses are of the engine code as compiled into the reference `GameAssembly.dll`, or `UnityPlayer.dll`
//! for native engine code (notes: `docs/notes/unity.md`).

pub mod conv;
pub mod geometry;
pub mod mathf;
pub mod quaternion;
pub mod random;
pub mod vector;
