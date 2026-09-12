//! Chargement des modèles GLTF (positions/normales/UV) -> buffers GPU par matériau.

use std::collections::HashMap;
use std::path::Path;
use wgpu::util::DeviceExt;
use wgpu::*;

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub pos: [f32; 3],
    pub nrm: [f32; 3],
    pub uv: [f32; 2],
}

impl Vertex {
    pub const LAYOUT: VertexBufferLayout<'static> = VertexBufferLayout {
        array_stride: std::mem::size_of::<Vertex>() as u64,
        step_mode: VertexStepMode::Vertex,
        attributes: &[
            VertexAttribute { offset: 0, shader_location: 0, format: VertexFormat::Float32x3 },
            VertexAttribute { offset: 12, shader_location: 1, format: VertexFormat::Float32x3 },
            VertexAttribute { offset: 24, shader_location: 2, format: VertexFormat::Float32x2 },
        ],
    };
}

pub struct PartGpu {
    pub mat: String,
    pub vertex_buf: Buffer,
    pub index_buf: Buffer,
    pub count: u32,
}

pub struct Model {
    pub parts: Vec<PartGpu>,
}

pub fn load_model(device: &Device, path: &Path) -> Model {
    let (doc, buffers, _images) =
        gltf::import(path).unwrap_or_else(|e| panic!("GLTF invalide {}: {}", path.display(), e));
    let mut parts: Vec<PartGpu> = Vec::new();

    for node in doc.nodes() {
        let mesh = match node.mesh() {
            Some(m) => m,
            None => continue,
        };
        for prim in mesh.primitives() {
            let reader = prim.reader(|b| buffers.get(b.index()).map(|d| d.0.as_slice()));
            let positions: Vec<[f32; 3]> = reader
                .read_positions()
                .map(|it| it.collect())
                .unwrap_or_default();
            let normals: Vec<[f32; 3]> =
                reader.read_normals().map(|it| it.collect()).unwrap_or_default();
            let uvs: Vec<[f32; 2]> = reader
                .read_tex_coords(0)
                .map(|it| it.into_f32().collect())
                .unwrap_or_else(|| vec![[0.0; 2]; positions.len()]);
            let indices: Vec<u32> = reader
                .read_indices()
                .map(|it| it.into_u32().collect())
                .unwrap_or_else(|| (0..positions.len() as u32).collect());
            if positions.is_empty() {
                continue;
            }
            let mat_name = prim
                .material()
                .name()
                .map(|s| s.to_string())
                .unwrap_or_else(|| "concrete".into());

            let verts: Vec<Vertex> = positions
                .iter()
                .enumerate()
                .map(|(i, p)| Vertex {
                    pos: *p,
                    nrm: normals.get(i).copied().unwrap_or([0.0, 1.0, 0.0]),
                    uv: uvs.get(i).copied().unwrap_or([0.0, 0.0]),
                })
                .collect();
            parts.push(PartGpu {
                mat: mat_name,
                vertex_buf: device.create_buffer_init(&util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&verts),
                    usage: BufferUsages::VERTEX,
                }),
                index_buf: device.create_buffer_init(&util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::cast_slice(&indices),
                    usage: BufferUsages::INDEX,
                }),
                count: indices.len() as u32,
            });
        }
    }
    Model { parts }
}

/// Charge tous les modèles du dossier assets/models.
pub fn load_all_models(device: &Device) -> HashMap<String, Model> {
    let mut out = HashMap::new();
    for entry in std::fs::read_dir("assets/models").expect("dossier assets/models") {
        let entry = entry.unwrap();
        let p = entry.path();
        if p.extension().map(|e| e == "gltf").unwrap_or(false) {
            let name = p.file_stem().unwrap().to_string_lossy().to_string();
            out.insert(name, load_model(device, &p));
        }
    }
    out
}
