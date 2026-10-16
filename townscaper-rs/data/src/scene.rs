//! A scene file (`level0..2`) as a tree of GameObjects with their components (row F8f).

use std::collections::{HashMap, HashSet};

use crate::objects::{class_id as cid, AssetSet, GameObject, LoadedFile, MonoBehaviourHeader, PPtr, Transform};

/// Unity class names for the ids a component can have (Unity's `ClassIDReference`), as far as the build uses them.
pub fn class_name(id: i32) -> Option<&'static str> {
    Some(match id {
        1 => "GameObject",
        4 => "Transform",
        20 => "Camera",
        23 => "MeshRenderer",
        25 => "Renderer",
        33 => "MeshFilter",
        54 => "Rigidbody",
        64 => "MeshCollider",
        65 => "BoxCollider",
        81 => "AudioListener",
        82 => "AudioSource",
        92 => "GUILayer",
        95 => "Animator",
        96 => "TrailRenderer",
        104 => "RenderSettings",
        108 => "Light",
        111 => "Animation",
        114 => "MonoBehaviour",
        120 => "LineRenderer",
        124 => "FlareLayer",
        135 => "SphereCollider",
        136 => "CapsuleCollider",
        137 => "SkinnedMeshRenderer",
        157 => "LightmapSettings",
        196 => "NavMeshSettings",
        198 => "ParticleSystem",
        199 => "ParticleSystemRenderer",
        212 => "SpriteRenderer",
        220 => "LightProbeGroup",
        222 => "CanvasRenderer",
        223 => "Canvas",
        224 => "RectTransform",
        225 => "CanvasGroup",
        226 => "BillboardRenderer",
        258 => "LightProbes",
        320 => "PlayableDirector",
        331 => "SpriteMask",
        1660057539 => "SceneRoots",
        _ => return None,
    })
}

#[derive(Debug, Clone)]
pub struct Component {
    pub path_id: i64,
    pub class_id: i32,
    /// `MeshRenderer`, or for a MonoBehaviour its script: `Placemaker.WorldMaster`.
    pub kind: String,
    pub enabled: Option<bool>,
}

#[derive(Debug, Clone)]
pub struct Node {
    pub game_object: i64,
    pub name: String,
    pub active: bool,
    pub layer: u32,
    pub tag: u16,
    pub transform: Transform,
    pub components: Vec<Component>,
    pub children: Vec<Node>,
}

impl Node {
    pub fn count(&self) -> usize {
        1 + self.children.iter().map(Node::count).sum::<usize>()
    }

    /// Depth-first walk with depth.
    pub fn walk<'a>(&'a self, depth: usize, f: &mut impl FnMut(&'a Node, usize)) {
        f(self, depth);
        for c in &self.children {
            c.walk(depth + 1, f);
        }
    }
}

#[derive(Debug)]
pub enum Error {
    Unresolved(String),
    Read(String),
    Cycle(i64),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Unresolved(s) => write!(f, "unresolved reference: {s}"),
            Error::Read(s) => write!(f, "read: {s}"),
            Error::Cycle(p) => write!(f, "transform cycle at {p}"),
        }
    }
}

impl std::error::Error for Error {}

fn component_kind(a: &AssetSet, f: &LoadedFile, class: i32, bytes: &[u8]) -> (String, Option<bool>) {
    if class == cid::MONO_BEHAVIOUR {
        if let Ok(mb) = MonoBehaviourHeader::read(bytes) {
            let name = a.script_of(f, &mb).map(|s| s.full_name()).unwrap_or_else(|| "MonoBehaviour(?)".into());
            return (name, Some(mb.enabled));
        }
    }
    (class_name(class).map(str::to_string).unwrap_or_else(|| format!("class {class}")), None)
}

fn build(a: &AssetSet, f: &LoadedFile, t_ptr: PPtr, seen: &mut HashSet<i64>) -> Result<Node, Error> {
    let (tf, to) = a.resolve(f, t_ptr).ok_or_else(|| Error::Unresolved(format!("transform {t_ptr:?}")))?;
    if !seen.insert(to.path_id) {
        return Err(Error::Cycle(to.path_id));
    }
    let is_rect = tf.class_id(to) == cid::RECT_TRANSFORM;
    let transform = Transform::read(tf.bytes_of(to), is_rect).map_err(|e| Error::Read(e.to_string()))?;
    let (gf, go) = a.resolve(tf, transform.game_object).ok_or_else(|| Error::Unresolved("game object".into()))?;
    let g = GameObject::read(gf.bytes_of(go)).map_err(|e| Error::Read(e.to_string()))?;
    let mut components = Vec::with_capacity(g.components.len());
    for p in &g.components {
        let (cf, co) = a.resolve(gf, *p).ok_or_else(|| Error::Unresolved(format!("component of {}", g.name)))?;
        let class = cf.class_id(co);
        let (kind, enabled) = component_kind(a, cf, class, cf.bytes_of(co));
        components.push(Component { path_id: co.path_id, class_id: class, kind, enabled });
    }
    let mut children = Vec::with_capacity(transform.children.len());
    for c in &transform.children {
        children.push(build(a, tf, *c, seen)?);
    }
    Ok(Node {
        game_object: go.path_id,
        name: g.name,
        active: g.is_active,
        layer: g.layer,
        tag: g.tag,
        transform,
        components,
        children,
    })
}

/// The root GameObjects of a file (transforms without a father), in path-id order, with their subtrees.
pub fn roots(a: &AssetSet, file: &str) -> Result<Vec<Node>, Error> {
    let f = a.file(file).ok_or_else(|| Error::Unresolved(file.into()))?;
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for o in &f.meta.objects {
        let class = f.class_id(o);
        if class != cid::TRANSFORM && class != cid::RECT_TRANSFORM {
            continue;
        }
        let t = Transform::read(f.bytes_of(o), class == cid::RECT_TRANSFORM).map_err(|e| Error::Read(e.to_string()))?;
        if t.father.is_null() {
            out.push(build(a, f, PPtr { file_id: 0, path_id: o.path_id }, &mut seen)?);
        }
    }
    Ok(out)
}

/// Count of components by kind over a set of trees.
pub fn component_histogram(roots: &[Node]) -> HashMap<String, usize> {
    let mut h = HashMap::new();
    for r in roots {
        r.walk(0, &mut |n, _| {
            for c in &n.components {
                *h.entry(c.kind.clone()).or_default() += 1;
            }
        });
    }
    h
}
