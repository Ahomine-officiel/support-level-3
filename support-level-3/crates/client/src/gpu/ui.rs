//! UI 2D : police bitmap (atlas PNG + métriques JSON) et quads colorés.

use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone)]
pub enum UiOp {
    Rect { x: f32, y: f32, w: f32, h: f32, color: [f32; 4] },
    Text { x: f32, y: f32, size: f32, color: [f32; 4], text: String },
}

impl UiOp {
    pub fn text(x: f32, y: f32, size: f32, color: [f32; 4], text: &str) -> UiOp {
        UiOp::Text { x, y, size, color, text: text.to_string() }
    }
}

#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct UiVertex {
    pub pos: [f32; 2],
    pub uv: [f32; 2],
    pub color: [f32; 4],
}

impl UiVertex {
    pub const LAYOUT: wgpu::VertexBufferLayout<'static> = wgpu::VertexBufferLayout {
        array_stride: std::mem::size_of::<UiVertex>() as u64,
        step_mode: wgpu::VertexStepMode::Vertex,
        attributes: &[
            wgpu::VertexAttribute { offset: 0, shader_location: 0, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset: 8, shader_location: 1, format: wgpu::VertexFormat::Float32x2 },
            wgpu::VertexAttribute { offset: 16, shader_location: 2, format: wgpu::VertexFormat::Float32x4 },
        ],
    };
}

#[derive(Deserialize)]
struct RawGlyph {
    x: u32,
    y: u32,
    gw: u32,
    gh: u32,
    gy: u32,
    adv: f32,
}

#[derive(Deserialize)]
struct RawFont {
    size: u32,
    glyphs: HashMap<String, RawGlyph>,
}

#[derive(Clone)]
pub struct Glyph {
    pub x: f32,
    pub y: f32,
    pub gw: f32,
    pub gh: f32,
    pub gy: f32,
    pub adv: f32,
}

#[derive(Clone)]
pub struct FontData {
    pub glyphs: HashMap<char, Glyph>,
    pub atlas_w: f32,
    pub atlas_h: f32,
    pub size: f32,
}

pub fn load_font(device: &wgpu::Device, queue: &wgpu::Queue) -> (super::texture::GpuTexture, FontData) {
    let path = std::path::Path::new("assets/font/font_atlas.png");
    let png = std::fs::read(path).expect("font_atlas.png manquant");
    let img = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .expect("atlas invalide")
        .to_rgba8();
    let (w, h) = img.dimensions();

    let raw: RawFont = serde_json::from_str(
        &std::fs::read_to_string("assets/font/font_atlas.json").expect("font_atlas.json manquant"),
    )
    .expect("font json invalide");

    let mut glyphs = HashMap::new();
    for (ch_s, g) in raw.glyphs {
        let ch = ch_s.chars().next().unwrap_or('?');
        glyphs.insert(
            ch,
            Glyph {
                x: g.x as f32,
                y: g.y as f32,
                gw: g.gw as f32,
                gh: g.gh as f32,
                gy: g.gy as f32,
                adv: g.adv,
            },
        );
    }

    let tex = super::texture::load_png(device, queue, path);
    (
        tex,
        FontData {
            glyphs,
            atlas_w: w as f32,
            atlas_h: h as f32,
            size: raw.size as f32,
        },
    )
}

/// Largeur en pixels d'un texte à la taille donnée.
pub fn text_width(font: &FontData, text: &str, size: f32) -> f32 {
    let scale = size / font.size;
    text.chars()
        .map(|c| font.glyphs.get(&c).map(|g| g.adv * scale).unwrap_or(size * 0.6))
        .sum()
}

/// Convertit les opérations UI en quads. Retourne (sommets, dessins par op).
pub fn build_ui_verts(
    ops: &[UiOp],
    font: &FontData,
) -> (Vec<UiVertex>, Vec<(bool, u32)>) {
    let mut verts: Vec<UiVertex> = Vec::new();
    let mut draws: Vec<(bool, u32)> = Vec::new();
    for op in ops {
        let before = verts.len() as u32 / 4;
        match op {
            UiOp::Rect { x, y, w, h, color } => {
                push_quad(&mut verts, *x, *y, *x + w, *y + h, 0.0, 0.0, 1.0, 1.0, *color);
            }
            UiOp::Text { x, y, size, color, text } => {
                let scale = size / font.size;
                let mut cx = *x;
                for ch in text.chars() {
                    let g = match font.glyphs.get(&ch) {
                        Some(g) => g,
                        None => font.glyphs.get(&'?').unwrap(),
                    };
                    if g.gw > 0.0 && g.gh > 0.0 {
                        let x0 = cx;
                        let y0 = y + g.gy * scale;
                        let x1 = x0 + g.gw * scale;
                        let y1 = y0 + g.gh * scale;
                        let (u0, v0) = (g.x / font.atlas_w, g.y / font.atlas_h);
                        let (u1, v1) = ((g.x + g.gw) / font.atlas_w, (g.y + g.gh) / font.atlas_h);
                        push_quad(&mut verts, x0, y0, x1, y1, u0, v0, u1, v1, *color);
                    }
                    cx += g.adv * scale;
                }
            }
        }
        let quads = verts.len() as u32 / 4 - before;
        let is_text = matches!(op, UiOp::Text { .. });
        draws.push((is_text, quads));
    }
    (verts, draws)
}

fn push_quad(
    out: &mut Vec<UiVertex>,
    x0: f32, y0: f32, x1: f32, y1: f32,
    u0: f32, v0: f32, u1: f32, v1: f32,
    color: [f32; 4],
) {
    out.push(UiVertex { pos: [x0, y0], uv: [u0, v0], color });
    out.push(UiVertex { pos: [x1, y0], uv: [u1, v0], color });
    out.push(UiVertex { pos: [x0, y1], uv: [u0, v1], color });
    out.push(UiVertex { pos: [x1, y1], uv: [u1, v1], color });
}
